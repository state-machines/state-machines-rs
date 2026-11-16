use crate::types::{StateMachine, TransitionEdge, TransitionKind};
use harel_machines::{Chart, MAX_STATES, State, StateId, TransitionKind as Kind};
use std::collections::HashMap;
use syn::{Error, Ident, Result};

struct Request {
    leaf: StateId,
    source: StateId,
    target: StateId,
    kind: Kind,
}

type Resolved = (Vec<StateId>, Vec<StateId>);

struct Tree {
    names: Vec<Ident>,
    ids: HashMap<String, StateId>,
    states: Vec<State>,
}

impl Tree {
    fn new(machine: &StateMachine) -> Result<Self> {
        let mut tree = Self {
            names: vec![machine.name.clone()],
            ids: HashMap::new(),
            states: vec![State::atomic(None)],
        };
        for leaf in &machine.states {
            let mut parent = 0;
            for scope in machine.hierarchy.ancestors_of(leaf).iter().chain([leaf]) {
                parent = tree.insert(scope, parent, &machine.name)?;
            }
        }
        for superstate in &machine.hierarchy.superstates {
            if let Some(id) = tree.id(&superstate.name) {
                let initial = tree.child_toward(id, &superstate.initial);
                tree.states[usize::from(id)] =
                    State::compound(tree.states[usize::from(id)].parent, initial);
            }
        }
        let initial = machine
            .hierarchy
            .resolve_target(&machine.initial)
            .map_or(1, |leaf| tree.child_toward(0, &leaf));
        tree.states[0] = State::compound(None, initial);
        Ok(tree)
    }

    fn insert(&mut self, name: &Ident, parent: StateId, machine: &Ident) -> Result<StateId> {
        if let Some(id) = self.id(name) {
            return Ok(id);
        }
        let id = StateId::try_from(self.states.len())
            .ok()
            .filter(|&id| usize::from(id) < MAX_STATES)
            .ok_or_else(|| Error::new(machine.span(), "too many states"))?;
        self.states.push(State::atomic(Some(parent)));
        self.names.push(name.clone());
        self.ids.insert(name.to_string(), id);
        Ok(id)
    }

    fn id(&self, name: &Ident) -> Option<StateId> {
        self.ids.get(&name.to_string()).copied()
    }

    fn child_toward(&self, ancestor: StateId, leaf: &Ident) -> StateId {
        let mut cursor = self.id(leaf);
        while let Some(id) = cursor {
            let parent = self.states[usize::from(id)].parent;
            if parent == Some(ancestor) {
                return id;
            }
            cursor = parent;
        }
        ancestor + 1
    }

    fn request(&self, source: &str, edge: &TransitionEdge) -> Option<Request> {
        if edge.internal {
            return None;
        }
        let leaf = *self.ids.get(source)?;
        let target = self.id(&edge.target)?;
        let (source, kind) = match edge.kind {
            Some(TransitionKind::External) => (self.id(&edge.scope)?, Kind::External),
            Some(TransitionKind::Local) => (self.id(&edge.scope)?, Kind::Internal),
            _ => (leaf, Kind::External),
        };
        Some(Request {
            leaf,
            source,
            target,
            kind,
        })
    }

    fn resolve(&self, requests: &[Option<Request>]) -> std::result::Result<Vec<Resolved>, String> {
        match self.states.len() {
            0..=64 => on_chart_stack::<64>(&self.states, requests),
            65..=4096 => on_chart_stack::<4096>(&self.states, requests),
            _ => on_chart_stack::<MAX_STATES>(&self.states, requests),
        }
    }

    fn idents(&self, ids: &[StateId]) -> Vec<Ident> {
        ids.iter()
            .map(|&id| self.names[usize::from(id)].clone())
            .collect()
    }
}

/// A `Chart<N>` keeps its state and scratch space on the stack: building one
/// and resolving on it takes about 6 MiB at `MAX_STATES` in a debug build
/// (harel-machines' measured figures), more than rustc promises a proc-macro.
/// The work runs on a thread whose stack is sized from `N`.
fn on_chart_stack<const N: usize>(
    states: &[State],
    requests: &[Option<Request>],
) -> std::result::Result<Vec<Resolved>, String> {
    const BASE: usize = 1 << 20;
    const PER_STATE: usize = 160;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("harel-chart".into())
            .stack_size(BASE + N * PER_STATE)
            .spawn_scoped(scope, || resolve_in::<N>(states, requests))
            .map_err(|error| format!("cannot start the chart thread: {error}"))?
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

fn resolve_in<const N: usize>(
    states: &[State],
    requests: &[Option<Request>],
) -> std::result::Result<Vec<Resolved>, String> {
    let chart = Chart::<N>::new(states).map_err(|error| error.to_string())?;
    requests
        .iter()
        .map(|request| {
            let Some(request) = request else {
                return Ok((Vec::new(), Vec::new()));
            };
            let transition = chart
                .configuration(&[request.leaf])
                .and_then(|configuration| {
                    chart.resolve(&configuration, request.source, request.target, request.kind)
                })
                .map_err(|refusal| refusal.to_string())?;
            Ok((transition.exit().to_vec(), transition.entry().to_vec()))
        })
        .collect()
}

impl StateMachine {
    pub fn resolve_domains(&mut self) -> Result<()> {
        let tree = Tree::new(self)?;
        let requests: Vec<_> = self
            .transition_graph
            .edges
            .iter()
            .flat_map(|(source, edges)| edges.iter().map(|edge| tree.request(source, edge)))
            .collect();
        let resolved = tree
            .resolve(&requests)
            .map_err(|error| Error::new(self.name.span(), error))?;
        let edges = self.transition_graph.edges.values_mut().flatten();
        for (edge, (exit, entry)) in edges.zip(resolved) {
            edge.exit = tree.idents(&exit);
            edge.entry = tree.idents(&entry);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
