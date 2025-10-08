use std::sync::atomic::{AtomicBool, Ordering};

use pollster::block_on;
use state_machines::{TransitionErrorKind, state_machine};

static BRIDGE_AUTHORIZED: AtomicBool = AtomicBool::new(false);
static CORE_STABLE: AtomicBool = AtomicBool::new(false);
static BEFORE_ENGAGE_CALLED: AtomicBool = AtomicBool::new(false);
static AFTER_ENGAGE_CALLED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: HyperdriveController,
    state: HyperdriveStatus,
    initial: Offline,
    async: true,
    states: [Offline, Charging, Spooling, Online, Failsafe],
    events {
        begin_charge {
            transition: { from: Offline, to: Charging }
        }
        energize {
            transition: { from: Charging, to: Spooling }
        }
        engage {
            guards: [bridge_authorized]
            before: [log_pre_engage]
            after: [log_post_engage]
            transition: { from: Spooling, to: Online, guards: [core_stable] }
        }
        trigger_failsafe {
            transition: { from: [Offline, Charging, Spooling, Online], to: Failsafe }
        }
    }
}

impl HyperdriveController {
    fn authorize_bridge(value: bool) {
        BRIDGE_AUTHORIZED.store(value, Ordering::SeqCst);
    }

    fn stabilize_core(value: bool) {
        CORE_STABLE.store(value, Ordering::SeqCst);
    }

    async fn bridge_authorized(&self) -> bool {
        BRIDGE_AUTHORIZED.load(Ordering::SeqCst)
    }

    async fn core_stable(&self) -> bool {
        CORE_STABLE.load(Ordering::SeqCst)
    }

    async fn log_pre_engage(&mut self) {
        BEFORE_ENGAGE_CALLED.store(true, Ordering::SeqCst);
    }

    async fn log_post_engage(&mut self) {
        AFTER_ENGAGE_CALLED.store(true, Ordering::SeqCst);
    }
}

#[test]
fn async_engage_sequence_requires_authorization_and_stability() {
    BRIDGE_AUTHORIZED.store(false, Ordering::SeqCst);
    CORE_STABLE.store(false, Ordering::SeqCst);
    BEFORE_ENGAGE_CALLED.store(false, Ordering::SeqCst);
    AFTER_ENGAGE_CALLED.store(false, Ordering::SeqCst);

    block_on(async {
        let mut controller = HyperdriveController::new();
        assert!(HyperdriveController::ASYNC_MODE);
        assert_eq!(controller.state(), HyperdriveStatus::Offline);

        controller
            .begin_charge()
            .await
            .expect("charging should start from Offline");
        controller
            .energize()
            .await
            .expect("energize should move to Spooling");
        assert_eq!(controller.state(), HyperdriveStatus::Spooling);

        let auth_error = controller
            .engage()
            .await
            .expect_err("bridge authorization must be granted");
        assert_eq!(
            auth_error.kind,
            TransitionErrorKind::GuardFailed {
                guard: "bridge_authorized"
            }
        );

        HyperdriveController::authorize_bridge(true);
        let core_error = controller
            .engage()
            .await
            .expect_err("core must be stable before engaging");
        assert_eq!(
            core_error.kind,
            TransitionErrorKind::GuardFailed {
                guard: "core_stable"
            }
        );

        HyperdriveController::stabilize_core(true);
        assert!(controller.can_engage().await);
        controller
            .engage()
            .await
            .expect("engage should succeed once guards pass");
        assert_eq!(controller.state(), HyperdriveStatus::Online);
        assert!(BEFORE_ENGAGE_CALLED.load(Ordering::SeqCst));
        assert!(AFTER_ENGAGE_CALLED.load(Ordering::SeqCst));
    });
}

#[test]
fn metadata_reflects_hyperdrive_machine() {
    let definition = HyperdriveController::definition();
    assert_eq!(definition.name, "HyperdriveController");
    assert_eq!(definition.initial, HyperdriveStatus::Offline);
    assert!(definition.async_mode);
    assert_eq!(definition.events.len(), 4);

    let engage = &definition.events[2];
    assert_eq!(engage.name, "engage");
    assert_eq!(engage.guards, &["bridge_authorized"]);
    assert_eq!(engage.before, &["log_pre_engage"]);
    assert_eq!(engage.after, &["log_post_engage"]);
    assert_eq!(engage.transitions.len(), 1);
    let engage_transition = &engage.transitions[0];
    assert_eq!(engage_transition.sources, &[HyperdriveStatus::Spooling]);
    assert_eq!(engage_transition.target, HyperdriveStatus::Online);
    assert_eq!(engage_transition.guards, &["core_stable"]);
    assert!(engage_transition.unless.is_empty());
}
