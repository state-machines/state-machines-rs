//! Serializable schema types for state machine introspection.
//!
//! Generated machines expose these through `schema()` (and the
//! [`Inspectable`] trait) when the `inspect` feature is enabled, allowing
//! serialization to JSON, Mermaid, and other formats.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::{collections::BTreeSet, format};
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
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
    pub final_states: Vec<String>,
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
    pub name: String,
    pub descendants: Vec<String>,
    pub initial: String,
}

/// Serializable representation of an event.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventSchema {
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
    pub sources: Vec<String>,
    pub target: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub fallback: bool,
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
    /// Validate references and determinism, and lint structural reachability.
    ///
    /// Dead ends and unreachable states are warnings: both can be intentional.
    /// Guard-dependent reachability cannot be decided from a schema.
    pub fn validate(&self) -> Vec<SchemaDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut report = |level, message| {
            diagnostics.push(SchemaDiagnostic { level, message });
        };
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
        let mut lifecycle_states = BTreeSet::new();
        for hooks in &self.lifecycle {
            if !names.contains(&hooks.state) || !lifecycle_states.insert(&hooks.state) {
                report(
                    DiagnosticLevel::Error,
                    format!("invalid or duplicate lifecycle `{}`", hooks.state),
                );
            }
        }
        let expand = |name: &String| -> Vec<&String> {
            if let Some(superstate) = self.superstates.iter().find(|s| &s.name == name) {
                superstate.descendants.iter().collect()
            } else {
                self.states.iter().filter(|s| *s == name).collect()
            }
        };
        let mut events = BTreeSet::new();
        let mut edges = Vec::new();
        for event in &self.events {
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
            let mut sources = BTreeSet::new();
            let mut fallbacks = BTreeSet::new();
            for transition in &event.transitions {
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
                for source in &transition.sources {
                    let leaves = expand(source);
                    if leaves.is_empty() {
                        report(DiagnosticLevel::Error, format!("unknown source `{source}`"));
                    }
                    for leaf in leaves {
                        if fallbacks.contains(leaf) {
                            report(
                                DiagnosticLevel::Error,
                                format!("fallback must be last for `{}` from `{leaf}`", event.name),
                            );
                        }
                        if transition.fallback {
                            fallbacks.insert(leaf);
                        }
                        if !sources.insert(leaf) && !event.branching {
                            report(
                                DiagnosticLevel::Error,
                                format!("ambiguous event `{}` from `{leaf}`", event.name),
                            );
                        }
                        edges.push((leaf, if transition.internal { leaf } else { target }));
                    }
                }
            }
        }
        let mut finals = BTreeSet::new();
        for state in &self.final_states {
            if !self.states.contains(state) || !finals.insert(state) {
                report(
                    DiagnosticLevel::Error,
                    format!("invalid or duplicate final state `{state}`"),
                );
            }
            if edges.iter().any(|(source, _)| *source == state) {
                report(
                    DiagnosticLevel::Error,
                    format!("final state `{state}` has outgoing transitions"),
                );
            }
        }
        let mut reachable = BTreeSet::from([&self.initial]);
        loop {
            let before = reachable.len();
            for &(source, target) in &edges {
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
        diagnostics
    }

    /// Render the state machine as a Mermaid state diagram.
    pub fn to_mermaid(&self) -> String {
        use alloc::fmt::Write;
        let mut out = String::new();

        writeln!(out, "stateDiagram-v2").unwrap();

        // Initial state
        writeln!(out, "    [*] --> {}", self.initial).unwrap();
        for state in &self.final_states {
            writeln!(out, "    {state} --> [*]").unwrap();
        }

        // Collect transitions
        for event in &self.events {
            for transition in &event.transitions {
                for source in &transition.sources {
                    let label = if transition.guards.is_empty() {
                        event.name.clone()
                    } else {
                        alloc::format!("{} [{}]", event.name, transition.guards.join(", "))
                    };
                    let target = if transition.internal {
                        source
                    } else {
                        &transition.target
                    };
                    let label = if transition.internal {
                        format!("{label} (internal)")
                    } else {
                        label
                    };
                    writeln!(out, "    {source} --> {target} : {label}").unwrap();
                }
            }
        }

        // Render superstates if present
        for superstate in &self.superstates {
            writeln!(out).unwrap();
            writeln!(out, "    state {} {{", superstate.name).unwrap();
            writeln!(out, "        [*] --> {}", superstate.initial).unwrap();
            for descendant in &superstate.descendants {
                writeln!(out, "        {}", descendant).unwrap();
            }
            writeln!(out, "    }}").unwrap();
        }

        out
    }

    /// Render the state machine as JSON.
    #[cfg(feature = "std")]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// Render the state machine as pretty-printed JSON.
    #[cfg(feature = "std")]
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"))
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use alloc::vec;

    fn sample_schema() -> MachineSchema {
        MachineSchema {
            name: "Airlock".into(),
            initial: "Pressurized".into(),
            states: vec!["Pressurized".into(), "Vacuum".into()],
            superstates: vec![],
            events: vec![
                EventSchema {
                    name: "depressurize".into(),
                    transitions: vec![TransitionSchema {
                        sources: vec!["Pressurized".into()],
                        target: "Vacuum".into(),
                        internal: false,
                        fallback: false,
                        guards: vec![],
                        unless: vec![],
                        before: vec![],
                        after: vec![],
                        around: vec![],
                        on_error: vec![],
                    }],
                    guards: vec![],
                    unless: vec![],
                    before: vec![],
                    after: vec![],
                    around: vec![],
                    on_error: vec![],
                    payload: None,
                    branching: false,
                },
                EventSchema {
                    name: "repressurize".into(),
                    transitions: vec![TransitionSchema {
                        sources: vec!["Vacuum".into()],
                        target: "Pressurized".into(),
                        internal: false,
                        fallback: false,
                        guards: vec![],
                        unless: vec![],
                        before: vec![],
                        after: vec![],
                        around: vec![],
                        on_error: vec![],
                    }],
                    guards: vec![],
                    unless: vec![],
                    before: vec![],
                    after: vec![],
                    around: vec![],
                    on_error: vec![],
                    payload: None,
                    branching: false,
                },
            ],
            async_mode: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_mermaid_output() {
        let schema = sample_schema();
        let mermaid = schema.to_mermaid();

        assert!(mermaid.contains("stateDiagram-v2"));
        assert!(mermaid.contains("[*] --> Pressurized"));
        assert!(mermaid.contains("Pressurized --> Vacuum : depressurize"));
        assert!(mermaid.contains("Vacuum --> Pressurized : repressurize"));
    }

    #[test]
    fn test_json_roundtrip() {
        let schema = sample_schema();
        let json = schema.to_json();
        let parsed: MachineSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(schema, parsed);
    }

    #[test]
    fn graph_validation() {
        let mut schema = sample_schema();
        assert_eq!(schema.validate(), []);
        schema.states.push("Unused".into());
        let warnings = schema.validate();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|d| d.level == DiagnosticLevel::Warning));

        let duplicate = schema.events[0].transitions[0].clone();
        schema.events[0].transitions.push(duplicate);
        schema.events[1].transitions[0].target = "Missing".into();
        schema.initial = "Missing".into();
        let errors: Vec<_> = schema
            .validate()
            .into_iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .collect();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn validates_expanded_superstate_sources() {
        let mut schema = sample_schema();
        schema.superstates.push(SuperstateSchema {
            name: "Air".into(),
            descendants: schema.states.clone(),
            initial: schema.initial.clone(),
        });
        schema.events[0].transitions[0].sources = vec!["Air".into()];
        schema.events[0].transitions.push(TransitionSchema {
            sources: vec!["Vacuum".into()],
            target: "Air".into(),
            ..Default::default()
        });
        let diagnostics = schema.validate();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].level, DiagnosticLevel::Error);
        assert!(diagnostics[0].message.contains("ambiguous"));
        schema.events[0].branching = true;
        schema.events[0].transitions[0].guards = vec!["ready".into()];
        schema.events[0].transitions[1].fallback = true;
        assert_eq!(schema.validate(), []);
    }
}
