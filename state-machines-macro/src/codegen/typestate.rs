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

use crate::codegen::utils::{
    ctx_generics, ctx_ty, empty_storage_inits, machine_params, maybe_async, maybe_await,
    to_snake_case, to_snake_case_ident, transition_error_ty,
};
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
    let outcomes = super::branching::enums(machine);

    Ok(quote! {
        #markers
        #machine_struct
        #outcomes
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
    // Leaf states followed by superstates
    let markers: Vec<_> = machine
        .states
        .iter()
        .chain(machine.hierarchy.all_superstates())
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
    let history_fields = super::history::fields(machine)
        .into_iter()
        .map(|field| quote! { #field: ::core::option::Option<usize> });

    let struct_params = machine_params(machine, quote! { S });
    let ctx_ty = ctx_ty(machine);

    Ok(quote! {
        #[derive(Debug)]
        pub struct #machine_name #struct_params {
            ctx: #ctx_ty,
            _state: ::core::marker::PhantomData<S>,
            #( #storage_fields, )*
            #( #history_fields, )*
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
    let machine_name = &machine.name;
    let generics = ctx_generics(machine);

    for state in &machine.states {
        let mut methods = vec![super::finality::state_methods(machine, state)];
        methods.push(generate_start(machine, state));

        // Generate constructor for initial state
        if state == &machine.initial {
            let constructor = generate_constructor(machine, state)?;
            methods.push(constructor);
        }

        // Generate transition methods for outgoing transitions, plus a
        // non-consuming can_<event>() predicate for each
        for edges in super::branching::groups(machine, state) {
            if edges.len() > 1 {
                methods.push(super::branching::methods(machine, state, &edges)?);
            } else {
                let edge = edges[0];
                let method = generate_transition_method(machine, state, edge, None, true)?;
                methods.push(method);
                let can_method = generate_can_method(machine, edge)?;
                methods.push(can_method);
                if machine.event(&edge.event).automatic {
                    let helper = quote::format_ident!("__sm_auto_{}", edge.event);
                    methods.push(generate_transition_method(
                        machine,
                        state,
                        edge,
                        Some(&helper),
                        false,
                    )?);
                }
            }
        }

        let params = machine_params(machine, state);
        let impl_block = quote! {
            impl #generics #machine_name #params {
                #( #methods )*
            }
        };

        impls.push(impl_block);
    }

    // Generate generic impl block with storage accessors (Option-based)
    if !machine.state_storage.is_empty() {
        let storage_accessors = generate_storage_accessors(machine)?;
        let params = machine_params(machine, quote! { S });

        let generic_impl = quote! {
            impl #params #machine_name #params {
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
    let storage_inits = empty_storage_inits(machine);
    let ctx_param_ty = ctx_ty(machine);

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

/// Explicit startup is separate from inert construction and restoration.
fn generate_start(machine: &StateMachine, state: &Ident) -> TokenStream2 {
    let async_ = maybe_async(machine.async_mode);
    let await_ = maybe_await(machine.async_mode);
    let mut path = machine
        .hierarchy
        .ancestors
        .get(&state.to_string())
        .cloned()
        .unwrap_or_default();
    path.push(state.clone());
    let hooks = path
        .iter()
        .flat_map(|scope| machine.lifecycle_for(scope))
        .flat_map(|hooks| &hooks.enter);
    let error = transition_error_ty(machine);
    let calls = hooks.map(|hook| {
        if let Some(ty) = &machine.error {
            quote! {
                let result: Result<(), #ty> = ::state_machines::core::FallibleCallbackReturn::into_result(self.#hook() #await_);
                if let Err(source) = result {
                    let error = ::state_machines::EventError::callback(stringify!(#hook), "__initialize", source);
                    return Err((self, error));
                }
            }
        } else {
            quote! { let (): () = self.#hook() #await_; }
        }
    });
    quote! {
        /// Run active ancestor/leaf entry hooks outer-to-inner.
        /// Call once for fresh startup; restore is intentionally inert.
        pub #async_ fn initialize(mut self) -> Result<Self, (Self, #error)> {
            #( #calls )*
            Ok(self)
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

/// `field: __sm_prev_field` — moves a storage field through the local it was
/// destructured into, in either direction.
fn prev_binding(field: &Ident) -> TokenStream2 {
    let local = quote::format_ident!("__sm_prev_{}", field);
    quote! { #field: #local }
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
    edge: &TransitionEdge,
) -> Vec<TokenStream2> {
    machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            let ty = &spec.ty;
            let init = quote! {
                #field: ::core::option::Option::Some(<#ty as ::core::default::Default>::default())
            };
            let clear = quote! { #field: ::core::option::Option::None };
            if machine.hierarchy.is_superstate(&spec.state_name) {
                let source_inside = spec_covers(machine, spec, source_state);
                let target_inside = spec_covers(machine, spec, target_state);
                match (source_inside, target_inside) {
                    (true, true)
                        if !machine.reentered_superstate(source_state, edge, &spec.state_name) =>
                    {
                        prev_binding(field)
                    }
                    (true, true) => init,
                    (false, true) => init,
                    _ => clear,
                }
            } else if &spec.state_name == target_state {
                init
            } else {
                clear
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
    edge: &TransitionEdge,
) -> Vec<Ident> {
    machine
        .state_storage
        .iter()
        .filter(|spec| {
            machine.hierarchy.is_superstate(&spec.state_name)
                && !machine.reentered_superstate(source_state, edge, &spec.state_name)
                && spec_covers(machine, spec, source_state)
                && spec_covers(machine, spec, target_state)
        })
        .map(|spec| spec.field.clone())
        .collect()
}

/// One side of a transition method: before the machine is moved into its
/// target state, or after.
struct Phase {
    /// The machine callbacks are invoked on.
    receiver: TokenStream2,
    /// The `AroundStage` variant passed to around callbacks.
    stage: TokenStream2,
    /// Statements that rebuild `owner` before an early return.
    rollback: TokenStream2,
    /// The pre-transition machine handed back in the `Err` tuple.
    owner: TokenStream2,
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
pub(super) fn generate_transition_method(
    machine: &StateMachine,
    source_state: &Ident,
    edge: &TransitionEdge,
    name_override: Option<&Ident>,
    check_guards: bool,
) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let event_name = &edge.event;
    let method_name = name_override
        .cloned()
        .unwrap_or_else(|| to_snake_case_ident(event_name));
    let visibility = if name_override.is_some() {
        quote! {}
    } else {
        quote! { pub }
    };
    let target_state = &edge.target;
    let maybe_async = maybe_async(machine.async_mode);
    let maybe_await = maybe_await(machine.async_mode);
    let core_path = quote!(::state_machines::core);
    let error_ty = machine.error.as_ref();

    // Guards and local callbacks borrow the payload; the method takes it by value.
    let mutability = edge.data.as_ref().map(|_| quote! { mut });
    let (payload_param, payload_ref) = match &edge.payload {
        Some(payload_ty) => (
            quote! { , #mutability payload: #payload_ty },
            quote! { &payload },
        ),
        None => (quote! {}, quote! {}),
    };
    let guard_args = if edge.payload.is_some() {
        quote! { , #payload_ref }
    } else {
        quote! {}
    };
    let method_sig = quote! { #visibility #maybe_async fn #method_name(mut self #payload_param) };

    let return_error_ty = if let Some(error_ty) = error_ty {
        quote! { #core_path::EventError<#error_ty> }
    } else {
        quote! { #core_path::GuardError }
    };

    let target_params = machine_params(machine, target_state);
    let return_type = quote! {
        ::core::result::Result<#machine_name #target_params, (Self, #return_error_ty)>
    };

    // With a declared error type, guard failures travel inside `EventError`.
    let wrap_guard_error = |err: TokenStream2| {
        if error_ty.is_some() {
            quote! { #core_path::EventError::guard(#err) }
        } else {
            err
        }
    };

    let kind_error = wrap_guard_error(quote! {
        #core_path::GuardError::with_kind(callback_name, stringify!(#event_name), err.kind)
    });

    // Cleanup is infallible and runs on the recovered source machine. It
    // cannot replace the original error or silently claim an external
    // transaction was rolled back.
    let failure = |owner: TokenStream2, error: TokenStream2| {
        let notifications = edge.hooks.on_error.iter().map(|callback| {
            quote! { let (): () = failed_machine.#callback(&failure) #maybe_await; }
        });
        quote! {
            let mut failed_machine = #owner;
            let failure = #error;
            #( #notifications )*
            return ::core::result::Result::Err((failed_machine, failure));
        }
    };

    // `guards` reject when they return false, `unless` when they return true.
    let guard_check = |guard: &Ident, reject_when: TokenStream2| {
        let guard_error = wrap_guard_error(quote! {
            #core_path::GuardError::new(stringify!(#guard), stringify!(#event_name))
        });
        let fail = failure(quote! { self }, guard_error);
        quote! {
            if #reject_when self.#guard(&self.ctx #guard_args) #maybe_await {
                #fail
            }
        }
    };

    let guard_checks: Vec<_> = edge
        .hooks
        .guards
        .iter()
        .map(|guard| guard_check(guard, quote! { ! }))
        .chain(
            edge.hooks
                .unless
                .iter()
                .map(|guard| guard_check(guard, quote! {})),
        )
        .filter(|_| check_guards)
        .collect();

    let mut source_field_bindings: Vec<_> = machine
        .state_storage
        .iter()
        .map(|spec| prev_binding(&spec.field))
        .collect();
    let history_fields = super::history::fields(machine);
    source_field_bindings.extend(history_fields.iter().map(prev_binding));

    let mut storage_transfers = if edge.internal {
        source_field_bindings.clone()
    } else {
        storage_transfers(machine, source_state, target_state, edge)
    };
    if !edge.internal {
        storage_transfers.extend(history_fields.iter().map(prev_binding));
    }
    let factory = edge.data.as_ref().map(|factory| {
        let args = if edge.payload.is_some() {
            quote! { &mut payload }
        } else {
            quote! {}
        };
        quote! { let __sm_entry_data = self.#factory(#args) #maybe_await; }
    });
    if edge.data.is_some() {
        let spec = machine
            .state_storage
            .iter()
            .find(|spec| &spec.state_name == target_state)
            .unwrap();
        let index = machine
            .state_storage
            .iter()
            .position(|item| item.field == spec.field)
            .unwrap();
        let field = &spec.field;
        storage_transfers[index] = quote! { #field: Some(__sm_entry_data) };
    }

    // Superstate data carried into the new machine consumed its __sm_prev_*
    // local; rollback paths recover it from the new machine by rebinding
    // the same local name in the destructuring pattern.
    let preserved_rebinds: Vec<_> = if edge.internal {
        source_field_bindings.clone()
    } else {
        preserved_storage_fields(machine, source_state, target_state, edge)
            .iter()
            .map(prev_binding)
            .collect()
    };

    let before = Phase {
        receiver: quote! { self },
        stage: quote! { Before },
        rollback: quote! {},
        owner: quote! { self },
    };
    let after = Phase {
        receiver: quote! { new_machine },
        stage: quote! { AfterSuccess },
        rollback: quote! {
            let #machine_name { ctx, #( #preserved_rebinds, )* .. } = new_machine;
            let old_machine = #machine_name {
                ctx,
                _state: ::core::marker::PhantomData,
                #( #source_field_bindings, )*
            };
        },
        owner: quote! { old_machine },
    };

    // Global callbacks are always invoked without the payload so one method
    // can serve every event its filters match, payload-carrying or not.
    let callback_step = |phase: &Phase, callback: &Ident, use_payload: bool| {
        let Phase {
            receiver,
            rollback,
            owner,
            ..
        } = phase;
        let args = if use_payload {
            payload_ref.clone()
        } else {
            quote! {}
        };
        let call = quote! { #receiver.#callback(#args) #maybe_await };
        if let Some(error_ty) = error_ty {
            let fail = failure(
                owner.clone(),
                quote! {
                    #core_path::EventError::callback(
                        stringify!(#callback), stringify!(#event_name), source,
                    )
                },
            );
            quote! {
                let callback_result: ::core::result::Result<(), #error_ty> =
                    #core_path::FallibleCallbackReturn::into_result(#call);
                if let Err(source) = callback_result {
                    #rollback
                    #fail
                }
            }
        } else {
            quote! {
                let (): () = #call;
            }
        }
    };

    let around_step = |phase: &Phase, callback: &Ident| {
        let Phase {
            receiver,
            stage,
            rollback,
            owner,
        } = phase;
        let fail = failure(owner.clone(), kind_error.clone());
        quote! {
            match #receiver.#callback(#core_path::AroundStage::#stage) #maybe_await {
                #core_path::AroundOutcome::Proceed => {},
                #core_path::AroundOutcome::Abort(err) => {
                    let callback_name = match &err.kind {
                        #core_path::TransitionErrorKind::GuardFailed { guard } => *guard,
                        #core_path::TransitionErrorKind::ActionFailed { action } => *action,
                        #core_path::TransitionErrorKind::InvalidTransition => stringify!(#callback),
                    };
                    #rollback
                    #fail
                }
            }
        }
    };

    let around_before_checks = edge.hooks.around.iter().map(|cb| around_step(&before, cb));
    let global_before_calls = edge
        .global_before
        .iter()
        .map(|cb| callback_step(&before, cb, false));
    let before_calls = edge
        .hooks
        .before
        .iter()
        .map(|cb| callback_step(&before, cb, true));
    let after_calls = edge
        .hooks
        .after
        .iter()
        .map(|cb| callback_step(&after, cb, true));
    let global_after_calls = edge
        .global_after
        .iter()
        .map(|cb| callback_step(&after, cb, false));
    let around_after_checks = edge.hooks.around.iter().map(|cb| around_step(&after, cb));
    let (exit_hooks, enter_hooks) = machine.lifecycle_callbacks(source_state, target_state, edge);
    let exit_calls = exit_hooks
        .iter()
        .map(|cb| callback_step(&before, cb, false));
    let enter_calls = enter_hooks
        .iter()
        .map(|cb| callback_step(&after, cb, false));
    let mut completed = super::finality::completed(machine, target_state);
    completed.dedup();
    let complete_calls = completed
        .iter()
        .filter(|_| !edge.internal)
        .flat_map(|scope| {
            machine
                .lifecycle
                .iter()
                .filter(move |hooks| &hooks.state == *scope)
        })
        .flat_map(|hooks| &hooks.complete)
        .map(|cb| callback_step(&after, cb, false));
    let history_update = super::history::record_exit(machine, source_state, edge);

    Ok(quote! {
        #method_sig -> #return_type {
            #( #around_before_checks )*
            #( #guard_checks )*
            #( #global_before_calls )*
            #( #before_calls )*
            #( #exit_calls )*
            #factory

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

            #( #enter_calls )*
            #( #after_calls )*
            #( #global_after_calls )*
            #( #around_after_checks )*
            #( #complete_calls )*
            #history_update

            ::core::result::Result::Ok(new_machine)
        }
    })
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
    let snake = to_snake_case(&event_name.to_string());
    let method_name = syn::Ident::new(&format!("can_{}", snake), event_name.span());
    let maybe_async = maybe_async(machine.async_mode);
    let maybe_await = maybe_await(machine.async_mode);

    let (payload_param, payload_args) = match &edge.payload {
        Some(payload_ty) => (quote! { , payload: &#payload_ty }, quote! { , payload }),
        None => (quote! {}, quote! {}),
    };
    let method_sig = quote! { pub #maybe_async fn #method_name(&self #payload_param) };

    let guard_checks: Vec<_> = edge
        .hooks
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
        .hooks
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

    // The payload is borrowed as `&T` whatever `T` is, so a `String` or
    // `Vec` payload trips `ptr_arg` in the caller's crate even though the
    // user cannot choose a slice type here.
    Ok(quote! {
        /// Check whether this event's guards would allow the transition
        /// right now, without consuming the machine or running callbacks.
        #[allow(clippy::ptr_arg)]
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

/// Generate state-specific optional data accessors.
///
/// For each state with associated data, we generate an impl block with
/// a uniquely named accessor based on the state name:
/// ```rust,ignore
/// impl<C> Machine<C, LaunchPrep> {
///     pub fn launch_prep_data(&self) -> Option<&PrepData> {
///         self.__state_data_launch_prep.as_ref()
///     }
///     pub fn launch_prep_data_mut(&mut self) -> Option<&mut PrepData> {
///         self.__state_data_launch_prep.as_mut()
///     }
/// }
/// ```
///
/// State membership restricts access, but constructors/restore can leave data absent.
/// The method names are unique per state to avoid conflicts.
fn generate_state_specific_accessors(machine: &StateMachine) -> Result<Vec<TokenStream2>> {
    let mut impls = Vec::new();
    let machine_name = &machine.name;
    let generics = ctx_generics(machine);

    for spec in &machine.state_storage {
        let state_name = &spec.state_name;
        let field = &spec.field;
        let ty = &spec.ty;

        // Generate method names from state name: LaunchPrep -> launch_prep_data
        let snake = to_snake_case(&state_name.to_string());
        let data_method = syn::Ident::new(&format!("{}_data", snake), state_name.span());
        let data_mut_method = syn::Ident::new(&format!("{}_data_mut", snake), state_name.span());
        let with_data_method = syn::Ident::new(&format!("with_{}_data", snake), state_name.span());

        // A leaf's accessors live on that leaf's impl. Superstate
        // data is accessible while inside the superstate, so its accessors
        // are emitted on every descendant leaf instead — the superstate
        // marker itself is never a machine's state parameter.
        let impl_states = if machine.hierarchy.is_superstate(state_name) {
            machine.hierarchy.expand_state(state_name, &machine.states)
        } else {
            std::slice::from_ref(state_name)
        };

        for impl_state in impl_states {
            let params = machine_params(machine, impl_state);
            let impl_block = quote! {
                impl #generics #machine_name #params {
                    /// Supply owned active-state data without Default or Clone.
                    pub fn #with_data_method(mut self, data: #ty) -> Self {
                        self.#field = Some(data);
                        self
                    }
                    /// Access the state-associated data for this specific state.
                    ///
                    /// Initial construction and restore may leave active data absent.
                    pub fn #data_method(&self) -> ::core::option::Option<&#ty> {
                        self.#field.as_ref()
                    }

                    /// Mutably access the state-associated data for this specific state.
                    ///
                    /// Initial construction and restore may leave active data absent.
                    pub fn #data_mut_method(&mut self) -> ::core::option::Option<&mut #ty> {
                        self.#field.as_mut()
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
