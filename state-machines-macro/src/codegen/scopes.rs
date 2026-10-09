//! Scope visit generations use the same transition domain as lifecycle hooks.
use crate::types::{StateMachine, TransitionEdge};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

pub fn names(machine: &StateMachine) -> Vec<String> {
    let mut names: Vec<_> = machine
        .states
        .iter()
        .map(ToString::to_string)
        .chain(machine.hierarchy.all_superstates().map(ToString::to_string))
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn commit(machine: &StateMachine, source: &Ident, edge: &TransitionEdge) -> TokenStream {
    if edge.internal {
        return quote! {};
    }
    let names = names(machine);
    let path = machine.active_path(source);
    let prefix = machine.retained_prefix(source, &edge.target, edge);
    let exited = path[prefix..].iter().map(|scope| {
        names
            .iter()
            .position(|name| scope == name)
            .expect("active paths only contain declared scopes")
    });
    quote! { self.__sm_commit(&[#(#exited),*]); }
}

pub fn methods(machine: &StateMachine) -> TokenStream {
    let state = quote::format_ident!("{}State", machine.name);
    let cases = names(machine).into_iter().enumerate().map(|(index, name)| {
        let active: Vec<_> = machine.states.iter().filter(|leaf| machine.active_path(leaf).iter().any(|scope| scope == &name)).collect();
        if active.is_empty() { return quote! {}; }
        quote! { #name if matches!(self.last_state, #(#state::#active)|*) => Some(self.scope_epochs[#index]), }
    });
    quote! {
        /// Generation of an active leaf or composite visit; None when inactive/poisoned.
        pub fn scope_epoch(&self, scope: &str) -> Option<u64> {
            if self.is_poisoned() { return None; }
            match scope { #(#cases)* _ => None }
        }
        fn __sm_commit(&mut self, exited: &[usize]) {
            self.epoch = self.epoch.wrapping_add(1);
            for index in exited { self.scope_epochs[*index] = self.scope_epochs[*index].wrapping_add(1); }
        }
        fn __sm_record_completions(&mut self, previous_epoch: u64) {
            if previous_epoch != self.epoch {
                self.completions.extend_from_slice(self.completion_events());
            }
        }
    }
}
