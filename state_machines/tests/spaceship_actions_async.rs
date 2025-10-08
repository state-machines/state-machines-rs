use std::sync::atomic::{AtomicBool, Ordering};

use pollster::block_on;
use state_machines::{TransitionErrorKind, state_machine};

static ACTION_OK: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: WarpCoreController,
    state: WarpState,
    initial: Idle,
    async: true,
    action: commit,
    states: [Idle, Primed],
    events {
        prime {
            transition: { from: Idle, to: Primed }
        }
    }
}

impl WarpCoreController {
    fn set_action_ok(value: bool) {
        ACTION_OK.store(value, Ordering::SeqCst);
    }
    async fn commit(&mut self) -> bool {
        ACTION_OK.load(Ordering::SeqCst)
    }
}

#[test]
fn action_failure_reverts_state_async() {
    WarpCoreController::set_action_ok(false);
    block_on(async {
        let mut c = WarpCoreController::new();
        let err = c.prime().await.expect_err("action should fail and revert");
        assert_eq!(
            err.kind,
            TransitionErrorKind::ActionFailed { action: "commit" }
        );
        assert_eq!(c.state(), WarpState::Idle);

        WarpCoreController::set_action_ok(true);
        c.prime()
            .await
            .expect("action success should keep new state");
        assert_eq!(c.state(), WarpState::Primed);
    });
}
