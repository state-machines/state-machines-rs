//! Validation logic for state machine definitions.
//!
//! This module ensures that the state machine definition is valid
//! before we try to generate code. It checks for:
//! - Valid initial state (must be a leaf, not a superstate)
//! - No duplicate states
//! - Event names follow snake_case convention
//! - All events have at least one transition
//! - All transitions have source and target states
//! - All referenced states exist
//! - Superstates that are used as targets have initial states

use crate::codegen::utils::to_snake_case;
use crate::types::*;
use proc_macro2::Span;
use std::collections::HashSet;
use syn::Result;

/// Return a spanned validation error when `cond` holds.
///
/// Lets each invariant in `validate` read as a single line instead of an
/// `if … { return Err(syn::Error::new(…)) }` block.
fn err_if(cond: bool, span: Span, msg: &str) -> Result<()> {
    if cond {
        Err(syn::Error::new(span, msg))
    } else {
        Ok(())
    }
}

/// Check if a string is in snake_case format.
///
/// Valid snake_case:
/// - All lowercase letters, digits, and underscores
/// - No consecutive underscores
/// - Doesn't start or end with underscore
/// - At least one character
fn is_snake_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must not start or end with underscore
    if s.starts_with('_') || s.ends_with('_') {
        return false;
    }

    let mut prev_was_underscore = false;
    for ch in s.chars() {
        // Only allow lowercase, digits, and underscores
        if !ch.is_lowercase() && !ch.is_ascii_digit() && ch != '_' {
            return false;
        }

        // No consecutive underscores
        if ch == '_' {
            if prev_was_underscore {
                return false;
            }
            prev_was_underscore = true;
        } else {
            prev_was_underscore = false;
        }
    }

    true
}

impl StateMachine {
    /// Validate the state machine definition.
    ///
    /// This performs comprehensive validation to catch errors early
    /// and provide helpful error messages. It's called before code
    /// generation begins.
    ///
    /// # Errors
    ///
    /// Returns a `syn::Error` if any validation check fails.
    /// The error includes a span pointing to the problematic element
    /// and a descriptive message.
    pub fn validate(&self) -> Result<()> {
        err_if(
            self.async_mode && !cfg!(feature = "async"),
            self.name.span(),
            "`async: true` requires enabling the `async` feature on `state-machines`",
        )?;

        // Validate initial state

        // The initial state must be a leaf state, not a superstate
        // This prevents ambiguity about which child state to start in
        err_if(
            self.hierarchy.is_superstate(&self.initial),
            self.initial.span(),
            "`initial` must reference a leaf state",
        )?;

        // The initial state must be declared in the states list
        err_if(
            !self.states.iter().any(|state| state == &self.initial),
            self.initial.span(),
            "`initial` must be a member of `states`",
        )?;

        // Validate states

        // Check for duplicate state names
        // Using a HashSet to track seen names
        let mut seen = HashSet::new();
        for state in &self.states {
            if !seen.insert(state.to_string()) {
                return Err(syn::Error::new(state.span(), "duplicate state"));
            }
        }

        // Validate events and transitions

        for event in &self.events {
            // Validate event naming convention
            // Events must be in snake_case to generate proper method names
            let event_name = event.name.to_string();
            if !is_snake_case(&event_name) {
                let suggested = to_snake_case(&event_name);
                return Err(syn::Error::new(
                    event.name.span(),
                    format!(
                        "event names must be in snake_case (e.g., '{}' instead of '{}')",
                        suggested, event_name
                    ),
                ));
            }

            // Each event must have at least one transition
            // An event with no transitions would be useless
            err_if(
                event.transitions.is_empty(),
                event.name.span(),
                "event must declare at least one transition",
            )?;

            for transition in &event.transitions {
                // Each transition must have at least one source state
                err_if(
                    transition.sources.is_empty(),
                    transition.target.span(),
                    "transition must declare at least one source state",
                )?;

                // Validate and resolve the target state

                // If the target is a superstate, resolve it to its initial child
                // This is allowed, and we'll use the superstate's initial state
                let resolved_target = if self.hierarchy.is_superstate(&transition.target) {
                    match self.hierarchy.resolve_target(&transition.target) {
                        Some(resolved) => resolved,
                        None => {
                            // Superstate has no initial state specified
                            return Err(syn::Error::new(
                                transition.target.span(),
                                "superstate target must declare an initial child",
                            ));
                        }
                    }
                } else {
                    transition.target.clone()
                };

                // The resolved target must be a declared leaf state
                err_if(
                    !self.states.iter().any(|state| state == &resolved_target),
                    transition.target.span(),
                    "target state not declared in `states`",
                )?;

                // Validate source states

                for source in &transition.sources {
                    // Check if this is a declared leaf state
                    let is_leaf = self.states.iter().any(|state| state == source);
                    // Check if this is a declared superstate
                    let is_super = self.hierarchy.is_superstate(source);

                    // Source must be either a leaf or a superstate
                    err_if(
                        !(is_leaf || is_super),
                        source.span(),
                        "source state not declared in `states` or superstates",
                    )?;

                    // If it's a superstate, verify it has descendants
                    // This catches edge cases where a superstate was declared
                    // but has no children (which should be caught earlier,
                    // but we check again for robustness)
                    err_if(
                        self.hierarchy.expand_state(source, &self.states).is_empty(),
                        source.span(),
                        "superstate does not resolve to any leaf states",
                    )?;
                }
            }
        }

        // Validate global callback filters

        // Every state referenced in a `from`/`to` filter must be a declared
        // leaf or superstate, and every event in an `on` filter must be a
        // declared event. Otherwise a typo would silently never match.
        let buckets = [
            &self.callbacks.before,
            &self.callbacks.after,
            &self.callbacks.around,
        ];
        for callback in buckets.iter().copied().flatten() {
            for filter in [&callback.from, &callback.to].into_iter().flatten() {
                for state in filter {
                    let is_leaf = self.states.iter().any(|s| s == state);
                    let is_super = self.hierarchy.is_superstate(state);
                    err_if(
                        !(is_leaf || is_super),
                        state.span(),
                        "callback filter references a state not declared in `states` or superstates",
                    )?;
                }
            }
            for filter_event in callback.on.iter().flatten() {
                err_if(
                    !self.events.iter().any(|e| &e.name == filter_event),
                    filter_event.span(),
                    "callback `on` filter references an undeclared event",
                )?;
            }
        }

        // All validation passed!
        Ok(())
    }
}
