#![no_std]

use core::fmt::{self, Debug};

/// The `serde` the schema types derive against, so consumers deriving on their
/// own payload types need no extra dependency: `#[serde(crate = "state_machines_core::serde")]`.
#[cfg(feature = "serde")]
pub use serde;
/// The `serde_json` behind [`MachineSchema::to_json`], for parsing it back.
#[cfg(feature = "serde_json")]
pub use serde_json;

#[cfg(feature = "inspect")]
pub mod schema;

#[cfg(feature = "inspect")]
pub use schema::{
    CallbackKind, CallbackSchema, DeadlineSchema, DiagnosticLevel, EventSchema, Inspectable,
    MachineCallbackSchema, MachineErrorCallbackSchema, MachineSchema, RegionEventSchema,
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
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError::UnsupportedVersion`] when `version` is not `1`,
    /// and [`SnapshotError::WrongMachine`] when `machine` differs from `expected`.
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
    pub const fn invalid_transition(from: S, event: &'static str) -> Self {
        Self {
            from,
            event,
            kind: TransitionErrorKind::InvalidTransition,
        }
    }

    pub const fn guard_failed(from: S, event: &'static str, guard: &'static str) -> Self {
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

/// Error returned when a guard or an around `Before` stage rejects an event
/// in typestate mode.
///
/// In typestate machines, guards and around callbacks can reject an event even though the
/// transition is valid. The source machine is returned along with this error so the caller
/// can retry or handle it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardError {
    pub guard: &'static str,
    pub event: &'static str,
    pub kind: TransitionErrorKind,
}

impl GuardError {
    #[must_use]
    pub const fn new(guard: &'static str, event: &'static str) -> Self {
        Self {
            guard,
            event,
            kind: TransitionErrorKind::GuardFailed { guard },
        }
    }

    #[must_use]
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

/// Error returned when a fallible callback or lifecycle hook returns a
/// user-defined error, before or after the state change (see
/// [`TransitionFailure`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackError<E> {
    pub action: &'static str,
    pub event: &'static str,
    pub source: E,
}

impl<E> CallbackError<E> {
    pub const fn new(action: &'static str, event: &'static str, source: E) -> Self {
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

/// Why a machine that declares `error: E` rejected an event before the
/// state changed: carried by [`TransitionFailure::Rejected`] and passed to
/// `on_error` hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError<E> {
    Guard(GuardError),
    Callback(CallbackError<E>),
}

impl<E> EventError<E> {
    #[must_use]
    pub const fn guard(err: GuardError) -> Self {
        Self::Guard(err)
    }

    pub const fn callback(action: &'static str, event: &'static str, source: E) -> Self {
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

/// Error returned from the typestate event methods of a machine that
/// declares `error: E`.
///
/// The state change is the commit point, as in Ruby `state_machines`:
///
/// - Before it, guards, before callbacks, around `Before` stages and exit
///   hooks may reject the event. Nothing changed state, the `on_error` hooks
///   ran, and [`Rejected`](Self::Rejected) hands back the source-state
///   machine so the caller can retry.
/// - After it, the machine is in the target state whatever happens next. A
///   fallible entry hook, after callback or completion hook that returns an
///   error stops the remaining post-commit callbacks, and
///   [`Committed`](Self::Committed) hands back the target-state machine with
///   that error. Side effects already happened, so nothing is reverted and
///   the `on_error` hooks do not run.
///
/// `Source` and `Target` are the typed machines; a branching event's target
/// is its outcome enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionFailure<Source, Target, E> {
    /// The event was rejected before the state changed.
    Rejected {
        machine: Source,
        error: EventError<E>,
    },
    /// The state changed, then a post-commit callback failed.
    Committed {
        machine: Target,
        error: CallbackError<E>,
    },
}

impl<Source, Target, E> TransitionFailure<Source, Target, E> {
    /// Whether the state change happened before the failure.
    #[must_use]
    pub const fn is_committed(&self) -> bool {
        matches!(self, Self::Committed { .. })
    }

    /// Map the committed machine, leaving a rejection untouched.
    pub fn map_committed<U>(self, f: impl FnOnce(Target) -> U) -> TransitionFailure<Source, U, E> {
        match self {
            Self::Rejected { machine, error } => TransitionFailure::Rejected { machine, error },
            Self::Committed { machine, error } => TransitionFailure::Committed {
                machine: f(machine),
                error,
            },
        }
    }
}

impl<Source, Target, E: Debug> fmt::Display for TransitionFailure<Source, Target, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { error, .. } => fmt::Display::fmt(error, f),
            Self::Committed { error, .. } => write!(f, "transition committed, then {error}"),
        }
    }
}
impl<Source: Debug, Target: Debug, E: Debug> core::error::Error
    for TransitionFailure<Source, Target, E>
{
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
    fn into_result(self) -> Self {
        self
    }
}

/// Synchronous, generic access to a dynamic machine.
///
/// `state_machine!` implements this for every `dynamic: true` machine that is
/// not `async: true`, forwarding to the generated `current_state()` and
/// `handle()` methods. Code that must drive a machine inside a synchronous
/// step, such as a behavior-tree leaf, can then accept any such machine.
/// Async machines dispatch through `runtime::Machine` instead.
pub trait DynamicMachine {
    /// The generated `<Name>State` enum.
    type State: Copy + Eq;
    /// The generated `<Name>Event` enum.
    type Event;
    /// The generated machine's [`DynamicError`] type.
    type Error;

    /// The current state, as `current_state()` reports it.
    fn state(&self) -> Self::State;

    /// Dispatches one event, as `handle()` does: guards and callbacks run,
    /// and automatic transitions settle, before this returns.
    ///
    /// # Errors
    ///
    /// Returns the machine's error when the event is not valid in the current
    /// state or a guard or callback rejects it.
    fn handle(&mut self, event: Self::Event) -> Result<(), Self::Error>;
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
    /// A before callback or exit hook returned a user-defined error before
    /// the state changed; the machine stays in its source state.
    CallbackFailed {
        action: &'static str,
        event: &'static str,
        source: E,
    },
    /// The state changed, then an entry hook, after callback or completion
    /// hook returned a user-defined error. The machine is in the target
    /// state; the remaining post-commit callbacks and automatic settling
    /// were skipped.
    CommittedCallbackFailed {
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
    #[must_use]
    pub const fn invalid_transition(from: &'static str, event: &'static str) -> Self {
        Self::InvalidTransition { from, event }
    }

    #[must_use]
    pub const fn guard_failed(guard: &'static str, event: &'static str) -> Self {
        Self::GuardFailed { guard, event }
    }

    #[must_use]
    pub const fn action_failed(action: &'static str, event: &'static str) -> Self {
        Self::ActionFailed { action, event }
    }

    pub const fn callback_failed(action: &'static str, event: &'static str, source: E) -> Self {
        Self::CallbackFailed {
            action,
            event,
            source,
        }
    }

    #[must_use]
    pub const fn wrong_state(
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

    /// Convert from `GuardError` to `DynamicError`.
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a conversion consumes its input, like `From`"
    )]
    pub const fn from_guard_error(err: GuardError) -> Self {
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

    /// Convert a post-commit callback failure (see
    /// [`TransitionFailure::Committed`]).
    pub fn from_committed_error(err: CallbackError<E>) -> Self {
        Self::CommittedCallbackFailed {
            action: err.action,
            event: err.event,
            source: err.source,
        }
    }

    /// Whether the machine changed state before this error: only
    /// [`CommittedCallbackFailed`](Self::CommittedCallbackFailed). Every
    /// other error leaves the last committed state in place.
    #[must_use]
    pub const fn is_committed(&self) -> bool {
        matches!(self, Self::CommittedCallbackFailed { .. })
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
            Self::CommittedCallbackFailed {
                action,
                event,
                source,
            } => write!(
                f,
                "transition committed, then callback `{action}` failed for `{event}`: {source:?}"
            ),
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

/// The point of a transition an around callback is called at.
///
/// An around enters where it was declared among the before callbacks, after
/// the guards passed, and exits once the state has changed, innermost first
/// and before any after callback (the schema's `CallbackKind` lists the full
/// order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AroundStage {
    /// Entering: runs in declaration order with the before callbacks.
    Before,
    /// Exiting after the state changed; skipped when the transition failed.
    /// The state change is already committed here: returning
    /// [`AroundOutcome::Abort`] cannot undo it, it halts the remaining around
    /// exits and the after callbacks, as an around halting after its yield
    /// does in Ruby. The event still succeeds.
    AfterSuccess,
}

/// What an around callback returns at each [`AroundStage`].
///
/// [`Abort`](Self::Abort) at [`AroundStage::Before`] rejects the event and
/// hands its error to the caller with the source-state machine. At
/// [`AroundStage::AfterSuccess`] it only halts the callbacks still to run;
/// the error is discarded because the transition stands.
#[derive(Debug, Clone)]
pub enum AroundOutcome<S>
where
    S: MachineState,
{
    Proceed,
    Abort(TransitionError<S>),
}
