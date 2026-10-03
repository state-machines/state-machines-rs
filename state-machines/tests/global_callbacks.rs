#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::Mutex;

use state_machines::core::{AroundOutcome, AroundStage};
use state_machines::state_machine;

static LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn log(entry: &'static str) {
    LOG.lock().unwrap().push(entry);
}

fn take_log() -> Vec<&'static str> {
    std::mem::take(&mut *LOG.lock().unwrap())
}

#[derive(Debug, Clone, PartialEq)]
pub struct BurnPlan {
    delta_v: u32,
}

state_machine! {
    name: MissionControl,
    initial: Standby,
    states: [
        Standby,
        superstate Flight {
            superstate Powered {
                state Ascent,
            },
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
        tick {
            transition: { from: Ascent, internal: true }
        }
        restart {
            transition: { from: Ascent, to: Ascent }
        }
    },
    lifecycle: {
        Flight { enter: [enter_flight], exit: [exit_flight] },
        Powered { enter: [enter_powered], exit: [exit_powered] },
        Ascent { enter: [enter_ascent], exit: [exit_ascent] },
        Coast { enter: [enter_coast] },
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
    fn enter_flight(&self) {
        log("enter_flight");
    }
    fn exit_flight(&self) {
        log("exit_flight");
    }
    fn enter_powered(&self) {
        log("enter_powered");
    }
    fn exit_powered(&self) {
        log("exit_powered");
    }
    fn enter_ascent(&self) {
        log("enter_ascent");
    }
    fn exit_ascent(&self) {
        log("exit_ascent");
    }
    fn enter_coast(&self) {
        log("enter_coast");
    }

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
    let _guard = TEST_LOCK.lock().unwrap();
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
            "enter_flight",
            "enter_powered",
            "enter_ascent",
            "wrap_launch:after",
        ]
    );

    // burn: unfiltered before, payload goes to the local callback only,
    // and to:Coast fires after the transition
    let mission = mission.burn(BurnPlan { delta_v: 42 }).expect("burn");
    assert_eq!(
        take_log(),
        vec![
            "audit",
            "record_burn",
            "exit_ascent",
            "exit_powered",
            "enter_coast",
            "entered_coast"
        ]
    );

    // abort from Coast: from:Flight expands to substates, on:abort matches
    let _mission = mission.abort().expect("abort");
    assert_eq!(take_log(), vec!["audit", "exit_flight", "left_flight"]);
}

#[test]
fn superstate_filter_matches_all_substates() {
    let _guard = TEST_LOCK.lock().unwrap();
    take_log();

    // Abort from Ascent (the other Flight substate) also triggers left_flight
    let mission = MissionControl::new(());
    let mission = mission.launch().expect("launch");
    take_log();

    let mission = mission.tick().unwrap();
    assert_eq!(take_log(), ["audit"]);
    let mission = mission.restart().unwrap();
    assert_eq!(take_log(), ["audit", "exit_ascent", "enter_ascent"]);

    let _mission = mission.abort().expect("abort from Ascent");
    assert_eq!(
        take_log(),
        vec![
            "audit",
            "exit_ascent",
            "exit_powered",
            "exit_flight",
            "left_flight"
        ]
    );
}
