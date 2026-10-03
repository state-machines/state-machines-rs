//! Final-state queries are compile-time constants in typestate mode.

use crate::types::StateMachine;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

pub fn parent<'a>(machine: &'a StateMachine, state: &Ident) -> Option<&'a Ident> {
    machine.hierarchy.parent(&state.to_string())
}

/// Bottom-up completed scopes, including the leaf itself for a root final leaf.
/// A non-final composite stops propagation after its own completion.
pub fn completed<'a>(machine: &'a StateMachine, leaf: &'a Ident) -> Vec<&'a Ident> {
    let mut scopes = Vec::new();
    let mut child = leaf;
    while machine.final_states.contains(child) {
        let scope = parent(machine, child).unwrap_or(child);
        if scopes.last() == Some(&scope) {
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

pub fn state_methods(machine: &StateMachine, state: &Ident) -> TokenStream {
    let mut scopes = completed(machine, state);
    scopes.dedup();
    let finished = scopes.last().is_some_and(|scope| {
        machine.final_states.contains(scope) && parent(machine, scope).is_none()
    });
    let events = scopes
        .iter()
        .filter(|scope| machine.hierarchy.is_superstate(scope))
        .map(|scope| {
            let name = scope.to_string();
            quote! { ::state_machines::CompletionEvent::Superstate(#name), }
        });
    let root = finished.then(|| quote! { ::state_machines::CompletionEvent::Machine, });
    quote! {
        /// Whether the root machine reached a final state.
        pub const fn is_finished(&self) -> bool { #finished }

        /// Completion signals represented by the current state (not a queue).
        pub const fn completion_events(&self) -> &'static [::state_machines::CompletionEvent] {
            &[#(#events)* #root]
        }
    }
}
