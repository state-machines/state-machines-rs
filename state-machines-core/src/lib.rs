#![no_std]

use core::fmt::{self, Debug};

#[cfg(feature = "inspect")]
pub mod schema;

#[cfg(feature = "inspect")]
pub use schema::{
    DeadlineSchema, DiagnosticLevel, EventSchema, Inspectable, MachineSchema, RegionEventSchema,
    RegionSchema, RuntimeLifecycleSchema, SchemaDiagnostic, StateLifecycleSchema, SuperstateSchema,
    TransitionSchema,
};

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

/// A committed transition reached a declared final leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionEvent {
    Machine,
    Superstate(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    UnsupportedVersion { expected: u32, actual: u32 },
    WrongMachine,
    UnknownState,
    InactiveData { state: &'static str },
    InvalidHistory { region: &'static str },
}

impl SnapshotError {
    /// Shared version/name validation for leaf and composed snapshot envelopes.
    pub fn validate_header(version: u32, machine: &str, expected: &str) -> Result<(), Self> {
        if version != 1 {
            return Err(Self::UnsupportedVersion {
                expected: 1,
                actual: version,
            });
        }
        if machine != expected {
            return Err(Self::WrongMachine);
        }
        Ok(())
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { expected, actual } => {
                write!(
                    f,
                    "unsupported snapshot version {actual} (expected {expected})"
                )
            }
            Self::WrongMachine => f.write_str("snapshot belongs to a different machine"),
            Self::UnknownState => f.write_str("snapshot names an unknown state"),
            Self::InactiveData { state } => {
                write!(f, "snapshot carries data for inactive state `{state}`")
            }
            Self::InvalidHistory { region } => {
                write!(f, "snapshot records invalid history for `{region}`")
            }
        }
    }
}
impl core::error::Error for SnapshotError {}

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

impl<S: MachineState> fmt::Display for TransitionError<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} for `{}` from {:?}", self.kind, self.event, self.from)
    }
}
impl<S: MachineState> core::error::Error for TransitionError<S> {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionErrorKind {
    InvalidTransition,
    GuardFailed { guard: &'static str },
    ActionFailed { action: &'static str },
}

impl fmt::Display for TransitionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition => f.write_str("invalid transition"),
            Self::GuardFailed { guard } => write!(f, "guard `{guard}` failed"),
            Self::ActionFailed { action } => write!(f, "action `{action}` failed"),
        }
    }
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

impl fmt::Display for GuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} for `{}`", self.kind, self.event)
    }
}
impl core::error::Error for GuardError {}

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

// User callback errors only need `Debug` (they may be `()` and need not
// implement `Error`), so they are rendered inline rather than via `source()`.
impl<E: Debug> fmt::Display for CallbackError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "callback `{}` failed for `{}`: {:?}",
            self.action, self.event, self.source
        )
    }
}
impl<E: Debug> core::error::Error for CallbackError<E> {}

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

impl<E: Debug> fmt::Display for EventError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Guard(error) => fmt::Display::fmt(error, f),
            Self::Callback(error) => fmt::Display::fmt(error, f),
        }
    }
}
impl<E: Debug> core::error::Error for EventError<E> {}

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
    /// Automatic transitions did not settle within the requested microstep budget.
    StepLimit { limit: usize },
    /// Dispatch was cancelled or unwound after taking ownership of the machine.
    /// The wrapper must be replaced; the last committed state is diagnostic only.
    Poisoned {
        from: &'static str,
        event: &'static str,
    },
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

impl<E: Debug> fmt::Display for DynamicError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StepLimit { limit } => {
                write!(
                    f,
                    "automatic transitions did not settle within {limit} steps"
                )
            }
            Self::Poisoned { from, event } => write!(
                f,
                "machine poisoned: `{event}` from `{from}` was cancelled or unwound"
            ),
            Self::InvalidTransition { from: "", event } => {
                write!(f, "invalid transition for `{event}`")
            }
            Self::InvalidTransition { from, event } => {
                write!(f, "invalid transition for `{event}` from `{from}`")
            }
            Self::GuardFailed { guard, event } => {
                write!(f, "guard `{guard}` failed for `{event}`")
            }
            Self::ActionFailed { action, event } => {
                write!(f, "action `{action}` failed for `{event}`")
            }
            Self::CallbackFailed {
                action,
                event,
                source,
            } => write!(f, "callback `{action}` failed for `{event}`: {source:?}"),
            Self::WrongState {
                expected,
                actual,
                operation,
            } => write!(
                f,
                "`{operation}` requires state `{expected}`, but the machine is in `{actual}`"
            ),
        }
    }
}
impl<E: Debug> core::error::Error for DynamicError<E> {}

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
