//! Type definitions for the state machine macro.
//!
//! This module contains all the core data structures used to represent
//! state machines during macro expansion, including:
//! - StateMachine: The root struct representing the entire machine
//! - Event and Transition: Event definitions and their transitions
//! - Callbacks: Global callback configurations (legacy, not used in typestate)
//! - Hierarchy: Superstate tracking and resolution
//! - Storage specifications for state-associated data

use std::collections::HashMap;
use syn::{Ident, Type};

/// The main state machine definition parsed from the macro input.
///
/// Contains all the information needed to generate the state machine code,
/// including states, events, transitions, and hierarchy information.
pub struct StateMachine {
    pub name: Ident,
    pub initial: Ident,
    #[allow(dead_code)]
    pub context: Option<Type>,
    pub states: Vec<Ident>,
    pub state_storage: Vec<StateStorageSpec>,
    pub hierarchy: Hierarchy,
    pub events: Vec<Event>,
    pub async_mode: bool,
    pub dynamic_mode: bool,
    pub transition_graph: TransitionGraph,
}

/// Graph of all possible transitions between states.
///
/// Maps each state to a list of (target_state, event, transition) tuples.
/// Used for typestate validation and code generation.
#[derive(Default)]
pub struct TransitionGraph {
    /// Maps source state -> Vec<(target, event_name, transition)>
    pub edges: HashMap<String, Vec<TransitionEdge>>,
}

/// A single edge in the transition graph.
#[derive(Clone)]
pub struct TransitionEdge {
    pub target: Ident,
    pub event: Ident,
    pub guards: Vec<Ident>,
    pub unless: Vec<Ident>,
    pub before: Vec<Ident>,
    pub after: Vec<Ident>,
    pub around: Vec<Ident>,
    pub payload: Option<Type>,
}

impl TransitionGraph {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a transition edge to the graph.
    #[allow(clippy::too_many_arguments)]
    pub fn add_edge(
        &mut self,
        source: &Ident,
        target: Ident,
        event: Ident,
        guards: Vec<Ident>,
        unless: Vec<Ident>,
        before: Vec<Ident>,
        after: Vec<Ident>,
        around: Vec<Ident>,
        payload: Option<Type>,
    ) {
        self.edges
            .entry(source.to_string())
            .or_default()
            .push(TransitionEdge {
                target,
                event,
                guards,
                unless,
                before,
                after,
                around,
                payload,
            });
    }

    /// Get all outgoing transitions from a state.
    pub fn outgoing(&self, state: &Ident) -> Option<&Vec<TransitionEdge>> {
        self.edges.get(&state.to_string())
    }
}

/// An event definition with its transitions and callbacks.
///
/// Events are the triggers that cause state transitions. Each event can have:
/// - Multiple transitions (from different source states to different targets)
/// - Guards that must pass before any transition can occur
/// - Before/after callbacks that run around transitions
/// - Around callbacks that wrap the entire transition
/// - An optional payload type for passing data
pub struct Event {
    pub name: Ident,
    pub payload: Option<Type>,
    pub transitions: Vec<Transition>,
    pub guards: Vec<Ident>,
    pub unless: Vec<Ident>,
    pub before: Vec<Ident>,
    pub after: Vec<Ident>,
    pub around: Vec<Ident>,
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
    pub around: Vec<Ident>,
}

/// Global callback configurations (legacy, not used in typestate).
#[derive(Default)]
#[allow(dead_code)]
pub struct Callbacks {
    pub before: Vec<TransitionCallback>,
    pub after: Vec<TransitionCallback>,
    pub around: Vec<TransitionCallback>,
}

/// A global callback with optional filtering conditions (legacy).
#[allow(dead_code)]
pub struct TransitionCallback {
    pub name: Ident,
    pub from: Vec<Ident>,
    pub to: Vec<Ident>,
    pub on: Vec<Ident>,
}

/// Specification for state-associated storage.
///
/// When a state has associated data (e.g., `Active(ConnectionData)`),
/// this describes how to store and manage that data in the machine struct.
pub struct StateStorageSpec {
    pub state_name: Ident,
    pub field: Ident,
    pub ty: Type,
}

/// Information about a superstate.
///
/// Superstates are composite states that contain multiple leaf states.
/// They enable hierarchical state machines.
#[derive(Clone)]
#[allow(dead_code)]
pub struct SuperstateInfo {
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
            descendants,
            initial,
        });
    }

    /// Register a leaf state with its ancestor chain.
    pub fn register_leaf(&mut self, leaf: &Ident, ancestors: &[Ident]) {
        if ancestors.is_empty() {
            return;
        }
        self.ancestors.insert(leaf.to_string(), ancestors.to_vec());
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

    /// Get all superstate names as identifiers.
    pub fn all_superstates(&self) -> Vec<Ident> {
        self.lookup
            .keys()
            .map(|k| syn::Ident::new(k, proc_macro2::Span::call_site()))
            .collect()
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
