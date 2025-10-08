//! Type definitions for the state machine macro.
//!
//! This module contains all the core data structures used to represent
//! state machines during macro expansion, including:
//! - StateMachine: The root struct representing the entire machine
//! - Event and Transition: Event definitions and their transitions
//! - Callbacks: Global callback configurations
//! - Hierarchy: Superstate tracking and resolution
//! - Storage specifications for state-associated data

use proc_macro2::TokenStream as TokenStream2;
use std::collections::{HashMap, HashSet};
use syn::{Ident, Type};

/// The main state machine definition parsed from the macro input.
///
/// Contains all the information needed to generate the state machine code,
/// including states, events, transitions, callbacks, and hierarchy information.
pub struct StateMachine {
    pub name: Ident,
    pub state: Ident,
    pub initial: Ident,
    pub states: Vec<Ident>,
    pub state_storage: Vec<StateStorageSpec>,
    pub hierarchy: Hierarchy,
    pub events: Vec<Event>,
    pub async_mode: bool,
    pub action: Option<Ident>,
    pub callbacks: Callbacks,
}

/// An event definition with its transitions and callbacks.
///
/// Events are the triggers that cause state transitions. Each event can have:
/// - Multiple transitions (from different source states to different targets)
/// - Guards that must pass before any transition can occur
/// - Before/after callbacks that run around transitions
/// - An optional payload type for passing data
pub struct Event {
    pub name: Ident,
    pub payload: Option<Type>,
    pub transitions: Vec<Transition>,
    pub guards: Vec<Ident>,
    pub unless: Vec<Ident>,
    pub before: Vec<Ident>,
    pub after: Vec<Ident>,
}

/// A single transition within an event.
///
/// Defines a transition from one or more source states to a target state.
/// Can have its own guards and callbacks in addition to event-level ones.
pub struct Transition {
    pub sources: Vec<Ident>,
    pub target: Ident,
    pub guards: Vec<Ident>,
    pub unless: Vec<Ident>,
    pub before: Vec<Ident>,
    pub after: Vec<Ident>,
}

/// Global callback configurations.
///
/// These callbacks apply to all transitions based on filters (from/to/on).
/// Unlike event-level callbacks, these can be conditionally applied.
#[derive(Default)]
pub struct Callbacks {
    pub before: Vec<TransitionCallback>,
    pub after: Vec<TransitionCallback>,
    pub around: Vec<TransitionCallback>,
}

/// A global callback with optional filtering conditions.
///
/// Can specify:
/// - from: Only trigger when transitioning from these states
/// - to: Only trigger when transitioning to these states
/// - on: Only trigger on these events
pub struct TransitionCallback {
    pub name: Ident,
    pub from: Vec<Ident>,
    pub to: Vec<Ident>,
    pub on: Vec<Ident>,
}

/// The generated token streams for an event's methods.
///
/// Contains both the event method (e.g., `activate()`) and
/// the corresponding can method (e.g., `can_activate()`).
pub struct EventTokens {
    pub event_method: TokenStream2,
    pub can_method: TokenStream2,
}

/// Specification for state-associated storage.
///
/// When a state has associated data (e.g., `Active(ConnectionData)`),
/// this describes how to store and manage that data in the machine struct.
pub struct StateStorageSpec {
    pub owner: Ident,
    pub field: Ident,
    pub ty: Type,
    pub is_superstate: bool,
}

/// Information about a superstate.
///
/// Superstates are composite states that contain multiple leaf states.
/// They enable hierarchical state machines.
#[derive(Clone)]
pub struct SuperstateInfo {
    pub name: Ident,
    pub descendants: Vec<Ident>,
    pub initial: Ident,
}

/// Hierarchy tracking for superstates.
///
/// Maintains mappings for:
/// - Which leaf states belong to which superstates
/// - Which superstates are ancestors of which leaf states
/// - Initial child states for superstates
#[derive(Default)]
pub struct Hierarchy {
    pub superstates: Vec<SuperstateInfo>,
    pub lookup: HashMap<String, Vec<Ident>>,
    pub ancestors: HashMap<String, Vec<Ident>>,
    pub initial_children: HashMap<String, Ident>,
}

impl Hierarchy {
    /// Register a superstate with its descendants and initial state.
    pub fn register_superstate(&mut self, name: Ident, descendants: Vec<Ident>, initial: Ident) {
        let lookup_key = name.to_string();
        self.lookup.insert(lookup_key.clone(), descendants.clone());
        self.initial_children.insert(lookup_key, initial.clone());
        self.superstates.push(SuperstateInfo {
            name,
            descendants,
            initial,
        });
    }

    /// Register a leaf state with its ancestor chain.
    pub fn register_leaf(&mut self, leaf: &Ident, ancestors: &[Ident]) {
        if ancestors.is_empty() {
            return;
        }
        self.ancestors
            .insert(leaf.to_string(), ancestors.iter().cloned().collect());
    }

    /// Expand a state identifier to its leaf states.
    ///
    /// If the identifier is a superstate, returns all its descendants.
    /// If it's a leaf state, returns a single-element vector with that state.
    /// If it's neither, returns an empty vector.
    pub fn expand_state(&self, ident: &Ident, leaves: &[Ident]) -> Vec<Ident> {
        if let Some(descendants) = self.lookup.get(&ident.to_string()) {
            return descendants.clone();
        }
        if leaves.iter().any(|leaf| leaf == ident) {
            return vec![ident.clone()];
        }
        Vec::new()
    }

    /// Check if an identifier refers to a superstate.
    pub fn is_superstate(&self, ident: &Ident) -> bool {
        self.lookup.contains_key(&ident.to_string())
    }

    /// Get the initial child state of a superstate.
    ///
    /// Returns the explicitly specified initial state if available,
    /// otherwise returns the first descendant.
    pub fn initial_child(&self, ident: &Ident) -> Option<Ident> {
        self.initial_children
            .get(&ident.to_string())
            .cloned()
            .or_else(|| {
                self.lookup
                    .get(&ident.to_string())
                    .and_then(|desc| desc.first().cloned())
            })
    }

    /// Resolve a target identifier to a concrete leaf state.
    ///
    /// If the identifier is a superstate, returns its initial child.
    /// If it's a leaf state, returns it unchanged.
    pub fn resolve_target(&self, ident: &Ident) -> Option<Ident> {
        if self.is_superstate(ident) {
            self.initial_child(ident)
        } else {
            Some(ident.clone())
        }
    }

    /// Expand a list of state identifiers to leaf states, removing duplicates.
    ///
    /// This is used for filtering conditions in callbacks and guards.
    pub fn expand_filter_states(&self, idents: &[Ident], leaves: &[Ident]) -> Vec<Ident> {
        let mut expanded = Vec::new();
        let mut seen = HashSet::new();

        for ident in idents {
            let resolved = self.expand_state(ident, leaves);
            for leaf in resolved {
                let key = leaf.to_string();
                if seen.insert(key) {
                    expanded.push(leaf);
                }
            }
        }

        expanded
    }
}

/// Result of parsing the states section.
///
/// Contains the leaf states, hierarchy information, and storage specifications.
pub struct ParsedStates {
    pub leaves: Vec<Ident>,
    pub hierarchy: Hierarchy,
    pub storage: Vec<StateStorageSpec>,
}

/// Result of parsing a superstate block.
///
/// Contains the descendants and the initial state for that superstate.
pub struct SuperstateParseResult {
    pub descendants: Vec<Ident>,
    pub initial: Ident,
}
