#![cfg(feature = "runtime")]
use state_machines::{
    runtime::{RunError, Runner, run_child},
    state_machine,
};
use std::assert_matches;

state_machine! {
    name: Initial, dynamic: true, snapshot: true, initial: Boot,
    states: [Boot, Ready, Finished], final_states: [Finished],
    events {
        boot { automatic: true, transition: { from: Boot, to: Ready } }
        finish { transition: { from: Ready, to: Finished } }
    }
}
state_machine! {
    name: Autonomous, dynamic: true, snapshot: true, initial: Cold,
    states: [Cold, Warm, Hot], final_states: [Hot],
    events {
        warm { automatic: true, transition: { from: Cold, to: Warm } }
        heat { automatic: true, transition: { from: Warm, to: Hot } }
    }
}
state_machine! {
    name: AutonomousPair, snapshot: true,
    regions: { first: DynamicAutonomous<()>, second: DynamicAutonomous<()> },
}

#[test]
fn explicit_drain_settles_before_the_first_queued_event() {
    let mut runner = Runner::new(DynamicInitial::new(()), 4);
    assert_eq!(runner.machine().current_state(), InitialState::Boot);
    runner.enqueue(InitialEvent::Finish).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)).unwrap(), 1);
    assert!(runner.machine().is_finished());
}

#[test]
fn invoked_child_can_finish_without_ever_receiving_an_event() {
    let child = Runner::new(DynamicAutonomous::new(()), 1);
    let machine = pollster::block_on(run_child(child, 1)).ok().unwrap();
    assert!(machine.is_finished());
    assert_eq!(machine.current_state(), AutonomousState::Hot);
}

#[test]
fn native_regions_settle_and_join_when_explicitly_driven_without_events() {
    let mut machine =
        AutonomousPair::new(DynamicAutonomous::new(()), DynamicAutonomous::new(()), 1);
    assert_eq!(machine.current_state().first, AutonomousState::Cold);
    assert!(machine.take_join().is_none());
    assert_eq!(pollster::block_on(machine.drain(1)).unwrap(), 0);
    assert!(machine.is_finished());
    assert!(machine.take_join().is_some());
    assert!(machine.take_join().is_none());
}

#[cfg(feature = "serde")]
#[test]
fn restore_remains_inert_until_explicit_driving() {
    let machine = AutonomousPair::new(DynamicAutonomous::new(()), DynamicAutonomous::new(()), 1);
    let mut restored = AutonomousPair::from_snapshot(machine.into_snapshot(), 1)
        .ok()
        .unwrap();
    assert_eq!(restored.current_state().first, AutonomousState::Cold);
    assert!(restored.take_join().is_none());
    pollster::block_on(restored.drain(0)).unwrap();
    assert!(restored.is_finished());
    assert!(restored.take_join().is_some());
}

mod cycle {
    use super::*;
    state_machine! {
        name: InitialCycle, dynamic: true, initial: A, states: [A, B],
        events {
            forward { automatic: true, transition: { from: A, to: B } }
            backward { automatic: true, transition: { from: B, to: A } }
        }
    }
    #[test]
    fn initial_cycles_use_the_existing_automatic_budget_not_an_infinite_child_wait() {
        let mut runner = Runner::new(DynamicInitialCycle::new(()), 1);
        assert_matches!(
            pollster::block_on(runner.drain(0)),
            Err(RunError::AutomaticStepLimit { limit: 64 })
        );
        assert!(!runner.machine().is_poisoned());
    }
}
