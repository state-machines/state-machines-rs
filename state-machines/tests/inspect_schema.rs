#![cfg(feature = "inspect")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;

state_machine! {
    name: Probe,
    initial: Idle,
    states: [Idle, superstate Active { state Scanning, state Calibrating }, Complete],
    final_states: [Complete],
    lifecycle: { Active { enter: [spin_up], exit: [log_scan], complete: [log_scan] } },
    events {
        scan {
            guards: [power_ok],
            unless: [storm_active],
            before: [spin_up],
            after: [log_scan],
            around: [wrap_scan],
            on_error: [report_error],
            transition: { from: Idle, to: Scanning }
        }
        stop {
            transition: { from: Scanning, to: Idle }
        }
        advance {
            transition: { from: Scanning, to: Calibrating }
        }
        tick {
            transition: { from: Active, internal: true }
        }
        finish {
            transition: { from: Active, to: Complete }
        }
        resume {
            transition: { from: Idle, to: Active, history: deep }
        }
        choose {
            branching: true,
            transition: { from: Idle, to: Scanning, guards: [power_ok] }
            transition: { from: Idle, to: Complete, fallback: true }
        }
    }
}

impl<C, S> Probe<C, S> {
    fn report_error(&self, _error: &state_machines::core::GuardError) {}
    fn power_ok(&self, _ctx: &C) -> bool {
        true
    }

    fn storm_active(&self, _ctx: &C) -> bool {
        false
    }

    fn spin_up(&self) {}

    fn log_scan(&self) {}

    fn wrap_scan(
        &self,
        _stage: state_machines::AroundStage,
    ) -> state_machines::AroundOutcome<Idle> {
        state_machines::AroundOutcome::Proceed
    }
}

#[test]
fn schema_exposes_guards_unless_and_callbacks() {
    let schema = Probe::<(), Idle>::schema();

    assert_eq!(schema.name, "Probe");
    assert_eq!(schema.initial, "Idle");

    let scan = schema.events.iter().find(|e| e.name == "scan").unwrap();
    assert_eq!(scan.guards, vec!["power_ok"]);
    assert_eq!(scan.unless, vec!["storm_active"]);
    assert_eq!(scan.before, vec!["spin_up"]);
    assert_eq!(scan.after, vec!["log_scan"]);
    assert_eq!(scan.around, vec!["wrap_scan"]);
    assert_eq!(scan.on_error, ["report_error"]);
    assert_eq!(schema.final_states, ["Complete"]);
    assert_eq!(schema.lifecycle[0].state, "Active");
    assert_eq!(schema.lifecycle[0].enter, ["spin_up"]);
    assert_eq!(schema.lifecycle[0].exit, ["log_scan"]);
    assert_eq!(schema.lifecycle[0].complete, ["log_scan"]);
    let resume = schema.events.iter().find(|e| e.name == "resume").unwrap();
    assert_eq!(resume.transitions[0].history.as_deref(), Some("deep"));
    let tick = schema.events.iter().find(|e| e.name == "tick").unwrap();
    assert!(tick.transitions[0].internal);
    let choose = schema.events.iter().find(|e| e.name == "choose").unwrap();
    assert!(choose.branching);
    assert!(choose.transitions[1].fallback);
    assert_eq!(schema.validate(), []);
}

#[test]
fn inspectable_trait_is_implemented() {
    use state_machines::Inspectable;

    fn schema_of<T: Inspectable>() -> state_machines::MachineSchema {
        T::schema()
    }

    let schema = schema_of::<Probe<(), Idle>>();
    assert_eq!(schema.name, "Probe");
}
