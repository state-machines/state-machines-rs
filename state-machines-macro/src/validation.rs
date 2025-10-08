//! Validation logic for state machine definitions.
//!
//! This module ensures that the state machine definition is valid
//! before we try to generate code. It checks for:
//! - Valid initial state (must be a leaf, not a superstate)
//! - No duplicate states
//! - All events have at least one transition
//! - All transitions have source and target states
//! - All referenced states exist
//! - Superstates that are used as targets have initial states

use crate::types::*;
use std::collections::HashSet;
use syn::Result;

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
        // Validate initial state

        // The initial state must be a leaf state, not a superstate
        // This prevents ambiguity about which child state to start in
        if self.hierarchy.is_superstate(&self.initial) {
            return Err(syn::Error::new(
                self.initial.span(),
                "`initial` must reference a leaf state",
            ));
        }

        // The initial state must be declared in the states list
        if !self.states.iter().any(|state| state == &self.initial) {
            return Err(syn::Error::new(
                self.initial.span(),
                "`initial` must be a member of `states`",
            ));
        }

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
            // Each event must have at least one transition
            // An event with no transitions would be useless
            if event.transitions.is_empty() {
                return Err(syn::Error::new(
                    event.name.span(),
                    "event must declare at least one transition",
                ));
            }

            for transition in &event.transitions {
                // Each transition must have at least one source state
                if transition.sources.is_empty() {
                    return Err(syn::Error::new(
                        transition.target.span(),
                        "transition must declare at least one source state",
                    ));
                }

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
                if !self.states.iter().any(|state| state == &resolved_target) {
                    return Err(syn::Error::new(
                        transition.target.span(),
                        "target state not declared in `states`",
                    ));
                }

                // Validate source states

                for source in &transition.sources {
                    // Check if this is a declared leaf state
                    let is_leaf = self.states.iter().any(|state| state == source);
                    // Check if this is a declared superstate
                    let is_super = self.hierarchy.is_superstate(source);

                    // Source must be either a leaf or a superstate
                    if !(is_leaf || is_super) {
                        return Err(syn::Error::new(
                            source.span(),
                            "source state not declared in `states` or superstates",
                        ));
                    }

                    // If it's a superstate, verify it has descendants
                    // This catches edge cases where a superstate was declared
                    // but has no children (which should be caught earlier,
                    // but we check again for robustness)
                    if self.hierarchy.expand_state(source, &self.states).is_empty() {
                        return Err(syn::Error::new(
                            source.span(),
                            "superstate does not resolve to any leaf states",
                        ));
                    }
                }
            }
        }

        // All validation passed!
        Ok(())
    }
}
