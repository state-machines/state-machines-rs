#![cfg(feature = "async")]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use pollster::block_on;
use state_machines::{DynamicError, EventError, state_machine};

static BEFORE_FAILS: AtomicBool = AtomicBool::new(false);
static AFTER_FAILS: AtomicBool = AtomicBool::new(false);
static BEFORE_CALLED: AtomicBool = AtomicBool::new(false);
static AFTER_CALLED: AtomicBool = AtomicBool::new(false);
static COMPLETED: AtomicBool = AtomicBool::new(false);
static CHOOSE_DONE: AtomicBool = AtomicBool::new(true);
static TEST_LOCK: Mutex<()> = Mutex::new(());
static ERRORS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Network,
    Audit,
}

state_machine! {
    name: AuthRecovery,
    dynamic: true,
    async: true,
    error: AuthError,
    initial: RefreshToken,
    states: [RefreshToken, Done],
    final_states: [Done],
    events {
        refresh {
            branching: true,
            before: [refresh_token],
            on_error: [recover],
            transition: { from: RefreshToken, to: Done, guards: [choose_done] }
            transition: { from: RefreshToken, internal: true, fallback: true }
        }
    },
    lifecycle: { Done { enter: [record_completion], complete: [notify] } }
}

impl<C, S> AuthRecovery<C, S> {
    async fn choose_done(&self, _ctx: &C) -> bool {
        CHOOSE_DONE.load(Ordering::SeqCst)
    }
    async fn notify(&self) {
        COMPLETED.store(true, Ordering::SeqCst);
    }
    async fn recover(&self, error: &EventError<AuthError>) {
        let EventError::Callback(error) = error else {
            panic!("expected callback failure");
        };
        ERRORS.lock().unwrap().push(error.action);
    }

    async fn refresh_token(&self) -> Result<(), AuthError> {
        BEFORE_CALLED.store(true, Ordering::SeqCst);
        if BEFORE_FAILS.load(Ordering::SeqCst) {
            Err(AuthError::Network)
        } else {
            Ok(())
        }
    }

    async fn record_completion(&self) -> Result<(), AuthError> {
        AFTER_CALLED.store(true, Ordering::SeqCst);
        if AFTER_FAILS.load(Ordering::SeqCst) {
            Err(AuthError::Audit)
        } else {
            Ok(())
        }
    }
}

fn reset_flags() {
    CHOOSE_DONE.store(true, Ordering::SeqCst);
    COMPLETED.store(false, Ordering::SeqCst);
    ERRORS.lock().unwrap().clear();
    BEFORE_FAILS.store(false, Ordering::SeqCst);
    AFTER_FAILS.store(false, Ordering::SeqCst);
    BEFORE_CALLED.store(false, Ordering::SeqCst);
    AFTER_CALLED.store(false, Ordering::SeqCst);
}

#[test]
fn async_before_callback_failure_returns_source_machine() {
    let _guard = TEST_LOCK.lock().unwrap();
    reset_flags();
    BEFORE_FAILS.store(true, Ordering::SeqCst);

    block_on(async {
        let machine = AuthRecovery::new(());
        let err = machine
            .refresh()
            .await
            .expect_err("before callback should fail");

        let (machine, err) = err;
        match err {
            EventError::Callback(err) => {
                assert_eq!(err.action, "refresh_token");
                assert_eq!(err.event, "refresh");
                assert_eq!(err.source, AuthError::Network);
            }
            other => panic!("expected callback error, got {other:?}"),
        }

        assert!(BEFORE_CALLED.load(Ordering::SeqCst));
        assert!(!AFTER_CALLED.load(Ordering::SeqCst));
        assert!(!COMPLETED.load(Ordering::SeqCst));
        assert_eq!(*ERRORS.lock().unwrap(), ["refresh_token"]);

        BEFORE_FAILS.store(false, Ordering::SeqCst);
        let _done = machine
            .refresh()
            .await
            .expect("retry should succeed from the original state");
        assert_eq!(*ERRORS.lock().unwrap(), ["refresh_token"]);
    });
}

#[test]
fn async_after_callback_failure_rolls_back_transition() {
    let _guard = TEST_LOCK.lock().unwrap();
    reset_flags();
    AFTER_FAILS.store(true, Ordering::SeqCst);

    block_on(async {
        let machine = AuthRecovery::new(());
        let err = machine
            .refresh()
            .await
            .expect_err("after callback should fail");

        let (machine, err) = err;
        match err {
            EventError::Callback(err) => {
                assert_eq!(err.action, "record_completion");
                assert_eq!(err.event, "refresh");
                assert_eq!(err.source, AuthError::Audit);
            }
            other => panic!("expected callback error, got {other:?}"),
        }

        assert!(BEFORE_CALLED.load(Ordering::SeqCst));
        assert!(AFTER_CALLED.load(Ordering::SeqCst));
        assert!(!COMPLETED.load(Ordering::SeqCst));
        assert_eq!(*ERRORS.lock().unwrap(), ["record_completion"]);

        AFTER_FAILS.store(false, Ordering::SeqCst);
        let _done = machine
            .refresh()
            .await
            .expect("machine should still be in RefreshToken after rollback");
        assert!(COMPLETED.load(Ordering::SeqCst));
    });
}

#[test]
fn async_dynamic_callback_failure_keeps_runtime_state() {
    let _guard = TEST_LOCK.lock().unwrap();
    reset_flags();
    BEFORE_FAILS.store(true, Ordering::SeqCst);

    block_on(async {
        let mut machine = DynamicAuthRecovery::new(());

        let err = machine
            .handle(AuthRecoveryEvent::Refresh)
            .await
            .expect_err("dynamic handle should surface callback errors");

        match err {
            DynamicError::CallbackFailed {
                action,
                event,
                source,
            } => {
                assert_eq!(action, "refresh_token");
                assert_eq!(event, "refresh");
                assert_eq!(source, AuthError::Network);
            }
            other => panic!("expected callback failure, got {other:?}"),
        }

        assert_eq!(machine.current_state(), AuthRecoveryState::RefreshToken);
        assert_eq!(*ERRORS.lock().unwrap(), ["refresh_token"]);

        BEFORE_FAILS.store(false, Ordering::SeqCst);
        CHOOSE_DONE.store(false, Ordering::SeqCst);
        assert_eq!(
            machine.get_available_events().await.len(),
            1,
            "branches do not duplicate events"
        );
        machine.handle(AuthRecoveryEvent::Refresh).await.unwrap();
        assert_eq!(machine.current_state(), AuthRecoveryState::RefreshToken);
        assert_eq!(machine.take_completion_events(), []);
        CHOOSE_DONE.store(true, Ordering::SeqCst);
        machine
            .handle(AuthRecoveryEvent::Refresh)
            .await
            .expect("retry should succeed without advancing on the failed attempt");
        assert_eq!(machine.current_state(), AuthRecoveryState::Done);
        assert_eq!(
            machine.take_completion_events(),
            [state_machines::CompletionEvent::Machine]
        );
    });
}
