//! Owned snapshots preserve non-Clone context and state data. Restoring is
//! construction, not a transition: no guards, callbacks, or notifications.

use super::utils::{ctx_generics, ctx_ty};
use crate::types::StateMachine;
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};

/// Identifiers and token fragments shared by every generated snapshot item.
struct SnapshotNames<'a> {
    name: &'a Ident,
    name_str: String,
    snapshot: Ident,
    dynamic: Ident,
    any: Ident,
    generics: TokenStream,
    context: TokenStream,
    scope_count: usize,
    /// Storage fields, in `state_storage` order.
    fields: Vec<&'a Ident>,
    history_fields: Vec<Ident>,
}

impl<'a> SnapshotNames<'a> {
    fn new(machine: &'a StateMachine) -> Self {
        let name = &machine.name;
        Self {
            name,
            name_str: name.to_string(),
            snapshot: format_ident!("{}Snapshot", name),
            dynamic: format_ident!("Dynamic{}", name),
            any: format_ident!("Any{}State", name),
            generics: ctx_generics(machine),
            context: ctx_ty(machine),
            scope_count: super::scopes::names(machine).len(),
            fields: machine
                .state_storage
                .iter()
                .map(|spec| &spec.field)
                .collect(),
            history_fields: super::history::fields(machine),
        }
    }
}

/// The serde-derived snapshot struct: envelope, context, data and history fields.
fn snapshot_struct(machine: &StateMachine, names: &SnapshotNames<'_>) -> TokenStream {
    let SnapshotNames {
        snapshot: snapshot_name,
        generics,
        context,
        ..
    } = names;
    let history_declarations = super::history::regions(machine).into_iter().map(|region| {
        let field = super::history::field(region);
        let key = format!("history_{region}");
        quote! {
            #[serde(default, rename = #key)]
            pub #field: ::core::option::Option<::state_machines::__private::String>,
        }
    });
    let declarations = machine.state_storage.iter().map(|spec| {
        let field = &spec.field;
        let ty = &spec.ty;
        let state = spec.state_name.to_string();
        quote! {
            #[serde(default, rename = #state)]
            pub #field: ::core::option::Option<#ty>,
        }
    });
    quote! {
        /// Versioned owned machine state, not a graph schema.
        #[derive(Debug, ::state_machines::__private::serde::Serialize, ::state_machines::__private::serde::Deserialize)]
        #[serde(crate = "::state_machines::__private::serde", deny_unknown_fields)]
        // Data/history fields keep their established `__` names (migrations
        // read and write them); serde renames them on the wire.
        #[allow(clippy::pub_underscore_fields)]
        pub struct #snapshot_name #generics {
            pub version: u32,
            pub machine: ::state_machines::__private::String,
            pub state: ::state_machines::__private::String,
            pub ctx: #context,
            #( #declarations )*
            #( #history_declarations )*
        }
    }
}

/// One `AnyState` match arm per leaf, destructuring the machine into the
/// `(state, ctx, data.., history..)` tuple `into_snapshot` rebuilds from.
fn capture_arms(machine: &StateMachine, names: &SnapshotNames<'_>) -> Vec<TokenStream> {
    let SnapshotNames { any, fields, .. } = names;
    machine
        .states
        .iter()
        .map(|state| {
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
        })
        .collect()
}

/// Envelope checks run by `validate_snapshot`: data only for its owning
/// state, then history only naming a leaf of its region.
fn validation_checks(machine: &StateMachine) -> Vec<TokenStream> {
    let checks = machine.state_storage.iter().map(|spec| {
        let field = &spec.field;
        let owner = spec.state_name.to_string();
        let states = machine
            .hierarchy
            .expand_state(&spec.state_name, &machine.states)
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>();
        quote! {
            if snapshot.#field.is_some() && ![#( #states, )*].contains(&snapshot.state.as_str()) {
                return Err(::state_machines::SnapshotError::InactiveData { state: #owner });
            }
        }
    });
    let history_checks = super::history::regions(machine).into_iter().map(|region| {
        let field = super::history::field(region);
        let region_str = region.to_string();
        let leaves = machine.hierarchy.expand_state(region, &machine.states).iter()
            .map(std::string::ToString::to_string).collect::<Vec<_>>();
        quote! {
            if snapshot.#field.as_ref().is_some_and(|leaf| ![#( #leaves, )*].contains(&leaf.as_str())) {
                return Err(::state_machines::SnapshotError::InvalidHistory { region: #region_str });
            }
        }
    });
    checks.chain(history_checks).collect()
}

/// One match arm per leaf rebuilding the typed machine from a validated snapshot.
fn restore_arms(machine: &StateMachine, names: &SnapshotNames<'_>) -> Vec<TokenStream> {
    let SnapshotNames {
        name, any, fields, ..
    } = names;
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
    machine
        .states
        .iter()
        .map(|state| {
            let state_str = state.to_string();
            quote! {
                #state_str => #any::#state(#name {
                    ctx: snapshot.ctx,
                    _state: ::core::marker::PhantomData,
                    #( #fields: snapshot.#fields, )*
                    #( #history_restores, )*
                })
            }
        })
        .collect()
}

/// `try_into_snapshot` and `into_snapshot` on the dynamic wrapper.
fn into_snapshot_methods(machine: &StateMachine, names: &SnapshotNames<'_>) -> TokenStream {
    let SnapshotNames {
        name_str,
        snapshot: snapshot_name,
        generics,
        fields,
        history_fields,
        ..
    } = names;
    let capture = capture_arms(machine, names);
    quote! {
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
    }
}

/// `from_snapshot`, `validate_snapshot` and the restore helper on the dynamic wrapper.
fn from_snapshot_methods(machine: &StateMachine, names: &SnapshotNames<'_>) -> TokenStream {
    let SnapshotNames {
        name_str,
        snapshot: snapshot_name,
        generics,
        scope_count,
        ..
    } = names;
    let state_names = machine
        .states
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>();
    let checks = validation_checks(machine);
    let restore = restore_arms(machine, names);
    quote! {
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
}

/// The runtime `SnapshotMachine` impl, delegating to the inherent methods.
fn runtime_snapshot_impl(names: &SnapshotNames<'_>) -> TokenStream {
    let SnapshotNames {
        snapshot: snapshot_name,
        dynamic,
        generics,
        ..
    } = names;
    quote! {
        ::state_machines::__sm_if_runtime! {
            impl #generics ::state_machines::runtime::SnapshotMachine for #dynamic #generics
            where #dynamic #generics: ::state_machines::runtime::Machine {
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

pub fn generate(machine: &StateMachine) -> TokenStream {
    if !machine.snapshot {
        return quote! {};
    }
    let names = SnapshotNames::new(machine);
    let dynamic = &names.dynamic;
    let generics = &names.generics;
    let snapshot_struct = snapshot_struct(machine, &names);
    let into_snapshot = into_snapshot_methods(machine, &names);
    let from_snapshot = from_snapshot_methods(machine, &names);
    let runtime_impl = runtime_snapshot_impl(&names);
    quote! {
        ::state_machines::__sm_if_serde! {
            #snapshot_struct

            impl #generics #dynamic #generics {
                #into_snapshot
                #from_snapshot
            }
            #runtime_impl
        }
    }
}
