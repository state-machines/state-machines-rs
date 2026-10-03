#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;
use std::sync::atomic::{AtomicBool, Ordering};

static FAIL_PAUSE: AtomicBool = AtomicBool::new(false);

#[derive(Default, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct MissionLog {
    entries: u32,
}

#[derive(Default, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct ExecData {
    step: u8,
}

state_machine! {
    name: MissionRunner,
    dynamic: true,
    snapshot: true,
    error: String,
    initial: Idle,
    states: [
        Idle,
        superstate Mission(MissionLog) {
            state Planning,
            superstate Work {
                state Executing(ExecData),
                state Verifying,
            },
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
            transition: { from: [Executing, Verifying], to: Planning }
        }
        finish {
            transition: { from: Mission, to: Done }
        }
        verify {
            transition: { from: Executing, to: Verifying }
        }
        pause {
            after: [confirm_pause],
            transition: { from: Mission, to: Idle }
        }
        resume_deep {
            transition: { from: Idle, to: Mission, history: deep }
        }
        resume_shallow {
            transition: { from: Idle, to: Mission, history: shallow }
        }
    }
}

impl<C, S> MissionRunner<C, S> {
    fn confirm_pause(&self) -> Result<(), String> {
        if FAIL_PAUSE.load(Ordering::SeqCst) {
            Err("pause rejected".into())
        } else {
            Ok(())
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
    assert_eq!(runner.mission_data().unwrap().entries, 0);

    // Mutations survive intra-superstate transitions in both directions
    runner.mission_data_mut().unwrap().entries = 3;
    let runner = runner.execute().expect("execute");
    assert_eq!(runner.mission_data().unwrap().entries, 3);
    // Leaf data of the target state is freshly initialised alongside
    assert_eq!(runner.executing_data().unwrap().step, 0);

    let mut runner = runner.replan().expect("replan");
    assert_eq!(runner.mission_data().unwrap().entries, 3);
    runner.mission_data_mut().unwrap().entries += 1;

    // Leaving the superstate clears its data
    let runner = runner.finish().expect("finish");
    assert!(runner.state_data_mission().is_none());

    // Before any exit, history falls back to the initial child.
    let MissionRunnerIdleResumeDeepOutcome::Planning(runner) =
        MissionRunner::new(()).resume_deep().unwrap()
    else {
        panic!("unvisited history must use initial child");
    };
    let runner = runner.execute().unwrap().verify().unwrap().pause().unwrap();

    // Shallow history restores Work, whose initial child is Executing.
    let MissionRunnerIdleResumeShallowOutcome::Executing(mut runner) =
        runner.resume_shallow().unwrap()
    else {
        panic!("shallow history restores the direct child");
    };
    assert_eq!(
        runner.executing_data().unwrap().step,
        0,
        "history restores control state, not old data"
    );
    runner.mission_data_mut().unwrap().entries = 9;

    // Failed exits must not replace remembered Verifying with Executing.
    FAIL_PAUSE.store(true, Ordering::SeqCst);
    let (runner, _) = runner.pause().unwrap_err();
    assert_eq!(runner.mission_data().unwrap().entries, 9);
    let runner = {
        #[cfg(feature = "serde")]
        {
            let json = serde_json::to_string(&runner.into_dynamic().into_snapshot()).unwrap();
            let snapshot: MissionRunnerSnapshot<()> = serde_json::from_str(&json).unwrap();
            assert_eq!(snapshot.__sm_history_mission.as_deref(), Some("Verifying"));
            DynamicMissionRunner::from_snapshot(snapshot)
                .unwrap()
                .into_executing()
                .unwrap()
        }
        #[cfg(not(feature = "serde"))]
        {
            runner
        }
    };
    FAIL_PAUSE.store(false, Ordering::SeqCst);
    let runner = runner.verify().unwrap().pause().unwrap();

    // Persist history while the region is inactive, then resume through
    // dynamic dispatch. Invalid history is rejected without losing data.
    let mut runner = runner.into_dynamic();
    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&runner.into_snapshot()).unwrap();
        let mut snapshot: MissionRunnerSnapshot<()> = serde_json::from_str(&json).unwrap();
        snapshot.__sm_history_mission = Some("Idle".into());
        let (mut snapshot, error) = DynamicMissionRunner::from_snapshot(snapshot).unwrap_err();
        assert_eq!(
            error,
            state_machines::SnapshotError::InvalidHistory { region: "Mission" }
        );
        snapshot.__sm_history_mission = Some("Verifying".into());
        runner = DynamicMissionRunner::from_snapshot(snapshot).unwrap();
    }
    runner.handle(MissionRunnerEvent::ResumeDeep).unwrap();
    assert_eq!(runner.current_state(), MissionRunnerState::Verifying);
    assert_eq!(runner.mission_data().unwrap().entries, 0);
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
        let mut runner = PreloadedRunner::new(());
        assert!(runner.state_data_mission().is_none());
        assert!(runner.mission_data().is_none());
        assert!(runner.mission_data_mut().is_none());

        // An intra-superstate transition from the initial state keeps the
        // (still absent) data absent rather than conjuring a default.
        let runner = runner.take_off().expect("take off");
        assert!(runner.state_data_mission().is_none());
        assert!(runner.mission_data().is_none());

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
        runner.mission_data_mut().unwrap().entries = 7;

        // The after callback fails on an intra-superstate transition; the
        // machine rolls back to Planning and must keep the mission data.
        let (runner, _err) = runner.execute().expect_err("after callback fails");
        assert_eq!(runner.mission_data().unwrap().entries, 7);

        FAIL_AFTER.store(false, Ordering::SeqCst);
        let runner = runner.execute().expect("execute succeeds now");
        assert_eq!(runner.mission_data().unwrap().entries, 7);
    }
}
