#![cfg(feature = "async")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};

use pollster::block_on;
use state_machines::state_machine;

static BRIDGE_AUTHORIZED: AtomicBool = AtomicBool::new(false);
static CORE_STABLE: AtomicBool = AtomicBool::new(false);
static BEFORE_ENGAGE_CALLED: AtomicBool = AtomicBool::new(false);
static AFTER_ENGAGE_CALLED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: HyperdriveController,
    dynamic: true,
    initial: Offline,
    async: true,
    states: [Offline, superstate Active {
        state Charging,
        state Spooling,
        state Online,
    }, Failsafe],
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
        recover {
            transition: { from: Failsafe, to: Active, history: deep }
        }
    }
}

impl<C, S> HyperdriveController<C, S> {
    fn authorize_bridge(value: bool) {
        BRIDGE_AUTHORIZED.store(value, Ordering::SeqCst);
    }

    fn stabilize_core(value: bool) {
        CORE_STABLE.store(value, Ordering::SeqCst);
    }

    async fn bridge_authorized(&self, _ctx: &C) -> bool {
        BRIDGE_AUTHORIZED.load(Ordering::SeqCst)
    }

    async fn core_stable(&self, _ctx: &C) -> bool {
        CORE_STABLE.load(Ordering::SeqCst)
    }

    async fn log_pre_engage(&self) {
        BEFORE_ENGAGE_CALLED.store(true, Ordering::SeqCst);
    }

    async fn log_post_engage(&self) {
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
        let controller = HyperdriveController::new(());
        // Type is HyperdriveController<(), Offline>

        let controller = controller
            .begin_charge()
            .await
            .expect("charging should start from Offline");
        let controller = controller
            .energize()
            .await
            .expect("energize should move to Spooling");
        // Type is HyperdriveController<(), Spooling>

        let auth_error = controller
            .engage()
            .await
            .expect_err("bridge authorization must be granted");
        let (controller, guard_err) = auth_error;
        assert_eq!(guard_err.guard, "bridge_authorized");
        assert_eq!(guard_err.event, "engage");

        HyperdriveController::<(), Spooling>::authorize_bridge(true);
        let core_error = controller
            .engage()
            .await
            .expect_err("core must be stable before engaging");
        let (controller, guard_err) = core_error;
        assert_eq!(guard_err.guard, "core_stable");
        assert_eq!(guard_err.event, "engage");

        HyperdriveController::<(), Spooling>::stabilize_core(true);
        let controller = controller
            .engage()
            .await
            .expect("engage should succeed once guards pass");
        // Type is HyperdriveController<(), Online>
        assert!(BEFORE_ENGAGE_CALLED.load(Ordering::SeqCst));
        assert!(AFTER_ENGAGE_CALLED.load(Ordering::SeqCst));
        let controller = controller.trigger_failsafe().await.unwrap();
        let HyperdriveControllerFailsafeRecoverOutcome::Online(controller) =
            controller.recover().await.unwrap()
        else {
            panic!("async history must resume Online");
        };
        let mut controller = controller.into_dynamic();
        controller
            .handle(HyperdriveControllerEvent::TriggerFailsafe)
            .await
            .unwrap();
        controller
            .handle(HyperdriveControllerEvent::Recover)
            .await
            .unwrap();
        assert_eq!(
            controller.current_state(),
            HyperdriveControllerState::Online
        );
    });
}
