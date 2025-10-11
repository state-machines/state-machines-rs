use state_machines::{
    core::{AroundOutcome, AroundStage},
    state_machine,
};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

state_machine! {
    name: Transaction,
    initial: Idle,
    states: [Idle, Processing, Complete, Failed],
    events {
        begin {
            around: [transaction_wrapper],
            transition: { from: Idle, to: Processing }
        }
        succeed {
            transition: { from: Processing, to: Complete }
        }
        fail {
            transition: { from: Processing, to: Failed }
        }
    }
}

impl<C, S> Transaction<C, S> {
    fn transaction_wrapper(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        match stage {
            AroundStage::Before => {
                CALL_COUNT.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess => {
                CALL_COUNT.fetch_add(10, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
        }
    }
}

#[test]
fn around_callback_executes_both_stages() {
    CALL_COUNT.store(0, Ordering::SeqCst);

    let transaction = Transaction::new(());
    let transaction = transaction.begin().unwrap();

    // Should have called Before (+1) and AfterSuccess (+10)
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 11);

    // Verify we're in the right state
    let _transaction = transaction.succeed().unwrap();
}

#[test]
fn around_callback_can_abort_at_before_stage() {
    state_machine! {
        name: Guarded,
        initial: Start,
        states: [Start, End],
        events {
            advance {
                around: [abort_guard],
                transition: { from: Start, to: End }
            }
        }
    }

    impl<C, S> Guarded<C, S> {
        fn abort_guard(&self, stage: AroundStage) -> AroundOutcome<Start> {
            match stage {
                AroundStage::Before => {
                    // Abort at Before stage
                    AroundOutcome::Abort(state_machines::core::TransitionError::guard_failed(
                        Start,
                        "advance",
                        "abort_guard",
                    ))
                }
                AroundStage::AfterSuccess => {
                    // Should not be called
                    panic!("AfterSuccess should not be called when Before aborts");
                }
            }
        }
    }

    let machine = Guarded::new(());
    let result = machine.advance();

    // Should have failed
    assert!(result.is_err());
    let (machine, err) = result.unwrap_err();

    // Machine should be returned
    assert_eq!(err.guard, "abort_guard");
    assert_eq!(err.event, "advance");

    // Can retry
    let _machine = machine;
}

#[test]
fn around_callback_with_guards_and_callbacks() {
    static EXECUTION_ORDER: AtomicUsize = AtomicUsize::new(0);

    state_machine! {
        name: OrderTest,
        initial: A,
        states: [A, B],
        events {
            go {
                guards: [check_guard],
                around: [wrapper],
                before: [before_cb],
                after: [after_cb],
                transition: { from: A, to: B }
            }
        }
    }

    impl<C, S> OrderTest<C, S> {
        fn check_guard(&self, _ctx: &C) -> bool {
            // Should run after around Before
            assert_eq!(EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst), 1);
            true
        }

        fn wrapper(&self, stage: AroundStage) -> AroundOutcome<A> {
            match stage {
                AroundStage::Before => {
                    // Should run first
                    assert_eq!(EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst), 0);
                    AroundOutcome::Proceed
                }
                AroundStage::AfterSuccess => {
                    // Should run last
                    assert_eq!(EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst), 4);
                    AroundOutcome::Proceed
                }
            }
        }

        fn before_cb(&self) {
            // Should run after guards, before transition
            assert_eq!(EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst), 2);
        }
    }

    impl<C> OrderTest<C, B> {
        fn after_cb(&self) {
            // Should run after transition, before around AfterSuccess
            assert_eq!(EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst), 3);
        }
    }

    EXECUTION_ORDER.store(0, Ordering::SeqCst);
    let machine = OrderTest::new(());
    let _machine = machine.go().unwrap();

    // All 5 stages executed
    assert_eq!(EXECUTION_ORDER.load(Ordering::SeqCst), 5);
}

#[test]
fn multiple_around_callbacks() {
    static FIRST: AtomicUsize = AtomicUsize::new(0);
    static SECOND: AtomicUsize = AtomicUsize::new(0);

    state_machine! {
        name: Multi,
        initial: X,
        states: [X, Y],
        events {
            transition_xy {
                around: [first_wrapper, second_wrapper],
                transition: { from: X, to: Y }
            }
        }
    }

    impl<C, S> Multi<C, S> {
        fn first_wrapper(&self, stage: AroundStage) -> AroundOutcome<X> {
            match stage {
                AroundStage::Before => {
                    FIRST.store(1, Ordering::SeqCst);
                    AroundOutcome::Proceed
                }
                AroundStage::AfterSuccess => {
                    FIRST.store(2, Ordering::SeqCst);
                    AroundOutcome::Proceed
                }
            }
        }

        fn second_wrapper(&self, stage: AroundStage) -> AroundOutcome<X> {
            match stage {
                AroundStage::Before => {
                    SECOND.store(1, Ordering::SeqCst);
                    AroundOutcome::Proceed
                }
                AroundStage::AfterSuccess => {
                    SECOND.store(2, Ordering::SeqCst);
                    AroundOutcome::Proceed
                }
            }
        }
    }

    FIRST.store(0, Ordering::SeqCst);
    SECOND.store(0, Ordering::SeqCst);

    let machine = Multi::new(());
    let _machine = machine.transition_xy().unwrap();

    // Both wrappers executed both stages
    assert_eq!(FIRST.load(Ordering::SeqCst), 2);
    assert_eq!(SECOND.load(Ordering::SeqCst), 2);
}

#[test]
fn around_callback_preserves_action_failed_error() {
    use state_machines::core::TransitionErrorKind;

    state_machine! {
        name: ActionTest,
        initial: Ready,
        states: [Ready, Done],
        events {
            execute {
                around: [action_wrapper],
                transition: { from: Ready, to: Done }
            }
        }
    }

    impl<C, S> ActionTest<C, S> {
        fn action_wrapper(&self, stage: AroundStage) -> AroundOutcome<Ready> {
            match stage {
                AroundStage::Before => {
                    // Abort with ActionFailed (not GuardFailed)
                    AroundOutcome::Abort(state_machines::core::TransitionError {
                        from: Ready,
                        event: "execute",
                        kind: TransitionErrorKind::ActionFailed {
                            action: "custom_action",
                        },
                    })
                }
                AroundStage::AfterSuccess => AroundOutcome::Proceed,
            }
        }
    }

    let machine = ActionTest::new(());
    let result = machine.execute();

    assert!(result.is_err());
    let (_machine, err) = result.unwrap_err();

    // Verify the guard name preserves the action name from the error
    assert_eq!(err.guard, "custom_action");
    assert_eq!(err.event, "execute");

    // Most importantly: verify the KIND is preserved (ActionFailed, not GuardFailed)
    assert_eq!(
        err.kind,
        TransitionErrorKind::ActionFailed {
            action: "custom_action"
        }
    );
}

#[test]
#[should_panic(expected = "Around callback 'after_fail' aborted at AfterSuccess stage")]
fn after_success_abort_panics_with_helpful_message() {
    state_machine! {
        name: AfterFail,
        initial: Start,
        states: [Start, End],
        events {
            go {
                around: [failing_after],
                transition: { from: Start, to: End }
            }
        }
    }

    impl<C, S> AfterFail<C, S> {
        fn failing_after(&self, stage: AroundStage) -> AroundOutcome<Start> {
            match stage {
                AroundStage::Before => AroundOutcome::Proceed,
                AroundStage::AfterSuccess => {
                    // This should panic with a helpful message
                    AroundOutcome::Abort(state_machines::core::TransitionError::guard_failed(
                        Start,
                        "go",
                        "after_fail",
                    ))
                }
            }
        }
    }

    let machine = AfterFail::new(());
    let _result = machine.go(); // Should panic
}
