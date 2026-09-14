#![cfg(feature = "inspect")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;

state_machine! {
    name: Probe,
    initial: Idle,
    states: [Idle, Scanning],
    events {
        scan {
            guards: [power_ok],
            unless: [storm_active],
            before: [spin_up],
            after: [log_scan],
            around: [wrap_scan],
            transition: { from: Idle, to: Scanning }
        }
        stop {
            transition: { from: Scanning, to: Idle }
        }
    }
}

impl<C, S> Probe<C, S> {
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
