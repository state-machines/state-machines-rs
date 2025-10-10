use super::storage::TransitionStorageOps;
use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Ident, Result, Type};

impl StateMachine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_event(
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

        for state in &self.states {
            let mut transition_blocks = Vec::new();
            let mut can_blocks = Vec::new();

            let mut source_chain: Vec<Ident> = vec![state.clone()];
            if let Some(ancestors) = self.hierarchy.ancestors.get(&state.to_string()) {
                let mut reversed = ancestors.clone();
                reversed.reverse();
                source_chain.extend(reversed);
            }

            for (level, source_ident) in source_chain.iter().enumerate() {
                for transition in &event.transitions {
                    if !transition.sources.iter().any(|src| src == source_ident) {
                        continue;
                    }

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

                    let event_before_calls: Vec<_> =
                        event.before.iter().map(&call_callback).collect();
                    let event_after_calls: Vec<_> =
                        event.after.iter().map(&call_callback).collect();
                    let transition_before_calls: Vec<_> =
                        transition.before.iter().map(&call_callback).collect();
                    let transition_after_calls: Vec<_> =
                        transition.after.iter().map(&call_callback).collect();

                    let resolved_target = self
                        .resolve_target_ident(&transition.target)
                        .unwrap_or_else(|| transition.target.clone());

                    let TransitionStorageOps {
                        pre_state: storage_pre,
                        post_state: storage_post,
                        rollback: storage_rollback,
                    } = self.build_transition_storage_ops(state_ident, state, &resolved_target);

                    let action_tokens = action_method
                        .map(|action| action_call(action, &storage_rollback))
                        .unwrap_or_else(|| quote! {});

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

            if transition_blocks.is_empty() {
                continue;
            }

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

    pub(super) fn expand_state_refs(&self, refs: &[Ident]) -> Vec<Ident> {
        let mut expanded = Vec::new();
        let mut seen = std::collections::HashSet::new();

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

    pub(super) fn resolve_target_ident(&self, ident: &Ident) -> Option<Ident> {
        self.hierarchy.resolve_target(ident)
    }
}
