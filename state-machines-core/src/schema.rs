//! Serializable schema types for state machine introspection.
//!
//! Generated machines expose these through `schema()` (and the
//! [`Inspectable`] trait) when the `inspect` feature is enabled, allowing
//! serialization to JSON, Mermaid, and other formats.

extern crate alloc;

use core::fmt::Write;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::{collections::BTreeSet, format};
use serde::{Deserialize, Serialize};

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde `skip_serializing_if` hands over a reference"
)]
const fn is_false(b: &bool) -> bool {
    !*b
}

fn validate_scopes<'a>(
    scopes: impl Iterator<Item = &'a String>,
    names: &BTreeSet<&String>,
    kind: &str,
    report: &mut impl FnMut(DiagnosticLevel, String),
) {
    let mut seen = BTreeSet::new();
    for scope in scopes {
        if !names.contains(scope) || !seen.insert(scope) {
            report(
                DiagnosticLevel::Error,
                format!("invalid or duplicate {kind} `{scope}`"),
            );
        }
    }
}

/// Per-event bookkeeping while validating its transitions in declaration order.
#[derive(Default)]
struct EventCoverage<'a> {
    /// Leaves already covered by an earlier transition.
    sources: BTreeSet<&'a String>,
    /// Hierarchical scopes already declared by an earlier source.
    declared_scopes: BTreeSet<&'a String>,
    /// Leaves whose fallback has already been declared.
    fallbacks: BTreeSet<&'a String>,
}

/// Serializable representation of a state machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineSchema {
    pub name: String,
    pub initial: String,
    pub states: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superstates: Vec<SuperstateSchema>,
    pub events: Vec<EventSchema>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub async_mode: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifecycle: Vec<StateLifecycleSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime: Vec<RuntimeLifecycleSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub final_states: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<RegionSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub region_events: Vec<RegionEventSchema>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegionSchema {
    pub name: String,
    pub machine: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegionEventSchema {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeLifecycleSchema {
    pub state: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<DeadlineSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invoke: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defer: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeadlineSchema {
    pub delay: u64,
    pub event: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateLifecycleSchema {
    pub state: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enter: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub complete: Vec<String>,
}

/// Serializable representation of a superstate (hierarchical state).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuperstateSchema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub name: String,
    pub descendants: Vec<String>,
    pub initial: String,
}

/// Serializable representation of an event.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventSchema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub automatic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hierarchical: bool,
    pub name: String,
    pub transitions: Vec<TransitionSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub around: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_error: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub branching: bool,
}

/// Serializable representation of a transition.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransitionSchema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    pub sources: Vec<String>,
    pub target: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub fallback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub around: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_error: Vec<String>,
}

/// Trait for types that can provide their schema for introspection.
pub trait Inspectable {
    /// Returns the schema describing this state machine.
    fn schema() -> MachineSchema;
}

/// Structural graph diagnostics do not evaluate user guards or callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
}

impl MachineSchema {
    /// Leaf states covered by a superstate or leaf name; empty when unknown.
    fn leaves(&self, name: &String) -> &[String] {
        if let Some(superstate) = self.superstates.iter().find(|s| &s.name == name) {
            return &superstate.descendants;
        }
        self.states
            .iter()
            .position(|state| state == name)
            .map_or(&[], |index| &self.states[index..=index])
    }

    fn completed_scopes<'a>(&'a self, leaf: &'a String) -> Vec<&'a String> {
        let mut scopes = Vec::new();
        let mut child = leaf;
        while self.final_states.contains(child) {
            let parent = if self.states.contains(child) {
                self.superstates
                    .iter()
                    .find(|s| {
                        s.descendants.contains(child)
                            && !self.superstates.iter().any(|nested| {
                                nested.parent.as_ref() == Some(&s.name)
                                    && nested.descendants.contains(child)
                            })
                    })
                    .map(|s| &s.name)
            } else {
                self.superstates
                    .iter()
                    .find(|s| &s.name == child)
                    .and_then(|s| s.parent.as_ref())
            };
            let scope = parent.unwrap_or(child);
            // Malformed schema cycles must not hang validation.
            if scopes.contains(&scope) {
                break;
            }
            scopes.push(scope);
            if scope == child {
                break;
            }
            child = scope;
        }
        scopes
    }

    /// Validate references and determinism, and lint structural reachability.
    ///
    /// Dead ends and unreachable states are warnings: both can be intentional.
    /// Guard-dependent reachability cannot be decided from a schema.
    #[must_use]
    pub fn validate(&self) -> Vec<SchemaDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut report = |level, message| {
            diagnostics.push(SchemaDiagnostic { level, message });
        };
        if !self.regions.is_empty() {
            self.validate_regions(&mut report);
            return diagnostics;
        }
        let names = self.validate_states(&mut report);
        self.validate_runtime_rules(&names, &mut report);
        let edges = self.validate_events(&mut report);
        self.validate_final_states(&names, &mut report);
        self.lint_reachability(&edges, &mut report);
        diagnostics
    }

    /// Orthogonal schemas only carry regions and routed region events.
    fn validate_regions(&self, report: &mut impl FnMut(DiagnosticLevel, String)) {
        let names: BTreeSet<_> = self.regions.iter().map(|region| &region.name).collect();
        if self.regions.len() < 2 || names.len() != self.regions.len() {
            report(
                DiagnosticLevel::Error,
                "invalid or duplicate orthogonal regions".into(),
            );
        }
        if !self.states.is_empty() || !self.events.is_empty() {
            report(
                DiagnosticLevel::Error,
                "orthogonal schema cannot also declare exclusive states/events".into(),
            );
        }
        let mut events = BTreeSet::new();
        for event in &self.region_events {
            if !events.insert(&event.name) || event.regions.is_empty() {
                report(
                    DiagnosticLevel::Error,
                    "invalid or duplicate region event".into(),
                );
            }
            validate_scopes(event.regions.iter(), &names, "routed region", report);
        }
    }

    /// Leaf and superstate declarations; returns every declared state name.
    fn validate_states(
        &self,
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) -> BTreeSet<&String> {
        let mut names = BTreeSet::new();
        for state in &self.states {
            if !names.insert(state) {
                report(DiagnosticLevel::Error, format!("duplicate state `{state}`"));
            }
        }
        if !self.states.contains(&self.initial) {
            report(
                DiagnosticLevel::Error,
                format!("unknown initial state `{}`", self.initial),
            );
        }
        for superstate in &self.superstates {
            if let Some(parent) = &superstate.parent
                && (parent == &superstate.name
                    || !self.superstates.iter().any(|s| {
                        &s.name == parent
                            && superstate
                                .descendants
                                .iter()
                                .all(|leaf| s.descendants.contains(leaf))
                    }))
            {
                report(
                    DiagnosticLevel::Error,
                    format!("invalid parent of `{}`", superstate.name),
                );
            }
            if !names.insert(&superstate.name) {
                report(
                    DiagnosticLevel::Error,
                    format!("duplicate state `{}`", superstate.name),
                );
            }
            if !superstate.descendants.contains(&superstate.initial)
                || !self.states.contains(&superstate.initial)
            {
                report(
                    DiagnosticLevel::Error,
                    format!("invalid initial child of `{}`", superstate.name),
                );
            }
            for child in &superstate.descendants {
                if !self.states.contains(child) {
                    report(DiagnosticLevel::Error, format!("unknown child `{child}`"));
                }
            }
        }
        names
    }

    /// Runtime/lifecycle scopes must name declared states; deferrals name external events.
    fn validate_runtime_rules(
        &self,
        names: &BTreeSet<&String>,
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) {
        validate_scopes(
            self.runtime.iter().map(|rule| &rule.state),
            names,
            "runtime scope",
            report,
        );
        validate_scopes(
            self.lifecycle.iter().map(|hooks| &hooks.state),
            names,
            "lifecycle",
            report,
        );
        let external_events: BTreeSet<_> = self
            .events
            .iter()
            .filter(|event| !event.automatic)
            .map(|event| &event.name)
            .collect();
        for rule in &self.runtime {
            let mut deferred = BTreeSet::new();
            for name in &rule.defer {
                if !deferred.insert(name) || !external_events.contains(name) {
                    report(
                        DiagnosticLevel::Error,
                        format!("invalid deferred event `{name}`"),
                    );
                }
            }
        }
    }

    /// Event-level checks and determinism; returns the leaf-to-leaf edges.
    fn validate_events(
        &self,
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) -> Vec<(&String, &String)> {
        let mut events = BTreeSet::new();
        let mut edges = Vec::new();
        for event in &self.events {
            if let Some(scope) = &event.completion
                && (!event.automatic
                    || !self.superstates.iter().any(|s| &s.name == scope)
                    || event
                        .transitions
                        .iter()
                        .any(|t| t.internal || t.sources.iter().any(|s| s != scope)))
            {
                report(
                    DiagnosticLevel::Error,
                    "invalid parent completion transition".into(),
                );
            }
            if event.automatic && event.payload.is_some() {
                report(
                    DiagnosticLevel::Error,
                    "automatic event has an external payload".into(),
                );
            }
            if event.hierarchical && event.branching {
                report(
                    DiagnosticLevel::Error,
                    "hierarchical and branching selection conflict".into(),
                );
            }
            if !events.insert(&event.name) {
                report(
                    DiagnosticLevel::Error,
                    format!("duplicate event `{}`", event.name),
                );
            }
            if event.transitions.is_empty() {
                report(
                    DiagnosticLevel::Error,
                    format!("event `{}` has no transitions", event.name),
                );
            }
            let mut coverage = EventCoverage::default();
            for transition in &event.transitions {
                let target = self.validate_transition_target(event, transition, report);
                self.validate_transition_sources(
                    event,
                    transition,
                    target,
                    &mut coverage,
                    &mut edges,
                    report,
                );
            }
        }
        edges
    }

    /// Kind, history, branch shape and target of one transition; returns the
    /// leaf entered by an external edge (a superstate resolves to its initial child).
    fn validate_transition_target<'a>(
        &'a self,
        event: &'a EventSchema,
        transition: &'a TransitionSchema,
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) -> &'a String {
        if let Some(kind) = &transition.kind {
            if !["internal", "local", "external"].contains(&kind.as_str())
                || (kind == "internal") != transition.internal
            {
                report(
                    DiagnosticLevel::Error,
                    format!("invalid transition kind `{kind}`"),
                );
            }
            let target_leaves = self.leaves(&transition.target);
            if kind == "local"
                && transition.sources.iter().any(|source| {
                    !self.superstates.iter().any(|s| {
                        &s.name == source
                            && s.descendants
                                .iter()
                                .any(|leaf| target_leaves.contains(leaf))
                    })
                })
            {
                report(
                    DiagnosticLevel::Error,
                    "local transition leaves its source superstate".into(),
                );
            }
        }
        if let Some(mode) = &transition.history
            && (transition.internal
                || !["shallow", "deep"].contains(&mode.as_str())
                || !self.superstates.iter().any(|s| s.name == transition.target))
        {
            report(
                DiagnosticLevel::Error,
                format!("invalid history target `{}`", transition.target),
            );
        }
        let guarded = !transition.guards.is_empty() || !transition.unless.is_empty();
        if (transition.fallback && (!event.branching || guarded))
            || (event.branching && !transition.fallback && !guarded)
        {
            report(
                DiagnosticLevel::Error,
                format!("invalid branch candidate for `{}`", event.name),
            );
        }
        let target = self
            .superstates
            .iter()
            .find(|s| s.name == transition.target)
            .map_or(&transition.target, |s| &s.initial);
        if !transition.internal && !self.states.contains(target) {
            report(
                DiagnosticLevel::Error,
                format!("unknown target `{}`", transition.target),
            );
        }
        if transition.sources.is_empty() {
            report(
                DiagnosticLevel::Error,
                format!("event `{}` has an empty source set", event.name),
            );
        }
        target
    }

    /// Source coverage and ambiguity of one transition; records its edges.
    fn validate_transition_sources<'a>(
        &'a self,
        event: &'a EventSchema,
        transition: &'a TransitionSchema,
        target: &'a String,
        coverage: &mut EventCoverage<'a>,
        edges: &mut Vec<(&'a String, &'a String)>,
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) {
        for source in &transition.sources {
            if event.hierarchical && !coverage.declared_scopes.insert(source) {
                report(
                    DiagnosticLevel::Error,
                    format!("ambiguous hierarchical scope `{source}`"),
                );
            }
            let leaves = self.leaves(source);
            if leaves.is_empty() {
                report(DiagnosticLevel::Error, format!("unknown source `{source}`"));
            }
            for leaf in leaves {
                if let Some(scope) = &event.completion
                    && !self.completed_scopes(leaf).contains(&scope)
                {
                    continue;
                }
                if coverage.fallbacks.contains(leaf) {
                    report(
                        DiagnosticLevel::Error,
                        format!("fallback must be last for `{}` from `{leaf}`", event.name),
                    );
                }
                if transition.fallback {
                    coverage.fallbacks.insert(leaf);
                }
                if !coverage.sources.insert(leaf) && !event.branching && !event.hierarchical {
                    report(
                        DiagnosticLevel::Error,
                        format!("ambiguous event `{}` from `{leaf}`", event.name),
                    );
                }
                if transition.history.is_some() {
                    for history_target in self.leaves(&transition.target) {
                        edges.push((leaf, history_target));
                    }
                } else {
                    edges.push((leaf, if transition.internal { leaf } else { target }));
                }
            }
        }
    }

    /// Final states must be declared and must not have outgoing transitions.
    fn validate_final_states(
        &self,
        names: &BTreeSet<&String>,
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) {
        validate_scopes(self.final_states.iter(), names, "final state", report);
        for state in &self.final_states {
            if self.events.iter().any(|event| {
                event
                    .transitions
                    .iter()
                    .any(|transition| transition.sources.contains(state))
            }) {
                report(
                    DiagnosticLevel::Error,
                    format!("final state `{state}` has outgoing transitions"),
                );
            }
        }
    }

    /// Unreachable and dead-end leaves over the structural edge set.
    fn lint_reachability(
        &self,
        edges: &[(&String, &String)],
        report: &mut impl FnMut(DiagnosticLevel, String),
    ) {
        let finals: BTreeSet<_> = self.final_states.iter().collect();
        let mut reachable = BTreeSet::from([&self.initial]);
        loop {
            let before = reachable.len();
            for &(source, target) in edges {
                if reachable.contains(source) {
                    reachable.insert(target);
                }
            }
            if reachable.len() == before {
                break;
            }
        }
        for state in &self.states {
            if !reachable.contains(state) {
                report(
                    DiagnosticLevel::Warning,
                    format!("unreachable state `{state}`"),
                );
            }
            if !finals.contains(state) && !edges.iter().any(|(source, _)| *source == state) {
                report(
                    DiagnosticLevel::Warning,
                    format!("dead-end state `{state}`"),
                );
            }
        }
    }

    /// Render the state machine as a Mermaid state diagram.
    #[must_use]
    pub fn to_mermaid(&self) -> String {
        alloc::string::ToString::to_string(&self.mermaid())
    }

    /// The Mermaid diagram as a [`Display`][core::fmt::Display] value, so a
    /// caller can write it straight into its own formatter.
    #[must_use]
    pub const fn mermaid(&self) -> Mermaid<'_> {
        Mermaid(self)
    }

    /// Render the state machine as JSON.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// Render the state machine as pretty-printed JSON.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"))
    }
}

#[cfg(all(test, feature = "std"))]
mod tests;

/// A [`MachineSchema`] rendered as a Mermaid state diagram, from
/// [`MachineSchema::mermaid`].
#[derive(Debug, Clone, Copy)]
pub struct Mermaid<'a>(&'a MachineSchema);

impl core::fmt::Display for Mermaid<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let schema = self.0;
        writeln!(f, "stateDiagram-v2")?;
        if !schema.regions.is_empty() {
            for (index, region) in schema.regions.iter().enumerate() {
                if index > 0 {
                    writeln!(f, "    --")?;
                }
                writeln!(f, "    state {} {{", region.name)?;
                writeln!(f, "        [*] --> {}", region.machine)?;
                writeln!(f, "    }}")?;
            }
            return Ok(());
        }

        writeln!(f, "    [*] --> {}", schema.initial)?;
        for state in &schema.final_states {
            writeln!(f, "    {state} --> [*]")?;
        }

        for event in &schema.events {
            for transition in &event.transitions {
                for source in &transition.sources {
                    let target = if transition.internal {
                        source
                    } else {
                        &transition.target
                    };
                    write!(f, "    {source} --> {target} : {}", event.name)?;
                    if let Some((first, rest)) = transition.guards.split_first() {
                        write!(f, " [{first}")?;
                        for guard in rest {
                            write!(f, ", {guard}")?;
                        }
                        f.write_char(']')?;
                    }
                    if transition.internal {
                        f.write_str(" (internal)")?;
                    }
                    f.write_char('\n')?;
                }
            }
        }

        for superstate in &schema.superstates {
            writeln!(f)?;
            writeln!(f, "    state {} {{", superstate.name)?;
            writeln!(f, "        [*] --> {}", superstate.initial)?;
            for descendant in &superstate.descendants {
                writeln!(f, "        {descendant}")?;
            }
            writeln!(f, "    }}")?;
        }

        Ok(())
    }
}
