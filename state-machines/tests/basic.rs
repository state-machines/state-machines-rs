use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::{TransitionErrorKind, state_machine};

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

impl FlightDeckController {
    fn flight_plan_verified(&self) -> bool {
        FLIGHT_PLAN_VERIFIED.load(Ordering::SeqCst)
    }

    fn toggle_flight_plan(value: bool) {
        FLIGHT_PLAN_VERIFIED.store(value, Ordering::SeqCst);
    }
}

#[test]
fn launch_sequence_obeys_guards() {
    FlightDeckController::toggle_flight_plan(false);

    let mut controller = FlightDeckController::new();
    assert_eq!(controller.state(), FlightStatus::Docked);
    assert!(controller.can_request_clearance());
    assert!(!controller.can_launch());

    controller
        .request_clearance()
        .expect("clearance request should succeed from Docked");
    assert_eq!(controller.state(), FlightStatus::ClearanceGranted);

    let error = controller
        .launch()
        .expect_err("launch should fail without a verified flight plan");
    assert_eq!(
        error.kind,
        TransitionErrorKind::GuardFailed {
            guard: "flight_plan_verified"
        }
    );
    assert_eq!(error.from, FlightStatus::ClearanceGranted);

    FlightDeckController::toggle_flight_plan(true);
    assert!(controller.can_launch());
    controller
        .launch()
        .expect("launch should succeed once plan verified");
    assert_eq!(controller.state(), FlightStatus::Launching);

    controller
        .stabilize()
        .expect("stabilize should move to InFlight");
    assert_eq!(controller.state(), FlightStatus::InFlight);

    let invalid = controller
        .request_clearance()
        .expect_err("cannot request clearance mid-flight");
    assert_eq!(invalid.kind, TransitionErrorKind::InvalidTransition);
    assert_eq!(invalid.from, FlightStatus::InFlight);
}

#[test]
fn metadata_reflects_flight_deck_machine() {
    let definition = FlightDeckController::definition();
    assert_eq!(definition.name, "FlightDeckController");
    assert_eq!(definition.initial, FlightStatus::Docked);
    assert!(!definition.async_mode);
    assert_eq!(
        definition.states,
        &[
            FlightStatus::Docked,
            FlightStatus::ClearanceGranted,
            FlightStatus::Launching,
            FlightStatus::InFlight,
            FlightStatus::Emergency,
        ]
    );
    assert_eq!(definition.events.len(), 4);

    let request_clearance = &definition.events[0];
    assert_eq!(request_clearance.name, "request_clearance");
    assert!(request_clearance.guards.is_empty());
    assert!(request_clearance.payload.is_none());
    assert_eq!(request_clearance.transitions.len(), 1);
    let request_transition = &request_clearance.transitions[0];
    assert_eq!(request_transition.sources, &[FlightStatus::Docked]);
    assert_eq!(request_transition.target, FlightStatus::ClearanceGranted);
    assert!(request_transition.guards.is_empty());
    assert!(request_transition.unless.is_empty());

    let launch = &definition.events[1];
    assert_eq!(launch.name, "launch");
    assert!(launch.guards.is_empty());
    assert!(launch.payload.is_none());
    let launch_transition = &launch.transitions[0];
    assert_eq!(launch_transition.sources, &[FlightStatus::ClearanceGranted]);
    assert_eq!(launch_transition.target, FlightStatus::Launching);
    assert_eq!(launch_transition.guards, &["flight_plan_verified"]);
    assert!(launch_transition.unless.is_empty());

    let abort = &definition.events[3];
    assert_eq!(abort.name, "abort_mission");
    assert_eq!(abort.transitions.len(), 1);
    assert!(abort.payload.is_none());
    let abort_transition = &abort.transitions[0];
    assert_eq!(
        abort_transition.sources,
        &[
            FlightStatus::Docked,
            FlightStatus::ClearanceGranted,
            FlightStatus::Launching,
            FlightStatus::InFlight,
        ]
    );
    assert_eq!(abort_transition.target, FlightStatus::Emergency);
    assert!(abort_transition.guards.is_empty());
    assert!(abort_transition.unless.is_empty());
}
