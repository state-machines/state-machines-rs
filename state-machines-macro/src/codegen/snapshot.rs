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
    let scope_count = super::scopes::names(machine).len();
    let state_names = machine
        .states
        .iter()
        .map(|state| state.to_string())
        .collect::<Vec<_>>();
    let history_fields = super::history::fields(machine);
    let history_declarations = super::history::regions(machine).into_iter().map(|region| {
        let field = super::history::field(region);
        let key = format!("history_{region}");
        quote! {
            #[serde(default, rename = #key)]
            pub #field: ::core::option::Option<::state_machines::__private::String>,
        }
    });
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
        let history = super::history::regions(machine).into_iter().map(|region| {
            let field = super::history::field(region);
            let cases = machine.states.iter().enumerate().map(|(index, state)| {
                let state = state.to_string();
                quote! { #index => ::state_machines::__private::String::from(#state) }
            });
            quote! { machine.#field.map(|index| match index {
                #( #cases, )*
                _ => unreachable!("invalid history index"),
            }) }
        });
        quote! {
            #any::#state(machine) => (
                ::state_machines::__private::String::from(#state_str),
                machine.ctx,
                #( machine.#fields, )*
                #( #history, )*
            )
        }
    });
    let history_checks = super::history::regions(machine).into_iter().map(|region| {
        let field = super::history::field(region);
        let region_str = region.to_string();
        let leaves = machine.hierarchy.expand_state(region, &machine.states).into_iter()
            .map(|state| state.to_string()).collect::<Vec<_>>();
        quote! {
            if snapshot.#field.as_ref().is_some_and(|leaf| ![#( #leaves, )*].contains(&leaf.as_str())) {
                return Err(::state_machines::SnapshotError::InvalidHistory { region: #region_str });
            }
        }
    });
    let history_restores: Vec<_> = super::history::regions(machine)
        .into_iter()
        .map(|region| {
            let field = super::history::field(region);
            let cases = machine.states.iter().enumerate().map(|(index, state)| {
                let state = state.to_string();
                quote! { #state => #index }
            });
            quote! { #field: snapshot.#field.map(|leaf| match leaf.as_str() {
                #( #cases, )*
                _ => unreachable!("validated history"),
            }) }
        })
        .collect();
    let checks = machine.state_storage.iter().map(|spec| {
        let field = &spec.field;
        let owner = spec.state_name.to_string();
        let states = machine
            .hierarchy
            .expand_state(&spec.state_name, &machine.states)
            .into_iter()
            .map(|state| state.to_string())
            .collect::<Vec<_>>();
        quote! {
            if snapshot.#field.is_some() && ![#( #states, )*].contains(&snapshot.state.as_str()) {
                return Err(::state_machines::SnapshotError::InactiveData { state: #owner });
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
                #( #history_restores, )*
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
                #( #history_declarations )*
            }

            impl #generics #dynamic #generics {
                /// A poisoned wrapper has no recoverable owned snapshot.
                pub fn try_into_snapshot(self) -> Result<#snapshot_name #generics, Self> {
                    if self.is_poisoned() { Err(self) } else { Ok(self.into_snapshot()) }
                }

                /// Consume the wrapper without requiring Clone on its data.
                /// Panics if poisoned; use `try_into_snapshot` after cancellation.
                pub fn into_snapshot(mut self) -> #snapshot_name #generics {
                    let (state, ctx, #( #fields, )* #( #history_fields, )*) = match self.inner.take()
                        .expect("dynamic machine in invalid state")
                    {
                        #( #capture, )*
                    };
                    #snapshot_name {
                        version: 1,
                        machine: ::state_machines::__private::String::from(#name_str),
                        state, ctx,
                        #( #fields, )*
                        #( #history_fields, )*
                    }
                }

                /// Validate the envelope before restoring. On failure the
                /// snapshot is returned intact, so callers can migrate it.
                pub fn from_snapshot(snapshot: #snapshot_name #generics) -> Result<Self, (#snapshot_name #generics, ::state_machines::SnapshotError)> {
                    if let Err(error) = Self::validate_snapshot(&snapshot) {
                        return Err((snapshot, error));
                    }
                    Ok(Self::__sm_restore_snapshot(snapshot))
                }

                /// Borrowed validation lets compositions check all regions before
                /// consuming any context/data. Uses the same checks as restoration.
                pub fn validate_snapshot(snapshot: &#snapshot_name #generics) -> Result<(), ::state_machines::SnapshotError> {
                    ::state_machines::SnapshotError::validate_header(snapshot.version, &snapshot.machine, #name_str)?;
                    if ![#( #state_names, )*].contains(&snapshot.state.as_str()) {
                        return Err(::state_machines::SnapshotError::UnknownState);
                    }
                    #( #checks )*
                    #( #history_checks )*
                    Ok(())
                }

                fn __sm_restore_snapshot(snapshot: #snapshot_name #generics) -> Self {
                    let inner = match snapshot.state.as_str() {
                        #( #restore, )*
                        _ => unreachable!("validated snapshot state"),
                    };
                    Self {
                        epoch: 0,
                        scope_epochs: [0; #scope_count],
                        last_state: inner.state(),
                        inner: Some(inner),
                        completions: ::state_machines::__private::Vec::new(),
                    }
                }
            }
            ::state_machines::__sm_if_runtime! {
                impl #generics ::state_machines::runtime::SnapshotMachine for #dynamic #generics {
                    type Snapshot = #snapshot_name #generics;
                    fn validate_snapshot(snapshot: &Self::Snapshot) -> Result<(), ::state_machines::SnapshotError> {
                        Self::validate_snapshot(snapshot)
                    }
                    fn into_snapshot(self) -> Self::Snapshot { self.into_snapshot() }
                    fn from_validated_snapshot(snapshot: Self::Snapshot, _capacity: usize) -> Self {
                        Self::__sm_restore_snapshot(snapshot)
                    }
                }
            }
        }
    }
}
