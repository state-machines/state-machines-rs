use super::*;
use quote::quote;

fn validate(tokens: proc_macro2::TokenStream) -> Result<()> {
    syn::parse2::<StateMachine>(tokens)?.validate()
}

#[test]
fn rejects_ambiguous_graphs() {
    for events in [
        quote! { go { transition: { from: A, to: B } } go { transition: { from: B, to: A } } },
        quote! { go { transition: { from: A, to: B } transition: { from: A, to: A } } },
    ] {
        let err = validate(quote! {
            name: Test, initial: A, states: [A, B], events { #events }
        })
        .unwrap_err();
        assert!(err.to_string().contains("duplicate") || err.to_string().contains("ambiguous"));
    }
}

#[test]
fn internal_transitions_omit_targets() {
    validate(quote! {
        name: Test, initial: A, states: [A, B],
        events { ping { transition: { from: [A, B], internal: true } } }
    })
    .unwrap();
    let err = validate(quote! {
        name: Test, initial: A, states: [A],
        events { ping { transition: { from: A, to: A, internal: true } } }
    })
    .unwrap_err();
    assert!(err.to_string().contains("omit `to`"));
}

#[test]
fn validates_final_states() {
    for (finals, events) in [
        (quote! { [Missing] }, quote! {}),
        (quote! { [A, A] }, quote! {}),
        (
            quote! { [A] },
            quote! { go { transition: { from: A, to: B } } },
        ),
    ] {
        let err = validate(quote! {
            name: Test, initial: A, states: [A, B],
            final_states: #finals, events { #events }
        })
        .unwrap_err();
        assert!(err.to_string().contains("final"));
    }
    let err = validate(quote! {
        name: Test, initial: A, states: [superstate Parent { state A, state B }, C],
        final_states: [B, Parent],
        events { exit { transition: { from: Parent, to: C } } }
    })
    .unwrap_err();
    assert!(err.to_string().contains("own outgoing"));
}

#[test]
fn validates_branch_fallbacks() {
    for transitions in [
        quote! { transition: { from: A, to: B } transition: { from: A, to: A, guards: [ready] } },
        quote! { transition: { from: A, to: B, fallback: true } transition: { from: A, to: A, guards: [ready] } },
        quote! { transition: { from: A, to: B, fallback: true, guards: [ready] } },
    ] {
        let err = validate(quote! {
            name: Test, initial: A, states: [A, B],
            events { go { branching: true, #transitions } }
        })
        .unwrap_err();
        assert!(err.to_string().contains("fallback"));
    }
}

#[test]
fn validates_history_targets() {
    let err = validate(quote! {
        name: Test, initial: A, states: [A, B],
        events { resume { transition: { from: A, to: B, history: deep } } }
    })
    .unwrap_err();
    assert!(err.to_string().contains("superstate"));
    let err = validate(quote! {
        name: Test, initial: A, states: [A, superstate Region { state B, state C }],
        events { resume { transition: { from: A, to: Region, history: forever } } }
    })
    .unwrap_err();
    assert!(err.to_string().contains("shallow"));
}

#[test]
fn rejects_event_named_schema() {
    let err = validate(quote! {
        name: Doc,
        initial: Draft,
        states: [Draft, Published],
        events {
            schema { transition: { from: Draft, to: Published } }
        }
    })
    .unwrap_err();

    assert!(err.to_string().contains("`schema` is reserved"), "{err}");
}

#[test]
fn rejects_duplicate_states() {
    let top_level = validate(quote! {
        name: Doc,
        initial: Draft,
        states: [Draft, Draft],
    })
    .unwrap_err();
    assert!(
        top_level.to_string().contains("duplicate state"),
        "{top_level}"
    );

    let nested = validate(quote! {
        name: Doc,
        initial: Draft,
        states: [Draft, superstate Review { state Draft }],
    })
    .unwrap_err();
    assert!(nested.to_string().contains("duplicate state"), "{nested}");
}

#[test]
fn accepts_event_names_containing_schema() {
    validate(quote! {
        name: Doc,
        initial: Draft,
        states: [Draft, Published],
        events {
            publish_schema { transition: { from: Draft, to: Published } }
        }
    })
    .unwrap();
}

#[test]
fn rejects_unsupported_automatic_payloads_and_completion_scopes() {
    for (event, expected) in [
        (
            quote! { go { automatic: true, payload: u32, transition: { from: A, to: B } } },
            "payload",
        ),
        (
            quote! { go { completion: A, transition: { from: A, to: B } } },
            "superstate",
        ),
        (
            quote! { go { completion: Region, transition: { from: A, to: B } } },
            "scope",
        ),
        (
            quote! { go { completion: Region, transition: { from: Region, internal: true } } },
            "scope",
        ),
    ] {
        let error = validate(quote! {
            name: Test, initial: A,
            states: [superstate Region { state A }, B],
            events { #event }
        })
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn rejects_ambiguous_hierarchy_policies_and_invalid_composite_kinds() {
    for (event, expected) in [
        (
            quote! { go { hierarchical: true, branching: true, transition: { from: A, to: B } } },
            "either",
        ),
        (
            quote! { go { hierarchical: true,
                transition: { from: A, to: B, guards: [one] }
                transition: { from: A, to: B, guards: [two] }
            } },
            "same scope",
        ),
        (
            quote! { go { transition: { from: A, to: B, kind: local } } },
            "inside",
        ),
        (
            quote! { go { transition: { from: Region, to: B, kind: local } } },
            "inside",
        ),
        (
            quote! { go { transition: { from: A, to: B, kind: unknown } } },
            "kind",
        ),
        (
            quote! { go { transition: { from: A, kind: external, internal: true } } },
            "conflicts",
        ),
    ] {
        let error = validate(quote! {
            name: Test, initial: A,
            states: [superstate Region { state A }, B],
            events { #event }
        })
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn factories_must_target_data_leaves_and_startup_name_is_reserved() {
    for event in [
        quote! { go { transition: { from: A, to: B, data: own } } },
        quote! { go { transition: { from: A, internal: true, data: own } } },
        quote! { go { transition: { from: B, to: Region, data: own } } },
    ] {
        let error = validate(quote! {
            name: Test, initial: A,
            states: [superstate Region { state A }, B],
            events { #event }
        })
        .unwrap_err();
        assert!(error.to_string().contains("data"), "{error}");
    }
    let error = validate(quote! {
        name: Test, initial: A, states: [A],
        events { initialize { transition: { from: A, internal: true } } }
    })
    .unwrap_err();
    assert!(error.to_string().contains("reserved"), "{error}");
}

#[test]
fn validates_runtime_declarations() {
    for (runtime, expected) in [
        (quote! { Missing { invoke: [work] } }, "scope"),
        (quote! { A { defer: [missing] } }, "event"),
        (quote! { A { defer: [go, go] } }, "duplicate"),
        (
            quote! { A { invoke: [work] }, A { invoke: [work] } },
            "duplicate",
        ),
        (quote! { A { after: [{event: timer}] } }, "delay"),
    ] {
        let error = validate(quote! {
            name: Test, dynamic: true, initial: A, states: [A, B],
            runtime: { #runtime },
            events { go { transition: { from: A, to: B } } }
        })
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
    let error = validate(quote! {
        name: Test, dynamic: true, initial: A, states: [A, B],
        runtime: { A { defer: [go] } },
        events { go { automatic: true, transition: { from: A, to: B } } }
    })
    .unwrap_err();
    assert!(error.to_string().contains("external"), "{error}");
}
