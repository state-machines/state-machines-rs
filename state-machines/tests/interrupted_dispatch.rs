#![cfg(all(feature = "async", not(feature = "runtime-send")))]

use state_machines::{DynamicError, state_machine};
use std::assert_matches;
use std::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};

state_machine! {
    name: Interruptible,
    dynamic: true,
    snapshot: true,
    async: true,
    initial: Ready,
    states: [Ready, Done],
    final_states: [Done],
    events {
        wait { before: [suspend], transition: { from: Ready, to: Done } }
    }
}

impl<C, S> Interruptible<C, S> {
    async fn suspend(&self) {
        std::future::pending::<()>().await;
    }
}

#[test]
fn cancellation_has_an_explicit_non_panicking_poison_contract() {
    let mut machine = DynamicInterruptible::new(());
    {
        let mut future = pin!(machine.handle(InterruptibleEvent::Wait));
        assert_matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        );
    }
    assert!(machine.is_poisoned());
    assert_eq!(machine.current_state(), InterruptibleState::Ready);
    assert!(!machine.is_finished());
    assert_eq!(machine.completion_events(), []);
    assert!(pollster::block_on(machine.get_available_events()).is_empty());
    assert!(!pollster::block_on(
        machine.is_available_event(&InterruptibleEvent::Wait)
    ));
    assert_matches!(
        pollster::block_on(machine.handle(InterruptibleEvent::Wait)),
        Err(DynamicError::Poisoned {
            from: "Ready",
            event: "wait"
        })
    );
    #[cfg(feature = "serde")]
    assert!(machine.try_into_snapshot().unwrap_err().is_poisoned());
}

#[test]
fn dropping_an_unpolled_future_keeps_the_machine() {
    let mut machine = DynamicInterruptible::new(());
    drop(machine.handle(InterruptibleEvent::Wait));
    assert!(!machine.is_poisoned());
}

mod unwind {
    use super::*;
    state_machine! {
        name: PanicMachine, dynamic: true, initial: Idle, states: [Idle, Active],
        events { go { before: [explode], transition: { from: Idle, to: Active } } }
    }
    impl<C, S> PanicMachine<C, S> {
        fn explode(&self) {
            panic!("callback");
        }
    }
    #[test]
    fn caught_unwind_is_observable_without_a_second_panic() {
        let mut machine = DynamicPanicMachine::new(());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || machine.handle(PanicMachineEvent::Go)
            ))
            .is_err()
        );
        assert!(machine.is_poisoned());
        assert_matches!(
            machine.handle(PanicMachineEvent::Go),
            Err(DynamicError::Poisoned { .. })
        );
    }
}
