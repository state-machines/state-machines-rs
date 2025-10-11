#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::state_machine;

static AIRLOCK_OPEN: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: AirlockController,

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

impl<C, S> AirlockController<C, S> {
    fn sealed(&self, _ctx: &C) -> bool {
        true
    }
    fn door_open(&self, _ctx: &C) -> bool {
        AIRLOCK_OPEN.load(Ordering::SeqCst)
    }
}

#[test]
fn unless_blocks_when_true() {
    AIRLOCK_OPEN.store(true, Ordering::SeqCst);
    let _a = AirlockController::new(());
    let err = _a.cycle().expect_err("unless should block when true");
    let (_a, guard_err) = err;
    assert_eq!(guard_err.guard, "door_open");
    assert_eq!(guard_err.event, "cycle");
}

#[test]
fn unless_allows_when_false() {
    AIRLOCK_OPEN.store(false, Ordering::SeqCst);
    let _a = AirlockController::new(());
    let _a = _a.cycle().expect("unless false allows cycling");
}
