use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use state_machines::{
    AroundOutcome, AroundStage, TransitionContext, TransitionError, TransitionErrorKind,
    state_machine,
};

static BEFORE_CYCLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static BEFORE_ANY_COUNT: AtomicUsize = AtomicUsize::new(0);
static AFTER_OPEN_COUNT: AtomicUsize = AtomicUsize::new(0);
static AFTER_ONLY_CYCLE_COUNT: AtomicUsize = AtomicUsize::new(0);
static AROUND_CYCLE_BEFORE: AtomicUsize = AtomicUsize::new(0);
static AROUND_CYCLE_AFTER: AtomicUsize = AtomicUsize::new(0);
static AROUND_ANY_BEFORE: AtomicUsize = AtomicUsize::new(0);
static AROUND_ANY_AFTER: AtomicUsize = AtomicUsize::new(0);
static SHORT_CIRCUIT_HIT: AtomicBool = AtomicBool::new(false);
static VENT_OVERRIDE_ENABLED: AtomicBool = AtomicBool::new(false);
static FAILSAFE_TRIGGERED: AtomicUsize = AtomicUsize::new(0);
static FAILSAFE_AROUND_BEFORE: AtomicUsize = AtomicUsize::new(0);
static FAILSAFE_AROUND_AFTER: AtomicUsize = AtomicUsize::new(0);

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
        around_transition [
            { name: audit_cycle_wrapper, from: Standby, to: Cycling, on: cycle },
            { name: audit_any_wrapper }
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

    fn audit_cycle_wrapper(
        &mut self,
        ctx: &TransitionContext<BayStatus>,
        stage: AroundStage,
    ) -> AroundOutcome<BayStatus> {
        match stage {
            AroundStage::Before => {
                AROUND_CYCLE_BEFORE.fetch_add(1, Ordering::SeqCst);
                assert_eq!(ctx.event, "cycle");
                assert_eq!(ctx.from, BayStatus::Standby);
                assert_eq!(ctx.to, BayStatus::Cycling);
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess => {
                AROUND_CYCLE_AFTER.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
        }
    }

    fn audit_any_wrapper(
        &mut self,
        _ctx: &TransitionContext<BayStatus>,
        stage: AroundStage,
    ) -> AroundOutcome<BayStatus> {
        match stage {
            AroundStage::Before => {
                AROUND_ANY_BEFORE.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess => {
                AROUND_ANY_AFTER.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
        }
    }
}

#[test]
fn callbacks_fire_with_filters() {
    BEFORE_CYCLE_COUNT.store(0, Ordering::SeqCst);
    BEFORE_ANY_COUNT.store(0, Ordering::SeqCst);
    AFTER_OPEN_COUNT.store(0, Ordering::SeqCst);
    AFTER_ONLY_CYCLE_COUNT.store(0, Ordering::SeqCst);
    AROUND_CYCLE_BEFORE.store(0, Ordering::SeqCst);
    AROUND_CYCLE_AFTER.store(0, Ordering::SeqCst);
    AROUND_ANY_BEFORE.store(0, Ordering::SeqCst);
    AROUND_ANY_AFTER.store(0, Ordering::SeqCst);

    let mut controller = ShuttleBayController::new();
    controller.cycle().expect("cycle should succeed");
    assert_eq!(controller.state(), BayStatus::Cycling);
    assert_eq!(BEFORE_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(BEFORE_ANY_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_ONLY_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 0);
    assert_eq!(AROUND_CYCLE_BEFORE.load(Ordering::SeqCst), 1);
    assert_eq!(AROUND_CYCLE_AFTER.load(Ordering::SeqCst), 1);
    assert_eq!(AROUND_ANY_BEFORE.load(Ordering::SeqCst), 1);
    assert_eq!(AROUND_ANY_AFTER.load(Ordering::SeqCst), 1);

    controller
        .open_doors()
        .expect("opening doors should succeed from Cycling");
    assert_eq!(controller.state(), BayStatus::Open);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AFTER_ONLY_CYCLE_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(BEFORE_ANY_COUNT.load(Ordering::SeqCst), 2);
    assert_eq!(AROUND_CYCLE_BEFORE.load(Ordering::SeqCst), 1);
    assert_eq!(AROUND_ANY_BEFORE.load(Ordering::SeqCst), 2);

    controller.lock_down().expect("lock down should succeed");
    assert_eq!(controller.state(), BayStatus::Locked);
    assert_eq!(BEFORE_ANY_COUNT.load(Ordering::SeqCst), 3);
    assert_eq!(AFTER_OPEN_COUNT.load(Ordering::SeqCst), 1);
    assert_eq!(AROUND_ANY_BEFORE.load(Ordering::SeqCst), 3);
    assert_eq!(AROUND_ANY_AFTER.load(Ordering::SeqCst), 3);
}

#[test]
fn around_callbacks_can_prevent_transition() {
    SHORT_CIRCUIT_HIT.store(false, Ordering::SeqCst);
    EmergencyVentController::set_override(false);

    let mut controller = EmergencyVentController::new();
    let err = controller
        .vent()
        .expect_err("override is required before venting");
    assert_eq!(
        err.kind,
        TransitionErrorKind::GuardFailed {
            guard: "vent_override"
        }
    );
    assert_eq!(controller.state(), VentState::Idle);
    assert!(SHORT_CIRCUIT_HIT.load(Ordering::SeqCst));

    SHORT_CIRCUIT_HIT.store(false, Ordering::SeqCst);
    EmergencyVentController::set_override(true);
    controller.vent().expect("override allows venting");
    assert_eq!(controller.state(), VentState::Venting);
    assert!(!SHORT_CIRCUIT_HIT.load(Ordering::SeqCst));
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
        around_transition [
            { name: log_wrap_any }
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

    async fn log_wrap_any(
        &mut self,
        _ctx: &TransitionContext<FailsafeState>,
        stage: AroundStage,
    ) -> AroundOutcome<FailsafeState> {
        match stage {
            AroundStage::Before => {
                FAILSAFE_AROUND_BEFORE.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess => {
                FAILSAFE_AROUND_AFTER.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
        }
    }
}

state_machine! {
    name: EmergencyVentController,
    state: VentState,
    initial: Idle,
    states: [Idle, Venting],
    callbacks {
        around_transition [
            { name: require_override, on: vent }
        ]
    }
    events {
        vent {
            transition: { from: Idle, to: Venting }
        }
        reset {
            transition: { from: Venting, to: Idle }
        }
    }
}

impl EmergencyVentController {
    fn require_override(
        &mut self,
        ctx: &TransitionContext<VentState>,
        stage: AroundStage,
    ) -> AroundOutcome<VentState> {
        match stage {
            AroundStage::Before => {
                if !VENT_OVERRIDE_ENABLED.load(Ordering::SeqCst) {
                    SHORT_CIRCUIT_HIT.store(true, Ordering::SeqCst);
                    AroundOutcome::Abort(TransitionError::guard_failed(
                        ctx.from,
                        ctx.event,
                        "vent_override",
                    ))
                } else {
                    AroundOutcome::Proceed
                }
            }
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }

    fn set_override(value: bool) {
        VENT_OVERRIDE_ENABLED.store(value, Ordering::SeqCst);
    }
}

#[test]
fn async_callbacks_execute() {
    use pollster::block_on;

    FAILSAFE_TRIGGERED.store(0, Ordering::SeqCst);
    FAILSAFE_AROUND_BEFORE.store(0, Ordering::SeqCst);
    FAILSAFE_AROUND_AFTER.store(0, Ordering::SeqCst);
    block_on(async {
        let mut controller = FailsafeController::new();
        controller.arm().await.expect("arm should succeed");
        controller.confirm().await.expect("confirm should succeed");
    });
    assert_eq!(FAILSAFE_TRIGGERED.load(Ordering::SeqCst), 2);
    assert_eq!(FAILSAFE_AROUND_BEFORE.load(Ordering::SeqCst), 2);
    assert_eq!(FAILSAFE_AROUND_AFTER.load(Ordering::SeqCst), 2);
}

#[test]
fn metadata_lists_callbacks_filters() {
    let definition = ShuttleBayController::definition();
    assert_eq!(definition.name, "ShuttleBayController");
    assert_eq!(definition.events.len(), 3);

    let cycle = &definition.events[0];
    assert_eq!(cycle.name, "cycle");
    assert!(cycle.guards.is_empty());
    assert!(cycle.payload.is_none());
    let transition = &cycle.transitions[0];
    assert!(transition.unless.is_empty());
}
