//! Owned snapshots preserve non-Clone context and state data. Restoring is
//! construction, not a transition: no guards, callbacks, or notifications.

use super::utils::{ctx_generics, ctx_ty};
use crate::types::StateMachine;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate(machine: &StateMachine) -> TokenStream {
    if !machine.snapshot {
        return quote! {};
    }
    let name = &machine.name;
    let name_str = name.to_string();
    let snapshot_name = format_ident!("{}Snapshot", name);
    let dynamic = format_ident!("Dynamic{}", name);
    let any = format_ident!("Any{}State", name);
    let generics = ctx_generics(machine);
    let context = ctx_ty(machine);
    let fields: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| &spec.field)
        .collect();
    let declarations = machine.state_storage.iter().map(|spec| {
        let field = &spec.field;
        let ty = &spec.ty;
        let state = spec.state_name.to_string();
        quote! {
            #[serde(default, rename = #state)]
            pub #field: ::core::option::Option<#ty>,
        }
    });
    let capture = machine.states.iter().map(|state| {
        let state_str = state.to_string();
        quote! {
            #any::#state(machine) => (
                ::state_machines::__private::String::from(#state_str),
                machine.ctx,
                #( machine.#fields, )*
            )
        }
    });
    let checks = machine.state_storage.iter().map(|spec| {
        let field = &spec.field;
        let owner = spec.state_name.to_string();
        let states = machine.hierarchy.expand_state(&spec.state_name, &machine.states)
            .into_iter().map(|state| state.to_string()).collect::<Vec<_>>();
        quote! {
            if snapshot.#field.is_some() && ![#( #states, )*].contains(&snapshot.state.as_str()) {
                return Err((snapshot, ::state_machines::SnapshotError::InactiveData { state: #owner }));
            }
        }
    });
    let restore = machine.states.iter().map(|state| {
        let state_str = state.to_string();
        quote! {
            #state_str => #any::#state(#name {
                ctx: snapshot.ctx,
                _state: ::core::marker::PhantomData,
                #( #fields: snapshot.#fields, )*
            })
        }
    });
    quote! {
        ::state_machines::__sm_if_serde! {
            /// Versioned owned machine state, not a graph schema.
            #[derive(Debug, ::state_machines::__private::serde::Serialize, ::state_machines::__private::serde::Deserialize)]
            #[serde(crate = "::state_machines::__private::serde", deny_unknown_fields)]
            pub struct #snapshot_name #generics {
                pub version: u32,
                pub machine: ::state_machines::__private::String,
                pub state: ::state_machines::__private::String,
                pub ctx: #context,
                #( #declarations )*
            }

            impl #generics #dynamic #generics {
                /// Consume the wrapper without requiring Clone on its data.
                pub fn into_snapshot(mut self) -> #snapshot_name #generics {
                    let (state, ctx, #( #fields, )*) = match self.inner.take()
                        .expect("dynamic machine in invalid state")
                    {
                        #( #capture, )*
                    };
                    #snapshot_name {
                        version: 1,
                        machine: ::state_machines::__private::String::from(#name_str),
                        state, ctx,
                        #( #fields, )*
                    }
                }

                /// Validate the envelope before restoring. On failure the
                /// snapshot is returned intact, so callers can migrate it.
                pub fn from_snapshot(snapshot: #snapshot_name #generics) -> Result<Self, (#snapshot_name #generics, ::state_machines::SnapshotError)> {
                    if snapshot.version != 1 {
                        let actual = snapshot.version;
                        return Err((snapshot, ::state_machines::SnapshotError::UnsupportedVersion { expected: 1, actual }));
                    }
                    if snapshot.machine != #name_str {
                        return Err((snapshot, ::state_machines::SnapshotError::WrongMachine));
                    }
                    #( #checks )*
                    let inner = match snapshot.state.as_str() {
                        #( #restore, )*
                        _ => return Err((snapshot, ::state_machines::SnapshotError::UnknownState)),
                    };
                    Ok(Self {
                        inner: Some(inner),
                        completions: ::state_machines::__private::Vec::new(),
                    })
                }
            }
        }
    }
}
