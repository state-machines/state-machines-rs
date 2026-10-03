#![cfg(feature = "runtime")]
use state_machines::{
    runtime::{Clock, InvokeFailure, Runner, ScheduleError, WorkScope},
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
pub struct Policy {
    local: Cell<bool>,
    fail: Cell<bool>,
    auto: Cell<bool>,
}
#[derive(Debug)]
pub struct Resource(Rc<Cell<usize>>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
state_machine! {
    name: Scoped, dynamic: true, context: Policy, error: &'static str, initial: A,
    states: [Outside, superstate Parent { state A, state B }],
    events {
        switch { transition: { from: A, to: B } }
        reset { transition: { from: Parent, to: A, kind: external } }
        exit { transition: { from: Parent, to: Outside } }
        return_home { automatic: true, transition: { from: Outside, to: A, guards: [auto] } }
        timeout { payload: Resource, transition: { from: Parent, to: Outside } }
        heartbeat { transition: { from: Parent, internal: true } }
        mixed { branching: true,
            transition: { from: Parent, to: B, kind: local, guards: [local] }
            transition: { from: Parent, to: B, kind: external, fallback: true }
        }
        broken { after: [reject], transition: { from: Parent, to: A, kind: external } }
    }
}
impl<S> Scoped<S> {
    fn auto(&self, ctx: &Policy) -> bool {
        ctx.auto.get()
    }
    fn local(&self, ctx: &Policy) -> bool {
        ctx.local.get()
    }
    fn reject(&self) -> Result<(), &'static str> {
        if self.ctx.fail.get() {
            Err("failed")
        } else {
            Ok(())
        }
    }
}
struct TestClock(u64);
impl Clock for TestClock {
    fn now(&self) -> u64 {
        self.0
    }
}
struct Pending(Rc<Cell<usize>>);
impl Future for Pending {
    type Output = ScopedEvent;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn timeout(counter: &Rc<Cell<usize>>) -> ScopedEvent {
    ScopedEvent::Timeout(Resource(counter.clone()))
}
const PARENT: WorkScope = WorkScope::Named("Parent");

#[test]
fn sibling_transition_keeps_parent_work_and_cancels_leaf_work() {
    let drops = Rc::new(Cell::new(0));
    let cancelled = Rc::new(Cell::new(0));
    let mut runner = Runner::new(DynamicScoped::new(Policy::default()), 4);
    runner
        .schedule_after_in(PARENT, &TestClock(0), 10, timeout(&drops))
        .unwrap();
    runner
        .schedule_after(&TestClock(0), 10, timeout(&drops))
        .unwrap();
    runner
        .invoke_future_in(PARENT, Pending(cancelled.clone()))
        .unwrap();
    runner.enqueue(ScopedEvent::Switch).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.machine().scope_epoch("Parent"), Some(0));
    assert_eq!(drops.get(), 1);
    assert_eq!(cancelled.get(), 0);
    runner.tick(&TestClock(10)).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.machine().current_state(), ScopedState::Outside);
    assert_eq!(runner.machine().scope_epoch("Parent"), None);
    assert_eq!(drops.get(), 2);
    assert_eq!(cancelled.get(), 1);
    assert_eq!(runner.pending(), 0);
}

#[test]
fn scope_reset_suppresses_queued_output_and_auto_return_cannot_revive_lease() {
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(
        DynamicScoped::new(Policy {
            auto: Cell::new(true),
            ..Policy::default()
        }),
        3,
    );
    runner.enqueue(ScopedEvent::Reset).unwrap();
    runner
        .schedule_after_in(PARENT, &TestClock(0), 0, timeout(&drops))
        .unwrap();
    runner.tick(&TestClock(0)).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(2));
    assert_eq!(runner.machine().scope_epoch("Parent"), Some(1));
    runner
        .schedule_after_in(PARENT, &TestClock(0), 100, timeout(&drops))
        .unwrap();
    runner.enqueue(ScopedEvent::Exit).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.machine().current_state(), ScopedState::A);
    assert_eq!(runner.machine().scope_epoch("Parent"), Some(2));
    assert_eq!(runner.pending(), 0);
    assert_eq!(drops.get(), 2);
}

#[test]
fn exact_selected_domain_and_failed_transition_preserve_expected_visits() {
    let policy = Policy {
        local: Cell::new(true),
        fail: Cell::new(true),
        ..Policy::default()
    };
    let mut machine = DynamicScoped::new(policy);
    machine.handle(ScopedEvent::Heartbeat).unwrap();
    machine.handle(ScopedEvent::Mixed).unwrap();
    assert_eq!(machine.scope_epoch("Parent"), Some(0));
    assert_matches!(machine.handle(ScopedEvent::Broken), Err(_));
    assert_eq!(machine.scope_epoch("Parent"), Some(0));
    let typed = machine.into_b().unwrap();
    typed.ctx.local.set(false);
    let mut machine = typed.into_dynamic();
    machine.handle(ScopedEvent::Mixed).unwrap();
    assert_eq!(machine.scope_epoch("Parent"), Some(1));
}

#[test]
fn inactive_scopes_return_unconsumed_resources_and_unpolled_futures() {
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(DynamicScoped::new(Policy::default()), 1);
    let error = runner
        .schedule_after_in(
            WorkScope::Named("Outside"),
            &TestClock(0),
            1,
            timeout(&drops),
        )
        .unwrap_err();
    assert_matches!(error, ScheduleError::InactiveScope(_));
    assert_eq!(drops.get(), 0);
    drop(error.into_event());
    let error = runner
        .invoke_future_in(WorkScope::Named("Missing"), Pending(drops.clone()))
        .unwrap_err();
    assert_eq!(error.reason, InvokeFailure::InactiveScope);
    assert_eq!(drops.get(), 1);
    drop(error.future);
    assert_eq!(drops.get(), 2);
    assert_eq!(runner.pending(), 0);
}
