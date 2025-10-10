//! Code generation for the state machine macro.
//!
//! This module contains all the logic for generating Rust code from
//! the parsed state machine definition. It generates:
//! - The state enum and machine struct
//! - Event methods (e.g., `activate()`)
//! - Can methods (e.g., `can_activate()`)
//! - Storage management for state-associated data
//! - Callback invocations and guard checks
//!
//! The main entry point is `StateMachine::expand()`, which orchestrates
//! the entire code generation process.

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::{Ident, Result, Type};

struct TransitionStorageOps {
    pre_state: TokenStream2,
    post_state: TokenStream2,
    rollback: TokenStream2,
}

impl StateMachine {
    /// Generate the complete state machine code.
    ///
    /// This is the main code generation entry point. It produces:
    /// 1. The state enum with all variants
    /// 2. The machine struct with state field and storage fields
    /// 3. Event methods for triggering transitions
    /// 4. Can methods for checking if transitions are allowed
    /// 5. Trait implementations (Machine, Default, etc.)
    /// 6. The machine definition constant for introspection
    pub fn expand(&self) -> Result<TokenStream2> {
        // First, validate the machine definition
        // (This is in validation.rs, but called here)
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
                // Convert state name to snake_case for getter names
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

            // Generate sync versions of event and can methods
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

            // Generate async versions if in async mode
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

        // Build state values for the definition
        let state_values: Vec<_> = states
            .iter()
            .map(|state| quote!(#state_ident::#state))
            .collect();

        // Build superstate definitions for the definition
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

        // Generate the machine definition constant
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

        // Generate the Machine trait implementation
        let machine_trait = quote! {
            impl #core_path::Machine for #machine_ident {
                type State = #state_ident;

                fn state(&self) -> Self::State {
                    self.state
                }
            }
        };

        // Generate the final impl block with event methods
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

    /// Generate code for a single event method and its can method.
    ///
    /// This is a complex method that handles:
    /// - Guard checking (both event-level and transition-level)
    /// - Unless checking (inverted guards)
    /// - Callback invocation (before, after, around)
    /// - Action method invocation
    /// - State transitions with storage management
    /// - Hierarchical state resolution
    ///
    /// The generated code is a match expression on the current state,
    /// with each arm handling transitions from that state.
    #[allow(clippy::too_many_arguments)]
    pub fn build_event(
        &self,
        state_ident: &Ident,
        method_ident: &Ident,
        can_ident: Ident,
        event: &Event,
        action_method: Option<&Ident>,
        payload_ty: Option<&Type>,
        is_async: bool,
    ) -> Result<EventTokens> {
        let core_path = quote!(::state_machines::core);
        let mut state_match_arms = Vec::new();
        let mut can_match_arms = Vec::new();

        // Don't pre-generate storage tokens - we'll generate them inline per-transition
        // to avoid referencing stale state values during rollback

        // Closure to generate guard checking code
        let guard_check = |guard: &Ident| -> TokenStream2 {
            if payload_ty.is_some() {
                if is_async {
                    quote! {
                        if !self.#guard(payload_ref).await {
                            if last_guard_failure.is_none() {
                                last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                    current_state,
                                    stringify!(#method_ident),
                                    stringify!(#guard),
                                ));
                            }
                            guard_failed = true;
                        }
                    }
                } else {
                    quote! {
                        if !self.#guard(payload_ref) {
                            if last_guard_failure.is_none() {
                                last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                    current_state,
                                    stringify!(#method_ident),
                                    stringify!(#guard),
                                ));
                            }
                            guard_failed = true;
                        }
                    }
                }
            } else if is_async {
                quote! {
                    if !self.#guard().await {
                        if last_guard_failure.is_none() {
                            last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                current_state,
                                stringify!(#method_ident),
                                stringify!(#guard),
                            ));
                        }
                        guard_failed = true;
                    }
                }
            } else {
                quote! {
                    if !self.#guard() {
                        if last_guard_failure.is_none() {
                            last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                current_state,
                                stringify!(#method_ident),
                                stringify!(#guard),
                            ));
                        }
                        guard_failed = true;
                    }
                }
            }
        };

        // Closure to generate guard checking code for can methods
        // (simpler, doesn't track error details)
        let guard_check_can = |guard: &Ident| -> TokenStream2 {
            if payload_ty.is_some() {
                if is_async {
                    quote! {
                        if !self.#guard(payload_ref).await {
                            guard_failed = true;
                        }
                    }
                } else {
                    quote! {
                        if !self.#guard(payload_ref) {
                            guard_failed = true;
                        }
                    }
                }
            } else if is_async {
                quote! {
                    if !self.#guard().await {
                        guard_failed = true;
                    }
                }
            } else {
                quote! {
                    if !self.#guard() {
                        guard_failed = true;
                    }
                }
            }
        };

        // Closure to generate unless checking code
        // (inverted guards - if the condition is true, the guard fails)
        let unless_check = |guard: &Ident| -> TokenStream2 {
            if payload_ty.is_some() {
                if is_async {
                    quote! {
                        if self.#guard(payload_ref).await {
                            if last_guard_failure.is_none() {
                                last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                    current_state,
                                    stringify!(#method_ident),
                                    stringify!(#guard),
                                ));
                            }
                            guard_failed = true;
                        }
                    }
                } else {
                    quote! {
                        if self.#guard(payload_ref) {
                            if last_guard_failure.is_none() {
                                last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                    current_state,
                                    stringify!(#method_ident),
                                    stringify!(#guard),
                                ));
                            }
                            guard_failed = true;
                        }
                    }
                }
            } else if is_async {
                quote! {
                    if self.#guard().await {
                        if last_guard_failure.is_none() {
                            last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                current_state,
                                stringify!(#method_ident),
                                stringify!(#guard),
                            ));
                        }
                        guard_failed = true;
                    }
                }
            } else {
                quote! {
                    if self.#guard() {
                        if last_guard_failure.is_none() {
                            last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                current_state,
                                stringify!(#method_ident),
                                stringify!(#guard),
                            ));
                        }
                        guard_failed = true;
                    }
                }
            }
        };

        // Closure to generate unless checking code for can methods
        let unless_check_can = |guard: &Ident| -> TokenStream2 {
            if payload_ty.is_some() {
                if is_async {
                    quote! {
                        if self.#guard(payload_ref).await {
                            guard_failed = true;
                        }
                    }
                } else {
                    quote! {
                        if self.#guard(payload_ref) {
                            guard_failed = true;
                        }
                    }
                }
            } else if is_async {
                quote! {
                    if self.#guard().await {
                        guard_failed = true;
                    }
                }
            } else {
                quote! {
                    if self.#guard() {
                        guard_failed = true;
                    }
                }
            }
        };

        // Closure to generate callback invocation code
        let call_callback = |callback: &Ident| -> TokenStream2 {
            if payload_ty.is_some() {
                if is_async {
                    quote! { self.#callback(payload_ref).await; }
                } else {
                    quote! { self.#callback(payload_ref); }
                }
            } else if is_async {
                quote! { self.#callback().await; }
            } else {
                quote! { self.#callback(); }
            }
        };

        // Closure to generate action method invocation code
        // Actions are special: if they return false, the transition is rolled back
        let action_call = |action: &Ident, rollback: &TokenStream2| -> TokenStream2 {
            if payload_ty.is_some() {
                if is_async {
                    quote! {
                        if !self.#action(payload_ref).await {
                            let failed_from = __prev_state;
                            #rollback
                            self.state = __prev_state;
                            return Err(#core_path::TransitionError {
                                from: failed_from,
                                event: stringify!(#method_ident),
                                kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                            });
                        }
                    }
                } else {
                    quote! {
                        if !self.#action(payload_ref) {
                            let failed_from = __prev_state;
                            #rollback
                            self.state = __prev_state;
                            return Err(#core_path::TransitionError {
                                from: failed_from,
                                event: stringify!(#method_ident),
                                kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                            });
                        }
                    }
                }
            } else if is_async {
                quote! {
                    if !self.#action().await {
                        let failed_from = __prev_state;
                        #rollback
                        self.state = __prev_state;
                        return Err(#core_path::TransitionError {
                            from: failed_from,
                            event: stringify!(#method_ident),
                            kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                        });
                    }
                }
            } else {
                quote! {
                    if !self.#action() {
                        let failed_from = __prev_state;
                        #rollback
                        self.state = __prev_state;
                        return Err(#core_path::TransitionError {
                            from: failed_from,
                            event: stringify!(#method_ident),
                            kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                        });
                    }
                }
            }
        };

        // Generate match arms for each state
        for state in &self.states {
            let mut transition_blocks = Vec::new();
            let mut can_blocks = Vec::new();

            // Build the source chain: [current_state, parent, grandparent, ...]
            // This implements hierarchical transition resolution
            let mut source_chain: Vec<Ident> = vec![state.clone()];
            if let Some(ancestors) = self.hierarchy.ancestors.get(&state.to_string()) {
                let mut reversed = ancestors.clone();
                reversed.reverse();
                source_chain.extend(reversed);
            }

            // Process transitions in order of specificity (leaf to root)
            for (level, source_ident) in source_chain.iter().enumerate() {
                for transition in &event.transitions {
                    // Check if this transition applies to this source
                    if !transition.sources.iter().any(|src| src == source_ident) {
                        continue;
                    }

                    // Skip if a more specific transition already matched
                    if level > 0 {
                        let prior_chain = &source_chain[..level];
                        if transition
                            .sources
                            .iter()
                            .any(|src| prior_chain.iter().any(|prior| prior == src))
                        {
                            continue;
                        }
                    }

                    // Combine event-level and transition-level guards
                    let combined_guards: Vec<_> = event
                        .guards
                        .iter()
                        .chain(transition.guards.iter())
                        .collect();
                    let combined_unless: Vec<_> = event
                        .unless
                        .iter()
                        .chain(transition.unless.iter())
                        .collect();

                    // Generate guard and unless checks
                    let guard_checks: Vec<_> = combined_guards
                        .iter()
                        .map(|guard| guard_check(guard))
                        .collect();
                    let guard_checks_can: Vec<_> = combined_guards
                        .iter()
                        .map(|guard| guard_check_can(guard))
                        .collect();
                    let unless_checks: Vec<_> = combined_unless
                        .iter()
                        .map(|guard| unless_check(guard))
                        .collect();
                    let unless_checks_can: Vec<_> = combined_unless
                        .iter()
                        .map(|guard| unless_check_can(guard))
                        .collect();

                    // Generate callback invocations
                    let event_before_calls: Vec<_> =
                        event.before.iter().map(&call_callback).collect();
                    let event_after_calls: Vec<_> =
                        event.after.iter().map(&call_callback).collect();
                    let transition_before_calls: Vec<_> =
                        transition.before.iter().map(&call_callback).collect();
                    let transition_after_calls: Vec<_> =
                        transition.after.iter().map(&call_callback).collect();

                    // Resolve target (convert superstates to their initial states)
                    let resolved_target = self
                        .resolve_target_ident(&transition.target)
                        .unwrap_or_else(|| transition.target.clone());

                    // Generate storage operations inline for this specific transition
                    let TransitionStorageOps {
                        pre_state: storage_pre,
                        post_state: storage_post,
                        rollback: storage_rollback,
                    } = self.build_transition_storage_ops(
                        state_ident,
                        state,            // source state
                        &resolved_target, // target state
                    );

                    // Generate action call if configured
                    let action_tokens = action_method
                        .map(|action| action_call(action, &storage_rollback))
                        .unwrap_or_else(|| quote! {});

                    // Generate global before callbacks (with filtering)
                    let mut before_globals = Vec::new();
                    for callback in &self.callbacks.before {
                        let filter = self.callback_filter_expr(
                            callback,
                            state_ident,
                            method_ident,
                            &quote!(current_state),
                            &quote!(target_state),
                        )?;
                        let callback_ident = &callback.name;
                        let tokens = if is_async {
                            quote! { if #filter { self.#callback_ident().await; } }
                        } else {
                            quote! { if #filter { self.#callback_ident(); } }
                        };
                        before_globals.push(tokens);
                    }

                    // Generate global after callbacks (with filtering)
                    let mut after_globals = Vec::new();
                    for callback in &self.callbacks.after {
                        let filter = self.callback_filter_expr(
                            callback,
                            state_ident,
                            method_ident,
                            &quote!(current_state),
                            &quote!(target_state),
                        )?;
                        let callback_ident = &callback.name;
                        let tokens = if is_async {
                            quote! { if #filter { self.#callback_ident().await; } }
                        } else {
                            quote! { if #filter { self.#callback_ident(); } }
                        };
                        after_globals.push(tokens);
                    }

                    // Generate around callbacks (these can abort transitions)
                    let (context_init, around_before_calls, around_after_calls) = if self
                        .callbacks
                        .around
                        .is_empty()
                    {
                        (quote! {}, Vec::new(), Vec::new())
                    } else {
                        let context_init = quote! {
                            let context = #core_path::TransitionContext::new(
                                current_state,
                                target_state,
                                stringify!(#method_ident),
                            );
                        };

                        let mut before_calls = Vec::new();
                        for callback in &self.callbacks.around {
                            let filter = self.callback_filter_expr(
                                callback,
                                state_ident,
                                method_ident,
                                &quote!(current_state),
                                &quote!(target_state),
                            )?;
                            let callback_ident = &callback.name;
                            let tokens = if is_async {
                                quote! {
                                    if #filter {
                                        match self.#callback_ident(&context, #core_path::AroundStage::Before).await {
                                            #core_path::AroundOutcome::Proceed => {}
                                            #core_path::AroundOutcome::Abort(error) => return Err(error),
                                        }
                                    }
                                }
                            } else {
                                quote! {
                                    if #filter {
                                        match self.#callback_ident(&context, #core_path::AroundStage::Before) {
                                            #core_path::AroundOutcome::Proceed => {}
                                            #core_path::AroundOutcome::Abort(error) => return Err(error),
                                        }
                                    }
                                }
                            };
                            before_calls.push(tokens);
                        }

                        let mut after_calls = Vec::new();
                        for callback in self.callbacks.around.iter().rev() {
                            let filter = self.callback_filter_expr(
                                callback,
                                state_ident,
                                method_ident,
                                &quote!(current_state),
                                &quote!(target_state),
                            )?;
                            let callback_ident = &callback.name;
                            let tokens = if is_async {
                                quote! {
                                    if #filter {
                                        match self.#callback_ident(&context, #core_path::AroundStage::AfterSuccess).await {
                                            #core_path::AroundOutcome::Proceed => {}
                                            #core_path::AroundOutcome::Abort(error) => return Err(error),
                                        }
                                    }
                                }
                            } else {
                                quote! {
                                    if #filter {
                                        match self.#callback_ident(&context, #core_path::AroundStage::AfterSuccess) {
                                            #core_path::AroundOutcome::Proceed => {}
                                            #core_path::AroundOutcome::Abort(error) => return Err(error),
                                        }
                                    }
                                }
                            };
                            after_calls.push(tokens);
                        }

                        (context_init, before_calls, after_calls)
                    };

                    // Build the complete transition block
                    transition_blocks.push(quote! {
                        {
                            let mut guard_failed = false;
                            #( #guard_checks )*
                            #( #unless_checks )*
                            if !guard_failed {
                                let target_state = #state_ident::#resolved_target;
                                #context_init
                                #( #before_globals )*
                                #( #around_before_calls )*
                                #( #event_before_calls )*
                                #( #transition_before_calls )*
                                let __prev_state = self.state;
                                #storage_pre
                                self.state = target_state;
                                #storage_post
                                #action_tokens
                                #( #transition_after_calls )*
                                #( #event_after_calls )*
                                #( #after_globals )*
                                #( #around_after_calls )*
                                return Ok(());
                            }
                        }
                    });

                    // Build the can check block (simpler, just checks guards)
                    can_blocks.push(quote! {
                        {
                            let mut guard_failed = false;
                            #( #guard_checks_can )*
                            #( #unless_checks_can )*
                            if !guard_failed {
                                allowed = true;
                            }
                        }
                    });
                }
            }

            // Skip states with no transitions for this event
            if transition_blocks.is_empty() {
                continue;
            }

            // Build the match arm for this state
            state_match_arms.push(quote! {
                #state_ident::#state => {
                    let mut last_guard_failure: Option<#core_path::TransitionError<#state_ident>> = None;
                    #( #transition_blocks )*
                    if let Some(error) = last_guard_failure {
                        Err(error)
                    } else {
                        Err(#core_path::TransitionError::invalid_transition(
                            current_state,
                            stringify!(#method_ident),
                        ))
                    }
                }
            });

            can_match_arms.push(quote! {
                #state_ident::#state => {
                    let mut allowed = false;
                    #( #can_blocks )*
                    allowed
                }
            });
        }

        // Generate the event method signature and body
        let event_method = if is_async {
            if let Some(payload_ty) = payload_ty {
                quote! {
                    pub async fn #method_ident(&mut self, payload: #payload_ty) -> #core_path::TransitionResult<#state_ident> {
                        let current_state = self.state;
                        let payload_ref = &payload;
                        match current_state {
                            #( #state_match_arms )*
                            state => Err(#core_path::TransitionError::invalid_transition(
                                state,
                                stringify!(#method_ident),
                            )),
                        }
                    }
                }
            } else {
                quote! {
                    pub async fn #method_ident(&mut self) -> #core_path::TransitionResult<#state_ident> {
                        let current_state = self.state;
                        match current_state {
                            #( #state_match_arms )*
                            state => Err(#core_path::TransitionError::invalid_transition(
                                state,
                                stringify!(#method_ident),
                            )),
                        }
                    }
                }
            }
        } else if let Some(payload_ty) = payload_ty {
            quote! {
                pub fn #method_ident(&mut self, payload: #payload_ty) -> #core_path::TransitionResult<#state_ident> {
                    let current_state = self.state;
                    let payload_ref = &payload;
                    match current_state {
                        #( #state_match_arms )*
                        state => Err(#core_path::TransitionError::invalid_transition(
                            state,
                            stringify!(#method_ident),
                        )),
                    }
                }
            }
        } else {
            quote! {
                pub fn #method_ident(&mut self) -> #core_path::TransitionResult<#state_ident> {
                    let current_state = self.state;
                    match current_state {
                        #( #state_match_arms )*
                        state => Err(#core_path::TransitionError::invalid_transition(
                            state,
                            stringify!(#method_ident),
                        )),
                    }
                }
            }
        };

        // Generate the can method signature and body
        let can_method = if is_async {
            if let Some(payload_ty) = payload_ty {
                quote! {
                    pub async fn #can_ident(&self, payload: &#payload_ty) -> bool {
                        let current_state = self.state;
                        let payload_ref = payload;
                        match current_state {
                            #( #can_match_arms )*
                            _ => false,
                        }
                    }
                }
            } else {
                quote! {
                    pub async fn #can_ident(&self) -> bool {
                        match self.state {
                            #( #can_match_arms )*
                            _ => false,
                        }
                    }
                }
            }
        } else if let Some(payload_ty) = payload_ty {
            quote! {
                pub fn #can_ident(&self, payload: &#payload_ty) -> bool {
                    let current_state = self.state;
                    let payload_ref = payload;
                    match current_state {
                        #( #can_match_arms )*
                        _ => false,
                    }
                }
            }
        } else {
            quote! {
                pub fn #can_ident(&self) -> bool {
                    match self.state {
                        #( #can_match_arms )*
                        _ => false,
                    }
                }
            }
        };

        Ok(EventTokens {
            event_method,
            can_method,
        })
    }

    /// Generate a boolean expression for filtering global callbacks.
    ///
    /// Callbacks can specify:
    /// - from: Only apply when transitioning from these states
    /// - to: Only apply when transitioning to these states
    /// - on: Only apply for these events
    ///
    /// This generates a compound boolean expression that combines all filters.
    pub fn callback_filter_expr(
        &self,
        callback: &TransitionCallback,
        state_ident: &Ident,
        event_ident: &Ident,
        current_expr: &TokenStream2,
        target_expr: &TokenStream2,
    ) -> Result<TokenStream2> {
        let leaves = &self.states;
        let callback_name = callback.name.to_string();

        let expand_filter = |idents: &[Ident], label: &str| -> Result<Vec<Ident>> {
            let mut expanded = Vec::new();
            let mut seen = HashSet::new();

            for ident in idents {
                let resolved = self.hierarchy.expand_state(ident, leaves);
                if resolved.is_empty() {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "unknown state `{}` in `{}` filter of callback `{}`",
                            ident, label, callback_name
                        ),
                    ));
                }

                for leaf in resolved {
                    let key = leaf.to_string();
                    if seen.insert(key) {
                        expanded.push(leaf);
                    }
                }
            }

            Ok(expanded)
        };

        // Generate from filter
        let from_tokens = if callback.from.is_empty() {
            quote!(true)
        } else {
            let expanded = expand_filter(&callback.from, "from")?;
            if expanded.len() == 1 {
                let state = &expanded[0];
                quote!(#current_expr == #state_ident::#state)
            } else {
                let patterns = expanded.iter().map(|state| quote!(#state_ident::#state));
                quote!(matches!(#current_expr, #( #patterns )|*))
            }
        };

        // Generate to filter
        let to_tokens = if callback.to.is_empty() {
            quote!(true)
        } else {
            let expanded = expand_filter(&callback.to, "to")?;
            if expanded.len() == 1 {
                let state = &expanded[0];
                quote!(#target_expr == #state_ident::#state)
            } else {
                let patterns = expanded.iter().map(|state| quote!(#state_ident::#state));
                quote!(matches!(#target_expr, #( #patterns )|*))
            }
        };

        // Generate on filter
        let event_expr = if callback.on.is_empty() {
            quote!(true)
        } else {
            let names = callback.on.iter().map(|event| quote!(stringify!(#event)));
            quote!({ let event_name = stringify!(#event_ident); #( event_name == #names )||* })
        };

        // Combine all filters with &&
        Ok(quote!(#from_tokens && #to_tokens && #event_expr))
    }

    /// Helper function to expand state references to leaf states.
    ///
    /// This handles both leaf states and superstates, expanding superstates
    /// to their descendants and removing duplicates.
    pub fn expand_state_refs(&self, refs: &[Ident]) -> Vec<Ident> {
        let mut expanded = Vec::new();
        let mut seen = HashSet::new();

        for ident in refs {
            let resolved = self.hierarchy.expand_state(ident, &self.states);
            for leaf in resolved {
                let key = leaf.to_string();
                if seen.insert(key) {
                    expanded.push(leaf);
                }
            }
        }

        expanded
    }

    /// Helper function to resolve a target identifier.
    ///
    /// If the target is a superstate, returns its initial child.
    /// Otherwise returns the target unchanged.
    pub fn resolve_target_ident(&self, ident: &Ident) -> Option<Ident> {
        self.hierarchy.resolve_target(ident)
    }

    /// Generate storage operations for a specific transition.
    ///
    /// This generates code that:
    /// 1. Clears storage for the source state (if it has storage)
    /// 2. Initializes storage for the target state (if it has storage)
    /// 3. Updates superstate storage as needed
    ///
    /// By generating these operations per-transition with explicit source/target,
    /// we avoid referencing stale state values during rollback.
    fn build_transition_storage_ops(
        &self,
        _state_ident: &Ident,
        source: &Ident,
        target: &Ident,
    ) -> TransitionStorageOps {
        let mut pre_ops = Vec::new();
        let mut post_ops = Vec::new();
        let mut rollback_ops = Vec::new();

        for spec in &self.state_storage {
            let field = &spec.field;
            let backup_ident = format_ident!("__storage_backup_{}", field);

            if !spec.is_superstate {
                let affects_source = &spec.owner == source;
                let affects_target = &spec.owner == target;

                if !(affects_source || affects_target) {
                    continue;
                }

                pre_ops.push(quote! {
                    let #backup_ident = self.#field.take();
                });

                if affects_target {
                    post_ops.push(quote! {
                        self.#field =
                            ::core::option::Option::Some(::core::default::Default::default());
                    });
                }

                rollback_ops.push(quote! {
                    self.#field = #backup_ident;
                });
            } else if let Some(descendants) = self.hierarchy.lookup.get(&spec.owner.to_string()) {
                let source_in = descendants.iter().any(|leaf| leaf == source);
                let target_in = descendants.iter().any(|leaf| leaf == target);

                if source_in == target_in {
                    continue;
                }

                pre_ops.push(quote! {
                    let #backup_ident = self.#field.take();
                });

                if !source_in && target_in {
                    post_ops.push(quote! {
                        self.#field =
                            ::core::option::Option::Some(::core::default::Default::default());
                    });
                }

                rollback_ops.push(quote! {
                    self.#field = #backup_ident;
                });
            }
        }

        TransitionStorageOps {
            pre_state: quote! { #( #pre_ops )* },
            post_state: quote! { #( #post_ops )* },
            rollback: quote! { #( #rollback_ops )* },
        }
    }
}
