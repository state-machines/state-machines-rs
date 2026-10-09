#![cfg(all(feature = "runtime", not(feature = "runtime-send")))]
use state_machines::{
    runtime::{QueueError, RunError, Runner},
    state_machine,
};
use std::assert_matches;
use std::sync::atomic::{AtomicBool, Ordering};
static INTERNAL: AtomicBool = AtomicBool::new(true);
#[derive(Debug)]
pub struct Request(Option<String>);
#[derive(Debug)]
struct Data(String);
state_machine! {
    name: Queued, dynamic: true, initial: Busy, states: [Busy, Ready, Loaded(Data)],
    events {
        ready { transition: { from: Busy, to: Ready } }
        load { payload: Request, transition: { from: Ready, to: Loaded, data: own } }
        heartbeat { transition: { from: Ready, internal: true } }
        reset { transition: { from: Ready, to: Ready } }
        mixed { branching: true,
            transition: { from: Ready, internal: true, guards: [internal] }
            transition: { from: Ready, to: Ready, fallback: true }
        }
    }
}
impl<C, S> Queued<C, S> {
    fn own(&self, request: &mut Request) -> Data {
        Data(request.0.take().unwrap())
    }
    fn internal(&self, _ctx: &C) -> bool {
        INTERNAL.load(Ordering::Relaxed)
    }
}
#[test]
fn deferral_recalls_owned_events_without_cloning_or_reordering() {
    let mut runner = Runner::new(DynamicQueued::new(()), 2);
    runner.defer_in(QueuedState::Busy, |event| {
        matches!(event, QueuedEvent::Load(_))
    });
    runner
        .enqueue(QueuedEvent::Load(Request(Some("owned".into()))))
        .unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.deferred(), 1);
    assert_eq!(runner.pending(), 1);
    runner.enqueue(QueuedEvent::Ready).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(2));
    assert_eq!(runner.into_machine().loaded_data().unwrap().0, "owned");
}
#[test]
fn raised_events_have_priority_and_backpressure_preserves_ownership() {
    let mut runner = Runner::new(DynamicQueued::new(()), 2);
    let sink = runner.sink();
    sink.enqueue(QueuedEvent::Load(Request(Some("first".into()))))
        .unwrap();
    sink.raise(QueuedEvent::Ready).unwrap();
    let event = sink
        .enqueue(QueuedEvent::Load(Request(Some("full".into()))))
        .unwrap_err()
        .into_event();
    assert_matches!(event, QueuedEvent::Load(Request(Some(value))) if value == "full");
    assert_eq!(
        pollster::block_on(runner.drain(1)),
        Err(RunError::StepLimit { limit: 1 })
    );
    assert_eq!(runner.machine().current_state(), QueuedState::Ready);
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
    drop(runner);
    assert_matches!(sink.raise(QueuedEvent::Ready), Err(QueueError::Closed(_)));
}
#[test]
fn internal_dispatch_preserves_epoch_and_external_self_reentry_changes_it() {
    let mut machine = DynamicQueued::new(());
    machine.handle(QueuedEvent::Ready).unwrap();
    let epoch = machine.transition_epoch();
    machine.handle(QueuedEvent::Heartbeat).unwrap();
    machine.handle(QueuedEvent::Mixed).unwrap();
    assert_eq!(machine.transition_epoch(), epoch);
    INTERNAL.store(false, Ordering::Relaxed);
    machine.handle(QueuedEvent::Mixed).unwrap();
    assert_ne!(machine.transition_epoch(), epoch);
    let epoch = machine.transition_epoch();
    machine.handle(QueuedEvent::Reset).unwrap();
    assert_ne!(machine.transition_epoch(), epoch);
}

mod callbacks {
    use super::*;
    use state_machines::runtime::EventSink;
    use std::{cell::RefCell, rc::Rc};
    type Signals = Rc<RefCell<Option<EventSink<RaisedEvent>>>>;
    state_machine! {
        name: Raised, dynamic: true, context: Signals, initial: Idle, states: [Idle, Middle, Done],
        events {
            begin { before: [notify], transition: { from: Idle, to: Middle } }
            next { transition: { from: Middle, to: Done } }
        }
    }
    impl<S> Raised<S> {
        fn notify(&self) {
            self.ctx
                .borrow()
                .as_ref()
                .unwrap()
                .raise(RaisedEvent::Next)
                .unwrap();
        }
    }
    #[test]
    fn callbacks_can_raise_without_reentrant_dispatch() {
        let signals = Rc::new(RefCell::new(None));
        let mut runner = Runner::new(DynamicRaised::new(signals.clone()), 2);
        *signals.borrow_mut() = Some(runner.sink());
        runner.enqueue(RaisedEvent::Begin).unwrap();
        runner.enqueue(RaisedEvent::Begin).unwrap();
        assert_matches!(
            pollster::block_on(runner.drain(3)),
            Err(RunError::Dispatch(
                state_machines::DynamicError::InvalidTransition { from: "Done", .. }
            ))
        );
        assert_eq!(runner.machine().current_state(), RaisedState::Done);
    }
}
#[test]
fn rejected_event_does_not_lose_the_remaining_queue() {
    let mut runner = Runner::new(DynamicQueued::new(()), 2);
    runner.enqueue(QueuedEvent::Heartbeat).unwrap();
    runner.enqueue(QueuedEvent::Ready).unwrap();
    let error = pollster::block_on(runner.drain(2)).unwrap_err();
    assert_matches!(error, RunError::Dispatch(_));
    // Runner errors chain to the machine error, so `?` into anyhow or
    // Box<dyn Error> keeps the cause.
    let error: Box<dyn std::error::Error + Send + Sync> = Box::new(error);
    let cause = error
        .source()
        .expect("dispatch failures expose the machine error");
    assert!(cause.to_string().contains("heartbeat"));
    assert_eq!(runner.pending(), 1);
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
}
