//! Type definitions for the state machine macro.
//!
//! This module contains all the core data structures used to represent
//! state machines during macro expansion, including:
//! - StateMachine: The root struct representing the entire machine
//! - Event and Transition: Event definitions and their transitions
//! - Hierarchy: Superstate tracking and resolution
//! - Storage specifications for state-associated data

use std::collections::{HashMap, hash_map::Entry};
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
    pub lifecycle: Vec<StateLifecycle>,
    pub runtime_lifecycle: Vec<RuntimeLifecycle>,
    pub final_states: Vec<Ident>,
    pub snapshot: bool,
    pub async_mode: bool,
    pub dynamic_mode: bool,
    pub transition_graph: TransitionGraph,
}

pub struct StateLifecycle {
    pub state: Ident,
    pub enter: Vec<Ident>,
    pub exit: Vec<Ident>,
    pub complete: Vec<Ident>,
}
pub struct RuntimeLifecycle {
    pub state: Ident,
    pub after: Vec<Deadline>,
    pub invoke: Vec<Ident>,
    pub defer: Vec<Ident>,
}
pub struct Deadline {
    pub delay: u64,
    pub event: Ident,
}

impl StateMachine {
    /// Graph edges are constructed from declared events during parsing.
    pub fn event(&self, name: &Ident) -> &Event {
        self.events
            .iter()
            .find(|event| &event.name == name)
            .expect("transition edge references a declared event")
    }

    pub fn lifecycle_for<'a>(
        &'a self,
        state: &'a Ident,
    ) -> impl Iterator<Item = &'a StateLifecycle> {
        self.lifecycle
            .iter()
            .filter(move |hooks| &hooks.state == state)
    }

    pub fn active_path(&self, leaf: &Ident) -> Vec<Ident> {
        let mut path = self
            .hierarchy
            .ancestors
            .get(&leaf.to_string())
            .cloned()
            .unwrap_or_default();
        path.push(leaf.clone());
        path
    }

    pub fn retained_prefix(&self, source: &Ident, target: &Ident, edge: &TransitionEdge) -> usize {
        let source_path = self.active_path(source);
        if edge.internal {
            return source_path.len();
        }
        let target_path = self.active_path(target);
        let common = source_path
            .iter()
            .zip(&target_path)
            .take_while(|(a, b)| a == b)
            .count()
            .min(source_path.len() - 1);
        if edge.kind == Some(TransitionKind::External) {
            common.min(
                source_path
                    .iter()
                    .position(|state| state == &edge.scope)
                    .expect("an external edge's scope is on its source path"),
            )
        } else {
            common
        }
    }

    /// The shared ancestor prefix is not exited or re-entered. External
    /// self-transitions still exit and enter the leaf.
    pub fn lifecycle_callbacks(
        &self,
        source: &Ident,
        target: &Ident,
        edge: &TransitionEdge,
    ) -> (Vec<Ident>, Vec<Ident>) {
        if edge.internal {
            return (Vec::new(), Vec::new());
        }
        let source_path = self.active_path(source);
        let target_path = self.active_path(target);
        let common = self.retained_prefix(source, target, edge);
        let exit = source_path[common..]
            .iter()
            .rev()
            .flat_map(|state| self.lifecycle_for(state))
            .flat_map(|hooks| hooks.exit.iter().cloned())
            .collect();
        let enter = target_path[common..]
            .iter()
            .flat_map(|state| self.lifecycle_for(state))
            .flat_map(|hooks| hooks.enter.iter().cloned())
            .collect();
        (exit, enter)
    }

    pub fn reentered_superstate(
        &self,
        source: &Ident,
        edge: &TransitionEdge,
        owner: &Ident,
    ) -> bool {
        if edge.kind != Some(TransitionKind::External) {
            return false;
        }
        let Some(path) = self.hierarchy.ancestors.get(&source.to_string()) else {
            return false;
        };
        match (
            path.iter().position(|s| s == &edge.scope),
            path.iter().position(|s| s == owner),
        ) {
            (Some(domain), Some(index)) => index >= domain,
            _ => false,
        }
    }
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
    pub on_error: Vec<GlobalCallback>,
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
/// `hooks` merges event- and transition-level lists; its `before`/`after`
/// callbacks receive the event payload when one is declared.
/// `global_before`/`global_after` hold matching callbacks from the
/// `callbacks:` block; those are always invoked without a payload so a single
/// method can serve every matching event. Matching global around callbacks
/// are merged into `hooks.around` directly, since around callbacks never
/// receive payloads.
#[derive(Clone)]
pub struct TransitionEdge {
    pub scope: Ident,
    pub kind: Option<TransitionKind>,
    pub data: Option<Ident>,
    pub target: Ident,
    pub event: Ident,
    pub hooks: Hooks,
    pub global_before: Vec<Ident>,
    pub global_after: Vec<Ident>,
    pub payload: Option<Type>,
    pub internal: bool,
    pub selection: Hooks,
    pub fallback: bool,
    pub history: Option<HistoryChoice>,
    pub origin: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HistoryMode {
    Shallow,
    Deep,
}

#[derive(Clone)]
pub struct HistoryChoice {
    pub region: Ident,
    /// Last-leaf indices selecting this destination; None selects the
    /// region's initial child before any successful exit has been recorded.
    pub stored: Vec<Option<usize>>,
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
    pub completion: Option<Ident>,
    pub automatic: bool,
    pub hierarchical: bool,
    pub name: Ident,
    pub payload: Option<Type>,
    pub transitions: Vec<Transition>,
    pub hooks: Hooks,
    pub branching: bool,
}

/// A single transition within an event.
///
/// Defines a transition from one or more source states to a target state.
/// Can have its own guards and callbacks in addition to event-level ones.
pub struct Transition {
    pub kind: Option<TransitionKind>,
    pub data: Option<Ident>,
    pub sources: Vec<Ident>,
    pub target: Ident,
    pub hooks: Hooks,
    pub internal: bool,
    pub fallback: bool,
    pub history: Option<HistoryMode>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Internal,
    Local,
    External,
}

/// The guard and callback lists declarable on an event or a transition.
#[derive(Clone, Default)]
pub struct Hooks {
    pub guards: Vec<Ident>,
    pub unless: Vec<Ident>,
    pub before: Vec<Ident>,
    pub after: Vec<Ident>,
    pub around: Vec<Ident>,
    pub on_error: Vec<Ident>,
}

impl Hooks {
    /// These hooks followed by `inner`'s, list by list, so event-level
    /// entries run before transition-level ones.
    pub fn merged(&self, inner: &Hooks) -> Hooks {
        let concat =
            |outer: &[Ident], inner: &[Ident]| outer.iter().chain(inner).cloned().collect();
        Hooks {
            guards: concat(&self.guards, &inner.guards),
            unless: concat(&self.unless, &inner.unless),
            before: concat(&self.before, &inner.before),
            after: concat(&self.after, &inner.after),
            around: concat(&self.around, &inner.around),
            on_error: concat(&self.on_error, &inner.on_error),
        }
    }
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
    /// Registration order (nested before parent). Iterate this, never a
    /// HashMap, so generated code and schemas are identical across builds.
    pub superstates: Vec<SuperstateInfo>,
    index: HashMap<String, usize>,
    pub ancestors: HashMap<String, Vec<Ident>>,
}

impl Hierarchy {
    pub fn superstate(&self, name: &str) -> Option<&SuperstateInfo> {
        self.index.get(name).map(|&index| &self.superstates[index])
    }

    /// Immediate parent of a leaf or composite, preserving unary hierarchy identity.
    pub fn parent(&self, name: &str) -> Option<&Ident> {
        if let Some(path) = self.ancestors.get(name) {
            return path.last();
        }
        let leaf = self.superstate(name)?.descendants.first()?;
        let path = self.ancestors.get(&leaf.to_string())?;
        let index = path.iter().position(|scope| scope == name)?;
        index.checked_sub(1).map(|index| &path[index])
    }

    /// Register a superstate with its descendants and initial state.
    /// A repeated name replaces the earlier registration in place.
    pub fn register_superstate(&mut self, name: Ident, descendants: Vec<Ident>, initial: Ident) {
        let info = SuperstateInfo {
            name,
            descendants,
            initial,
        };
        match self.index.entry(info.name.to_string()) {
            Entry::Occupied(slot) => self.superstates[*slot.get()] = info,
            Entry::Vacant(slot) => {
                slot.insert(self.superstates.len());
                self.superstates.push(info);
            }
        }
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
    /// If it's a leaf state, returns a single-element slice with that state.
    /// If it's neither, returns an empty slice.
    pub fn expand_state<'a>(&'a self, ident: &Ident, leaves: &'a [Ident]) -> &'a [Ident] {
        if let Some(superstate) = self.superstate(&ident.to_string()) {
            return &superstate.descendants;
        }
        leaves
            .iter()
            .position(|leaf| leaf == ident)
            .map_or(&[], |index| &leaves[index..=index])
    }

    /// Check if an identifier refers to a superstate.
    pub fn is_superstate(&self, ident: &Ident) -> bool {
        self.index.contains_key(&ident.to_string())
    }

    /// Get the initial child state of a superstate (explicit, or its first
    /// descendant as resolved during parsing).
    pub fn initial_child(&self, ident: &Ident) -> Option<Ident> {
        self.superstate(&ident.to_string())
            .map(|superstate| superstate.initial.clone())
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

    /// All superstate names, in registration order.
    pub fn all_superstates(&self) -> impl Iterator<Item = &Ident> {
        self.superstates.iter().map(|superstate| &superstate.name)
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
