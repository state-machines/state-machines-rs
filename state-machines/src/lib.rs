#![no_std]
#![allow(clippy::needless_doctest_main)]
#![cfg_attr(test, allow(non_camel_case_types, non_snake_case))]
#![doc = include_str!("../README.md")]

extern crate alloc;

#[doc(hidden)]
pub mod __private {
    pub use alloc::vec::Vec;
}

pub mod core {
    pub use state_machines_core::*;
}

pub use state_machines_core::{
    AroundOutcome, AroundStage, CallbackError, DynamicError, EventError, MachineState, SubstateOf,
    TransitionError, TransitionErrorKind,
};
pub use state_machines_macro::state_machine;

#[cfg(feature = "inspect")]
pub use state_machines_core::{
    EventSchema, Inspectable, MachineSchema, SuperstateSchema, TransitionSchema,
};

/// Abort an around callback with a guard-style error.
///
/// Around callbacks receive an [`AroundStage`] and return an
/// [`AroundOutcome`]; this macro builds the `Abort` arm without spelling
/// out the [`TransitionError`](core::TransitionError) by hand.
///
/// ```rust,ignore
/// use state_machines::{abort_guard, core::{AroundOutcome, AroundStage}};
///
/// fn resource_wrapper(&self, stage: AroundStage) -> AroundOutcome<Idle> {
///     if matches!(stage, AroundStage::Before) && !self.check_resources() {
///         // state marker, event name, failing guard
///         return abort_guard!(Idle, "provision", check_resources);
///     }
///     AroundOutcome::Proceed
/// }
/// ```
#[macro_export]
macro_rules! abort_guard {
    ($from:expr, $event:expr, $guard:ident) => {
        $crate::core::AroundOutcome::Abort($crate::core::TransitionError::guard_failed(
            $from,
            $event,
            stringify!($guard),
        ))
    };
    ($from:expr, $event:expr, $guard:expr) => {
        $crate::core::AroundOutcome::Abort($crate::core::TransitionError::guard_failed(
            $from, $event, $guard,
        ))
    };
}

/// Abort an around callback with a custom [`TransitionErrorKind`](core::TransitionErrorKind).
///
/// ```rust,ignore
/// use state_machines::{abort_with, core::{AroundOutcome, AroundStage, TransitionErrorKind}};
///
/// fn quota_wrapper(&self, stage: AroundStage) -> AroundOutcome<Idle> {
///     if matches!(stage, AroundStage::Before) && self.quota_exceeded() {
///         return abort_with!(Idle, "provision", TransitionErrorKind::ActionFailed {
///             action: "quota_check",
///         });
///     }
///     AroundOutcome::Proceed
/// }
/// ```
#[macro_export]
macro_rules! abort_with {
    ($from:expr, $event:expr, $kind:expr) => {
        $crate::core::AroundOutcome::Abort($crate::core::TransitionError {
            from: $from,
            event: $event,
            kind: $kind,
        })
    };
}
