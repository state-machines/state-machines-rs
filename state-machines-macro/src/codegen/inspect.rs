//! Code generation for the `Inspectable` trait implementation.
//!
//! Generates introspection capabilities for state machines, allowing them to
//! provide their schema as JSON or Mermaid diagrams at runtime.

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Result;

/// Generate the `Inspectable` trait implementation.
///
/// The output is wrapped in `::state_machines::__sm_if_inspect!`, so it only
/// compiles when the `state-machines` crate the caller links has its `inspect`
/// feature on. A `#[cfg(feature = "inspect")]` here would test the calling
/// crate's features instead, and `cfg!` in the macro would test the host build.
pub fn generate_inspectable_impl(machine: &StateMachine) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let machine_name_str = machine_name.to_string();
    let initial_str = machine.initial.to_string();

    // Collect all state names as strings
    let state_strs: Vec<String> = machine.states.iter().map(|s| s.to_string()).collect();

    // Generate superstate schemas from the hierarchy lookup table
    let superstate_schemas: Vec<TokenStream2> = machine
        .hierarchy
        .lookup
        .iter()
        .map(|(name, descendants)| {
            let name_str = name.clone();
            let descendants_strs: Vec<String> = descendants.iter().map(|s| s.to_string()).collect();
            let initial_str = machine
                .hierarchy
                .initial_children
                .get(name)
                .map(|i| i.to_string())
                .unwrap_or_else(|| descendants_strs.first().cloned().unwrap_or_default());

            quote! {
                ::state_machines::SuperstateSchema {
                    name: ::state_machines::__private::String::from(#name_str),
                    descendants: ::state_machines::__private::vec![
                        #( ::state_machines::__private::String::from(#descendants_strs), )*
                    ],
                    initial: ::state_machines::__private::String::from(#initial_str),
                }
            }
        })
        .collect();

    // Generate event schemas
    let event_schemas: Vec<TokenStream2> = machine
        .events
        .iter()
        .map(|event| {
            let event_name = event.name.to_string();
            let to_strings =
                |idents: &[syn::Ident]| idents.iter().map(|i| i.to_string()).collect::<Vec<_>>();
            let guards = to_strings(&event.guards);
            let unless = to_strings(&event.unless);
            let before = to_strings(&event.before);
            let after = to_strings(&event.after);
            let around = to_strings(&event.around);

            // Generate transition schemas for this event
            let transition_schemas: Vec<TokenStream2> = event
                .transitions
                .iter()
                .map(|trans| {
                    let sources: Vec<String> =
                        trans.sources.iter().map(|s| s.to_string()).collect();
                    let target_str = trans.target.to_string();
                    let trans_guards = to_strings(&trans.guards);
                    let trans_unless = to_strings(&trans.unless);
                    let trans_before = to_strings(&trans.before);
                    let trans_after = to_strings(&trans.after);
                    let trans_around = to_strings(&trans.around);

                    quote! {
                        ::state_machines::TransitionSchema {
                            sources: ::state_machines::__private::vec![
                                #( ::state_machines::__private::String::from(#sources), )*
                            ],
                            target: ::state_machines::__private::String::from(#target_str),
                            guards: ::state_machines::__private::vec![
                                #( ::state_machines::__private::String::from(#trans_guards), )*
                            ],
                            unless: ::state_machines::__private::vec![
                                #( ::state_machines::__private::String::from(#trans_unless), )*
                            ],
                            before: ::state_machines::__private::vec![
                                #( ::state_machines::__private::String::from(#trans_before), )*
                            ],
                            after: ::state_machines::__private::vec![
                                #( ::state_machines::__private::String::from(#trans_after), )*
                            ],
                            around: ::state_machines::__private::vec![
                                #( ::state_machines::__private::String::from(#trans_around), )*
                            ],
                        }
                    }
                })
                .collect();

            let payload_expr = if let Some(payload) = &event.payload {
                let payload_str = quote!(#payload).to_string();
                quote! { ::core::option::Option::Some(::state_machines::__private::String::from(#payload_str)) }
            } else {
                quote! { ::core::option::Option::None }
            };

            quote! {
                ::state_machines::EventSchema {
                    name: ::state_machines::__private::String::from(#event_name),
                    transitions: ::state_machines::__private::vec![
                        #( #transition_schemas, )*
                    ],
                    guards: ::state_machines::__private::vec![
                        #( ::state_machines::__private::String::from(#guards), )*
                    ],
                    unless: ::state_machines::__private::vec![
                        #( ::state_machines::__private::String::from(#unless), )*
                    ],
                    before: ::state_machines::__private::vec![
                        #( ::state_machines::__private::String::from(#before), )*
                    ],
                    after: ::state_machines::__private::vec![
                        #( ::state_machines::__private::String::from(#after), )*
                    ],
                    around: ::state_machines::__private::vec![
                        #( ::state_machines::__private::String::from(#around), )*
                    ],
                    payload: #payload_expr,
                }
            }
        })
        .collect();

    let async_mode = machine.async_mode;

    // Generate a schema() function that's callable on the machine type.
    // We generate an impl block for all generic parameters that provides schema().
    // This allows calling Airlock::schema() regardless of the type parameters.
    let (impl_generics, type_params) = if machine.context.is_some() {
        // Concrete context: impl for Machine<S>
        (quote! { <__S> }, quote! { <__S> })
    } else {
        // Generic context: impl for Machine<C, S>
        (quote! { <__C, __S> }, quote! { <__C, __S> })
    };

    Ok(quote! {
        ::state_machines::__sm_if_inspect! {
            impl #impl_generics #machine_name #type_params {
                /// Returns the schema describing this state machine.
                ///
                /// This provides introspection into the machine's states, events,
                /// and transitions for visualization or debugging purposes.
                pub fn schema() -> ::state_machines::MachineSchema {
                    ::state_machines::MachineSchema {
                        name: ::state_machines::__private::String::from(#machine_name_str),
                        initial: ::state_machines::__private::String::from(#initial_str),
                        states: ::state_machines::__private::vec![
                            #( ::state_machines::__private::String::from(#state_strs), )*
                        ],
                        superstates: ::state_machines::__private::vec![
                            #( #superstate_schemas, )*
                        ],
                        events: ::state_machines::__private::vec![
                            #( #event_schemas, )*
                        ],
                        async_mode: #async_mode,
                    }
                }
            }

            impl #impl_generics ::state_machines::Inspectable for #machine_name #type_params {
                fn schema() -> ::state_machines::MachineSchema {
                    // Delegates to the inherent method above (inherent
                    // methods win over trait methods in path resolution).
                    <#machine_name #type_params>::schema()
                }
            }
        }
    })
}
