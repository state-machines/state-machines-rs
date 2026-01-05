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
/// This generates code wrapped in `#[cfg(feature = "inspect")]` so it only
/// compiles when the user has enabled the inspect feature.
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
                    name: __sm_alloc::string::String::from(#name_str),
                    descendants: __sm_alloc::vec![
                        #( __sm_alloc::string::String::from(#descendants_strs), )*
                    ],
                    initial: __sm_alloc::string::String::from(#initial_str),
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
            let guards: Vec<String> = event.guards.iter().map(|g| g.to_string()).collect();

            // Generate transition schemas for this event
            let transition_schemas: Vec<TokenStream2> = event
                .transitions
                .iter()
                .map(|trans| {
                    let sources: Vec<String> =
                        trans.sources.iter().map(|s| s.to_string()).collect();
                    let target_str = trans.target.to_string();
                    let trans_guards: Vec<String> =
                        trans.guards.iter().map(|g| g.to_string()).collect();
                    let unless_guards: Vec<String> =
                        trans.unless.iter().map(|g| g.to_string()).collect();

                    quote! {
                        ::state_machines::TransitionSchema {
                            sources: __sm_alloc::vec![
                                #( __sm_alloc::string::String::from(#sources), )*
                            ],
                            target: __sm_alloc::string::String::from(#target_str),
                            guards: __sm_alloc::vec![
                                #( __sm_alloc::string::String::from(#trans_guards), )*
                            ],
                            unless: __sm_alloc::vec![
                                #( __sm_alloc::string::String::from(#unless_guards), )*
                            ],
                        }
                    }
                })
                .collect();

            let payload_expr = if let Some(payload) = &event.payload {
                let payload_str = quote!(#payload).to_string();
                quote! { ::core::option::Option::Some(__sm_alloc::string::String::from(#payload_str)) }
            } else {
                quote! { ::core::option::Option::None }
            };

            quote! {
                ::state_machines::EventSchema {
                    name: __sm_alloc::string::String::from(#event_name),
                    transitions: __sm_alloc::vec![
                        #( #transition_schemas, )*
                    ],
                    guards: __sm_alloc::vec![
                        #( __sm_alloc::string::String::from(#guards), )*
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
        #[cfg(feature = "inspect")]
        const _: () = {
            extern crate alloc as __sm_alloc;

            impl #impl_generics #machine_name #type_params {
                /// Returns the schema describing this state machine.
                ///
                /// This provides introspection into the machine's states, events,
                /// and transitions for visualization or debugging purposes.
                pub fn schema() -> ::state_machines::MachineSchema {
                    ::state_machines::MachineSchema {
                        name: __sm_alloc::string::String::from(#machine_name_str),
                        initial: __sm_alloc::string::String::from(#initial_str),
                        states: __sm_alloc::vec![
                            #( __sm_alloc::string::String::from(#state_strs), )*
                        ],
                        superstates: __sm_alloc::vec![
                            #( #superstate_schemas, )*
                        ],
                        events: __sm_alloc::vec![
                            #( #event_schemas, )*
                        ],
                        async_mode: #async_mode,
                    }
                }
            }
        };
    })
}
