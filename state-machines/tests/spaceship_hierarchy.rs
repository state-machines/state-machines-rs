#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;

#[derive(Default, Debug, Clone)]
struct PrepData(#[allow(dead_code)] u8);

#[derive(Default, Debug, Clone)]
struct LaunchData(#[allow(dead_code)] bool);

state_machine! {
    name: LaunchSequence,
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
    // Start in Standby state
    let sequence = LaunchSequence::new(());

    // Entering Flight superstate resolves to initial child LaunchPrep
    let sequence = sequence
        .enter_flight()
        .expect("entering Flight resolves to initial child");

    // LaunchPrep storage should be automatically initialized
    assert!(sequence.state_data_launch_prep().is_some());
    // Launching state data should be None (not in that state)
    assert!(sequence.state_data_launching().is_none());

    // Transition to Launching within Flight superstate
    let sequence = sequence
        .cycle_engines()
        .expect("cycle engines advances to Launching");

    // Now in Launching state - its data is initialized
    assert!(sequence.state_data_launching().is_some());
    // LaunchPrep data is cleared (not in that state anymore)
    assert!(sequence.state_data_launch_prep().is_none());

    // Abort from Flight superstate (works from any child state)
    let sequence = sequence.abort().expect("abort from Flight works");

    // Back in Standby
    let _sequence = sequence.ignite().expect("can ignite from Standby");
}

#[test]
fn substate_of_trait_is_implemented() {
    use state_machines::SubstateOf;

    // Verify that substates implement SubstateOf<Superstate>
    fn assert_substate<T: SubstateOf<Flight>>() {}

    // These should compile because LaunchPrep and Launching are substates of Flight
    assert_substate::<LaunchPrep>();
    assert_substate::<Launching>();

    // Standby and InOrbit are NOT substates of Flight, so these would fail to compile:
    // assert_substate::<Standby>();    // ERROR: Standby doesn't impl SubstateOf<Flight>
    // assert_substate::<InOrbit>();    // ERROR: InOrbit doesn't impl SubstateOf<Flight>
}

#[test]
fn superstate_transitions_work_from_any_substate() {
    // Start in Standby
    let sequence = LaunchSequence::new(());

    // Go directly to LaunchPrep (bypassing Flight superstate entry)
    let sequence = sequence.ignite().expect("ignite works");
    // Type: LaunchSequence<C, LaunchPrep>

    // abort() is defined on Flight superstate, but works from LaunchPrep
    let sequence = sequence.abort().expect("abort from LaunchPrep");
    // Type: LaunchSequence<C, Standby>

    // Go back to Flight via enter_flight (resolves to LaunchPrep)
    let sequence = sequence.enter_flight().expect("enter flight");

    // Move to Launching within Flight
    let sequence = sequence.cycle_engines().expect("cycle engines");
    // Type: LaunchSequence<C, Launching>

    // abort() ALSO works from Launching (different substate, same superstate)
    let _sequence = sequence.abort().expect("abort from Launching");
    // Type: LaunchSequence<C, Standby>

    // This demonstrates polymorphism: abort() works from ANY substate of Flight
}
