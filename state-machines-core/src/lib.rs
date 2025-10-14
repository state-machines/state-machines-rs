#![no_std]

use core::fmt::Debug;

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

pub type TransitionResult<S> = Result<(), TransitionError<S>>;

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

/// Error returned when dynamic dispatch fails.
///
/// This error type is used by the dynamic mode wrapper when runtime
/// event dispatch encounters errors like invalid transitions or guard failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DynamicError {
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
    /// Attempted to access or modify state data when in wrong state.
    WrongState {
        expected: &'static str,
        actual: &'static str,
        operation: &'static str,
    },
}

impl DynamicError {
    pub fn invalid_transition(from: &'static str, event: &'static str) -> Self {
        Self::InvalidTransition { from, event }
    }

    pub fn guard_failed(guard: &'static str, event: &'static str) -> Self {
        Self::GuardFailed { guard, event }
    }

    pub fn action_failed(action: &'static str, event: &'static str) -> Self {
        Self::ActionFailed { action, event }
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
}

pub trait Machine {
    type State: MachineState;

    fn state(&self) -> Self::State;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionContext<S>
where
    S: MachineState,
{
    pub from: S,
    pub to: S,
    pub event: &'static str,
}

impl<S> TransitionContext<S>
where
    S: MachineState,
{
    pub const fn new(from: S, to: S, event: &'static str) -> Self {
        Self { from, to, event }
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

#[derive(Debug, Clone)]
pub struct TransitionDefinition<S>
where
    S: MachineState,
{
    pub sources: &'static [S],
    pub target: S,
    pub guards: &'static [&'static str],
    pub unless: &'static [&'static str],
    pub before: &'static [&'static str],
    pub after: &'static [&'static str],
    pub around: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct EventDefinition<S>
where
    S: MachineState,
{
    pub name: &'static str,
    pub guards: &'static [&'static str],
    pub before: &'static [&'static str],
    pub after: &'static [&'static str],
    pub around: &'static [&'static str],
    pub payload: Option<&'static str>,
    pub transitions: &'static [TransitionDefinition<S>],
}

#[derive(Debug, Clone)]
pub struct SuperstateDefinition<S>
where
    S: MachineState,
{
    pub name: &'static str,
    pub descendants: &'static [S],
    pub initial: S,
}

#[derive(Debug, Clone)]
pub struct MachineDefinition<S>
where
    S: MachineState,
{
    pub name: &'static str,
    pub states: &'static [S],
    pub initial: S,
    pub async_mode: bool,
    pub superstates: &'static [SuperstateDefinition<S>],
    pub events: &'static [EventDefinition<S>],
}
