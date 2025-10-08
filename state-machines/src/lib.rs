#![no_std]
#![doc = include_str!("../README.md")]

pub mod core {
    pub use state_machines_core::*;
}

pub use state_machines_core::{
    AroundOutcome, AroundStage, EventDefinition, Machine, MachineDefinition, MachineState,
    SuperstateDefinition, TransitionContext, TransitionDefinition, TransitionError,
    TransitionErrorKind, TransitionResult,
};
pub use state_machines_macro::state_machine;
