#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;

#[derive(Default, Debug, Clone, PartialEq)]
struct MissionLog {
    entries: u32,
}

#[derive(Default, Debug, Clone, PartialEq)]
struct ExecData {
    step: u8,
}

state_machine! {
    name: MissionRunner,
    initial: Idle,
    states: [
        Idle,
        superstate Mission(MissionLog) {
            state Planning,
            state Executing(ExecData),
        },
        Done,
    ],
    events {
        begin {
            transition: { from: Idle, to: Mission }
        }
        execute {
            transition: { from: Planning, to: Executing }
        }
        replan {
            transition: { from: Executing, to: Planning }
        }
        finish {
            transition: { from: Mission, to: Done }
        }
    }
}

#[test]
fn superstate_data_lifecycle() {
    // Outside the superstate: no mission data
    let runner = MissionRunner::new(());
    assert!(runner.state_data_mission().is_none());

    // Entering the superstate initialises its data
    let mut runner = runner.begin().expect("begin");
    assert_eq!(runner.mission_data().entries, 0);

    // Mutations survive intra-superstate transitions in both directions
    runner.mission_data_mut().entries = 3;
    let runner = runner.execute().expect("execute");
    assert_eq!(runner.mission_data().entries, 3);
    // Leaf data of the target state is freshly initialised alongside
    assert_eq!(runner.executing_data().step, 0);

    let mut runner = runner.replan().expect("replan");
    assert_eq!(runner.mission_data().entries, 3);
    runner.mission_data_mut().entries += 1;

    // Leaving the superstate clears its data
    let runner = runner.finish().expect("finish");
    assert!(runner.state_data_mission().is_none());
}

mod initial_inside_superstate {
    use super::MissionLog;
    use state_machines::state_machine;

    state_machine! {
        name: PreloadedRunner,
        initial: Briefing,
        states: [
            superstate Mission(MissionLog) {
                state Briefing,
                state Flying,
            },
            Landed,
        ],
        events {
            take_off {
                transition: { from: Briefing, to: Flying }
            }
            land {
                transition: { from: Mission, to: Landed }
            }
        }
    }

    #[test]
    fn initial_state_starts_without_data_like_leaf_states() {
        // Constructors never require Default: storage starts as None even
        // when the initial state sits inside a data-carrying superstate.
        let runner = PreloadedRunner::new(());
        assert!(runner.state_data_mission().is_none());

        // An intra-superstate transition from the initial state keeps the
        // (still absent) data absent rather than conjuring a default.
        let runner = runner.take_off().expect("take off");
        assert!(runner.state_data_mission().is_none());

        let runner = runner.land().expect("land");
        assert!(runner.state_data_mission().is_none());
    }
}

mod rollback_preserves_superstate_data {
    use super::MissionLog;
    use state_machines::state_machine;
    use std::sync::atomic::{AtomicBool, Ordering};

    static FAIL_AFTER: AtomicBool = AtomicBool::new(false);

    state_machine! {
        name: FallibleRunner,
        initial: Idle,
        error: String,
        states: [
            Idle,
            superstate Mission(MissionLog) {
                state Planning,
                state Executing,
            },
        ],
        events {
            begin {
                transition: { from: Idle, to: Mission }
            }
            execute {
                after: [confirm_execute],
                transition: { from: Planning, to: Executing }
            }
        }
    }

    impl<C, S> FallibleRunner<C, S> {
        fn confirm_execute(&self) -> Result<(), String> {
            if FAIL_AFTER.load(Ordering::SeqCst) {
                Err(String::from("telemetry offline"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn failed_after_callback_rolls_back_without_losing_superstate_data() {
        FAIL_AFTER.store(true, Ordering::SeqCst);

        let runner = FallibleRunner::new(());
        let mut runner = runner.begin().expect("begin");
        runner.mission_data_mut().entries = 7;

        // The after callback fails on an intra-superstate transition; the
        // machine rolls back to Planning and must keep the mission data.
        let (runner, _err) = runner.execute().expect_err("after callback fails");
        assert_eq!(runner.mission_data().entries, 7);

        FAIL_AFTER.store(false, Ordering::SeqCst);
        let runner = runner.execute().expect("execute succeeds now");
        assert_eq!(runner.mission_data().entries, 7);
    }
}
