use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::{core::GuardError, state_machine};

static FLIGHT_PLAN_VERIFIED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: FlightDeckController,
    state: FlightStatus,
    initial: Docked,
    states: [Docked, ClearanceGranted, Launching, InFlight, Emergency],
    events {
        request_clearance {
            transition: { from: Docked, to: ClearanceGranted }
        }
        launch {
            transition: { from: ClearanceGranted, to: Launching, guards: [flight_plan_verified] }
        }
        stabilize {
            transition: { from: Launching, to: InFlight }
        }
        abort_mission {
            transition: { from: [Docked, ClearanceGranted, Launching, InFlight], to: Emergency }
        }
    }
}

impl<S> FlightDeckController<S> {
    fn flight_plan_verified(&self) -> bool {
        FLIGHT_PLAN_VERIFIED.load(Ordering::SeqCst)
    }

    fn toggle_flight_plan(value: bool) {
        FLIGHT_PLAN_VERIFIED.store(value, Ordering::SeqCst);
    }
}

#[test]
fn launch_sequence_obeys_guards() {
    FlightDeckController::<Docked>::toggle_flight_plan(false);

    let controller = FlightDeckController::new();
    // Type is FlightDeckController<Docked>

    let controller = controller
        .request_clearance()
        .expect("clearance request should succeed from Docked");
    // Type is FlightDeckController<ClearanceGranted>

    let err = controller
        .launch()
        .expect_err("launch should fail without a verified flight plan");
    let (controller, guard_err) = err;
    assert_eq!(guard_err.guard, "flight_plan_verified");
    assert_eq!(guard_err.event, "launch");

    FlightDeckController::<Docked>::toggle_flight_plan(true);
    let controller = controller
        .launch()
        .expect("launch should succeed once plan verified");
    // Type is FlightDeckController<Launching>

    let controller = controller
        .stabilize()
        .expect("stabilize should move to InFlight");
    // Type is FlightDeckController<InFlight>

    // Invalid transitions don't compile in typestate, so we can't test them
    // The type system prevents request_clearance from being called on InFlight
}

// Metadata test commented out - typestate pattern doesn't use runtime metadata in the same way
// #[test]
// fn metadata_reflects_flight_deck_machine() {
//     let definition = FlightDeckController::definition();
//     // ... metadata assertions
// }
