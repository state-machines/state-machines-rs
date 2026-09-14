//! Type definitions for the state machine macro.
//!
//! This module contains all the core data structures used to represent
//! state machines during macro expansion, including:
//! - StateMachine: The root struct representing the entire machine
//! - Event and Transition: Event definitions and their transitions
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
    pub context: Option<Type>,
    pub error: Option<Type>,
    pub states: Vec<Ident>,
    pub state_storage: Vec<StateStorageSpec>,
    pub hierarchy: Hierarchy,
    pub events: Vec<Event>,
    pub callbacks: GlobalCallbacks,
    pub async_mode: bool,
    pub dynamic_mode: bool,
    pub transition_graph: TransitionGraph,
}

/// A global callback with optional `from`/`to`/`on` filters.
///
/// Declared in the `callbacks:` block. A missing filter matches everything;
/// `from`/`to` entries may name superstates, which match any descendant leaf.
pub struct GlobalCallback {
    pub name: Ident,
    pub from: Option<Vec<Ident>>,
    pub to: Option<Vec<Ident>>,
    pub on: Option<Vec<Ident>>,
}

/// The `callbacks:` block: machine-wide callbacks applied to every
/// transition that matches their filters.
#[derive(Default)]
pub struct GlobalCallbacks {
    pub before: Vec<GlobalCallback>,
    pub after: Vec<GlobalCallback>,
    pub around: Vec<GlobalCallback>,
}

impl GlobalCallback {
    /// Check whether this callback applies to a concrete edge.
    ///
    /// `source` and `target` are resolved leaf states; filter entries that
    /// name a superstate are expanded to their descendant leaves.
    pub fn matches(
        &self,
        hierarchy: &Hierarchy,
        leaves: &[Ident],
        source: &Ident,
        target: &Ident,
        event: &Ident,
    ) -> bool {
        let state_matches = |filter: &Option<Vec<Ident>>, state: &Ident| match filter {
            None => true,
            Some(entries) => entries.iter().any(|entry| {
                hierarchy
                    .expand_state(entry, leaves)
                    .iter()
                    .any(|leaf| leaf == state)
            }),
        };
        let event_matches = match &self.on {
            None => true,
            Some(events) => events.iter().any(|e| e == event),
        };
        state_matches(&self.from, source) && state_matches(&self.to, target) && event_matches
    }
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
///
/// `before`/`after` hold event- and transition-level callbacks, which receive
/// the event payload when one is declared. `global_before`/`global_after` hold
/// matching callbacks from the `callbacks:` block; those are always invoked
/// without a payload so a single method can serve every matching event.
/// Matching global around callbacks are merged into `around` directly, since
/// around callbacks never receive payloads.
#[derive(Clone)]
pub struct TransitionEdge {
    pub target: Ident,
    pub event: Ident,
    pub guards: Vec<Ident>,
    pub unless: Vec<Ident>,
    pub before: Vec<Ident>,
    pub after: Vec<Ident>,
    pub around: Vec<Ident>,
    pub global_before: Vec<Ident>,
    pub global_after: Vec<Ident>,
    pub payload: Option<Type>,
}

impl TransitionGraph {
    /// Add a transition edge to the graph.
    pub fn add_edge(&mut self, source: &Ident, edge: TransitionEdge) {
        self.edges.entry(source.to_string()).or_default().push(edge);
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
