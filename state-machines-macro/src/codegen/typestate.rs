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

use crate::codegen::utils::to_snake_case_ident;
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
    let substate_impls = generate_substate_impls(machine)?;
    let superstate_transition_impls = generate_superstate_transition_impls(machine)?;

    Ok(quote! {
        #markers
        #machine_struct
        #( #impls )*
        #( #substate_impls )*
        #( #superstate_transition_impls )*
    })
}

/// Generate empty state marker structs.
///
/// Each state becomes a zero-sized type used as a phantom type parameter.
/// These markers serve as compile-time tags to track the current state.
/// This includes both leaf states and superstates.
///
/// # Example Output
///
/// ```rust,ignore
/// pub struct Docked;
/// pub struct Launching;
/// pub struct InFlight;
/// pub struct Flight;  // superstate
/// ```
fn generate_state_markers(machine: &StateMachine) -> Result<TokenStream2> {
    let mut all_states = machine.states.clone();

    // Add superstates to the list
    all_states.extend(machine.hierarchy.all_superstates());

    let markers: Vec<_> = all_states
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

/// Generate the generic Machine<C, S> struct.
///
/// The struct is parameterized by context type `C` and state type `S`:
/// - C: Context type for hardware access or shared state
/// - S: Current state type (typestate pattern)
///
/// Contains:
/// - A PhantomData marker to track the state type
/// - A context field for hardware/external dependencies
/// - Storage fields for state-associated data (if any)
///
/// # Example Output
///
/// ```rust,ignore
/// pub struct FlightDeck<C, S> {
///     ctx: C,
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
        pub struct #machine_name<C, S> {
            ctx: C,
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
            impl<C> #machine_name<C, #state> {
                #( #methods )*
            }
        };

        impls.push(impl_block);
    }

    // Generate generic impl block with storage accessors (Option-based)
    if !machine.state_storage.is_empty() {
        let storage_accessors = generate_storage_accessors(machine)?;
        let machine_name = &machine.name;
        let generic_impl = quote! {
            impl<C, S> #machine_name<C, S> {
                #( #storage_accessors )*
            }
        };
        impls.push(generic_impl);

        // Generate state-specific guaranteed accessors
        let state_specific_accessors = generate_state_specific_accessors(machine)?;
        impls.extend(state_specific_accessors);
    }

    Ok(impls)
}

/// Generate a constructor method for the initial state.
///
/// Creates a new machine instance in the initial state with all storage fields
/// initialized to None. Takes a context parameter for hardware/external dependencies.
///
/// # Example Output
///
/// ```rust,ignore
/// pub fn new(ctx: C) -> Self {
///     Self {
///         ctx,
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
        pub fn new(ctx: C) -> Self {
            Self {
                ctx,
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

    // Convert event name to snake_case for the method name
    // Example: Trip → trip, EnterHalfOpen → enter_half_open
    // This ensures generated methods follow Rust naming conventions
    let method_name = to_snake_case_ident(event_name);

    let target_state = &edge.target;
    let is_async = machine.async_mode;
    let core_path = quote!(::state_machines::core);

    // Build method signature using snake_case method name
    let (method_sig, payload_ref) = if let Some(payload_ty) = &edge.payload {
        let sig = if is_async {
            quote! {
                pub async fn #method_name(mut self, payload: #payload_ty)
            }
        } else {
            quote! {
                pub fn #method_name(mut self, payload: #payload_ty)
            }
        };
        (sig, quote! { &payload })
    } else {
        let sig = if is_async {
            quote! {
                pub async fn #method_name(mut self)
            }
        } else {
            quote! {
                pub fn #method_name(mut self)
            }
        };
        (sig, quote! {})
    };

    let return_type = quote! {
        ::core::result::Result<#machine_name<C, #target_state>, (Self, #core_path::GuardError)>
    };

    // Build guard checks
    let mut guard_checks = Vec::new();

    // Event-level guards
    for guard in &edge.guards {
        let check = if edge.payload.is_some() {
            if is_async {
                quote! {
                    if !self.#guard(&self.ctx, #payload_ref).await {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            } else {
                quote! {
                    if !self.#guard(&self.ctx, #payload_ref) {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            }
        } else if is_async {
            quote! {
                if !self.#guard(&self.ctx).await {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                    ));
                }
            }
        } else {
            quote! {
                if !self.#guard(&self.ctx) {
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
                    if self.#guard(&self.ctx, #payload_ref).await {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            } else {
                quote! {
                    if self.#guard(&self.ctx, #payload_ref) {
                        return ::core::result::Result::Err((
                            self,
                            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                        ));
                    }
                }
            }
        } else if is_async {
            quote! {
                if self.#guard(&self.ctx).await {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                    ));
                }
            }
        } else {
            quote! {
                if self.#guard(&self.ctx) {
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

    // Build storage field transfers with initialization for target state
    let storage_transfers: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            let state_name = &spec.state_name;
            let ty = &spec.ty;

            // If transitioning to this state, initialize with Default
            // Otherwise, clear the data (set to None) since we're not in that state
            if state_name == target_state {
                quote! {
                    #field: ::core::option::Option::Some(<#ty as ::core::default::Default>::default())
                }
            } else {
                quote! {
                    #field: ::core::option::Option::None
                }
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

    // Check if we have around callbacks
    let has_around = !edge.around.is_empty();

    // Assemble the complete method
    if has_around {
        // Generate around callback invocations - Before stage (on self)
        let around_before_checks: Vec<_> = edge
            .around
            .iter()
            .map(|callback| {
                if is_async {
                    quote! {
                        match self.#callback(#core_path::AroundStage::Before).await {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                // Preserve the full TransitionError kind (GuardFailed, ActionFailed, etc.)
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                return ::core::result::Result::Err((
                                    self,
                                    #core_path::GuardError::with_kind(callback_name, stringify!(#event_name), err.kind)
                                ));
                            }
                        }
                    }
                } else {
                    quote! {
                        match self.#callback(#core_path::AroundStage::Before) {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                // Preserve the full TransitionError kind (GuardFailed, ActionFailed, etc.)
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                return ::core::result::Result::Err((
                                    self,
                                    #core_path::GuardError::with_kind(callback_name, stringify!(#event_name), err.kind)
                                ));
                            }
                        }
                    }
                }
            })
            .collect();

        // Generate around callback invocations - AfterSuccess stage (on new_machine)
        let around_after_checks: Vec<_> = edge
            .around
            .iter()
            .map(|callback| {
                if is_async {
                    quote! {
                        match new_machine.#callback(#core_path::AroundStage::AfterSuccess).await {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                // LIMITATION: AfterSuccess aborts cannot be properly handled with current typestate return type.
                                // The transition has already occurred, so we can't return the old machine.
                                // We panic here to make this limitation explicit rather than silently ignoring the error.
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                panic!(
                                    "Around callback '{}' aborted at AfterSuccess stage during event '{}', but typestate machines \
                                     cannot properly surface this error because the state transition has already occurred. \
                                     Consider using Before stage aborts instead, or changing your callback to return Proceed.",
                                    callback_name, stringify!(#event_name)
                                );
                            }
                        }
                    }
                } else {
                    quote! {
                        match new_machine.#callback(#core_path::AroundStage::AfterSuccess) {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                // LIMITATION: AfterSuccess aborts cannot be properly handled with current typestate return type.
                                // The transition has already occurred, so we can't return the old machine.
                                // We panic here to make this limitation explicit rather than silently ignoring the error.
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                panic!(
                                    "Around callback '{}' aborted at AfterSuccess stage during event '{}', but typestate machines \
                                     cannot properly surface this error because the state transition has already occurred. \
                                     Consider using Before stage aborts instead, or changing your callback to return Proceed.",
                                    callback_name, stringify!(#event_name)
                                );
                            }
                        }
                    }
                }
            })
            .collect();

        Ok(quote! {
            #method_sig -> #return_type {
                // Around callbacks - Before stage
                #( #around_before_checks )*

                // Check guards
                #( #guard_checks )*

                // Execute before callbacks on current machine
                #( #before_calls )*

                // Create new machine with target state
                let mut new_machine = #machine_name {
                    ctx: self.ctx,
                    _state: ::core::marker::PhantomData,
                    #( #storage_transfers, )*
                };

                // Execute after callbacks on new machine
                #( #after_calls )*

                // Around callbacks - AfterSuccess stage
                #( #around_after_checks )*

                ::core::result::Result::Ok(new_machine)
            }
        })
    } else {
        // No around callbacks - generate simpler code
        Ok(quote! {
            #method_sig -> #return_type {
                // Check guards
                #( #guard_checks )*

                // Execute before callbacks on current machine
                #( #before_calls )*

                // Create new machine with target state
                let mut new_machine = #machine_name {
                    ctx: self.ctx,
                    _state: ::core::marker::PhantomData,
                    #( #storage_transfers, )*
                };

                // Execute after callbacks on new machine
                #( #after_calls )*

                ::core::result::Result::Ok(new_machine)
            }
        })
    }
}

/// Generate storage accessor methods for state-local data.
///
/// For each state with associated data, we generate:
/// - `state_data()` - Returns `Option<&T>`
/// - `state_data_mut()` - Returns `Option<&mut T>`
///
/// These methods are available on all states via `impl<S>`, allowing
/// access to state-local storage from any state in the machine.
///
/// # Example Output
///
/// ```rust,ignore
/// pub fn launch_prep_data(&self) -> Option<&PrepData> {
///     self.__launch_prep_data.as_ref()
/// }
///
/// pub fn launch_prep_data_mut(&mut self) -> Option<&mut PrepData> {
///     self.__launch_prep_data.as_mut()
/// }
/// ```
fn generate_storage_accessors(machine: &StateMachine) -> Result<Vec<TokenStream2>> {
    let mut accessors = Vec::new();

    for spec in &machine.state_storage {
        let field = &spec.field;
        let ty = &spec.ty;

        // Generate accessor method name from field name
        // __launch_prep_data -> launch_prep_data
        let field_str = field.to_string();
        let accessor_name = field_str.trim_start_matches("__");
        let accessor_ident = syn::Ident::new(accessor_name, field.span());
        let mut_accessor_name = format!("{}_mut", accessor_name);
        let mut_accessor_ident = syn::Ident::new(&mut_accessor_name, field.span());

        // Immutable accessor
        accessors.push(quote! {
            pub fn #accessor_ident(&self) -> ::core::option::Option<&#ty> {
                self.#field.as_ref()
            }
        });

        // Mutable accessor
        accessors.push(quote! {
            pub fn #mut_accessor_ident(&mut self) -> ::core::option::Option<&mut #ty> {
                self.#field.as_mut()
            }
        });
    }

    Ok(accessors)
}

/// Generate state-specific guaranteed data accessors.
///
/// For each state with associated data, we generate an impl block with
/// a uniquely named accessor based on the state name:
/// ```rust,ignore
/// impl<C> Machine<C, LaunchPrep> {
///     pub fn launch_prep_data(&self) -> &PrepData {
///         self.__state_data_launch_prep.as_ref().unwrap()
///     }
///     pub fn launch_prep_data_mut(&mut self) -> &mut PrepData {
///         self.__state_data_launch_prep.as_mut().unwrap()
///     }
/// }
/// ```
///
/// These methods provide guaranteed access to state data without Option,
/// as we know the data exists when in that specific state.
/// The method names are unique per state to avoid conflicts.
fn generate_state_specific_accessors(machine: &StateMachine) -> Result<Vec<TokenStream2>> {
    let mut impls = Vec::new();
    let machine_name = &machine.name;

    for spec in &machine.state_storage {
        let state_name = &spec.state_name;
        let field = &spec.field;
        let ty = &spec.ty;

        // Generate method names from state name: LaunchPrep -> launch_prep_data
        let state_str = state_name.to_string();
        let snake = crate::parser::to_snake_case(&state_str);
        let data_method = syn::Ident::new(&format!("{}_data", snake), state_name.span());
        let data_mut_method = syn::Ident::new(&format!("{}_data_mut", snake), state_name.span());

        // Generate state-specific impl block
        let impl_block = quote! {
            impl<C> #machine_name<C, #state_name> {
                /// Access the state-associated data for this specific state.
                ///
                /// This method is guaranteed to return a reference because
                /// the data is always present when in this state.
                pub fn #data_method(&self) -> &#ty {
                    self.#field.as_ref().unwrap()
                }

                /// Mutably access the state-associated data for this specific state.
                ///
                /// This method is guaranteed to return a mutable reference because
                /// the data is always present when in this state.
                pub fn #data_mut_method(&mut self) -> &mut #ty {
                    self.#field.as_mut().unwrap()
                }
            }
        };

        impls.push(impl_block);
    }

    Ok(impls)
}

/// Generate SubstateOf trait implementations for hierarchy relationships.
///
/// For each leaf state that has ancestors, we generate:
/// ```rust,ignore
/// impl SubstateOf<Flight> for LaunchPrep {}
/// impl SubstateOf<Flight> for Launching {}
/// ```
///
/// This enables polymorphic transitions from any substate.
fn generate_substate_impls(machine: &StateMachine) -> Result<Vec<TokenStream2>> {
    let mut impls = Vec::new();

    // For each leaf state, check if it has ancestors (is in a superstate)
    for leaf in &machine.states {
        if let Some(ancestors) = machine.hierarchy.ancestors.get(&leaf.to_string()) {
            // Generate SubstateOf impl for each ancestor
            for ancestor in ancestors {
                impls.push(quote! {
                    impl ::state_machines::SubstateOf<#ancestor> for #leaf {}
                });
            }
        }
    }

    Ok(impls)
}

/// Generate blanket impl blocks for superstate transitions.
///
/// For transitions that originate from a superstate, we generate:
/// ```rust,ignore
/// impl<C, S: SubstateOf<Flight>> Machine<C, S> {
///     pub fn abort(self) -> Result<Machine<C, Standby>, (Self, GuardError)> { ... }
/// }
/// ```
///
/// This allows the transition to be called from any substate of the superstate.
fn generate_superstate_transition_impls(machine: &StateMachine) -> Result<Vec<TokenStream2>> {
    let mut impls = Vec::new();
    let machine_name = &machine.name;

    // Group transitions by superstate
    for superstate in machine.hierarchy.all_superstates() {
        // Find all transitions that originate from this superstate
        if let Some(edges) = machine.transition_graph.outgoing(&superstate) {
            let methods: Vec<_> = edges
                .iter()
                .map(|edge| generate_superstate_transition_method(machine, &superstate, edge))
                .collect::<Result<Vec<_>>>()?;

            if !methods.is_empty() {
                let impl_block = quote! {
                    impl<C, S: ::state_machines::SubstateOf<#superstate>> #machine_name<C, S> {
                        #( #methods )*
                    }
                };
                impls.push(impl_block);
            }
        }
    }

    Ok(impls)
}

/// Generate a transition method for a superstate transition.
///
/// Similar to generate_transition_method but works with generic substates.
fn generate_superstate_transition_method(
    machine: &StateMachine,
    _superstate: &Ident,
    edge: &TransitionEdge,
) -> Result<TokenStream2> {
    // This is similar to generate_transition_method but simpler
    // since we don't have callbacks or guards at the superstate level yet
    let machine_name = &machine.name;
    let event_name = &edge.event;

    // Convert event name to snake_case for the method name
    let method_name = to_snake_case_ident(event_name);

    let target_state = &edge.target;
    let is_async = machine.async_mode;
    let core_path = quote!(::state_machines::core);

    // Build method signature (no payload support for now)
    let method_sig = if is_async {
        quote! {
            pub async fn #method_name(self)
        }
    } else {
        quote! {
            pub fn #method_name(self)
        }
    };

    let return_type = quote! {
        ::core::result::Result<#machine_name<C, #target_state>, (Self, #core_path::GuardError)>
    };

    // Build storage field transfers for target state
    let storage_transfers: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            let state_name = &spec.state_name;
            let ty = &spec.ty;

            if state_name == target_state {
                quote! {
                    #field: ::core::option::Option::Some(<#ty as ::core::default::Default>::default())
                }
            } else {
                quote! {
                    #field: ::core::option::Option::None
                }
            }
        })
        .collect();

    Ok(quote! {
        #method_sig -> #return_type {
            // Create new machine with target state
            let new_machine = #machine_name {
                ctx: self.ctx,
                _state: ::core::marker::PhantomData,
                #( #storage_transfers, )*
            };

            ::core::result::Result::Ok(new_machine)
        }
    })
}
