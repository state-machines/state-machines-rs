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
