//! Dynamic dispatch code generation.
//!
//! Generates runtime event-driven state machine wrappers that work alongside
//! the compile-time typestate pattern. Only generated when:
//! - The `dynamic` feature flag is enabled, OR
//! - The macro explicitly specifies `dynamic: true`

use crate::codegen::utils::{to_pascal_case, to_snake_case, to_snake_case_ident};
use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Result;

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
        let pascal_name =
            syn::Ident::new(&to_pascal_case(&event.name.to_string()), event.name.span());
        if let Some(payload_ty) = &event.payload {
            quote! { #pascal_name(#payload_ty) }
        } else {
            quote! { #pascal_name }
        }
    });

    let match_arms = machine.events.iter().map(|event| {
        let pascal_name =
            syn::Ident::new(&to_pascal_case(&event.name.to_string()), event.name.span());
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
    let variants = if machine.context.is_some() {
        // Concrete context: Machine<S> only
        machine
            .states
            .iter()
            .map(|state| {
                quote! { #state(#machine_name<#state>) }
            })
            .collect::<Vec<_>>()
    } else {
        // Generic context: Machine<C, S>
        machine
            .states
            .iter()
            .map(|state| {
                quote! { #state(#machine_name<C, #state>) }
            })
            .collect::<Vec<_>>()
    };

    // Generate match arms for the name() method
    let name_arms = machine.states.iter().map(|state| {
        let state_str = state.to_string();
        quote! { Self::#state(_) => #state_str }
    });

    // Determine enum generics
    let (enum_generics, impl_generics) = if machine.context.is_some() {
        // Concrete context: no generics
        (quote! {}, quote! {})
    } else {
        // Generic context
        (quote! { <C> }, quote! { <C> })
    };

    Ok(quote! {
        /// Internal enum wrapping all typed state machines.
        ///
        /// This enables runtime polymorphism over different states while
        /// preserving the compile-time safety of the typestate pattern.
        #[derive(Debug)]
        enum #any_state_name #enum_generics {
            #(#variants,)*
        }

        impl #impl_generics #any_state_name #enum_generics {
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
    let initial_state = &machine.initial;
    let is_async = machine.async_mode;

    // Helper to convert snake_case to PascalCase for enum variants
    let to_pascal_case = |s: &str| -> String {
        s.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect()
    };

    // Generate match arms for handle() method
    let mut match_arms = Vec::new();

    for event in &machine.events {
        let event_snake = &event.name; // snake_case from macro definition
        let event_pascal = syn::Ident::new(
            &to_pascal_case(&event_snake.to_string()),
            event_snake.span(),
        );
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
                        let arm = if event.payload.is_some() {
                            if is_async {
                                quote! {
                                    (#any_state_name::#source_state(m), #event_name::#event_pascal(payload)) => {
                                        match m.#event_method(payload).await {
                                            Ok(new_machine) => #any_state_name::#target_state(new_machine),
                                            Err((old_machine, err)) => {
                                                self.inner = ::core::option::Option::Some(#any_state_name::#source_state(old_machine));
                                                return Err(state_machines::DynamicError::from_guard_error(err));
                                            }
                                        }
                                    }
                                }
                            } else {
                                quote! {
                                    (#any_state_name::#source_state(m), #event_name::#event_pascal(payload)) => {
                                        match m.#event_method(payload) {
                                            Ok(new_machine) => #any_state_name::#target_state(new_machine),
                                            Err((old_machine, err)) => {
                                                self.inner = ::core::option::Option::Some(#any_state_name::#source_state(old_machine));
                                                return Err(state_machines::DynamicError::from_guard_error(err));
                                            }
                                        }
                                    }
                                }
                            }
                        } else if is_async {
                            quote! {
                                (#any_state_name::#source_state(m), #event_name::#event_pascal) => {
                                    match m.#event_method().await {
                                        Ok(new_machine) => #any_state_name::#target_state(new_machine),
                                        Err((old_machine, err)) => {
                                            self.inner = ::core::option::Option::Some(#any_state_name::#source_state(old_machine));
                                            return Err(state_machines::DynamicError::from_guard_error(err));
                                        }
                                    }
                                }
                            }
                        } else {
                            quote! {
                                (#any_state_name::#source_state(m), #event_name::#event_pascal) => {
                                    match m.#event_method() {
                                        Ok(new_machine) => #any_state_name::#target_state(new_machine),
                                        Err((old_machine, err)) => {
                                            self.inner = ::core::option::Option::Some(#any_state_name::#source_state(old_machine));
                                            return Err(state_machines::DynamicError::from_guard_error(err));
                                        }
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
            return Err(state_machines::DynamicError::invalid_transition(
                state_name,
                event.name(),
            ));
        }
    };

    let handle_sig = if is_async {
        quote! { pub async fn handle(&mut self, event: #event_name) -> Result<(), state_machines::DynamicError> }
    } else {
        quote! { pub fn handle(&mut self, event: #event_name) -> Result<(), state_machines::DynamicError> }
    };

    // Determine struct and impl generics based on context
    let (struct_generics, impl_generics, ctx_param_ty, any_state_generics) =
        if let Some(concrete_ctx) = &machine.context {
            // Concrete context: no generics, specific context type
            (quote! {}, quote! {}, quote! { #concrete_ctx }, quote! {})
        } else {
            // Generic context
            (quote! { <C> }, quote! { <C> }, quote! { C }, quote! { <C> })
        };

    // Default impl only for generic context with Default bound, or concrete context with Default
    let default_impl = if machine.context.is_some() {
        // Concrete context: only generate Default impl if the concrete type has Default
        // We can't check that at macro time, so we conditionally generate with where clause
        let concrete_ctx = machine.context.as_ref().unwrap();
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

    Ok(quote! {
        /// Dynamic wrapper for runtime event dispatch.
        ///
        /// This struct wraps the typestate machine and provides a `handle()` method
        /// for dispatching events at runtime. Use this when events come from external
        /// sources and can't be determined at compile time.
        #[derive(Debug)]
        pub struct #dynamic_name #struct_generics {
            inner: ::core::option::Option<#any_state_name #any_state_generics>,
        }

        impl #impl_generics #dynamic_name #struct_generics {
            /// Create a new dynamic machine in the initial state.
            pub fn new(ctx: #ctx_param_ty) -> Self {
                Self {
                    inner: ::core::option::Option::Some(#any_state_name::#initial_state(#machine_name::new(ctx))),
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
                Ok(())
            }

            /// Get the name of the current state.
            pub fn current_state(&self) -> &'static str {
                self.inner.as_ref()
                    .expect("dynamic machine in invalid state")
                    .name()
            }
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

    // Determine context type for conversions
    let (impl_generics, dynamic_generics) = if machine.context.is_some() {
        // Concrete context
        (quote! {}, quote! {})
    } else {
        // Generic context
        (quote! { <C> }, quote! { <C> })
    };

    // Generate into_dynamic() methods for each state
    let into_dynamic_methods =
        if machine.context.is_some() {
            // Concrete context: Machine<S>
            machine.states.iter().map(|state| {
            quote! {
                impl #machine_name<#state> {
                    /// Convert this typestate machine into a dynamic wrapper.
                    ///
                    /// This allows runtime event dispatch at the cost of losing
                    /// compile-time guarantees about state transitions.
                    pub fn into_dynamic(self) -> #dynamic_name {
                        #dynamic_name {
                            inner: ::core::option::Option::Some(#any_state_name::#state(self)),
                        }
                    }
                }
            }
        }).collect::<Vec<_>>()
        } else {
            // Generic context: Machine<C, S>
            machine.states.iter().map(|state| {
            quote! {
                impl<C> #machine_name<C, #state> {
                    /// Convert this typestate machine into a dynamic wrapper.
                    ///
                    /// This allows runtime event dispatch at the cost of losing
                    /// compile-time guarantees about state transitions.
                    pub fn into_dynamic(self) -> #dynamic_name<C> {
                        #dynamic_name {
                            inner: ::core::option::Option::Some(#any_state_name::#state(self)),
                        }
                    }
                }
            }
        }).collect::<Vec<_>>()
        };

    // Generate into_{state}() methods for extracting typed machines
    let extract_methods = if machine.context.is_some() {
        // Concrete context: Machine<S>
        machine
            .states
            .iter()
            .map(|state| {
                let method_name =
                    quote::format_ident!("into_{}", to_snake_case(&state.to_string()));
                quote! {
                    /// Try to extract a typestate machine in the `#state` state.
                    ///
                    /// Returns `Ok` if the machine is currently in this state,
                    /// otherwise returns `Err(self)` so you can try another state.
                    pub fn #method_name(mut self) -> Result<#machine_name<#state>, Self> {
                        match self.inner.take() {
                            ::core::option::Option::Some(#any_state_name::#state(m)) => Ok(m),
                            other => {
                                self.inner = other;
                                Err(self)
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>()
    } else {
        // Generic context: Machine<C, S>
        machine
            .states
            .iter()
            .map(|state| {
                let method_name =
                    quote::format_ident!("into_{}", to_snake_case(&state.to_string()));
                quote! {
                    /// Try to extract a typestate machine in the `#state` state.
                    ///
                    /// Returns `Ok` if the machine is currently in this state,
                    /// otherwise returns `Err(self)` so you can try another state.
                    pub fn #method_name(mut self) -> Result<#machine_name<C, #state>, Self> {
                        match self.inner.take() {
                            ::core::option::Option::Some(#any_state_name::#state(m)) => Ok(m),
                            other => {
                                self.inner = other;
                                Err(self)
                            }
                        }
                    }
                }
            })
            .collect::<Vec<_>>()
    };

    Ok(quote! {
        #(#into_dynamic_methods)*

        impl #impl_generics #dynamic_name #dynamic_generics {
            #(#extract_methods)*
        }
    })
}
