//! Host side: links `state-machines` with `std` (and so `inspect`), while the
//! binary links it with no features. The proc-macro is built once for the
//! host with features unified across both, so this crate pins that `schema()`
//! follows the facade each side links rather than the macro's own build.
//!
//! This crate declares no `inspect` feature of its own and must still get
//! `schema()`.

use state_machines::state_machine;

state_machine! {
    name: Probe,
    dynamic: true,
    snapshot: true,
    initial: Idle,
    states: [Idle, Scanning],
    events {
        scan {
            transition: { from: Idle, to: Scanning }
        }
    }
}

fn main() {
    let schema = Probe::<(), Idle>::schema();
    assert_eq!(schema.name, "Probe");
    assert_eq!(schema.states, ["Idle", "Scanning"]);
    let snapshot = Probe::new(()).into_dynamic().into_snapshot();
    assert_eq!(snapshot.state, "Idle");
    // The runtime adapter follows the host facade while vanishing on the target.
    fn assert_runtime<M: state_machines::runtime::Machine>(_: &M) {}
    assert_runtime(&DynamicProbe::new(()));
}
