use state_machines::{TransitionErrorKind, state_machine};

#[derive(Default, Debug)]
struct PrepData(u8);

#[derive(Default, Debug)]
struct LaunchData(bool);

state_machine! {
    name: LaunchSequence,
    state: LaunchState,
    initial: Standby,
    states: [
        Standby,
        superstate Flight {
            state LaunchPrep(PrepData),
            state Launching(LaunchData),
        },
        InOrbit,
    ],
    events {
        enter_flight {
            transition: { from: Standby, to: Flight }
        }
        ignite {
            transition: { from: Standby, to: LaunchPrep }
        }
        cycle_engines {
            transition: { from: LaunchPrep, to: Launching }
        }
        ascend {
            transition: { from: Flight, to: InOrbit }
        }
        abort {
            transition: { from: Flight, to: Standby }
        }
    }
}

#[test]
fn superstate_sources_expand() {
    let mut sequence = LaunchSequence::new();
    assert_eq!(sequence.state(), LaunchState::Standby);

    // Ascend is not available until we enter the Flight superstate.
    let err = sequence
        .ascend()
        .expect_err("cannot ascend before ignition");
    assert_eq!(err.kind, TransitionErrorKind::InvalidTransition);

    sequence
        .enter_flight()
        .expect("entering Flight resolves to initial child");
    assert_eq!(sequence.state(), LaunchState::LaunchPrep);
    assert!(sequence.launch_prep_data().is_some());
    assert!(sequence.launching_data().is_none());
    assert!(sequence.can_abort());
    assert!(sequence.can_ascend());

    sequence
        .cycle_engines()
        .expect("cycle engines advances to Launching");
    assert_eq!(sequence.state(), LaunchState::Launching);
    assert!(sequence.launching_data().is_some());
    assert!(sequence.launch_prep_data().is_none());
    assert!(sequence.can_abort());
    assert!(sequence.can_ascend());

    sequence
        .ascend()
        .expect("Flight superstate expands to allow ascend");
    assert_eq!(sequence.state(), LaunchState::InOrbit);
    assert!(sequence.launching_data().is_none());
}

#[test]
fn metadata_lists_superstates() {
    let definition = LaunchSequence::definition();
    assert_eq!(definition.superstates.len(), 1);
    let flight = &definition.superstates[0];
    assert_eq!(flight.name, "Flight");
    assert_eq!(
        flight.descendants,
        [LaunchState::LaunchPrep, LaunchState::Launching]
    );
    assert_eq!(flight.initial, LaunchState::LaunchPrep);
}
