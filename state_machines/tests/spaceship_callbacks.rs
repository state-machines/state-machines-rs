use std::sync::atomic::{AtomicUsize, Ordering};

use state_machines::state_machine;

static BEFORE_CYCLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static BEFORE_ANY_COUNT: AtomicUsize = AtomicUsize::new(0);
static AFTER_OPEN_COUNT: AtomicUsize = AtomicUsize::new(0);
static AFTER_ONLY_CYCLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static FAILSAFE_TRIGGERED: AtomicUsize = AtomicUsize::new(0);

state_machine! {
    name: ShuttleBayController,
    state: BayStatus,
    initial: Standby,
    states: [Standby, Cycling, Open, Locked],
    callbacks {
        before_transition [
            { name: audit_before_cycle, from: Standby, to: Cycling, on: cycle },
            { name: audit_any_transition }
        ]
        after_transition [
            { name: audit_after_open, to: Open },
            { name: audit_after_cycle_only, on: cycle }
        ]
    }
    events {
        cycle {
            transition: { from: Standby, to: Cycling }
        }
        open_doors {
            transition: { from: Cycling, to: Open }
        }
        lock_down {
            transition: { from: Open, to: Locked }
        }
    }
}

impl ShuttleBayController {
    fn audit_before_cycle(&mut self) {
        BEFORE_CYCLE_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn audit_any_transition(&mut self) {
        BEFORE_ANY_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn audit_after_open(&mut self) {
        AFTER_OPEN_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    fn audit_after_cycle_only(&mut self) {
        AFTER_ONLY_CYCLE_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn callbacks_fire_with_filters() {
    BEFORE_CYCLE_COUNT.store(0, Ordering::SeqCst);
    BEFORE_ANY_COUNT.store(0, Ordering::SeqCst);
    AFTER_OPEN_COUNT.store(0, Ordering::SeqCst);
    AFTER_ONLY_CYCLE_COUNT.store(0, Ordering::SeqCst);

    let mut controller = ShuttleBayController::new();
    controller.cycle().expect("cycle should succeed");
    assert_eq!(controller.state(), BayStatus::Cycling);
    assert_eq!(BEFORE_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(BEFORE_ANY_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_ONLY_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 0);

    controller
        .open_doors()
        .expect("opening doors should succeed from Cycling");
    assert_eq!(controller.state(), BayStatus::Open);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_ONLY_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(BEFORE_ANY_COUNT.load(Ordering::SeqCst), 2);

    controller.lock_down().expect("lock down should succeed");
    assert_eq!(controller.state(), BayStatus::Locked);
    assert_eq!(BEFORE_ANY_COUNT.load(Ordering::SeqCst), 3);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 1);
}

state_machine! {
    name: FailsafeController,
    state: FailsafeState,
    initial: Dormant,
    async: true,
    states: [Dormant, Arming, Armed],
    callbacks {
        before_transition [
            { name: log_before_any }
        ]
    }
    events {
        arm {
            transition: { from: Dormant, to: Arming }
        }
        confirm {
            transition: { from: Arming, to: Armed }
        }
    }
}

impl FailsafeController {
    async fn log_before_any(&mut self) {
        FAILSAFE_TRIGGERED.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn async_callbacks_execute() {
    use pollster::block_on;

    FAILSAFE_TRIGGERED.store(0, Ordering::SeqCst);
    block_on(async {
        let mut controller = FailsafeController::new();
        controller.arm().await.expect("arm should succeed");
        controller.confirm().await.expect("confirm should succeed");
    });
    assert_eq!(FAILSAFE_TRIGGERED.load(Ordering::SeqCst), 2);
}

#[test]
fn metadata_lists_callbacks_filters() {
    let definition = ShuttleBayController::definition();
    assert_eq!(definition.name, "ShuttleBayController");
    assert_eq!(definition.events.len(), 3);

    let cycle = &definition.events[0];
    assert_eq!(cycle.name, "cycle");
    assert!(cycle.guards.is_empty());
    let transition = &cycle.transitions[0];
    assert!(transition.unless.is_empty());
}
