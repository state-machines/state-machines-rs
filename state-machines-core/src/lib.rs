#![no_std]

use core::fmt::Debug;

#[cfg(feature = "inspect")]
pub mod schema;

#[cfg(feature = "inspect")]
pub use schema::{EventSchema, Inspectable, MachineSchema, SuperstateSchema, TransitionSchema};

/// Marker trait for states used by the generated state machines.
pub trait MachineState: Copy + Eq + Debug + Send + Sync + 'static {}

impl<T> MachineState for T where T: Copy + Eq + Debug + Send + Sync + 'static {}

/// Marker trait indicating that a state is a substate of a superstate.
///
/// This enables polymorphic transitions from any substate to work as if
/// they were from the superstate. For example:
///
/// ```rust,ignore
/// // If LaunchPrep and Launching are substates of Flight:
/// impl SubstateOf<Flight> for LaunchPrep {}
/// impl SubstateOf<Flight> for Launching {}
///
/// // Then a transition "from Flight" can accept any Flight substate:
/// impl<C, S: SubstateOf<Flight>> Machine<C, S> {
///     pub fn abort(self) -> Machine<C, Standby> { ... }
/// }
/// ```
pub trait SubstateOf<Super> {}

/// Represents an error that occurred while attempting a transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionError<S>
where
    S: MachineState,
{
    pub from: S,
    pub event: &'static str,
    pub kind: TransitionErrorKind,
}

impl<S> TransitionError<S>
where
    S: MachineState,
{
    pub fn invalid_transition(from: S, event: &'static str) -> Self {
        Self {
            from,
            event,
            kind: TransitionErrorKind::InvalidTransition,
        }
    }

    pub fn guard_failed(from: S, event: &'static str, guard: &'static str) -> Self {
        Self {
            from,
            event,
            kind: TransitionErrorKind::GuardFailed { guard },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionErrorKind {
    InvalidTransition,
    GuardFailed { guard: &'static str },
    ActionFailed { action: &'static str },
}

/// Error returned when a guard or around callback fails in typestate mode.
///
/// In typestate machines, guards and around callbacks can fail even though the transition is valid.
/// The machine is returned along with this error so the caller can retry or handle it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardError {
    pub guard: &'static str,
    pub event: &'static str,
    pub kind: TransitionErrorKind,
}

impl GuardError {
    pub const fn new(guard: &'static str, event: &'static str) -> Self {
        Self {
            guard,
            event,
            kind: TransitionErrorKind::GuardFailed { guard },
        }
    }

    pub const fn with_kind(
        guard: &'static str,
        event: &'static str,
        kind: TransitionErrorKind,
    ) -> Self {
        Self { guard, event, kind }
    }
}

/// Error returned when a before/after callback returns a user-defined error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackError<E> {
    pub action: &'static str,
    pub event: &'static str,
    pub source: E,
}

impl<E> CallbackError<E> {
    pub fn new(action: &'static str, event: &'static str, source: E) -> Self {
        Self {
            action,
            event,
            source,
        }
    }
}

/// Error returned from typestate event methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError<E> {
    Guard(GuardError),
    Callback(CallbackError<E>),
}

impl<E> EventError<E> {
    pub const fn guard(err: GuardError) -> Self {
        Self::Guard(err)
    }

    pub fn callback(action: &'static str, event: &'static str, source: E) -> Self {
        Self::Callback(CallbackError::new(action, event, source))
    }
}

#[doc(hidden)]
pub trait FallibleCallbackReturn<E> {
    fn into_result(self) -> Result<(), E>;
}

impl<E> FallibleCallbackReturn<E> for () {
    fn into_result(self) -> Result<(), E> {
        Ok(())
    }
}

impl<E> FallibleCallbackReturn<E> for Result<(), E> {
    fn into_result(self) -> Result<(), E> {
        self
    }
}

/// Error returned when dynamic dispatch fails.
///
/// This error type is used by the dynamic mode wrapper when runtime
/// event dispatch encounters errors like invalid transitions or guard failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicError<E = ()> {
    /// Attempted to trigger an event that's not valid from the current state.
    InvalidTransition {
        from: &'static str,
        event: &'static str,
    },
    /// A guard callback failed during the transition.
    GuardFailed {
        guard: &'static str,
        event: &'static str,
    },
    /// An action callback failed during the transition.
    ActionFailed {
        action: &'static str,
        event: &'static str,
    },
    /// A before/after callback returned a user-defined error.
    CallbackFailed {
        action: &'static str,
        event: &'static str,
        source: E,
    },
    /// Attempted to access or modify state data when in wrong state.
    WrongState {
        expected: &'static str,
        actual: &'static str,
        operation: &'static str,
    },
}

impl<E> DynamicError<E> {
    pub fn invalid_transition(from: &'static str, event: &'static str) -> Self {
        Self::InvalidTransition { from, event }
    }

    pub fn guard_failed(guard: &'static str, event: &'static str) -> Self {
        Self::GuardFailed { guard, event }
    }

    pub fn action_failed(action: &'static str, event: &'static str) -> Self {
        Self::ActionFailed { action, event }
    }

    pub fn callback_failed(action: &'static str, event: &'static str, source: E) -> Self {
        Self::CallbackFailed {
            action,
            event,
            source,
        }
    }

    pub fn wrong_state(
        expected: &'static str,
        actual: &'static str,
        operation: &'static str,
    ) -> Self {
        Self::WrongState {
            expected,
            actual,
            operation,
        }
    }

    /// Convert from GuardError to DynamicError.
    pub fn from_guard_error(err: GuardError) -> Self {
        match err.kind {
            TransitionErrorKind::GuardFailed { guard } => Self::GuardFailed {
                guard,
                event: err.event,
            },
            TransitionErrorKind::ActionFailed { action } => Self::ActionFailed {
                action,
                event: err.event,
            },
            TransitionErrorKind::InvalidTransition => Self::InvalidTransition {
                from: "",
                event: err.event,
            },
        }
    }

    pub fn from_event_error(err: EventError<E>) -> Self {
        match err {
            EventError::Guard(err) => Self::from_guard_error(err),
            EventError::Callback(err) => Self::callback_failed(err.action, err.event, err.source),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AroundStage {
    Before,
    AfterSuccess,
}

#[derive(Debug, Clone)]
pub enum AroundOutcome<S>
where
    S: MachineState,
{
    Proceed,
    Abort(TransitionError<S>),
}
