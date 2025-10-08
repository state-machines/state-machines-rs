use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::{TransitionErrorKind, state_machine};

static ACTION_OK: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: DockController,
    state: DockState,
    initial: Standby,
    action: commit,
    states: [Standby, Armed],
    events {
        arm {
            transition: { from: Standby, to: Armed }
        }
    }
}

impl DockController {
    fn set_action_ok(value: bool) {
        ACTION_OK.store(value, Ordering::SeqCst);
    }
    fn commit(&mut self) -> bool {
        ACTION_OK.load(Ordering::SeqCst)
    }
}

#[test]
fn action_failure_reverts_state_sync() {
    DockController::set_action_ok(false);
    let mut c = DockController::new();
    assert_eq!(c.state(), DockState::Standby);
    let err = c.arm().expect_err("action should fail and revert");
    assert_eq!(
        err.kind,
        TransitionErrorKind::ActionFailed { action: "commit" }
    );
    assert_eq!(c.state(), DockState::Standby);

    DockController::set_action_ok(true);
    c.arm().expect("action success should keep new state");
    assert_eq!(c.state(), DockState::Armed);
}
