use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use pollster::block_on;
use state_machines::{TransitionErrorKind, state_machine};

#[derive(Clone)]
struct BurnRequest {
    power: u8,
}

static FUEL_AVAILABLE: AtomicBool = AtomicBool::new(false);
static GUARD_INSPECTED_POWER: AtomicU8 = AtomicU8::new(0);
static BEFORE_CALLED_WITH: AtomicU8 = AtomicU8::new(0);
static AFTER_CALLED_WITH: AtomicU8 = AtomicU8::new(0);

state_machine! {
    name: ThrusterController,
    state: ThrusterState,
    initial: Idle,
    states: [Idle, Firing, Cooling],
    events {
        fire {
            payload: BurnRequest,
            guards: [fuel_ready],
            before: [record_before],
            after: [record_after],
            transition: { from: Idle, to: Firing }
        }
        cooldown {
            transition: { from: Firing, to: Cooling }
        }
    }
}

impl ThrusterController {
    fn fuel_ready(&self, request: &BurnRequest) -> bool {
        GUARD_INSPECTED_POWER.store(request.power, Ordering::SeqCst);
        FUEL_AVAILABLE.load(Ordering::SeqCst) && request.power <= 5
    }

    fn record_before(&mut self, request: &BurnRequest) {
        BEFORE_CALLED_WITH.store(request.power, Ordering::SeqCst);
    }

    fn record_after(&mut self, request: &BurnRequest) {
        AFTER_CALLED_WITH.store(request.power, Ordering::SeqCst);
    }
}

#[test]
fn sync_payload_event_runs_guards_and_callbacks() {
    FUEL_AVAILABLE.store(false, Ordering::SeqCst);
    GUARD_INSPECTED_POWER.store(0, Ordering::SeqCst);
    BEFORE_CALLED_WITH.store(0, Ordering::SeqCst);
    AFTER_CALLED_WITH.store(0, Ordering::SeqCst);

    let mut controller = ThrusterController::new();
    let request = BurnRequest { power: 6 };

    assert!(!controller.can_fire(&request));
    let err = controller
        .fire(request.clone())
        .expect_err("guard should prevent overpowered burn");
    assert_eq!(
        err.kind,
        TransitionErrorKind::GuardFailed {
            guard: "fuel_ready"
        }
    );
    assert_eq!(controller.state(), ThrusterState::Idle);
    assert_eq!(GUARD_INSPECTED_POWER.load(Ordering::SeqCst), 6);
    assert_eq!(BEFORE_CALLED_WITH.load(Ordering::SeqCst), 0);
    assert_eq!(AFTER_CALLED_WITH.load(Ordering::SeqCst), 0);

    FUEL_AVAILABLE.store(true, Ordering::SeqCst);
    let request = BurnRequest { power: 3 };
    assert!(controller.can_fire(&request));
    controller
        .fire(request.clone())
        .expect("payload guard should now pass");
    assert_eq!(controller.state(), ThrusterState::Firing);
    assert_eq!(BEFORE_CALLED_WITH.load(Ordering::SeqCst), 3);
    assert_eq!(AFTER_CALLED_WITH.load(Ordering::SeqCst), 3);
}

#[derive(Clone)]
struct Cargo {
    weight: u8,
}

static ROUTE_CLEAR: AtomicBool = AtomicBool::new(false);
static ASYNC_BEFORE_CALLED: AtomicBool = AtomicBool::new(false);
static ASYNC_AFTER_CALLED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: CargoBayController,
    state: CargoState,
    initial: Awaiting,
    async: true,
    states: [Awaiting, Dispatching, Delivered],
    events {
        dispatch {
            payload: Cargo,
            guards: [route_clear],
            before: [log_departure],
            after: [log_arrival],
            transition: { from: Awaiting, to: Dispatching }
        }
        finish {
            transition: { from: Dispatching, to: Delivered }
        }
    }
}

impl CargoBayController {
    async fn route_clear(&self, cargo: &Cargo) -> bool {
        ROUTE_CLEAR.load(Ordering::SeqCst) && cargo.weight <= 4
    }

    async fn log_departure(&mut self, _cargo: &Cargo) {
        ASYNC_BEFORE_CALLED.store(true, Ordering::SeqCst);
    }

    async fn log_arrival(&mut self, _cargo: &Cargo) {
        ASYNC_AFTER_CALLED.store(true, Ordering::SeqCst);
    }
}

#[test]
fn async_payload_event_obeys_guards() {
    ROUTE_CLEAR.store(false, Ordering::SeqCst);
    ASYNC_BEFORE_CALLED.store(false, Ordering::SeqCst);
    ASYNC_AFTER_CALLED.store(false, Ordering::SeqCst);

    block_on(async {
        let mut controller = CargoBayController::new();
        let cargo = Cargo { weight: 5 };
        assert!(!controller.can_dispatch(&cargo).await);
        let err = controller
            .dispatch(cargo.clone())
            .await
            .expect_err("route should be blocked");
        assert_eq!(
            err.kind,
            TransitionErrorKind::GuardFailed {
                guard: "route_clear"
            }
        );
        assert_eq!(controller.state(), CargoState::Awaiting);

        ROUTE_CLEAR.store(true, Ordering::SeqCst);
        let cargo = Cargo { weight: 3 };
        assert!(controller.can_dispatch(&cargo).await);
        controller
            .dispatch(cargo.clone())
            .await
            .expect("dispatch should succeed once guard passes");
        assert_eq!(controller.state(), CargoState::Dispatching);
        assert!(ASYNC_BEFORE_CALLED.load(Ordering::SeqCst));
        assert!(ASYNC_AFTER_CALLED.load(Ordering::SeqCst));
    });
}

#[test]
fn metadata_reflects_payload_types() {
    let thruster = ThrusterController::definition();
    let fire = &thruster.events[0];
    assert_eq!(fire.name, "fire");
    assert_eq!(fire.payload, Some("BurnRequest"));

    let cargo = CargoBayController::definition();
    let dispatch = &cargo.events[0];
    assert_eq!(dispatch.name, "dispatch");
    assert_eq!(dispatch.payload, Some("Cargo"));
}
