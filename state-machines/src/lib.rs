#![no_std]
#![cfg_attr(test, allow(non_camel_case_types, non_snake_case))]
#![doc = include_str!("../README.md")]

extern crate alloc;
#[cfg(feature = "runtime-send")]
extern crate std;

/// Select runtime adapter bounds using the facade's features, not the macro host's.
#[doc(hidden)]
#[cfg(feature = "runtime-send")]
#[macro_export]
macro_rules! __sm_runtime_mode {
    (local { $($local:tt)* } send { $($send:tt)* }) => { $($send)* };
}
#[doc(hidden)]
#[cfg(not(feature = "runtime-send"))]
#[macro_export]
macro_rules! __sm_runtime_mode {
    (local { $($local:tt)* } send { $($send:tt)* }) => { $($local)* };
}

#[cfg(feature = "runtime")]
pub mod runtime;

#[doc(hidden)]
#[cfg(feature = "runtime")]
#[macro_export]
macro_rules! __sm_require_runtime {
    () => {};
}
#[doc(hidden)]
#[cfg(not(feature = "runtime"))]
#[macro_export]
macro_rules! __sm_require_runtime {
    () => {
        compile_error!("runtime declarations require the state-machines runtime feature");
    };
}

#[doc(hidden)]
#[cfg(feature = "runtime")]
#[macro_export]
macro_rules! __sm_if_runtime {
    ($($item:tt)*) => { $($item)* };
}

#[doc(hidden)]
#[cfg(not(feature = "runtime"))]
#[macro_export]
macro_rules! __sm_if_runtime {
    ($($item:tt)*) => {};
}

#[doc(hidden)]
pub mod __private {
    pub use alloc::boxed::Box;
    pub use alloc::string::String;
    pub use alloc::vec;
    pub use alloc::vec::Vec;
    #[cfg(feature = "serde")]
    pub use serde;
}

#[doc(hidden)]
#[cfg(feature = "serde")]
#[macro_export]
macro_rules! __sm_if_serde {
    ($($item:tt)*) => { $($item)* };
}

#[doc(hidden)]
#[cfg(not(feature = "serde"))]
#[macro_export]
macro_rules! __sm_if_serde {
    ($($item:tt)*) => {};
}

/// Emit the macro-generated introspection code only when this crate's
/// `inspect` feature is on.
///
/// The gate has to live here, not in the proc-macro: the macro is built once
/// for the host with features unified across build-dependencies, so it cannot
/// tell whether the `state-machines` a caller links exports `MachineSchema`.
#[doc(hidden)]
#[cfg(feature = "inspect")]
#[macro_export]
macro_rules! __sm_if_inspect {
    ($($item:tt)*) => { $($item)* };
}

#[doc(hidden)]
#[cfg(not(feature = "inspect"))]
#[macro_export]
macro_rules! __sm_if_inspect {
    ($($item:tt)*) => {};
}

pub mod core {
    pub use state_machines_core::*;
}

pub use state_machines_core::{
    AroundOutcome, AroundStage, CallbackError, CompletionEvent, DynamicError, EventError,
    MachineState, SnapshotError, SubstateOf, TransitionError, TransitionErrorKind,
};
pub use state_machines_macro::state_machine;

#[cfg(feature = "inspect")]
pub use state_machines_core::{
    DeadlineSchema, DiagnosticLevel, EventSchema, Inspectable, MachineSchema, RegionEventSchema,
    RegionSchema, RuntimeLifecycleSchema, SchemaDiagnostic, StateLifecycleSchema, SuperstateSchema,
    TransitionSchema,
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
