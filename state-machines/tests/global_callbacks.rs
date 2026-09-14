#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::Mutex;

use state_machines::core::{AroundOutcome, AroundStage};
use state_machines::state_machine;

static LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn log(entry: &'static str) {
    LOG.lock().unwrap().push(entry);
}

fn take_log() -> Vec<&'static str> {
    std::mem::take(&mut *LOG.lock().unwrap())
}

#[derive(Debug, Clone, PartialEq)]
struct BurnPlan {
    delta_v: u32,
}

state_machine! {
    name: MissionControl,
    initial: Standby,
    states: [
        Standby,
        superstate Flight {
            state Ascent,
            state Coast,
        },
        Aborted,
    ],
    events {
        launch {
            transition: { from: Standby, to: Ascent }
        }
        burn {
            payload: BurnPlan,
            before: [record_burn]
            transition: { from: Ascent, to: Coast }
        }
        abort {
            transition: { from: Flight, to: Aborted }
        }
    },
    callbacks: {
        before_transition [
            { name: audit_every_transition },
            { name: leaving_standby, from: Standby },
        ],
        after_transition [
            { name: entered_coast, to: Coast },
            { name: left_flight, from: Flight, on: abort },
        ],
        around_transition [
            { name: wrap_launch, on: [launch] }
        ]
    }
}

impl<C, S> MissionControl<C, S> {
    fn audit_every_transition(&self) {
        log("audit");
    }

    fn leaving_standby(&self) {
        log("leaving_standby");
    }

    fn entered_coast(&self) {
        log("entered_coast");
    }

    fn left_flight(&self) {
        log("left_flight");
    }

    fn record_burn(&self, payload: &BurnPlan) {
        assert_eq!(payload.delta_v, 42);
        log("record_burn");
    }

    fn wrap_launch(&self, stage: AroundStage) -> AroundOutcome<Standby> {
        match stage {
            AroundStage::Before => log("wrap_launch:before"),
            AroundStage::AfterSuccess => log("wrap_launch:after"),
        }
        AroundOutcome::Proceed
    }
}

#[test]
fn global_callbacks_fire_with_filters() {
    take_log();

    let mission = MissionControl::new(());

    // launch: unfiltered before + from:Standby + around wrapper
    let mission = mission.launch().expect("launch");
    assert_eq!(
        take_log(),
        vec![
            "wrap_launch:before",
            "audit",
            "leaving_standby",
            "wrap_launch:after",
        ]
    );

    // burn: unfiltered before, payload goes to the local callback only,
    // and to:Coast fires after the transition
    let mission = mission.burn(BurnPlan { delta_v: 42 }).expect("burn");
    assert_eq!(take_log(), vec!["audit", "record_burn", "entered_coast"]);

    // abort from Coast: from:Flight expands to substates, on:abort matches
    let _mission = mission.abort().expect("abort");
    assert_eq!(take_log(), vec!["audit", "left_flight"]);
}

#[test]
fn superstate_filter_matches_all_substates() {
    take_log();

    // Abort from Ascent (the other Flight substate) also triggers left_flight
    let mission = MissionControl::new(());
    let mission = mission.launch().expect("launch");
    take_log();

    let _mission = mission.abort().expect("abort from Ascent");
    assert_eq!(take_log(), vec!["audit", "left_flight"]);
}
