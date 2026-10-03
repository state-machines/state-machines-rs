//! Dynamic dispatch code generation.
//!
//! Generates runtime event-driven state machine wrappers that work alongside
//! the compile-time typestate pattern. Only generated when:
//! - The `dynamic` feature flag is enabled, OR
//! - The macro explicitly specifies `dynamic: true`

use crate::codegen::utils::{
    ctx_generics, ctx_ty, empty_storage_inits, event_pascal, machine_params, maybe_async,
    maybe_await, to_snake_case, to_snake_case_ident,
};
use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Result;

/// The edges leaving `state`, flattened across the optional adjacency entry.
fn outgoing_edges<'a>(
    machine: &'a StateMachine,
    state: &syn::Ident,
) -> impl Iterator<Item = &'a TransitionEdge> {
    machine
        .transition_graph
        .outgoing(state)
        .into_iter()
        .flatten()
}

/// The `true && guard.. && !unless..` boolean expression that decides whether an
/// edge's transition is currently enabled.
///
/// `payload_ref` carries the extra argument tokens (e.g. `, payload`) passed to
/// guards for payload-bearing events, or is empty for payload-free ones.
fn edge_guard_condition(
    edge: &TransitionEdge,
    is_async: bool,
    payload_ref: &TokenStream2,
) -> TokenStream2 {
    let maybe_await = maybe_await(is_async);
    let guard_checks = edge.hooks.guards.iter().map(|guard| {
        quote! { machine.#guard(&machine.ctx #payload_ref) #maybe_await }
    });
    let unless_checks = edge.hooks.unless.iter().map(|guard| {
        quote! { !machine.#guard(&machine.ctx #payload_ref) #maybe_await }
    });
    quote! { true #( && #guard_checks )* #( && #unless_checks )* }
}

/// Generate dynamic dispatch wrapper code for the state machine.
///
/// This generates:
/// - Event enum for runtime event dispatch
/// - AnyState enum wrapping all typed state machines
/// - DynamicMachine struct with handle() method
/// - Conversion methods between typestate and dynamic modes
pub fn generate_dynamic_wrapper(machine: &StateMachine) -> Result<TokenStream2> {
    let event_enum = generate_event_enum(machine)?;
    let any_state_enum = generate_any_state_enum(machine)?;
    let dynamic_machine = generate_dynamic_machine(machine)?;
    let conversions = generate_conversions(machine)?;

    Ok(quote! {
        #event_enum
        #any_state_enum
        #dynamic_machine
        #conversions
    })
}

/// Generate the Event enum from the events definition.
///
/// Example output:
/// ```ignore
/// #[derive(Debug)]
/// pub enum FlightEvent {
///     Launch,
///     Land,
///     SetThrust(u8),  // With payload
/// }
/// ```
fn generate_event_enum(machine: &StateMachine) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let event_name = quote::format_ident!("{}Event", machine_name);

    let enum_variants = machine.events.iter().map(|event| {
        let pascal_name = event_pascal(&event.name);
        if let Some(payload_ty) = &event.payload {
            quote! { #pascal_name(#payload_ty) }
        } else {
            quote! { #pascal_name }
        }
    });

    let match_arms = machine.events.iter().map(|event| {
        let pascal_name = event_pascal(&event.name);
        let name_str = event.name.to_string();
        if event.payload.is_some() {
            quote! { Self::#pascal_name(_) => #name_str }
        } else {
            quote! { Self::#pascal_name => #name_str }
        }
    });

    Ok(quote! {
        #[derive(Debug)]
        pub enum #event_name {
            #(#enum_variants,)*
        }

        impl #event_name {
            /// Get the name of this event as a static string.
            pub fn name(&self) -> &'static str {
                match self {
                    #(#match_arms,)*
                }
            }
        }
    })
}

/// Generate the AnyState enum that wraps all typed state machines.
///
/// Example output:
/// ```ignore
/// enum AnyFlightState {
///     Docked(FlightController<Docked>),
///     InFlight(FlightController<InFlight>),
///     Landed(FlightController<Landed>),
/// }
/// ```
fn generate_any_state_enum(machine: &StateMachine) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let any_state_name = quote::format_ident!("Any{}State", machine_name);

    // Generate enum variants for each state
    let variants = machine.states.iter().map(|state| {
        let params = machine_params(machine, state);
        quote! { #state(#machine_name #params) }
    });

    // Generate match arms for the name() method
    let name_arms = machine.states.iter().map(|state| {
        let state_str = state.to_string();
        quote! { Self::#state(_) => #state_str }
    });

    let generics = ctx_generics(machine);

    Ok(quote! {
        /// Internal enum wrapping all typed state machines.
        ///
        /// This enables runtime polymorphism over different states while
        /// preserving the compile-time safety of the typestate pattern.
        #[derive(Debug)]
        enum #any_state_name #generics {
            #(#variants,)*
        }

        impl #generics #any_state_name #generics {
            /// Get the name of the current state.
            fn name(&self) -> &'static str {
                match self {
                    #(#name_arms,)*
                }
            }
        }
    })
}

/// Generate the DynamicMachine struct with handle() method.
///
/// Example output:
/// ```ignore
/// pub struct DynamicFlightController {
///     inner: AnyFlightState,
/// }
///
/// impl DynamicFlightController {
///     pub fn handle(&mut self, event: FlightEvent) -> Result<(), DynamicError> {
///         // Runtime dispatch logic
///     }
/// }
/// ```
fn generate_dynamic_machine(machine: &StateMachine) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let dynamic_name = quote::format_ident!("Dynamic{}", machine_name);
    let any_state_name = quote::format_ident!("Any{}State", machine_name);
    let event_name = quote::format_ident!("{}Event", machine_name);
    let state_enum_name = quote::format_ident!("{}State", machine_name);
    let initial_state = &machine.initial;
    let is_async = machine.async_mode;
    let maybe_async = maybe_async(is_async);
    let maybe_await = maybe_await(is_async);
    let (dynamic_error_ty, dynamic_error_ctor) = match &machine.error {
        Some(error_ty) => (
            quote! { state_machines::DynamicError<#error_ty> },
            quote! { state_machines::DynamicError::<#error_ty> },
        ),
        None => (
            quote! { state_machines::DynamicError },
            quote! { state_machines::DynamicError },
        ),
    };
    let map_event_error = if machine.error.is_some() {
        quote! { state_machines::DynamicError::from_event_error(err) }
    } else {
        quote! { state_machines::DynamicError::from_guard_error(err) }
    };

    // Generate match arms for handle() method
    let mut match_arms = Vec::new();

    for event in &machine.events {
        let event_snake = &event.name; // snake_case from macro definition
        let event_pascal = event_pascal(event_snake);
        let event_method = to_snake_case_ident(event_snake); // method name (already snake_case)

        // Get all transitions for this event from the transition graph
        for state in &machine.states {
            if let Some(edges) = machine.transition_graph.outgoing(state) {
                for edge in edges {
                    if edge.event == *event_snake {
                        let source_state = state;
                        let target_state = &edge.target;

                        // Generate the match arm for this transition
                        // Use event_pascal for enum variant matching
                        // Use event_method for calling the snake_case typestate method
                        let (payload_pattern, payload_arg) = if event.payload.is_some() {
                            (quote! { (payload) }, quote! { payload })
                        } else {
                            (quote! {}, quote! {})
                        };
                        let arm = quote! {
                            (#any_state_name::#source_state(m), #event_name::#event_pascal #payload_pattern) => {
                                match m.#event_method(#payload_arg) #maybe_await {
                                    Ok(new_machine) => #any_state_name::#target_state(new_machine),
                                    Err((old_machine, err)) => {
                                        self.inner = ::core::option::Option::Some(#any_state_name::#source_state(old_machine));
                                        return Err(#map_event_error);
                                    }
                                }
                            }
                        };

                        match_arms.push(arm);
                    }
                }
            }
        }
    }

    // Add a catch-all arm for invalid transitions
    let catch_all = quote! {
        (state, event) => {
            let state_name = state.name();
            self.inner = ::core::option::Option::Some(state);
            return Err(#dynamic_error_ctor::invalid_transition(
                state_name,
                event.name(),
            ));
        }
    };

    let handle_sig = quote! {
        pub #maybe_async fn handle(&mut self, event: #event_name) -> Result<(), #dynamic_error_ty>
    };

    let available_event_arms = machine.states.iter().map(|state| {
        let checks = outgoing_edges(machine, state)
            .filter(|edge| edge.payload.is_none())
            .map(|edge| {
                let event_pascal = event_pascal(&edge.event);
                let condition = edge_guard_condition(edge, is_async, &quote! {});
                quote! {
                    if #condition {
                        events.push(#event_name::#event_pascal);
                    }
                }
            })
            .collect::<Vec<_>>();

        if checks.is_empty() {
            quote! {
                #any_state_name::#state(_) => {}
            }
        } else {
            quote! {
                #any_state_name::#state(machine) => {
                    #( #checks )*
                }
            }
        }
    });

    let is_available_event_arms = machine.states.iter().flat_map(|state| {
        outgoing_edges(machine, state)
            .map(|edge| {
                let event_pascal = event_pascal(&edge.event);
                let (event_pattern, payload_ref) = if edge.payload.is_some() {
                    (
                        quote! { #event_name::#event_pascal(payload) },
                        quote! { , payload },
                    )
                } else {
                    (quote! { #event_name::#event_pascal }, quote! {})
                };
                let condition = edge_guard_condition(edge, is_async, &payload_ref);

                quote! {
                    (#any_state_name::#state(machine), #event_pattern) => {
                        #condition
                    }
                }
            })
            .collect::<Vec<_>>()
    });

    let available_events_method = quote! {
        /// Return the payload-free events currently enabled by state and guards.
        ///
        /// Events with payloads are omitted because their guards cannot be
        /// evaluated without a payload value.
        pub #maybe_async fn get_available_events(
            &self,
        ) -> ::state_machines::__private::Vec<#event_name> {
            #[allow(unused_mut)]
            let mut events = ::state_machines::__private::Vec::new();
            match self.inner.as_ref()
                .expect("dynamic machine in invalid state")
            {
                #( #available_event_arms, )*
            }
            events
        }

        /// Return whether this event is enabled by the current state and guards.
        pub #maybe_async fn is_available_event(&self, event: &#event_name) -> bool {
            match (
                self.inner.as_ref()
                    .expect("dynamic machine in invalid state"),
                event,
            ) {
                #( #is_available_event_arms, )*
                _ => false,
            }
        }
    };

    let generics = ctx_generics(machine);
    let ctx_param_ty = ctx_ty(machine);

    let state_variants = &machine.states;
    let state_name_arms = machine.states.iter().map(|state| {
        let state_str = state.to_string();
        quote! { Self::#state => #state_str }
    });
    let current_state_arms = machine.states.iter().map(|state| {
        quote! { #any_state_name::#state(_) => #state_enum_name::#state }
    });
    let finished_arms = machine.states.iter().map(|state| {
        quote! { #any_state_name::#state(machine) => machine.is_finished() }
    });
    let completion_arms = machine.states.iter().map(|state| {
        quote! { #any_state_name::#state(machine) => machine.completion_events() }
    });

    let initial_state_constructor = quote! {
        #any_state_name::#initial_state(#machine_name::new(ctx))
    };
    let state_constructor_arms = machine.states.iter().map(|state| {
        if state == initial_state {
            return quote! {
                #state_enum_name::#state => #any_state_name::#state(#machine_name::new(ctx))
            };
        }
        let storage_inits = empty_storage_inits(machine);
        quote! {
            #state_enum_name::#state => #any_state_name::#state(
                #machine_name {
                    ctx,
                    _state: ::core::marker::PhantomData,
                    #( #storage_inits, )*
                }
            )
        }
    });

    // Default impl only for generic context with Default bound, or concrete context with Default
    let default_impl = if let Some(concrete_ctx) = &machine.context {
        // Concrete context: only generate Default impl if the concrete type has Default
        // We can't check that at macro time, so we conditionally generate with where clause
        quote! {
            impl Default for #dynamic_name where #concrete_ctx: ::core::default::Default {
                fn default() -> Self {
                    Self::new(<#concrete_ctx as ::core::default::Default>::default())
                }
            }
        }
    } else {
        // Generic context: use C: Default bound
        quote! {
            impl<C: ::core::default::Default> Default for #dynamic_name<C> {
                fn default() -> Self {
                    Self::new(C::default())
                }
            }
        }
    };

    // Generate state data accessor methods
    let state_data_accessors = if machine.state_storage.is_empty() {
        quote! {}
    } else {
        let accessor_methods = machine.state_storage.iter().map(|spec| {
            let state_name = &spec.state_name;
            let data_ty = &spec.ty;
            let field = &spec.field;
            let state_snake = to_snake_case(&state_name.to_string());
            let read_method = quote::format_ident!("{}_data", state_snake);
            let write_method = quote::format_ident!("{}_data_mut", state_snake);
            let set_method = quote::format_ident!("set_{}_data", state_snake);
            let state_str = state_name.to_string();
            let reachable_states = machine
                .hierarchy
                .expand_state(state_name, &machine.states);

            if reachable_states.is_empty() {
                quote! {}
            } else {
                let read_match_arms = reachable_states.iter().map(|reachable| {
                    quote! { #any_state_name::#reachable(machine) => machine.#field.as_ref(), }
                });

                let write_match_arms = reachable_states.iter().map(|reachable| {
                    quote! { #any_state_name::#reachable(machine) => machine.#field.as_mut(), }
                });

                let set_match_arms = reachable_states.iter().map(|reachable| {
                    quote! {
                        #any_state_name::#reachable(machine) => {
                            machine.#field = ::core::option::Option::Some(data);
                            ::core::result::Result::Ok(())
                        }
                    }
                });

                quote! {
                    /// Read access to state data when in the `#state_name` state.
                    ///
                    /// Returns `None` if not currently in this state or if the machine
                    /// has been extracted via `into_{state}()` methods.
                    pub fn #read_method(&self) -> ::core::option::Option<&#data_ty> {
                        match self.inner.as_ref()? {
                            #(#read_match_arms)*
                            _ => ::core::option::Option::None,
                        }
                    }

                    /// Mutable access to state data when in the `#state_name` state.
                    ///
                    /// Returns `None` if not currently in this state or if the machine
                    /// has been extracted via `into_{state}()` methods.
                    pub fn #write_method(&mut self) -> ::core::option::Option<&mut #data_ty> {
                        match self.inner.as_mut()? {
                            #(#write_match_arms)*
                            _ => ::core::option::Option::None,
                        }
                    }

                    /// Set state data when in the `#state_name` state.
                    ///
                    /// Returns an error if:
                    /// - Not currently in the `#state_name` state
                    /// - The machine has been extracted via `into_{state}()` methods
                    pub fn #set_method(&mut self, data: #data_ty) -> Result<(), #dynamic_error_ty> {
                        match self.inner.as_mut() {
                            ::core::option::Option::Some(state) => match state {
                                #(#set_match_arms)*
                                other => Err(#dynamic_error_ctor::wrong_state(
                                    #state_str,
                                    other.name(),
                                    stringify!(#set_method),
                                )),
                            },
                            ::core::option::Option::None => Err(#dynamic_error_ctor::wrong_state(
                                #state_str,
                                "<extracted>",
                                stringify!(#set_method),
                            )),
                        }
                    }
                }
            }
        }).collect::<Vec<_>>();

        quote! {
            #(#accessor_methods)*
        }
    };

    Ok(quote! {
        /// Runtime state selector used when constructing a dynamic machine.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum #state_enum_name {
            #( #state_variants, )*
        }

        impl #state_enum_name {
            /// Get the state name as a static string.
            pub fn name(self) -> &'static str {
                match self {
                    #( #state_name_arms, )*
                }
            }
        }

        impl ::core::fmt::Display for #state_enum_name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str((*self).name())
            }
        }

        /// Dynamic wrapper for runtime event dispatch.
        ///
        /// This struct wraps the typestate machine and provides a `handle()` method
        /// for dispatching events at runtime. Use this when events come from external
        /// sources and can't be determined at compile time.
        #[derive(Debug)]
        pub struct #dynamic_name #generics {
            inner: ::core::option::Option<#any_state_name #generics>,
            completions: ::state_machines::__private::Vec<::state_machines::CompletionEvent>,
        }

        impl #generics #dynamic_name #generics {
            /// Create a new dynamic machine in the declared initial state.
            pub fn new(ctx: #ctx_param_ty) -> Self {
                Self {
                    inner: ::core::option::Option::Some(#initial_state_constructor),
                    completions: ::state_machines::__private::Vec::new(),
                }
            }

            /// Create a new dynamic machine in the specified state.
            pub fn new_init_state(ctx: #ctx_param_ty, state: #state_enum_name) -> Self {
                Self {
                    inner: ::core::option::Option::Some(match state {
                        #( #state_constructor_arms, )*
                    }),
                    completions: ::state_machines::__private::Vec::new(),
                }
            }

            /// Dispatch an event to the state machine at runtime.
            ///
            /// Returns an error if:
            /// - The event is not valid from the current state
            /// - A guard callback fails
            /// - An action callback fails
            #handle_sig {
                // Take ownership of inner state temporarily
                let current = self.inner.take().expect("dynamic machine in invalid state");

                let new_state = match (current, event) {
                    #(#match_arms)*
                    #catch_all
                };

                self.inner = ::core::option::Option::Some(new_state);
                self.completions.extend_from_slice(self.completion_events());
                Ok(())
            }

            /// Get the current runtime state.
            pub fn current_state(&self) -> #state_enum_name {
                match self.inner.as_ref()
                    .expect("dynamic machine in invalid state")
                {
                    #( #current_state_arms, )*
                }
            }

            #available_events_method

            pub fn is_finished(&self) -> bool {
                match self.inner.as_ref().expect("dynamic machine in invalid state") {
                    #( #finished_arms, )*
                }
            }

            pub fn completion_events(&self) -> &'static [::state_machines::CompletionEvent] {
                match self.inner.as_ref().expect("dynamic machine in invalid state") {
                    #( #completion_arms, )*
                }
            }

            /// Drain notifications emitted by successful handle() calls.
            pub fn take_completion_events(&mut self) -> ::state_machines::__private::Vec<::state_machines::CompletionEvent> {
                ::core::mem::take(&mut self.completions)
            }

            #state_data_accessors
        }

        #default_impl
    })
}

/// Generate conversion methods between typestate and dynamic modes.
///
/// Example output:
/// ```ignore
/// impl<S> FlightController<S> {
///     pub fn into_dynamic(self) -> DynamicFlightController { ... }
/// }
///
/// impl DynamicFlightController {
///     pub fn into_docked(self) -> Result<FlightController<Docked>, Self> { ... }
///     pub fn into_in_flight(self) -> Result<FlightController<InFlight>, Self> { ... }
/// }
/// ```
fn generate_conversions(machine: &StateMachine) -> Result<TokenStream2> {
    let machine_name = &machine.name;
    let dynamic_name = quote::format_ident!("Dynamic{}", machine_name);
    let any_state_name = quote::format_ident!("Any{}State", machine_name);

    let generics = ctx_generics(machine);

    // Generate into_dynamic() methods for each state
    let into_dynamic_methods = machine.states.iter().map(|state| {
        let params = machine_params(machine, state);
        quote! {
            impl #generics #machine_name #params {
                /// Convert this typestate machine into a dynamic wrapper.
                ///
                /// This allows runtime event dispatch at the cost of losing
                /// compile-time guarantees about state transitions.
                pub fn into_dynamic(self) -> #dynamic_name #generics {
                    #dynamic_name {
                        inner: ::core::option::Option::Some(#any_state_name::#state(self)),
                        completions: ::state_machines::__private::Vec::new(),
                    }
                }
            }
        }
    });

    // Generate into_{state}() methods for extracting typed machines
    let extract_methods = machine.states.iter().map(|state| {
        let method_name = quote::format_ident!("into_{}", to_snake_case(&state.to_string()));
        let params = machine_params(machine, state);
        quote! {
            /// Try to extract a typestate machine in the `#state` state.
            ///
            /// Returns `Ok` if the machine is currently in this state,
            /// otherwise returns `Err(self)` so you can try another state.
            pub fn #method_name(mut self) -> Result<#machine_name #params, Self> {
                match self.inner.take() {
                    ::core::option::Option::Some(#any_state_name::#state(m)) => Ok(m),
                    other => {
                        self.inner = other;
                        Err(self)
                    }
                }
            }
        }
    });

    Ok(quote! {
        #(#into_dynamic_methods)*

        impl #generics #dynamic_name #generics {
            #(#extract_methods)*
        }
    })
}
