use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::{TransitionErrorKind, state_machine};

static AIRLOCK_OPEN: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: AirlockController,
    state: AirlockState,
    initial: Closed,
    states: [Closed, Cycling, Open],
    events {
        cycle {
            // Proceed only if guard passes AND unless guard is false
            guards: [sealed]
            unless: [door_open]
            transition: { from: Closed, to: Cycling }
        }
        finish {
            transition: { from: Cycling, to: Open }
        }
    }
}

impl AirlockController {
    fn sealed(&self) -> bool {
        true
    }
    fn door_open(&self) -> bool {
        AIRLOCK_OPEN.load(Ordering::SeqCst)
    }
}

#[test]
fn unless_blocks_when_true() {
    AIRLOCK_OPEN.store(true, Ordering::SeqCst);
    let mut a = AirlockController::new();
    let err = a.cycle().expect_err("unless should block when true");
    assert_eq!(
        err.kind,
        TransitionErrorKind::GuardFailed { guard: "door_open" }
    );
    assert_eq!(a.state(), AirlockState::Closed);
}

#[test]
fn unless_allows_when_false() {
    AIRLOCK_OPEN.store(false, Ordering::SeqCst);
    let mut a = AirlockController::new();
    a.cycle().expect("unless false allows cycling");
    assert_eq!(a.state(), AirlockState::Cycling);
}

#[test]
fn metadata_reflects_unless_filters() {
    let definition = AirlockController::definition();
    let cycle = &definition.events[0];
    assert_eq!(cycle.name, "cycle");
    assert_eq!(cycle.guards, &["sealed"]);
    assert_eq!(cycle.transitions.len(), 1);
    let transition = &cycle.transitions[0];
    assert_eq!(transition.unless, &["door_open"]);
}
