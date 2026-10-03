//! Ordered guard selection with typed outcome enums. Selection evaluates
//! guards once, and only the selected candidate runs transition callbacks.

use super::utils::{ctx_generics, event_pascal, machine_params, maybe_async, maybe_await};
use crate::types::{Hooks, StateMachine, TransitionEdge};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Result};

pub fn groups<'a>(machine: &'a StateMachine, state: &Ident) -> Vec<Vec<&'a TransitionEdge>> {
    let mut groups: Vec<Vec<&TransitionEdge>> = Vec::new();
    for edge in machine
        .transition_graph
        .outgoing(state)
        .into_iter()
        .flatten()
    {
        if let Some(group) = groups.iter_mut().find(|group| group[0].event == edge.event) {
            group.push(edge);
        } else {
            groups.push(vec![edge]);
        }
    }
    groups
}

pub fn outcome_name(machine: &StateMachine, source: &Ident, event: &Ident) -> Ident {
    format_ident!("{}{}{}Outcome", machine.name, source, event_pascal(event))
}

pub fn targets<'a>(edges: &[&'a TransitionEdge]) -> Vec<&'a Ident> {
    let mut targets = Vec::new();
    for edge in edges {
        if !targets.contains(&&edge.target) {
            targets.push(&edge.target);
        }
    }
    targets
}

pub fn enums(machine: &StateMachine) -> TokenStream {
    let generics = ctx_generics(machine);
    let name = &machine.name;
    let enums = machine.states.iter().flat_map(|source| {
        groups(machine, source)
            .into_iter()
            .filter(|edges| edges.len() > 1)
            .map(|edges| {
                let outcome = outcome_name(machine, source, &edges[0].event);
                let variants = targets(&edges)
                    .into_iter()
                    .map(|target| {
                        let params = machine_params(machine, target);
                        quote! { #target(#name #params) }
                    })
                    .collect::<Vec<_>>();
                quote! {
                    #[derive(Debug)]
                    pub enum #outcome #generics { #( #variants, )* }
                }
            })
    });
    quote! { #( #enums )* }
}

fn condition(machine: &StateMachine, hooks: &Hooks, payload: &TokenStream) -> TokenStream {
    let await_ = maybe_await(machine.async_mode);
    let guards = hooks
        .guards
        .iter()
        .map(|guard| quote! { self.#guard(&self.ctx #payload) #await_ });
    let unless = hooks
        .unless
        .iter()
        .map(|guard| quote! { !self.#guard(&self.ctx #payload) #await_ });
    quote! { true #( && #guards )* #( && #unless )* }
}

pub fn methods(
    machine: &StateMachine,
    source: &Ident,
    edges: &[&TransitionEdge],
) -> Result<TokenStream> {
    let event = machine
        .events
        .iter()
        .find(|event| event.name == edges[0].event)
        .unwrap();
    let name = &event.name;
    let outcome = outcome_name(machine, source, name);
    let generics = ctx_generics(machine);
    let can_name = format_ident!("can_{}", name);
    let async_ = maybe_async(machine.async_mode);
    let await_ = maybe_await(machine.async_mode);
    let (param, can_param, payload_ref, payload_arg) =
        event
            .payload
            .as_ref()
            .map_or((quote! {}, quote! {}, quote! {}, quote! {}), |ty| {
                (
                    quote! { , payload: #ty },
                    quote! { , payload: &#ty },
                    quote! { , &payload },
                    quote! { payload },
                )
            });
    let can_payload = if event.payload.is_some() {
        quote! { , payload }
    } else {
        quote! {}
    };
    let error_ty = machine.error.as_ref().map_or(
        quote! { ::state_machines::core::GuardError },
        |ty| quote! { ::state_machines::EventError<#ty> },
    );
    let failure = |error: TokenStream| {
        let error = if machine.error.is_some() {
            quote! { ::state_machines::EventError::Guard(#error) }
        } else {
            error
        };
        let globals = machine.callbacks.on_error.iter().filter(|cb| {
            cb.to.is_none() && cb.matches(&machine.hierarchy, &machine.states, source, source, name)
        });
        let hooks = globals.map(|cb| &cb.name).chain(&event.hooks.on_error);
        let calls = hooks.map(|cb| quote! { let (): () = self.#cb(&error) #await_; });
        quote! {
            let error = #error;
            #( #calls )*
            return Err((self, error));
        }
    };
    let event_guards = event.hooks.guards.iter().map(|guard| (guard, quote! { ! }))
        .chain(event.hooks.unless.iter().map(|guard| (guard, quote! {}))).map(|(guard, negation)| {
            let fail = failure(quote! { ::state_machines::core::GuardError::new(stringify!(#guard), stringify!(#name)) });
            quote! { if #negation self.#guard(&self.ctx #payload_ref) #await_ { #fail } }
        });
    let mut helpers = Vec::new();
    let mut choices = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        let helper = format_ident!("__sm_{}_{}", name, index);
        helpers.push(super::typestate::generate_transition_method(
            machine,
            source,
            edge,
            Some(&helper),
            false,
        )?);
        let enabled = condition(machine, &edge.selection, &payload_ref);
        let target = &edge.target;
        choices.push(quote! {
            if #enabled {
                return self.#helper(#payload_arg) #await_.map(#outcome::#target);
            }
        });
    }
    let no_match = failure(
        quote! { ::state_machines::core::GuardError::new("branch_selection", stringify!(#name)) },
    );
    let event_enabled = condition(machine, &event.hooks, &can_payload);
    let candidates = edges
        .iter()
        .map(|edge| condition(machine, &edge.selection, &can_payload));
    Ok(quote! {
        #( #helpers )*
        pub #async_ fn #name(mut self #param) -> Result<#outcome #generics, (Self, #error_ty)> {
            #( #event_guards )*
            #( #choices )*
            #no_match
        }
        #[allow(clippy::ptr_arg)]
        pub #async_ fn #can_name(&self #can_param) -> bool {
            #event_enabled && (false #( || (#candidates) )*)
        }
    })
}
