//! Final-state queries are compile-time constants in typestate mode.

use crate::types::StateMachine;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

pub fn parent<'a>(machine: &'a StateMachine, state: &Ident) -> Option<&'a Ident> {
    machine
        .hierarchy
        .ancestors
        .get(&state.to_string())
        .and_then(|path| path.last())
}

pub fn state_methods(machine: &StateMachine, state: &Ident) -> TokenStream {
    let final_state = machine.final_states.contains(state);
    let parent = parent(machine, state);
    let finished = final_state && parent.is_none();
    let events = if !final_state {
        quote! { &[] }
    } else if let Some(parent) = parent {
        let name = parent.to_string();
        quote! { &[::state_machines::CompletionEvent::Superstate(#name)] }
    } else {
        quote! { &[::state_machines::CompletionEvent::Machine] }
    };
    quote! {
        /// Whether the root machine reached a final state.
        pub const fn is_finished(&self) -> bool { #finished }

        /// Completion signals represented by the current state (not a queue).
        pub const fn completion_events(&self) -> &'static [::state_machines::CompletionEvent] {
            #events
        }
    }
}
