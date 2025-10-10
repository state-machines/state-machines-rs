//! Code generation for the state machine macro.
//!
//! The code is split across smaller modules:
//! - `event`: generation of event/can methods
//! - `callbacks`: filtering logic for global callbacks
//! - `storage`: helpers for state data storage handling

mod callbacks;
mod event;
mod storage;

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Result;

impl StateMachine {
    /// Generate the complete state machine code.
    ///
    /// This orchestrates validation, event generation, struct/enum creation,
    /// and metadata definitions.
    pub fn expand(&self) -> Result<TokenStream2> {
        self.validate()?;

        let machine_ident = &self.name;
        let state_ident = &self.state;
        let initial_state = &self.initial;
        let states = &self.states;
        let async_mode = self.async_mode;

        // Generate the state enum
        let state_variants = states.iter();
        let state_enum = quote! {
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
            pub enum #state_ident {
                #( #state_variants, )*
            }
        };

        // Generate Default implementation for the state enum
        let state_default = quote! {
            impl Default for #state_ident {
                fn default() -> Self {
                    #state_ident::#initial_state
                }
            }
        };

        // Generate storage fields and methods
        let storage_specs = &self.state_storage;
        let storage_fields: Vec<_> = storage_specs
            .iter()
            .map(|spec| {
                let field = &spec.field;
                let ty = &spec.ty;
                quote!(#field: ::core::option::Option<#ty>)
            })
            .collect();

        let storage_inits: Vec<_> = storage_specs
            .iter()
            .map(|spec| {
                let field = &spec.field;
                quote!(#field: ::core::option::Option::None)
            })
            .collect();

        let storage_getters: Vec<_> = storage_specs
            .iter()
            .map(|spec| {
                let field = &spec.field;
                let ty = &spec.ty;
                let name_str = crate::parser::to_snake_case(&spec.owner.to_string());
                let getter = format_ident!("{}_data", name_str.clone());
                let getter_mut = format_ident!("{}_data_mut", name_str);
                quote! {
                    pub fn #getter(&self) -> ::core::option::Option<& #ty> {
                        self.#field.as_ref()
                    }

                    pub fn #getter_mut(&mut self) -> ::core::option::Option<&mut #ty> {
                        self.#field.as_mut()
                    }
                }
            })
            .collect();

        // Generate the machine struct
        let machine_struct = quote! {
            #[derive(Debug)]
            pub struct #machine_ident {
                state: #state_ident,
                #( #storage_fields, )*
            }
        };

        // Generate the machine implementation with constructor and accessors
        let machine_impl = quote! {
            impl #machine_ident {
                pub fn new() -> Self {
                    Self {
                        state: #state_ident::#initial_state,
                        #( #storage_inits, )*
                    }
                }

                pub fn state(&self) -> #state_ident {
                    self.state
                }

                #( #storage_getters )*
            }
        };

        // Generate Default implementation for the machine
        let machine_default = quote! {
            impl Default for #machine_ident {
                fn default() -> Self {
                    Self::new()
                }
            }
        };

        let core_path = quote!(::state_machines::core);
        let action_method = self.action.as_ref();

        // Generate event methods and can methods
        let mut sync_event_methods = Vec::new();
        let mut sync_can_methods = Vec::new();
        let mut async_event_methods = Vec::new();
        let mut async_can_methods = Vec::new();
        let mut event_definitions = Vec::new();

        for event in &self.events {
            let method_ident = &event.name;
            let can_ident = format_ident!("can_{}", method_ident);
            let payload_ty = event.payload.as_ref();

            // Build metadata for the event definition
            let event_guard_names: Vec<_> = event
                .guards
                .iter()
                .map(|guard| quote!(stringify!(#guard)))
                .collect();
            let event_before_names: Vec<_> = event
                .before
                .iter()
                .map(|callback| quote!(stringify!(#callback)))
                .collect();
            let event_after_names: Vec<_> = event
                .after
                .iter()
                .map(|callback| quote!(stringify!(#callback)))
                .collect();
            let payload_meta = if let Some(payload_ty) = payload_ty {
                quote!(::core::option::Option::Some(stringify!(#payload_ty)))
            } else {
                quote!(::core::option::Option::None)
            };

            // Build transition definitions for introspection
            let mut transition_defs = Vec::new();
            for transition in &event.transitions {
                let expanded_sources = self.expand_state_refs(&transition.sources);
                let sources_tokens: Vec<_> = expanded_sources
                    .iter()
                    .map(|source| quote!(#state_ident::#source))
                    .collect();
                let transition_guard_names: Vec<_> = transition
                    .guards
                    .iter()
                    .map(|guard| quote!(stringify!(#guard)))
                    .collect();
                let transition_unless_names: Vec<_> = event
                    .unless
                    .iter()
                    .chain(transition.unless.iter())
                    .map(|guard| quote!(stringify!(#guard)))
                    .collect();
                let transition_before_names: Vec<_> = transition
                    .before
                    .iter()
                    .map(|callback| quote!(stringify!(#callback)))
                    .collect();
                let transition_after_names: Vec<_> = transition
                    .after
                    .iter()
                    .map(|callback| quote!(stringify!(#callback)))
                    .collect();
                let resolved_target = self
                    .resolve_target_ident(&transition.target)
                    .unwrap_or_else(|| transition.target.clone());

                transition_defs.push(quote! {
                    #core_path::TransitionDefinition {
                        sources: &[ #( #sources_tokens ),* ],
                        target: #state_ident::#resolved_target,
                        guards: &[ #( #transition_guard_names ),* ],
                        unless: &[ #( #transition_unless_names ),* ],
                        before: &[ #( #transition_before_names ),* ],
                        after: &[ #( #transition_after_names ),* ],
                    }
                });
            }

            event_definitions.push(quote! {
                #core_path::EventDefinition {
                    name: stringify!(#method_ident),
                    guards: &[ #( #event_guard_names ),* ],
                    before: &[ #( #event_before_names ),* ],
                    after: &[ #( #event_after_names ),* ],
                    payload: #payload_meta,
                    transitions: &[ #( #transition_defs ),* ],
                }
            });

            let sync_tokens = self.build_event(
                state_ident,
                method_ident,
                can_ident.clone(),
                event,
                action_method,
                payload_ty,
                false,
            )?;
            sync_event_methods.push(sync_tokens.event_method);
            sync_can_methods.push(sync_tokens.can_method);

            if async_mode {
                let async_tokens = self.build_event(
                    state_ident,
                    method_ident,
                    can_ident.clone(),
                    event,
                    action_method,
                    payload_ty,
                    true,
                )?;
                async_event_methods.push(async_tokens.event_method);
                async_can_methods.push(async_tokens.can_method);
            }
        }

        let state_values: Vec<_> = states
            .iter()
            .map(|state| quote!(#state_ident::#state))
            .collect();

        let superstate_definitions: Vec<_> = self
            .hierarchy
            .superstates
            .iter()
            .map(|superstate| {
                let name = &superstate.name;
                let descendants: Vec<_> = superstate
                    .descendants
                    .iter()
                    .map(|leaf| quote!(#state_ident::#leaf))
                    .collect();
                let initial = &superstate.initial;
                quote! {
                    #core_path::SuperstateDefinition {
                        name: stringify!(#name),
                        descendants: &[ #( #descendants ),* ],
                        initial: #state_ident::#initial,
                    }
                }
            })
            .collect();

        let machine_definition = quote! {
            pub const DEFINITION: #core_path::MachineDefinition<#state_ident> = #core_path::MachineDefinition {
                name: stringify!(#machine_ident),
                states: &[ #( #state_values ),* ],
                initial: #state_ident::#initial_state,
                async_mode: #async_mode,
                superstates: &[ #( #superstate_definitions ),* ],
                events: &[ #( #event_definitions ),* ],
            };

            pub const fn definition() -> &'static #core_path::MachineDefinition<#state_ident> {
                &Self::DEFINITION
            }
        };

        let machine_trait = quote! {
            impl #core_path::Machine for #machine_ident {
                type State = #state_ident;

                fn state(&self) -> Self::State {
                    self.state
                }
            }
        };

        let impl_block = if async_mode {
            quote! {
                impl #machine_ident {
                    pub const ASYNC_MODE: bool = true;
                    #machine_definition
                    #( #async_event_methods )*
                    #( #async_can_methods )*
                }
            }
        } else {
            quote! {
                impl #machine_ident {
                    pub const ASYNC_MODE: bool = false;
                    #machine_definition
                    #( #sync_event_methods )*
                    #( #sync_can_methods )*
                }
            }
        };

        Ok(quote! {
            #state_enum
            #state_default
            #machine_struct
            #machine_impl
            #machine_default
            #machine_trait
            #impl_block
        })
    }
}
