//! Target side: links `state-machines` with no features, so no `inspect`/`runtime`.
//! Generated introspection/runtime code must vanish here even though the
//! host-built macro (see `build.rs`) has seen those enabled.
//!
//! Build this package on its own (`cargo build --manifest-path tests/feature_split/Cargo.toml`); a
//! workspace-wide build unifies features and hides the split.

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
    let probe = Probe::new(());
    let _probe = probe.scan().unwrap();
}
