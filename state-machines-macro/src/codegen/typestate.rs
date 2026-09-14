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

    Ok(quote! {
        #markers
        #machine_struct
        #( #impls )*
        #( #substate_impls )*
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
/// - C: Context type for hardware access or shared state (generic if not specified)
/// - S: Current state type (typestate pattern)
///
/// If a concrete context type is specified in the macro, the struct will use that type directly.
/// Otherwise, it remains generic over C for maximum flexibility.
///
/// Contains:
/// - A PhantomData marker to track the state type
/// - A context field for hardware/external dependencies
/// - Storage fields for state-associated data (if any)
///
/// # Example Output
///
/// Generic context:
/// ```rust,ignore
/// pub struct FlightDeck<C, S> {
///     ctx: C,
///     _state: core::marker::PhantomData<S>,
///     __docking_data: Option<DockingData>,
/// }
/// ```
///
/// Concrete context:
/// ```rust,ignore
/// pub struct FlightDeck<S> {
///     ctx: MyContext,
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

    // Determine struct generics and context field type
    let (struct_generics, ctx_ty) = if let Some(concrete_ctx) = &machine.context {
        // Concrete context: only generic over state
        (quote! { <S> }, quote! { #concrete_ctx })
    } else {
        // Generic context: generic over both context and state
        (quote! { <C, S> }, quote! { C })
    };

    Ok(quote! {
        #[derive(Debug)]
        pub struct #machine_name #struct_generics {
            ctx: #ctx_ty,
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

        // Generate transition methods for outgoing transitions, plus a
        // non-consuming can_<event>() predicate for each
        if let Some(edges) = machine.transition_graph.outgoing(state) {
            for edge in edges {
                let method = generate_transition_method(machine, state, edge)?;
                methods.push(method);
                let can_method = generate_can_method(machine, edge)?;
                methods.push(can_method);
            }
        }

        let machine_name = &machine.name;

        // Determine impl generics and type parameters
        let (impl_generics, type_params) = if machine.context.is_some() {
            // Concrete context: impl for specific state only (struct is Machine<S>)
            (quote! {}, quote! { <#state> })
        } else {
            // Generic context: impl generic over C (struct is Machine<C, S>)
            (quote! { <C> }, quote! { <C, #state> })
        };

        let impl_block = quote! {
            impl #impl_generics #machine_name #type_params {
                #( #methods )*
            }
        };

        impls.push(impl_block);
    }

    // Generate generic impl block with storage accessors (Option-based)
    if !machine.state_storage.is_empty() {
        let storage_accessors = generate_storage_accessors(machine)?;
        let machine_name = &machine.name;

        // Determine impl generics for storage accessors
        let (impl_generics, type_params) = if machine.context.is_some() {
            // Concrete context: impl generic only over state (struct is Machine<S>)
            (quote! { <S> }, quote! { <S> })
        } else {
            // Generic context: impl generic over both (struct is Machine<C, S>)
            (quote! { <C, S> }, quote! { <C, S> })
        };

        let generic_impl = quote! {
            impl #impl_generics #machine_name #type_params {
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
/// The context parameter type depends on whether a concrete context was specified:
/// - Generic context: `ctx: C`
/// - Concrete context: `ctx: ConcreteType`
///
/// # Example Output
///
/// Generic:
/// ```rust,ignore
/// pub fn new(ctx: C) -> Self {
///     Self {
///         ctx,
///         _state: core::marker::PhantomData,
///         __data_field: None,
///     }
/// }
/// ```
///
/// Concrete:
/// ```rust,ignore
/// pub fn new(ctx: MyContext) -> Self {
///     Self {
///         ctx,
///         _state: core::marker::PhantomData,
///         __data_field: None,
///     }
/// }
/// ```
fn generate_constructor(machine: &StateMachine, _state: &Ident) -> Result<TokenStream2> {
    // All storage starts as None — even for the initial state — so that
    // constructing a machine never requires the data type to implement
    // Default. Data is auto-initialised when a transition enters its state;
    // use the Option-returning accessors before the first transition.
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

    // Determine context parameter type
    let ctx_param_ty = if let Some(concrete_ctx) = &machine.context {
        quote! { #concrete_ctx }
    } else {
        quote! { C }
    };

    Ok(quote! {
        pub fn new(ctx: #ctx_param_ty) -> Self {
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
/// Build the `Result<...>` return type for a transition method.
///
/// The success variant is the machine specialised to `target_state`; the error
/// variant carries `(Self, return_error_ty)`. The generic `C` context parameter
/// is included only when the machine has no concrete context type.
fn transition_return_type(
    machine: &StateMachine,
    target_state: &Ident,
    return_error_ty: &TokenStream2,
) -> TokenStream2 {
    let machine_name = &machine.name;
    if machine.context.is_some() {
        quote! {
            ::core::result::Result<#machine_name<#target_state>, (Self, #return_error_ty)>
        }
    } else {
        quote! {
            ::core::result::Result<#machine_name<C, #target_state>, (Self, #return_error_ty)>
        }
    }
}

/// Check whether a storage spec's owning state covers the given leaf state.
///
/// A leaf spec covers only itself; a superstate spec covers every
/// descendant leaf.
fn spec_covers(machine: &StateMachine, spec: &StateStorageSpec, leaf: &Ident) -> bool {
    if machine.hierarchy.is_superstate(&spec.state_name) {
        machine
            .hierarchy
            .expand_state(&spec.state_name, &machine.states)
            .iter()
            .any(|member| member == leaf)
    } else {
        &spec.state_name == leaf
    }
}

/// Build the per-field storage initialisers for the machine in its target state.
///
/// Leaf-state data: the target's field is default-initialised, every other
/// leaf field is reset to `None`.
///
/// Superstate data lives as long as the machine stays anywhere inside the
/// superstate: entering from outside default-initialises it, moving between
/// two of its substates carries the previous value over (via the
/// `__sm_prev_*` locals bound from the source machine), and leaving clears
/// it back to `None`.
fn storage_transfers(
    machine: &StateMachine,
    source_state: &Ident,
    target_state: &Ident,
) -> Vec<TokenStream2> {
    machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            let ty = &spec.ty;
            if machine.hierarchy.is_superstate(&spec.state_name) {
                let source_inside = spec_covers(machine, spec, source_state);
                let target_inside = spec_covers(machine, spec, target_state);
                match (source_inside, target_inside) {
                    (true, true) => {
                        let local = quote::format_ident!("__sm_prev_{}", field);
                        quote! { #field: #local }
                    }
                    (false, true) => quote! {
                        #field: ::core::option::Option::Some(<#ty as ::core::default::Default>::default())
                    },
                    _ => quote! { #field: ::core::option::Option::None },
                }
            } else if &spec.state_name == target_state {
                quote! {
                    #field: ::core::option::Option::Some(<#ty as ::core::default::Default>::default())
                }
            } else {
                quote! {
                    #field: ::core::option::Option::None
                }
            }
        })
        .collect()
}

/// Fields whose previous value is moved into the new machine (superstate
/// data on an intra-superstate transition). Rollback paths must rebind
/// these from the new machine, since the `__sm_prev_*` local was consumed.
fn preserved_storage_fields(
    machine: &StateMachine,
    source_state: &Ident,
    target_state: &Ident,
) -> Vec<Ident> {
    machine
        .state_storage
        .iter()
        .filter(|spec| {
            machine.hierarchy.is_superstate(&spec.state_name)
                && spec_covers(machine, spec, source_state)
                && spec_covers(machine, spec, target_state)
        })
        .map(|spec| spec.field.clone())
        .collect()
}

fn generate_transition_method(
    machine: &StateMachine,
    source_state: &Ident,
    edge: &TransitionEdge,
) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let event_name = &edge.event;
    let method_name = to_snake_case_ident(event_name);
    let target_state = &edge.target;
    let is_async = machine.async_mode;
    let core_path = quote!(::state_machines::core);
    let error_ty = machine.error.as_ref();

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

    let return_error_ty = if let Some(error_ty) = error_ty {
        quote! { #core_path::EventError<#error_ty> }
    } else {
        quote! { #core_path::GuardError }
    };

    let return_type = transition_return_type(machine, target_state, &return_error_ty);

    let guard_error = |guard: &Ident| {
        if error_ty.is_some() {
            quote! {
                #core_path::EventError::guard(
                    #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
                )
            }
        } else {
            quote! {
                #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
            }
        }
    };

    let kind_error = || {
        if error_ty.is_some() {
            quote! {
                #core_path::EventError::guard(
                    #core_path::GuardError::with_kind(callback_name, stringify!(#event_name), err.kind)
                )
            }
        } else {
            quote! {
                #core_path::GuardError::with_kind(callback_name, stringify!(#event_name), err.kind)
            }
        }
    };

    // Global callbacks are always invoked without the payload so one method
    // can serve every event its filters match, payload-carrying or not.
    let callback_call = |receiver: TokenStream2, callback: &Ident, use_payload: bool| {
        if use_payload && edge.payload.is_some() {
            if is_async {
                quote! { #receiver.#callback(#payload_ref).await }
            } else {
                quote! { #receiver.#callback(#payload_ref) }
            }
        } else if is_async {
            quote! { #receiver.#callback().await }
        } else {
            quote! { #receiver.#callback() }
        }
    };

    let mut guard_checks = Vec::new();
    for guard in &edge.guards {
        let guard_error = guard_error(guard);
        let check = if edge.payload.is_some() {
            if is_async {
                quote! {
                    if !self.#guard(&self.ctx, #payload_ref).await {
                        return ::core::result::Result::Err((self, #guard_error));
                    }
                }
            } else {
                quote! {
                    if !self.#guard(&self.ctx, #payload_ref) {
                        return ::core::result::Result::Err((self, #guard_error));
                    }
                }
            }
        } else if is_async {
            quote! {
                if !self.#guard(&self.ctx).await {
                    return ::core::result::Result::Err((self, #guard_error));
                }
            }
        } else {
            quote! {
                if !self.#guard(&self.ctx) {
                    return ::core::result::Result::Err((self, #guard_error));
                }
            }
        };
        guard_checks.push(check);
    }

    for guard in &edge.unless {
        let guard_error = guard_error(guard);
        let check = if edge.payload.is_some() {
            if is_async {
                quote! {
                    if self.#guard(&self.ctx, #payload_ref).await {
                        return ::core::result::Result::Err((self, #guard_error));
                    }
                }
            } else {
                quote! {
                    if self.#guard(&self.ctx, #payload_ref) {
                        return ::core::result::Result::Err((self, #guard_error));
                    }
                }
            }
        } else if is_async {
            quote! {
                if self.#guard(&self.ctx).await {
                    return ::core::result::Result::Err((self, #guard_error));
                }
            }
        } else {
            quote! {
                if self.#guard(&self.ctx) {
                    return ::core::result::Result::Err((self, #guard_error));
                }
            }
        };
        guard_checks.push(check);
    }

    let make_before_call = |callback: &Ident, use_payload: bool| {
        let call = callback_call(quote! { self }, callback, use_payload);
        if let Some(error_ty) = error_ty {
            quote! {
                let callback_result: ::core::result::Result<(), #error_ty> =
                    #core_path::FallibleCallbackReturn::into_result(#call);
                if let Err(source) = callback_result {
                    return ::core::result::Result::Err((
                        self,
                        #core_path::EventError::callback(
                            stringify!(#callback),
                            stringify!(#event_name),
                            source,
                        ),
                    ));
                }
            }
        } else {
            quote! {
                let (): () = #call;
            }
        }
    };

    let global_before_calls: Vec<_> = edge
        .global_before
        .iter()
        .map(|callback| make_before_call(callback, false))
        .collect();

    let before_calls: Vec<_> = edge
        .before
        .iter()
        .map(|callback| make_before_call(callback, true))
        .collect();

    let source_field_bindings: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            let local = quote::format_ident!("__sm_prev_{}", field);
            quote! { #field: #local }
        })
        .collect();

    let restore_source_fields = source_field_bindings.clone();

    let storage_transfers = storage_transfers(machine, source_state, target_state);

    // Superstate data carried into the new machine consumed its __sm_prev_*
    // local; rollback paths recover it from the new machine by rebinding
    // the same local name in the destructuring pattern.
    let preserved_rebinds: Vec<_> = preserved_storage_fields(machine, source_state, target_state)
        .into_iter()
        .map(|field| {
            let local = quote::format_ident!("__sm_prev_{}", field);
            quote! { #field: #local }
        })
        .collect();

    let make_after_call = |callback: &Ident, use_payload: bool| {
        let call = callback_call(quote! { new_machine }, callback, use_payload);
        if let Some(error_ty) = error_ty {
            quote! {
                let callback_result: ::core::result::Result<(), #error_ty> =
                    #core_path::FallibleCallbackReturn::into_result(#call);
                if let Err(source) = callback_result {
                    let #machine_name { ctx, #( #preserved_rebinds, )* .. } = new_machine;
                    let old_machine = #machine_name {
                        ctx,
                        _state: ::core::marker::PhantomData,
                        #( #restore_source_fields, )*
                    };
                    return ::core::result::Result::Err((
                        old_machine,
                        #core_path::EventError::callback(
                            stringify!(#callback),
                            stringify!(#event_name),
                            source,
                        ),
                    ));
                }
            }
        } else {
            quote! {
                let (): () = #call;
            }
        }
    };

    let after_calls: Vec<_> = edge
        .after
        .iter()
        .map(|callback| make_after_call(callback, true))
        .collect();

    let global_after_calls: Vec<_> = edge
        .global_after
        .iter()
        .map(|callback| make_after_call(callback, false))
        .collect();

    let rollback_old_machine = quote! {
        let #machine_name { ctx, #( #preserved_rebinds, )* .. } = new_machine;
        let old_machine = #machine_name {
            ctx,
            _state: ::core::marker::PhantomData,
            #( #restore_source_fields, )*
        };
    };

    let has_around = !edge.around.is_empty();
    if has_around {
        let around_before_checks: Vec<_> = edge
            .around
            .iter()
            .map(|callback| {
                let kind_error = kind_error();
                if is_async {
                    quote! {
                        match self.#callback(#core_path::AroundStage::Before).await {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                return ::core::result::Result::Err((self, #kind_error));
                            }
                        }
                    }
                } else {
                    quote! {
                        match self.#callback(#core_path::AroundStage::Before) {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                return ::core::result::Result::Err((self, #kind_error));
                            }
                        }
                    }
                }
            })
            .collect();

        let around_after_checks: Vec<_> = edge
            .around
            .iter()
            .map(|callback| {
                let kind_error = kind_error();
                if is_async {
                    quote! {
                        match new_machine.#callback(#core_path::AroundStage::AfterSuccess).await {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                #rollback_old_machine
                                return ::core::result::Result::Err((old_machine, #kind_error));
                            }
                        }
                    }
                } else {
                    quote! {
                        match new_machine.#callback(#core_path::AroundStage::AfterSuccess) {
                            #core_path::AroundOutcome::Proceed => {},
                            #core_path::AroundOutcome::Abort(err) => {
                                let callback_name = match &err.kind {
                                    #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                                    #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                                    #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                                };
                                #rollback_old_machine
                                return ::core::result::Result::Err((old_machine, #kind_error));
                            }
                        }
                    }
                }
            })
            .collect();

        Ok(quote! {
            #method_sig -> #return_type {
                #( #around_before_checks )*
                #( #guard_checks )*
                #( #global_before_calls )*
                #( #before_calls )*

                let #machine_name {
                    ctx,
                    _state: _,
                    #( #source_field_bindings, )*
                } = self;

                let mut new_machine = #machine_name {
                    ctx,
                    _state: ::core::marker::PhantomData,
                    #( #storage_transfers, )*
                };

                #( #after_calls )*
                #( #global_after_calls )*
                #( #around_after_checks )*

                ::core::result::Result::Ok(new_machine)
            }
        })
    } else {
        Ok(quote! {
            #method_sig -> #return_type {
                #( #guard_checks )*
                #( #global_before_calls )*
                #( #before_calls )*

                let #machine_name {
                    ctx,
                    _state: _,
                    #( #source_field_bindings, )*
                } = self;

                let mut new_machine = #machine_name {
                    ctx,
                    _state: ::core::marker::PhantomData,
                    #( #storage_transfers, )*
                };

                #( #after_calls )*
                #( #global_after_calls )*

                ::core::result::Result::Ok(new_machine)
            }
        })
    }
}

/// Generate a `can_<event>()` predicate for a single edge.
///
/// The predicate evaluates the edge's guards and unless conditions without
/// consuming the machine or running any callbacks, answering "would this
/// event succeed right now?". Events with a payload take it by reference,
/// since guards may inspect it. In async mode the predicate is async
/// because guards are.
fn generate_can_method(machine: &StateMachine, edge: &TransitionEdge) -> Result<TokenStream2> {
    let event_name = &edge.event;
    let snake = crate::codegen::utils::to_snake_case(&event_name.to_string());
    let method_name = syn::Ident::new(&format!("can_{}", snake), event_name.span());
    let is_async = machine.async_mode;

    let (method_sig, payload_args) = if let Some(payload_ty) = &edge.payload {
        let sig = if is_async {
            quote! { pub async fn #method_name(&self, payload: &#payload_ty) }
        } else {
            quote! { pub fn #method_name(&self, payload: &#payload_ty) }
        };
        (sig, quote! { , payload })
    } else {
        let sig = if is_async {
            quote! { pub async fn #method_name(&self) }
        } else {
            quote! { pub fn #method_name(&self) }
        };
        (sig, quote! {})
    };

    let maybe_await = if is_async {
        quote! { .await }
    } else {
        quote! {}
    };

    let guard_checks: Vec<_> = edge
        .guards
        .iter()
        .map(|guard| {
            quote! {
                if !self.#guard(&self.ctx #payload_args) #maybe_await {
                    return false;
                }
            }
        })
        .collect();

    let unless_checks: Vec<_> = edge
        .unless
        .iter()
        .map(|guard| {
            quote! {
                if self.#guard(&self.ctx #payload_args) #maybe_await {
                    return false;
                }
            }
        })
        .collect();

    Ok(quote! {
        /// Check whether this event's guards would allow the transition
        /// right now, without consuming the machine or running callbacks.
        #method_sig -> bool {
            #( #guard_checks )*
            #( #unless_checks )*
            true
        }
    })
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
        let snake = crate::codegen::utils::to_snake_case(&state_str);
        let data_method = syn::Ident::new(&format!("{}_data", snake), state_name.span());
        let data_mut_method = syn::Ident::new(&format!("{}_data_mut", snake), state_name.span());

        // A leaf's guaranteed accessors live on that leaf's impl. Superstate
        // data is guaranteed while inside the superstate, so its accessors
        // are emitted on every descendant leaf instead — the superstate
        // marker itself is never a machine's state parameter.
        let impl_states = if machine.hierarchy.is_superstate(state_name) {
            machine.hierarchy.expand_state(state_name, &machine.states)
        } else {
            vec![state_name.clone()]
        };

        for impl_state in impl_states {
            // Determine impl generics and type parameters
            let (impl_generics, type_params) = if machine.context.is_some() {
                // Concrete context (struct is Machine<S>)
                (quote! {}, quote! { <#impl_state> })
            } else {
                // Generic context (struct is Machine<C, S>)
                (quote! { <C> }, quote! { <C, #impl_state> })
            };

            // Generate state-specific impl block
            let impl_block = quote! {
                impl #impl_generics #machine_name #type_params {
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

// Note: there is deliberately no blanket `impl<S: SubstateOf<Super>>` block
// for superstate-sourced transitions. `build_transition_graph` expands a
// superstate source to its descendant leaves, so each leaf gets a full
// inherent transition method — guards, callbacks, and payloads included.
// A blanket impl would duplicate those method names and make call sites
// ambiguous without adding capability.
