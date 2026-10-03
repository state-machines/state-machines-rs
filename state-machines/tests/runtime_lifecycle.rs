#![cfg(feature = "runtime")]
use state_machines::{
    runtime::{Clock, InvokeFailure, RunError, Runner, SetupFailure},
    state_machine,
};
use std::{
    assert_matches,
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

#[derive(Debug, Default)]
pub struct Log {
    factories: Cell<usize>,
    drops: Rc<Cell<usize>>,
    automatic: bool,
}
#[derive(Debug)]
struct Owned(String);
state_machine! {
    name: Wired, dynamic: true, context: Log, initial: Outside,
    states: [Outside, superstate Parent { state A, state B }, Done(Owned)],
    final_states: [Done],
    runtime: { Parent {
        after: [{ delay: 5, event: make_timeout }],
        invoke: [work],
        defer: [load],
    } },
    events {
        begin { transition: { from: Outside, to: A } }
        switch { transition: { from: A, to: B } }
        reset { transition: { from: Parent, to: A, kind: external } }
        exit { transition: { from: Parent, to: Outside } }
        load { payload: String, transition: { from: Outside, to: Done, data: own } }
        timeout { transition: { from: Parent, to: Outside } }
        advance { automatic: true, transition: { from: Parent, to: Outside, guards: [automatic] } }
    }
}
struct Work(Rc<Cell<usize>>);
impl Future for Work {
    type Output = WiredEvent;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
impl<S> Wired<S> {
    fn make_timeout(&self) -> WiredEvent {
        self.ctx.factories.set(self.ctx.factories.get() + 1);
        WiredEvent::Timeout
    }
    fn work(&self) -> Work {
        self.ctx.factories.set(self.ctx.factories.get() + 1);
        Work(self.ctx.drops.clone())
    }
    fn own(&self, value: &mut String) -> Owned {
        Owned(std::mem::take(value))
    }
    fn automatic(&self, ctx: &Log) -> bool {
        ctx.automatic
    }
}
struct Time(u64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0
    }
}
#[test]
fn declarative_parent_setup_is_once_per_visit_and_deferral_recalls_on_exit() {
    let mut runner = Runner::new(DynamicWired::new(Log::default()), 4);
    runner.start(&Time(0)).unwrap();
    runner.enqueue(WiredEvent::Begin).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    runner.enqueue(WiredEvent::Load("owned".into())).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    runner.enqueue(WiredEvent::Switch).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    let drops = runner.machine().current_state();
    assert_eq!(drops, WiredState::B);
    assert_eq!(runner.pending(), 3);
    runner.enqueue(WiredEvent::Exit).unwrap();
    assert_matches!(
        pollster::block_on(runner.drain(1)),
        Err(RunError::StepLimit { limit: 1 })
    );
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    let machine = runner.into_machine().into_done().unwrap();
    assert_eq!(machine.ctx.factories.get(), 2);
    assert_eq!(machine.ctx.drops.get(), 1);
    assert_eq!(machine.done_data().unwrap().0, "owned");
}
#[test]
fn initial_setup_and_reentry_use_observed_clock_and_retry_without_duplicate_factories() {
    let mut runner = Runner::new(
        DynamicWired::new_init_state(Log::default(), WiredState::A),
        1,
    );
    assert_matches!(pollster::block_on(runner.drain(1)), Err(RunError::Setup(_)));
    let error = runner.start(&Time(10)).unwrap_err();
    assert_eq!(error.reason, SetupFailure::Invoke(InvokeFailure::Full));
    assert!(runner.set_capacity(3));
    runner.start(&Time(10)).unwrap();
    runner.start(&Time(10)).unwrap();
    assert_eq!(runner.pending(), 2);
    assert_eq!(runner.next_deadline(), Some(15));
    runner.enqueue(WiredEvent::Reset).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.pending(), 2);
    assert_eq!(runner.tick(&Time(14)), Ok(0));
    assert_eq!(runner.tick(&Time(15)), Ok(1));
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    let machine = runner.into_machine().into_outside().unwrap();
    assert_eq!(machine.ctx.factories.get(), 4);
    assert_eq!(machine.ctx.drops.get(), 2);
}
#[test]
fn transient_automatic_scope_is_initialized_and_cancelled_before_next_microstep() {
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(
        DynamicWired::new(Log {
            automatic: true,
            drops: drops.clone(),
            ..Log::default()
        }),
        3,
    );
    runner.start(&Time(0)).unwrap();
    runner.enqueue(WiredEvent::Begin).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    let machine = runner.into_machine().into_outside().unwrap();
    assert_eq!(machine.ctx.factories.get(), 2);
    assert_eq!(drops.get(), 1);
}
#[cfg(feature = "inspect")]
#[test]
fn declarative_runtime_metadata_is_inspectable() {
    let schema = Wired::<A>::schema();
    assert_eq!(schema.runtime[0].state, "Parent");
    assert_eq!(schema.runtime[0].after[0].delay, 5);
    assert_eq!(schema.runtime[0].invoke, ["work"]);
    assert_eq!(schema.runtime[0].defer, ["load"]);
    assert!(
        !schema
            .validate()
            .iter()
            .any(|d| d.level == state_machines::DiagnosticLevel::Error)
    );
}

mod child {
    use super::*;
    use state_machines::runtime::{EventSink, QueueError, run_child};
    use std::cell::RefCell;
    type Channel = Rc<RefCell<Option<EventSink<ChildEvent>>>>;
    state_machine! {
        name: Child, dynamic: true, initial: Active, states: [Active, Complete], final_states: [Complete],
        events { finish { transition: { from: Active, to: Complete } } }
    }
    state_machine! {
        name: Invoker, dynamic: true, context: Channel, initial: Waiting,
        states: [Waiting, Finished], final_states: [Finished],
        runtime: { Waiting { invoke: [child] } },
        events {
            done { transition: { from: Waiting, to: Finished } }
            reset { transition: { from: Waiting, to: Waiting } }
        }
    }
    impl<S> Invoker<S> {
        fn child(&self) -> impl Future<Output = InvokerEvent> + 'static {
            let child = Runner::new(DynamicChild::new(()), 1);
            *self.ctx.borrow_mut() = Some(child.sink());
            async move {
                let _child = run_child(child, 2).await.unwrap();
                InvokerEvent::Done
            }
        }
    }
    #[test]
    fn declarative_child_invocation_closes_old_channel_on_reentry() {
        let channel = Channel::default();
        let mut runner = Runner::new(DynamicInvoker::new(channel.clone()), 2);
        runner.start(&Time(0)).unwrap();
        let old = channel.borrow().as_ref().unwrap().clone();
        runner.enqueue(InvokerEvent::Reset).unwrap();
        assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
        assert_matches!(old.enqueue(ChildEvent::Finish), Err(QueueError::Closed(_)));
        channel
            .borrow()
            .as_ref()
            .unwrap()
            .enqueue(ChildEvent::Finish)
            .unwrap();
        assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
        assert!(runner.machine().is_finished());
    }
}
