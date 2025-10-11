//! Typestate pattern code generation for state machines.
//!
//! This module generates compile-time type-safe state machines using the typestate pattern.
//! Instead of runtime state enums, each state becomes a distinct type and transitions
//! consume the machine, returning a new machine with the new state type.
//!
//! # Generated Code Structure
//!
//! For each state machine, we generate:
//! 1. Empty marker structs for each state (e.g., `struct Docked;`)
//! 2. A generic `Machine<S>` struct parameterized by state type
//! 3. State-specific impl blocks with transition methods
//!
//! # Typestate Benefits
//!
//! - Invalid transitions are compile errors
//! - No runtime state checking needed
//! - Zero-cost abstractions
//! - Self-documenting API (IDE autocomplete shows valid transitions)
//!
//! # Example Generated Code
//!
//! ```rust,ignore
//! pub struct Docked;
//! pub struct InFlight;
//!
//! pub struct FlightDeck<S> {
//!     _state: core::marker::PhantomData<S>,
//!     // storage fields...
//! }
//!
//! impl FlightDeck<Docked> {
//!     pub fn new() -> Self { /* ... */ }
//!
//!     pub fn launch(self) -> Result<FlightDeck<InFlight>, (Self, GuardError)> {
//!         // Check guards, run callbacks, return new typed machine
//!     }
//! }
//! ```

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Ident, Result};

/// Generate all typestate code for the machine.
///
/// This is the main entry point for typestate generation. It orchestrates
/// the generation of state markers, the machine struct, and all state implementations.
pub fn generate_typestate_machine(machine: &StateMachine) -> Result<TokenStream2> {
    let markers = generate_state_markers(machine)?;
    let machine_struct = generate_machine_struct(machine)?;
    let impls = generate_state_impls(machine)?;

    Ok(quote! {
        #markers
        #machine_struct
        #( #impls )*
    })
}

/// Generate empty state marker structs.
///
/// Each state becomes a zero-sized type used as a phantom type parameter.
/// These markers serve as compile-time tags to track the current state.
///
/// # Example Output
///
/// ```rust,ignore
/// pub struct Docked;
/// pub struct Launching;
/// pub struct InFlight;
/// ```
fn generate_state_markers(machine: &StateMachine) -> Result<TokenStream2> {
    let markers: Vec<_> = machine
        .states
        .iter()
        .map(|state| {
            quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
                pub struct #state;
            }
        })
        .collect();

    Ok(quote! {
        #( #markers )*
    })
}

/// Generate the generic Machine<S> struct.
///
/// The struct is parameterized by state type `S` and contains:
/// - A PhantomData marker to track the state type
/// - Storage fields for state-associated data (if any)
///
/// # Example Output
///
/// ```rust,ignore
/// pub struct FlightDeck<S> {
///     _state: core::marker::PhantomData<S>,
///     __docking_data: Option<DockingData>,
/// }
/// ```
fn generate_machine_struct(machine: &StateMachine) -> Result<TokenStream2> {
    let machine_name = &machine.name;

    // Generate storage fields for state-associated data
    let storage_fields: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            let ty = &spec.ty;
            quote! {
                #field: ::core::option::Option<#ty>
            }
        })
        .collect();

    Ok(quote! {
        #[derive(Debug)]
        pub struct #machine_name<S> {
            _state: ::core::marker::PhantomData<S>,
            #( #storage_fields, )*
        }
    })
}

/// Generate impl blocks for each state.
///
/// For each state, we create an `impl Machine<State>` block containing:
/// - Constructor (for initial state only)
/// - Transition methods for each valid outgoing event
/// - Storage accessor methods
///
/// Each transition method:
/// 1. Checks guards (event-level and transition-level)
/// 2. Executes before callbacks
/// 3. Creates new machine with target state
/// 4. Executes after callbacks on new machine
/// 5. Returns Result with new typed machine or original machine with error
fn generate_state_impls(machine: &StateMachine) -> Result<Vec<TokenStream2>> {
    let mut impls = Vec::new();

    for state in &machine.states {
        let mut methods = Vec::new();

        // Generate constructor for initial state
        if state == &machine.initial {
            let constructor = generate_constructor(machine, state)?;
            methods.push(constructor);
        }

        // Generate transition methods for outgoing transitions
        if let Some(edges) = machine.transition_graph.outgoing(state) {
            for edge in edges {
                let method = generate_transition_method(machine, state, edge)?;
                methods.push(method);
            }
        }

        let machine_name = &machine.name;
        let impl_block = quote! {
            impl #machine_name<#state> {
                #( #methods )*
            }
        };

        impls.push(impl_block);
    }

    Ok(impls)
}

/// Generate a constructor method for the initial state.
///
/// Creates a new machine instance in the initial state with all storage fields
/// initialized to None.
///
/// # Example Output
///
/// ```rust,ignore
/// pub fn new() -> Self {
///     Self {
///         _state: core::marker::PhantomData,
///         __data_field: None,
///     }
/// }
/// ```
fn generate_constructor(machine: &StateMachine, _state: &Ident) -> Result<TokenStream2> {
    let storage_inits: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            quote! {
                #field: ::core::option::Option::None
            }
        })
        .collect();

    Ok(quote! {
        pub fn new() -> Self {
            Self {
                _state: ::core::marker::PhantomData,
                #( #storage_inits, )*
            }
        }
    })
}

/// Generate a transition method for a single edge in the transition graph.
///
/// The method signature depends on:
/// - Payload type (adds parameter if present)
/// - Async mode (makes method async if enabled)
///
/// The method body:
/// 1. Evaluates event-level guards
/// 2. Evaluates transition-level guards
/// 3. Runs before callbacks
/// 4. Creates new machine with target state
/// 5. Runs after callbacks
/// 6. Returns Ok(new_machine) or Err((self, GuardError))
///
/// # Example Output
///
/// ```rust,ignore
/// pub fn launch(self) -> Result<FlightDeck<InFlight>, (Self, GuardError)> {
///     if !self.fuel_check() {
///         return Err((self, GuardError::new("fuel_check", "launch")));
///     }
///     self.pre_launch_callback();
///     let mut new_machine = FlightDeck {
///         _state: PhantomData,
///         __fuel_data: self.__fuel_data,
///     };
///     new_machine.post_launch_callback();
///     Ok(new_machine)
/// }
/// ```
fn generate_transition_method(
    machine: &StateMachine,
    _source_state: &Ident,
    edge: &TransitionEdge,
) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let event_name = &edge.event;
    let target_state = &edge.target;
    let is_async = machine.async_mode;
    let core_path = quote!(::state_machines::core);

    // Build method signature
    let (method_sig, payload_ref) = if let Some(payload_ty) = &edge.payload {
        let sig = if is_async {
            quote! {
                pub async fn #event_name(self, payload: #payload_ty)
            }
        } else {
            quote! {
                pub fn #event_name(self, payload: #payload_ty)
            }
        };
        (sig, quote! { &payload })
    } else {
        let sig = if is_async {
            quote! {
                pub async fn #event_name(self)
            }
        } else {
            quote! {
                pub fn #event_name(self)
            }
        };
        (sig, quote! {})
    };

    let return_type = quote! {
        ::core::result::Result<#machine_name<#target_state>, (Self, #core_path::GuardError)>
    };

    // Build guard checks
    let mut guard_checks = Vec::new();

    // Event-level guards
    for guard in &edge.guards {
        let check = if edge.payload.is_some() {
            if is_async {
                quote! {
                    if !self.#guard(#payload_ref).await {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            } else {
                quote! {
                    if !self.#guard(#payload_ref) {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            }
        } else if is_async {
            quote! {
                if !self.#guard().await {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                    ));
                }
            }
        } else {
            quote! {
                if !self.#guard() {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                    ));
                }
            }
        };
        guard_checks.push(check);
    }

    // Unless guards (inverted)
    for guard in &edge.unless {
        let check = if edge.payload.is_some() {
            if is_async {
                quote! {
                    if self.#guard(#payload_ref).await {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            } else {
                quote! {
                    if self.#guard(#payload_ref) {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            }
        } else if is_async {
            quote! {
                if self.#guard().await {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                    ));
                }
            }
        } else {
            quote! {
                if self.#guard() {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                    ));
                }
            }
        };
        guard_checks.push(check);
    }

    // Build before callback calls
    let before_calls: Vec<_> = edge
        .before
        .iter()
        .map(|callback| {
            if edge.payload.is_some() {
                if is_async {
                    quote! { self.#callback(#payload_ref).await; }
                } else {
                    quote! { self.#callback(#payload_ref); }
                }
            } else if is_async {
                quote! { self.#callback().await; }
            } else {
                quote! { self.#callback(); }
            }
        })
        .collect();

    // Build storage field transfers
    let storage_transfers: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            quote! {
                #field: self.#field
            }
        })
        .collect();

    // Build after callback calls (on new machine)
    let after_calls: Vec<_> = edge
        .after
        .iter()
        .map(|callback| {
            if edge.payload.is_some() {
                if is_async {
                    quote! { new_machine.#callback(#payload_ref).await; }
                } else {
                    quote! { new_machine.#callback(#payload_ref); }
                }
            } else if is_async {
                quote! { new_machine.#callback().await; }
            } else {
                quote! { new_machine.#callback(); }
            }
        })
        .collect();

    // Assemble the complete method
    Ok(quote! {
        #method_sig -> #return_type {
            // Check guards
            #( #guard_checks )*

            // Execute before callbacks on current machine
            #( #before_calls )*

            // Create new machine with target state
            let mut new_machine = #machine_name {
                _state: ::core::marker::PhantomData,
                #( #storage_transfers, )*
            };

            // Execute after callbacks on new machine
            #( #after_calls )*

            ::core::result::Result::Ok(new_machine)
        }
    })
}
