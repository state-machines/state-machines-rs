//! Code generation for the `Inspectable` trait implementation.
//!
//! Generates introspection capabilities for state machines, allowing them to
//! provide their schema as JSON or Mermaid diagrams at runtime.

use crate::codegen::utils::machine_params;
use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Result;

/// A `vec![String::from(..), ..]` literal over `items`' display forms.
fn string_vec<T: ToString>(items: impl IntoIterator<Item = T>) -> TokenStream2 {
    let strs = items.into_iter().map(|item| item.to_string());
    quote! {
        ::state_machines::__private::vec![
            #( ::state_machines::__private::String::from(#strs), )*
        ]
    }
}

/// The hook-list fields shared by `EventSchema` and `TransitionSchema`.
fn hook_schema_fields(hooks: &Hooks) -> TokenStream2 {
    let guards = string_vec(&hooks.guards);
    let unless = string_vec(&hooks.unless);
    let before = string_vec(&hooks.before);
    let after = string_vec(&hooks.after);
    let around = string_vec(&hooks.around);
    let on_error = string_vec(&hooks.on_error);
    quote! {
        guards: #guards,
        unless: #unless,
        before: #before,
        after: #after,
        around: #around,
        on_error: #on_error,
    }
}

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

    let states = string_vec(&machine.states);

    // Generate superstate schemas from the hierarchy lookup table
    let superstate_schemas: Vec<TokenStream2> = machine
        .hierarchy
        .lookup
        .iter()
        .map(|(name, descendants)| {
            let initial_str = machine
                .hierarchy
                .initial_children
                .get(name)
                .or(descendants.first())
                .map(|i| i.to_string())
                .unwrap_or_default();
            let descendants = string_vec(descendants);

            quote! {
                ::state_machines::SuperstateSchema {
                    name: ::state_machines::__private::String::from(#name),
                    descendants: #descendants,
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
            let event_hooks = hook_schema_fields(&event.hooks);

            // Generate transition schemas for this event
            let transition_schemas: Vec<TokenStream2> = event
                .transitions
                .iter()
                .map(|trans| {
                    let sources = string_vec(&trans.sources);
                    let target_str = trans.target.to_string();
                    let trans_hooks = hook_schema_fields(&trans.hooks);

                    quote! {
                        ::state_machines::TransitionSchema {
                            sources: #sources,
                            target: ::state_machines::__private::String::from(#target_str),
                            #trans_hooks
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
                    #event_hooks
                    payload: #payload_expr,
                }
            }
        })
        .collect();

    let async_mode = machine.async_mode;

    // Generate a schema() function that's callable on the machine type.
    // We generate an impl block for all generic parameters that provides schema().
    // This allows calling Airlock::schema() regardless of the type parameters.
    let params = machine_params(machine, quote! { S });

    Ok(quote! {
        ::state_machines::__sm_if_inspect! {
            impl #params #machine_name #params {
                /// Returns the schema describing this state machine.
                ///
                /// This provides introspection into the machine's states, events,
                /// and transitions for visualization or debugging purposes.
                pub fn schema() -> ::state_machines::MachineSchema {
                    ::state_machines::MachineSchema {
                        name: ::state_machines::__private::String::from(#machine_name_str),
                        initial: ::state_machines::__private::String::from(#initial_str),
                        states: #states,
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

            impl #params ::state_machines::Inspectable for #machine_name #params {
                fn schema() -> ::state_machines::MachineSchema {
                    // Delegates to the inherent method above (inherent
                    // methods win over trait methods in path resolution).
                    <#machine_name #params>::schema()
                }
            }
        }
    })
}
