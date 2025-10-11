#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicUsize, Ordering};

use state_machines::state_machine;

static BEFORE_CYCLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static AFTER_OPEN_COUNT: AtomicUsize = AtomicUsize::new(0);

state_machine! {
    name: ShuttleBayController,
    initial: Standby,
    states: [Standby, Cycling, Open, Locked],
    events {
        cycle {
            before: [audit_before_cycle]
            transition: { from: Standby, to: Cycling }
        }
        open_doors {
            after: [audit_after_open]
            transition: { from: Cycling, to: Open }
        }
        lock_down {
            transition: { from: Open, to: Locked }
        }
    }
}

impl<C, S> ShuttleBayController<C, S> {
    fn audit_before_cycle(&self) {
        BEFORE_CYCLE_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn audit_after_open(&self) {
        AFTER_OPEN_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn callbacks_fire_on_events() {
    BEFORE_CYCLE_COUNT.store(0, Ordering::SeqCst);
    AFTER_OPEN_COUNT.store(0, Ordering::SeqCst);

    let controller = ShuttleBayController::new(());
    let controller = controller.cycle().expect("cycle should succeed");
    assert_eq!(BEFORE_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 0);

    let controller = controller
        .open_doors()
        .expect("opening doors should succeed from Cycling");
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(BEFORE_CYCLE_COUNT.load(Ordering::SeqCst), 1);

    let _controller = controller.lock_down().expect("lock down should succeed");
    assert_eq!(BEFORE_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 1);
}
