use state_machines::{DynamicError, state_machine};
use std::cell::Cell;
#[derive(Debug, Default)]
pub struct Policy {
    enabled: bool,
    calls: Cell<u32>,
    fail: bool,
}
state_machine! {
    name: Automatic, dynamic: true, context: Policy, error: &'static str,
    initial: Idle, states: [Idle, Checking, Approved, Rejected],
    events {
        begin { transition: { from: Idle, to: Checking } }
        decide { automatic: true, branching: true,
            transition: { from: Checking, to: Approved, guards: [enabled], before: [effect] }
            transition: { from: Checking, to: Rejected, fallback: true }
        }
    }
}
impl<S> Automatic<S> {
    fn enabled(&self, ctx: &Policy) -> bool {
        ctx.calls.set(ctx.calls.get() + 1);
        ctx.enabled
    }
    fn effect(&self) -> Result<(), &'static str> {
        if self.ctx.fail { Err("effect") } else { Ok(()) }
    }
}
#[test]
fn handle_settles_and_selection_evaluates_guards_once() {
    let mut machine = DynamicAutomatic::new(Policy {
        enabled: true,
        ..Policy::default()
    });
    machine.handle(AutomaticEvent::Begin).unwrap();
    assert_eq!(machine.current_state(), AutomaticState::Approved);
    assert_eq!(machine.into_approved().unwrap().ctx.calls.get(), 1);
    let mut machine = DynamicAutomatic::new(Policy::default());
    machine.handle(AutomaticEvent::Begin).unwrap();
    assert_eq!(machine.current_state(), AutomaticState::Rejected);
}
#[test]
fn automatic_callback_failure_recovers_source_without_silent_fallback() {
    let mut machine = DynamicAutomatic::new(Policy {
        enabled: true,
        fail: true,
        ..Policy::default()
    });
    assert!(matches!(
        machine.handle(AutomaticEvent::Begin),
        Err(DynamicError::CallbackFailed { .. })
    ));
    assert_eq!(machine.current_state(), AutomaticState::Checking);
    assert!(!machine.is_poisoned());
}
mod cycle {
    use super::*;
    state_machine! {
        name: Cycle, dynamic: true, initial: A, states: [A, B],
        events {
            first { automatic: true, transition: { from: A, to: B } }
            second { automatic: true, transition: { from: B, to: A } }
        }
    }
    #[test]
    fn cycles_and_zero_budget_are_bounded_without_extra_commits() {
        let mut machine = DynamicCycle::new(());
        assert_eq!(
            machine.stabilize(0),
            Err(DynamicError::StepLimit { limit: 0 })
        );
        assert_eq!(machine.current_state(), CycleState::A);
        assert_eq!(
            machine.stabilize(3),
            Err(DynamicError::StepLimit { limit: 3 })
        );
        assert_eq!(machine.current_state(), CycleState::B);
        assert!(!machine.is_poisoned());
    }
}
#[cfg(feature = "async")]
mod asynchronous {
    use super::*;
    state_machine! {
        name: AsyncAuto, dynamic: true, async: true, initial: Start,
        states: [Start, Middle, Done],
        events {
            first { automatic: true, transition: { from: Start, to: Middle, guards: [ready] } }
            second { automatic: true, transition: { from: Middle, to: Done } }
        }
    }
    impl<C, S> AsyncAuto<C, S> {
        async fn ready(&self, _ctx: &C) -> bool {
            true
        }
    }
    #[test]
    fn async_microsteps_reach_stability() {
        let mut machine = DynamicAsyncAuto::new(());
        assert_eq!(pollster::block_on(machine.stabilize(4)), Ok(2));
        assert_eq!(machine.current_state(), AsyncAutoState::Done);
        assert_eq!(pollster::block_on(machine.stabilize(0)), Ok(0));
    }
}
