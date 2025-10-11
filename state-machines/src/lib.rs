#![no_std]
#![allow(clippy::needless_doctest_main)]
#![cfg_attr(test, allow(non_camel_case_types, non_snake_case))]
#![doc = include_str!("../../README.md")]

pub mod core {
    pub use state_machines_core::*;
}

pub use state_machines_core::{
    AroundOutcome, AroundStage, DynamicError, EventDefinition, Machine, MachineDefinition,
    MachineState, SubstateOf, SuperstateDefinition, TransitionContext, TransitionDefinition,
    TransitionError, TransitionErrorKind, TransitionResult,
};
pub use state_machines_macro::state_machine;

/// Convenience macro for aborting an around callback with a guard-style error.
///
/// ```rust,ignore
/// use state_machines::{abort_guard, core::{AroundOutcome, AroundStage, TransitionContext}};
///
/// fn guard(
///     ctx: &TransitionContext<MyState>,
///     stage: AroundStage,
/// ) -> AroundOutcome<MyState> {
///     if matches!(stage, AroundStage::Before) && !check_resources() {
///         return abort_guard!(ctx, check_resources);
///     }
///     AroundOutcome::Proceed
/// }
/// ```
#[macro_export]
macro_rules! abort_guard {
    ($ctx:expr, $guard:ident) => {
        $crate::core::AroundOutcome::Abort($crate::core::TransitionError::guard_failed(
            $ctx.from,
            $ctx.event,
            stringify!($guard),
        ))
    };
    ($ctx:expr, $guard:expr) => {
        $crate::core::AroundOutcome::Abort($crate::core::TransitionError::guard_failed(
            $ctx.from, $ctx.event, $guard,
        ))
    };
}

/// Build a custom transition error from within an around callback.
///
/// ```rust,ignore
/// use state_machines::{abort_with, core::{AroundOutcome, TransitionContext, TransitionErrorKind}};
///
/// fn guard(ctx: &TransitionContext<MyState>) -> AroundOutcome<MyState> {
///     if quota_exceeded() {
///         return abort_with!(ctx, TransitionErrorKind::ActionFailed { action: "quota_check" });
///     }
///     AroundOutcome::Proceed
/// }
/// ```
#[macro_export]
macro_rules! abort_with {
    ($ctx:expr, $kind:expr) => {
        $crate::core::AroundOutcome::Abort($crate::core::TransitionError {
            from: $ctx.from,
            event: $ctx.event,
            kind: $kind,
        })
    };
}
