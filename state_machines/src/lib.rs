pub mod core {
    pub use state_machines_core::*;
}

pub use state_machines_core::{
    EventDefinition, Machine, MachineDefinition, MachineState, TransitionDefinition,
    TransitionError, TransitionErrorKind, TransitionResult,
};
pub use state_machines_macro::state_machine;
