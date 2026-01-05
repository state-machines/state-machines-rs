//! Serializable schema types for state machine introspection.
//!
//! These types are decoupled from the generic `MachineDefinition<S>` types,
//! allowing serialization to JSON, Mermaid, and other formats.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

/// Serializable representation of a state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineSchema {
    pub name: String,
    pub initial: String,
    pub states: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub superstates: Vec<SuperstateSchema>,
    pub events: Vec<EventSchema>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub async_mode: bool,
}

/// Serializable representation of a superstate (hierarchical state).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SuperstateSchema {
    pub name: String,
    pub descendants: Vec<String>,
    pub initial: String,
}

/// Serializable representation of an event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventSchema {
    pub name: String,
    pub transitions: Vec<TransitionSchema>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
}

/// Serializable representation of a transition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransitionSchema {
    pub sources: Vec<String>,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unless: Vec<String>,
}

/// Trait for types that can provide their schema for introspection.
pub trait Inspectable {
    /// Returns the schema describing this state machine.
    fn schema() -> MachineSchema;
}

impl MachineSchema {
    /// Render the state machine as a Mermaid state diagram.
    pub fn to_mermaid(&self) -> String {
        use alloc::fmt::Write;
        let mut out = String::new();

        writeln!(out, "stateDiagram-v2").unwrap();

        // Initial state
        writeln!(out, "    [*] --> {}", self.initial).unwrap();

        // Collect transitions
        for event in &self.events {
            for transition in &event.transitions {
                for source in &transition.sources {
                    let label = if transition.guards.is_empty() {
                        event.name.clone()
                    } else {
                        alloc::format!("{} [{}]", event.name, transition.guards.join(", "))
                    };
                    writeln!(out, "    {} --> {} : {}", source, transition.target, label).unwrap();
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
                        guards: vec![],
                        unless: vec![],
                    }],
                    guards: vec![],
                    payload: None,
                },
                EventSchema {
                    name: "repressurize".into(),
                    transitions: vec![TransitionSchema {
                        sources: vec!["Vacuum".into()],
                        target: "Pressurized".into(),
                        guards: vec![],
                        unless: vec![],
                    }],
                    guards: vec![],
                    payload: None,
                },
            ],
            async_mode: false,
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
}
