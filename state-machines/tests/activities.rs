#![cfg(all(feature = "runtime", not(feature = "runtime-send")))]
use state_machines::{
    runtime::{InvokeFailure, QueueError, RunError, Runner},
    state_machine,
};
use std::assert_matches;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

state_machine! {
    name: Parent, dynamic: true, initial: Working,
    states: [Working, Success, Failed], final_states: [Success, Failed],
    events {
        done { transition: { from: Working, to: Success } }
        error { transition: { from: Working, to: Failed } }
        reset { transition: { from: Working, to: Working } }
        heartbeat { transition: { from: Working, internal: true } }
    }
}
state_machine! {
    name: Child, dynamic: true, initial: Running,
    states: [Running, Complete], final_states: [Complete],
    events {
        finish { transition: { from: Running, to: Complete } }
        heartbeat { transition: { from: Running, internal: true } }
    }
}

struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
#[derive(Default)]
struct Control {
    polls: Cell<usize>,
    drops: Cell<usize>,
    ready: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}
struct Activity(Rc<Control>);
impl Future for Activity {
    type Output = ParentEvent;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.polls.set(self.0.polls.get() + 1);
        if self.0.ready.get() {
            Poll::Ready(ParentEvent::Done)
        } else {
            *self.0.waker.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        self.0.drops.set(self.0.drops.get() + 1);
    }
}

#[test]
fn pending_activity_is_cancelled_on_exit_but_not_internal_transition() {
    let control = Rc::new(Control::default());
    let mut runner = Runner::new(DynamicParent::new(()), 2);
    runner.invoke_future(Activity(control.clone())).unwrap();
    runner.enqueue(ParentEvent::Heartbeat).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(control.drops.get(), 0);
    runner.enqueue(ParentEvent::Reset).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(control.drops.get(), 1);
    assert_eq!(runner.activity_count(), 0);
    assert_eq!(runner.pending(), 0);
}

#[test]
fn mixed_timers_and_activities_share_exact_reservation_cleanup() {
    struct Clock;
    impl state_machines::runtime::Clock for Clock {
        fn now(&self) -> u64 {
            0
        }
    }
    let control = Rc::new(Control::default());
    let mut runner = Runner::new(DynamicParent::new(()), 3);
    runner
        .schedule_after(&Clock, 100, ParentEvent::Heartbeat)
        .unwrap();
    runner.invoke_future(Activity(control.clone())).unwrap();
    runner.enqueue(ParentEvent::Heartbeat).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.pending(), 2);
    runner.enqueue(ParentEvent::Reset).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.pending(), 0);
    assert_eq!(runner.next_deadline(), None);
    assert_eq!(runner.activity_count(), 0);
    assert_eq!(control.drops.get(), 1);
}
#[test]
fn completion_is_owned_raised_once_and_stale_queued_output_is_skipped() {
    let mut runner = Runner::new(DynamicParent::new(()), 2);
    runner.raise(ParentEvent::Reset).unwrap();
    runner.invoke_future(async { ParentEvent::Done }).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(2));
    assert_eq!(runner.machine().current_state(), ParentState::Working);
    assert_eq!(runner.activity_count(), 0);
    runner.invoke_future(async { ParentEvent::Done }).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
    assert!(runner.machine().is_finished());
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(0));
}
#[test]
fn wait_for_work_registers_real_wakers_without_busy_polling() {
    let control = Rc::new(Control::default());
    let mut runner = Runner::new(DynamicParent::new(()), 1);
    runner.invoke_future(Activity(control.clone())).unwrap();
    let wake_count = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(wake_count.clone());
    let mut cx = Context::from_waker(&waker);
    {
        let mut wait = Box::pin(runner.wait_for_work());
        assert!(wait.as_mut().poll(&mut cx).is_pending());
        assert_eq!(control.polls.get(), 1);
        assert_eq!(wake_count.0.load(Ordering::Relaxed), 0);
        control.ready.set(true);
        control.waker.borrow().as_ref().unwrap().wake_by_ref();
        assert_eq!(wake_count.0.load(Ordering::Relaxed), 1);
        assert_matches!(wait.as_mut().poll(&mut cx), Poll::Ready(Ok(())));
    }
    assert_eq!(control.drops.get(), 1);
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
}
#[test]
fn mailbox_wakes_a_waiting_runner_and_backpressure_retains_future() {
    let mut runner = Runner::new(DynamicParent::new(()), 1);
    let sink = runner.sink();
    let woken = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(woken.clone());
    let mut cx = Context::from_waker(&waker);
    {
        let mut wait = Box::pin(runner.wait_for_work());
        assert!(wait.as_mut().poll(&mut cx).is_pending());
        sink.enqueue(ParentEvent::Heartbeat).unwrap();
        assert_eq!(woken.0.load(Ordering::Relaxed), 1);
        assert_matches!(wait.as_mut().poll(&mut cx), Poll::Ready(Ok(())));
    }
    let control = Rc::new(Control::default());
    let error = runner.invoke_future(Activity(control.clone())).unwrap_err();
    assert_eq!(error.reason, InvokeFailure::Full);
    assert_eq!(control.drops.get(), 0);
    assert_eq!(control.polls.get(), 0);
    drop(error.future);
    assert_eq!(control.drops.get(), 1);
}
#[test]
fn explicit_cancellation_works_before_and_after_completion_queueing() {
    let mut runner = Runner::new(DynamicParent::new(()), 1);
    let control = Rc::new(Control::default());
    let id = runner.invoke_future(Activity(control.clone())).unwrap();
    assert!(runner.cancel_activity(id));
    assert!(!runner.cancel_activity(id));
    assert_eq!(runner.pending(), 0);
    assert_eq!(control.drops.get(), 1);
    let id = runner.invoke_future(async { ParentEvent::Done }).unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(runner.poll_activities(&mut cx), 1);
    assert_eq!(runner.poll_activities(&mut cx), 0);
    assert!(runner.cancel_activity(id));
    assert_eq!(runner.pending(), 1);
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.machine().current_state(), ParentState::Working);
}
#[test]
fn child_channel_routes_completion_and_closes_after_done() {
    let mut runner = Runner::new(DynamicParent::new(()), 1);
    let completions = Rc::new(Cell::new(0));
    let observed = completions.clone();
    let (_, sink) = runner
        .invoke_child(
            Runner::new(DynamicChild::new(()), 2),
            2,
            move |child| {
                assert!(child.is_finished());
                observed.set(observed.get() + 1);
                ParentEvent::Done
            },
            |_, _| ParentEvent::Error,
        )
        .unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(0));
    sink.enqueue(ChildEvent::Finish).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
    assert_eq!(completions.get(), 1);
    assert_eq!(runner.machine().current_state(), ParentState::Success);
    assert_matches!(sink.enqueue(ChildEvent::Finish), Err(QueueError::Closed(_)));
}
#[test]
fn child_cancels_on_parent_exit_and_rejection_preserves_child() {
    let mut runner = Runner::new(DynamicParent::new(()), 2);
    let (_, sink) = runner
        .invoke_child(
            Runner::new(DynamicChild::new(()), 1),
            1,
            |_| ParentEvent::Done,
            |_, _| ParentEvent::Error,
        )
        .unwrap();
    runner.raise(ParentEvent::Reset).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
    assert_matches!(sink.enqueue(ChildEvent::Finish), Err(QueueError::Closed(_)));
    let child = Runner::new(DynamicChild::new(()), 1);
    let sink = child.sink();
    let error = runner
        .invoke_child(child, 0, |_| ParentEvent::Done, |_, _| ParentEvent::Error)
        .unwrap_err();
    assert_eq!(error.reason, InvokeFailure::ZeroBudget);
    sink.enqueue(ChildEvent::Finish).unwrap();
    drop(error.child);
    assert_matches!(sink.enqueue(ChildEvent::Finish), Err(QueueError::Closed(_)));
}
#[test]
fn child_dispatch_failure_maps_to_exactly_one_error_event() {
    struct Rejected;
    impl state_machines::runtime::Machine for Rejected {
        type Event = ();
        type Error = &'static str;
        type State = ();
        fn state(&self) {}
        fn epoch(&self) -> u64 {
            0
        }
        fn is_finished(&self) -> bool {
            false
        }
        fn is_poisoned(&self) -> bool {
            false
        }
        async fn dispatch(&mut self, _: ()) -> Result<(), Self::Error> {
            Err("child error")
        }
    }
    let errors = Rc::new(Cell::new(0));
    let observed = errors.clone();
    let mut runner = Runner::new(DynamicParent::new(()), 1);
    let (_, sink) = runner
        .invoke_child(
            Runner::new(Rejected, 1),
            2,
            |_| ParentEvent::Done,
            move |error, _| {
                assert_eq!(error, RunError::Dispatch("child error"));
                observed.set(observed.get() + 1);
                ParentEvent::Error
            },
        )
        .unwrap();
    sink.enqueue(()).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
    assert_eq!(runner.machine().current_state(), ParentState::Failed);
    assert_eq!(errors.get(), 1);
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(0));
}

#[test]
fn child_batches_yield_and_child_mailbox_wakes_parent() {
    let mut runner = Runner::new(DynamicParent::new(()), 1);
    let (_, sink) = runner
        .invoke_child(
            Runner::new(DynamicChild::new(()), 2),
            1,
            |_| ParentEvent::Done,
            |_, _| ParentEvent::Error,
        )
        .unwrap();
    let woken = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(woken.clone());
    let mut cx = Context::from_waker(&waker);
    {
        let mut wait = Box::pin(runner.wait_for_work());
        assert!(wait.as_mut().poll(&mut cx).is_pending());
        assert_eq!(woken.0.load(Ordering::Relaxed), 0);
        sink.enqueue(ChildEvent::Heartbeat).unwrap();
        sink.enqueue(ChildEvent::Finish).unwrap();
        assert!(woken.0.load(Ordering::Relaxed) > 0);
        // The first batch cannot spin through the second event in the same poll.
        assert!(wait.as_mut().poll(&mut cx).is_pending());
        assert_matches!(wait.as_mut().poll(&mut cx), Poll::Ready(Ok(())));
    }
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.machine().current_state(), ParentState::Success);
}

#[cfg(feature = "async")]
mod asynchronous {
    use super::*;
    state_machine! {
        name: AsyncChild, dynamic: true, async: true, initial: Active,
        states: [Active, Finished], final_states: [Finished],
        events { finish { transition: { from: Active, to: Finished, before: [effect] } } }
    }
    impl<C, S> AsyncChild<C, S> {
        async fn effect(&self) {}
    }
    #[test]
    fn async_child_uses_the_same_invocation_contract() {
        let mut runner = Runner::new(DynamicParent::new(()), 1);
        let (_, sink) = runner
            .invoke_child(
                Runner::new(DynamicAsyncChild::new(()), 1),
                2,
                |_| ParentEvent::Done,
                |_, _| ParentEvent::Error,
            )
            .unwrap();
        sink.enqueue(AsyncChildEvent::Finish).unwrap();
        assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
        assert!(runner.machine().is_finished());
    }
}
