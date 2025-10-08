use core::fmt::Debug;

/// Marker trait for states used by the generated state machines.
pub trait MachineState: Copy + Eq + Debug + Send + Sync + 'static {}

impl<T> MachineState for T where T: Copy + Eq + Debug + Send + Sync + 'static {}

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

pub trait Machine {
    type State: MachineState;

    fn state(&self) -> Self::State;
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
    pub transitions: &'static [TransitionDefinition<S>],
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
    pub events: &'static [EventDefinition<S>],
}
