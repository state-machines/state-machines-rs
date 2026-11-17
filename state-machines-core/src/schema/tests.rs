use super::*;
use alloc::string::ToString;
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
                    data: None,
                    kind: None,
                    sources: vec!["Pressurized".into()],
                    target: "Vacuum".into(),
                    internal: false,
                    fallback: false,
                    history: None,
                    guards: vec![],
                    unless: vec![],
                    callbacks: vec![],
                    on_error: vec![],
                }],
                guards: vec![],
                unless: vec![],
                callbacks: vec![],
                on_error: vec![],
                payload: None,
                branching: false,
                hierarchical: false,
                automatic: false,
                completion: None,
            },
            EventSchema {
                name: "repressurize".into(),
                transitions: vec![TransitionSchema {
                    data: None,
                    kind: None,
                    sources: vec!["Vacuum".into()],
                    target: "Pressurized".into(),
                    internal: false,
                    fallback: false,
                    history: None,
                    guards: vec![],
                    unless: vec![],
                    callbacks: vec![],
                    on_error: vec![],
                }],
                guards: vec![],
                unless: vec![],
                callbacks: vec![],
                on_error: vec![],
                payload: None,
                branching: false,
                hierarchical: false,
                automatic: false,
                completion: None,
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
    let errors = schema
        .validate()
        .into_iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    assert_eq!(errors, 3);
}

#[test]
fn validates_expanded_superstate_sources() {
    let mut schema = sample_schema();
    schema.superstates.push(SuperstateSchema {
        parent: None,
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

#[test]
fn callbacks_serialize_in_declaration_order_with_their_kind() {
    let mut schema = sample_schema();
    schema.events[0].callbacks = vec![
        CallbackSchema::new(CallbackKind::Before, "before1"),
        CallbackSchema::new(CallbackKind::Around, "around1"),
        CallbackSchema::new(CallbackKind::Before, "before2"),
        CallbackSchema::new(CallbackKind::Around, "around2"),
        CallbackSchema::new(CallbackKind::After, "after1"),
    ];
    let json = schema.to_json();
    assert!(json.contains(
        r#""callbacks":[{"kind":"before","name":"before1"},{"kind":"around","name":"around1"},{"kind":"before","name":"before2"},{"kind":"around","name":"around2"},{"kind":"after","name":"after1"}]"#
    ));
    let parsed: MachineSchema = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, schema);
    assert_eq!(parsed.validate(), []);
}

#[test]
fn the_old_split_callback_arrays_are_refused() {
    let json = r#"{"name":"M","initial":"A","states":["A","B"],"events":[{"name":"go",
        "transitions":[{"sources":["A"],"target":"B","before":["log"],"around":["wrap"]}]}]}"#;
    let error = serde_json::from_str::<MachineSchema>(json).unwrap_err();
    assert!(
        error.to_string().contains("unknown field `before`"),
        "{error}"
    );
}

#[test]
fn unknown_callback_kinds_are_refused() {
    let json = r#"{"name":"M","initial":"A","states":["A","B"],"events":[{"name":"go",
        "callbacks":[{"kind":"during","name":"log"}],
        "transitions":[{"sources":["A"],"target":"B"}]}]}"#;
    let error = serde_json::from_str::<MachineSchema>(json).unwrap_err();
    assert!(
        error.to_string().contains("unknown variant `during`"),
        "{error}"
    );
}

#[test]
fn unnamed_callbacks_are_errors() {
    let mut schema = sample_schema();
    schema.events[1].transitions[0].callbacks =
        vec![CallbackSchema::new(CallbackKind::Around, " ")];
    let diagnostics = schema.validate();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].level, DiagnosticLevel::Error);
    assert_eq!(
        diagnostics[0].message,
        "unnamed around callback on `repressurize`"
    );
}
