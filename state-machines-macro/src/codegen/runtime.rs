//! Compile declarations into the shared runner's lifecycle rules, not a second engine.
use crate::types::StateMachine;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(machine: &StateMachine) -> (TokenStream, TokenStream, TokenStream) {
    if machine.runtime_lifecycle.is_empty() {
        return (quote! {}, quote! {}, quote! {});
    }
    let any = format_ident!("Any{}State", machine.name);
    let event = format_ident!("{}Event", machine.name);
    let mut rules = Vec::new();
    let mut methods = Vec::new();
    for declaration in &machine.runtime_lifecycle {
        let scope = declaration.state.to_string();
        for (factory, delay) in declaration
            .after
            .iter()
            .map(|timer| (&timer.event, Some(timer.delay)))
            .chain(declaration.invoke.iter().map(|factory| (factory, None)))
        {
            let method = format_ident!("__sm_runtime_factory_{}", methods.len());
            let (ty, call) = if delay.is_some() {
                (quote! { #event }, quote! { machine.#factory() })
            } else {
                (
                    quote! { ::state_machines::runtime::ActivityFuture<#event> },
                    quote! { ::state_machines::__private::Box::pin(machine.#factory()) },
                )
            };
            let arms = machine
                .hierarchy
                .expand_state(&declaration.state, &machine.states)
                .iter()
                .map(|state| {
                    quote! { #any::#state(machine) => #call }
                });
            methods.push(quote! {
                fn #method(&self) -> #ty {
                    match self.inner.as_ref().expect("active runtime scope") {
                        #(#arms,)*
                        _ => unreachable!("factory invoked outside its declared scope"),
                    }
                }
            });
            rules.push(if let Some(delay) = delay {
                quote! { ::state_machines::runtime::LifecycleRule::After { scope: #scope, delay: #delay, event: Self::#method } }
            } else {
                quote! { ::state_machines::runtime::LifecycleRule::Invoke { scope: #scope, start: Self::#method } }
            });
        }
        for deferred in &declaration.defer {
            let variant = super::utils::event_pascal(deferred);
            let payload = machine
                .event(deferred)
                .payload
                .as_ref()
                .map(|_| quote! { (..) });
            rules.push(quote! {
                ::state_machines::runtime::LifecycleRule::Defer {
                    scope: #scope, matches: |event| matches!(event, #event::#variant #payload)
                }
            });
        }
    }
    (
        quote! { ::state_machines::__sm_require_runtime!(); },
        quote! { ::state_machines::__sm_if_runtime! { #(#methods)* } },
        quote! {
            fn runtime_rules() -> ::state_machines::__private::Vec<::state_machines::runtime::LifecycleRule<Self>> {
                ::state_machines::__private::vec![#(#rules),*]
            }
        },
    )
}
