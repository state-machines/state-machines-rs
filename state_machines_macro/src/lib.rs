use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::{
    Ident, LitBool, Result, Token, braced, bracketed,
    parse::{Parse, ParseBuffer, ParseStream},
};

#[proc_macro]
pub fn state_machine(input: TokenStream) -> TokenStream {
    let machine = syn::parse_macro_input!(input as StateMachine);
    match machine.expand() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

struct StateMachine {
    name: Ident,
    state: Ident,
    initial: Ident,
    states: Vec<Ident>,
    events: Vec<Event>,
    async_mode: bool,
    action: Option<Ident>,
    callbacks: Callbacks,
}

impl Parse for StateMachine {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut name = None;
        let mut state = None;
        let mut initial = None;
        let mut states = None;
        let mut events = None;
        let mut async_mode = false;
        let mut action: Option<Ident> = None;
        let mut callbacks = Callbacks::default();

        while !input.is_empty() {
            if input.peek(Token![async]) {
                let _: Token![async] = input.parse()?;
                input.parse::<Token![:]>()?;
                let value: LitBool = input.parse()?;
                async_mode = value.value();
            } else {
                let key: Ident = input.parse()?;
                let key_str = key.to_string();

                match key_str.as_str() {
                    "name" => {
                        input.parse::<Token![:]>()?;
                        name = Some(input.parse()?);
                    }
                    "state" => {
                        input.parse::<Token![:]>()?;
                        state = Some(input.parse()?);
                    }
                    "initial" => {
                        input.parse::<Token![:]>()?;
                        initial = Some(input.parse()?);
                    }
                    "action" => {
                        input.parse::<Token![:]>()?;
                        action = Some(input.parse()?);
                    }
                    "states" => {
                        input.parse::<Token![:]>()?;
                        let content;
                        bracketed!(content in input);
                        states = Some(parse_ident_list(&content)?);
                    }
                    "events" => {
                        if input.peek(Token![:]) {
                            input.parse::<Token![:]>()?;
                        }
                        let content;
                        braced!(content in input);
                        events = Some(parse_events(&content)?);
                    }
                    "callbacks" => {
                        if input.peek(Token![:]) {
                            input.parse::<Token![:]>()?;
                        }
                        let content;
                        braced!(content in input);
                        callbacks = parse_callbacks(&content)?;
                    }
                    other => {
                        return Err(syn::Error::new(
                            key.span(),
                            format!("unexpected key `{}`", other),
                        ));
                    }
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            name: name.ok_or_else(|| syn::Error::new(Span::call_site(), "missing `name` field"))?,
            state: state
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `state` field"))?,
            initial: initial
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `initial` field"))?,
            states: states
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `states` field"))?,
            events: events.unwrap_or_default(),
            async_mode,
            action,
            callbacks,
        })
    }
}

impl StateMachine {
    fn expand(&self) -> Result<proc_macro2::TokenStream> {
        self.validate()?;

        let machine_ident = &self.name;
        let state_ident = &self.state;
        let initial_state = &self.initial;
        let states = &self.states;
        let async_mode = self.async_mode;
        let async_mode_literal = if async_mode {
            quote!(true)
        } else {
            quote!(false)
        };

        let state_variants = states.iter();
        let state_variant_defs = quote! {
            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
            pub enum #state_ident {
                #( #state_variants, )*
            }
        };

        let state_impl = quote! {
            impl Default for #state_ident {
                fn default() -> Self {
                    #state_ident::#initial_state
                }
            }
        };

        let machine_struct = quote! {
            #[derive(Debug)]
            pub struct #machine_ident {
                state: #state_ident,
            }
        };

        let new_method = quote! {
            impl #machine_ident {
                pub fn new() -> Self {
                    Self { state: #state_ident::#initial_state }
                }

                pub fn state(&self) -> #state_ident {
                    self.state
                }
            }
        };

        let state_values: Vec<_> = self
            .states
            .iter()
            .map(|state| quote!(#state_ident::#state))
            .collect();

        let machine_default = quote! {
            impl Default for #machine_ident {
                fn default() -> Self {
                    Self::new()
                }
            }
        };

        let core_path = quote!(::state_machines::core);
        let action_method = self.action.as_ref();
        // Per-event action call snippets are built inside the event loop below.

        let machine_trait_impl = quote! {
            impl #core_path::Machine for #machine_ident {
                type State = #state_ident;

                fn state(&self) -> Self::State {
                    self.state
                }
            }
        };

        let mut sync_event_methods = Vec::new();
        let mut sync_can_methods = Vec::new();
        let mut async_event_methods = Vec::new();
        let mut async_can_methods = Vec::new();
        let mut event_definitions = Vec::new();

        let callbacks = &self.callbacks;

        for event in &self.events {
            let method_ident = &event.name;
            let can_ident = format_ident!("can_{}", method_ident);

            let payload_ty = event.payload.as_ref().map(|payload| &payload.ty);

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

            let mut transition_definitions_tokens = Vec::new();
            for transition in &event.transitions {
                let sources_tokens: Vec<_> = transition
                    .sources
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
                let target = &transition.target;

                transition_definitions_tokens.push(quote! {
                    #core_path::TransitionDefinition {
                        sources: &[ #( #sources_tokens ),* ],
                        target: #state_ident::#target,
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
                    transitions: &[ #( #transition_definitions_tokens ),* ],
                }
            });

            let payload_binding_sync = payload_ty.map(|_| quote!(let payload_ref = &payload;));
            let payload_binding_async = payload_ty.map(|_| quote!(let payload_ref = &payload;));
            let payload_binding_can_sync = payload_ty.map(|_| quote!(let payload_ref = payload;));
            let payload_binding_can_async = payload_ty.map(|_| quote!(let payload_ref = payload;));

            let payload_arg_sync = payload_ty.map(|_| quote!(payload_ref)).unwrap_or_else(|| quote!());
            let payload_arg_async = payload_ty.map(|_| quote!(payload_ref)).unwrap_or_else(|| quote!());
            let payload_arg_can = payload_ty.map(|_| quote!(payload_ref)).unwrap_or_else(|| quote!());

            let payload_any_sync = payload_ty
                .map(|_| quote!(::core::option::Option::Some(&payload as &dyn ::core::any::Any)))
                .unwrap_or_else(|| quote!(::core::option::Option::None));
            let payload_any_async = payload_ty
                .map(|_| quote!(::core::option::Option::Some(&payload as &dyn ::core::any::Any)))
                .unwrap_or_else(|| quote!(::core::option::Option::None));
            let payload_any_can = payload_ty
                .map(|_| quote!(::core::option::Option::Some(payload as &dyn ::core::any::Any)))
                .unwrap_or_else(|| quote!(::core::option::Option::None));

            let before_global_sync_tokens: Vec<_> = callbacks
                .before
                .iter()
                .map(|callback| {
                    Self::build_callback_invocation(
                        callback,
                        state_ident,
                        method_ident,
                        false,
                        &quote!(self),
                        &payload_arg_sync,
                        &quote!(current_state),
                        &quote!(target_state),
                    )
                })
                .collect();
            let before_global_async_tokens: Vec<_> = callbacks
                .before
                .iter()
                .map(|callback| {
                    Self::build_callback_invocation(
                        callback,
                        state_ident,
                        method_ident,
                        true,
                        &quote!(self),
                        &payload_arg_async,
                        &quote!(current_state),
                        &quote!(target_state),
                    )
                })
                .collect();
            let after_global_sync_tokens: Vec<_> = callbacks
                .after
                .iter()
                .map(|callback| {
                    Self::build_callback_invocation(
                        callback,
                        state_ident,
                        method_ident,
                        false,
                        &quote!(self),
                        &payload_arg_sync,
                        &quote!(current_state),
                        &quote!(target_state),
                    )
                })
                .collect();
            let after_global_async_tokens: Vec<_> = callbacks
                .after
                .iter()
                .map(|callback| {
                    Self::build_callback_invocation(
                        callback,
                        state_ident,
                        method_ident,
                        true,
                        &quote!(self),
                        &payload_arg_async,
                        &quote!(current_state),
                        &quote!(target_state),
                    )
                })
                .collect();

            let mut state_match_arms = Vec::new();
            let mut can_match_arms = Vec::new();

            for state in states {
                let applicable: Vec<_> = event
                    .transitions
                    .iter()
                    .filter(|transition| transition.sources.iter().any(|source| source == state))
                    .collect();

                if applicable.is_empty() {
                    continue;
                }

                let mut transition_blocks_async = Vec::new();
                let mut can_blocks_async = Vec::new();
                let mut transition_blocks_sync = Vec::new();
                let mut can_blocks_sync = Vec::new();

                for transition in applicable.iter() {
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

                    let guard_checks_async = combined_guards
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
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
                            }
                        })
                        .collect::<Vec<_>>();

                    let guard_checks_can_async = combined_guards
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
                                quote! {
                                    if !self.#guard(payload_ref).await {
                                        guard_failed = true;
                                    }
                                }
                            } else {
                                quote! {
                                    if !self.#guard().await {
                                        guard_failed = true;
                                    }
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    let unless_checks_async = combined_unless
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
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
                            }
                        })
                        .collect::<Vec<_>>();

                    let unless_checks_can_async = combined_unless
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
                                quote! {
                                    if self.#guard(payload_ref).await {
                                        guard_failed = true;
                                    }
                                }
                            } else {
                                quote! {
                                    if self.#guard().await {
                                        guard_failed = true;
                                    }
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    let guard_checks_sync = combined_guards
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
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
                        })
                        .collect::<Vec<_>>();

                    let guard_checks_can_sync = combined_guards
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
                                quote! {
                                    if !self.#guard(payload_ref) {
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
                        })
                        .collect::<Vec<_>>();

                    let unless_checks_sync = combined_unless
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
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
                        })
                        .collect::<Vec<_>>();

                    let unless_checks_can_sync = combined_unless
                        .iter()
                        .map(|guard| {
                            if payload_ty.is_some() {
                                quote! {
                                    if self.#guard(payload_ref) {
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
                        })
                        .collect::<Vec<_>>();

                    let mut before_async = Vec::new();
                    before_async.extend(event.before.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref).await; }
                        } else {
                            quote! { self.#callback().await; }
                        }
                    }));
                    before_async.extend(transition.before.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref).await; }
                        } else {
                            quote! { self.#callback().await; }
                        }
                    }));

                    let mut after_async = Vec::new();
                    after_async.extend(transition.after.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref).await; }
                        } else {
                            quote! { self.#callback().await; }
                        }
                    }));
                    after_async.extend(event.after.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref).await; }
                        } else {
                            quote! { self.#callback().await; }
                        }
                    }));

                    let mut before_sync = Vec::new();
                    before_sync.extend(event.before.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref); }
                        } else {
                            quote! { self.#callback(); }
                        }
                    }));
                    before_sync.extend(transition.before.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref); }
                        } else {
                            quote! { self.#callback(); }
                        }
                    }));

                    let mut after_sync = Vec::new();
                    after_sync.extend(transition.after.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref); }
                        } else {
                            quote! { self.#callback(); }
                        }
                    }));
                    after_sync.extend(event.after.iter().map(|callback| {
                        if payload_ty.is_some() {
                            quote! { self.#callback(payload_ref); }
                        } else {
                            quote! { self.#callback(); }
                        }
                    }));

                    let target = &transition.target;

                    let target_state_expr = quote!(target_state);
                    let around_checks_async = callbacks
                        .around
                        .iter()
                        .map(|callback| {
                            let filter = Self::build_filter_expr(
                                &callback.from,
                                &callback.to,
                                &callback.on,
                                state_ident,
                                method_ident,
                                &quote!(current_state),
                                &target_state_expr,
                            );
                            let callback_ident = &callback.name;
                            quote! {
                                if #filter {
                                    if !self.#callback_ident(
                                        current_state,
                                        target_state,
                                        stringify!(#method_ident),
                                        #payload_any_async,
                                    ).await {
                                        if last_guard_failure.is_none() {
                                            last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                                current_state,
                                                stringify!(#method_ident),
                                                stringify!(#callback_ident),
                                            ));
                                        }
                                        guard_failed = true;
                                    }
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    let around_checks_sync = callbacks
                        .around
                        .iter()
                        .map(|callback| {
                            let filter = Self::build_filter_expr(
                                &callback.from,
                                &callback.to,
                                &callback.on,
                                state_ident,
                                method_ident,
                                &quote!(current_state),
                                &target_state_expr,
                            );
                            let callback_ident = &callback.name;
                            quote! {
                                if #filter {
                                    if !self.#callback_ident(
                                        current_state,
                                        target_state,
                                        stringify!(#method_ident),
                                        #payload_any_sync,
                                    ) {
                                        if last_guard_failure.is_none() {
                                            last_guard_failure = Some(#core_path::TransitionError::guard_failed(
                                                current_state,
                                                stringify!(#method_ident),
                                                stringify!(#callback_ident),
                                            ));
                                        }
                                        guard_failed = true;
                                    }
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    transition_blocks_async.push(quote! {
                        {
                            let mut guard_failed = false;
                            #( #guard_checks_async )*
                            #( #unless_checks_async )*
                            if !guard_failed {
                                let target_state = #state_ident::#target;
                                #( #around_checks_async )*
                                if !guard_failed {
                                    #( #before_async )*
                                    let __prev_state = self.state;
                                    self.state = target_state;
                                    #(
                                        if let Some(_payload) = payload_ty
                                    )*
                                    #action_call_async
                                    #( #after_async )*
                                    return Ok(());
                                }
                            }
                        }
                    });

                    can_blocks_async.push(quote! {
                        {
                            let mut guard_failed = false;
                            #( #guard_checks_can_async )*
                            #( #unless_checks_can_async )*
                            if !guard_failed {
                                let target_state = #state_ident::#target;
                                #( #around_checks_async )*
                                if !guard_failed {
                                    allowed = true;
                                }
                            }
                        }
                    });

                    let action_call_sync = if let Some(action) = action_method {
                        if payload_ty.is_some() {
                            quote! {
                                if !self.#action(payload_ref) {
                                    let __failed_from = __prev_state;
                                    self.state = __prev_state;
                                    return Err(#core_path::TransitionError {
                                        from: __failed_from,
                                        event: stringify!(#method_ident),
                                        kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                                    });
                                }
                            }
                        } else {
                            quote! {
                                if !self.#action() {
                                    let __failed_from = __prev_state;
                                    self.state = __prev_state;
                                    return Err(#core_path::TransitionError {
                                        from: __failed_from,
                                        event: stringify!(#method_ident),
                                        kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                                    });
                                }
                            }
                        }
                    } else {
                        quote! {}
                    };

                    let action_call_async = if let Some(action) = action_method {
                        if payload_ty.is_some() {
                            quote! {
                                if !self.#action(payload_ref).await {
                                    let __failed_from = __prev_state;
                                    self.state = __prev_state;
                                    return Err(#core_path::TransitionError {
                                        from: __failed_from,
                                        event: stringify!(#method_ident),
                                        kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                                    });
                                }
                            }
                        } else {
                            quote! {
                                if !self.#action().await {
                                    let __failed_from = __prev_state;
                                    self.state = __prev_state;
                                    return Err(#core_path::TransitionError {
                                        from: __failed_from,
                                        event: stringify!(#method_ident),
                                        kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                                    });
                                }
                            }
                        }
                    } else {
                        quote! {}
                    };

                    transition_blocks_sync.push(quote! {
                        {
                            let mut guard_failed = false;
                            #( #guard_checks_sync )*
                            #( #unless_checks_sync )*
                            if !guard_failed {
                                let target_state = #state_ident::#target;
                                #( #around_checks_sync )*
                                if !guard_failed {
                                    #( #before_sync )*
                                    let __prev_state = self.state;
                                    self.state = target_state;
                                    #action_call_sync
                                    #( #after_sync )*
                                    return Ok(());
                                }
                            }
                        }
                    });

                    can_blocks_sync.push(quote! {
                        {
                            let mut guard_failed = false;
                            #( #guard_checks_can_sync )*
                            #( #unless_checks_can_sync )*
                            if !guard_failed {
                                let target_state = #state_ident::#target;
                                #( #around_checks_sync )*
                                if !guard_failed {
                                    allowed = true;
                                }
                            }
                        }
                    });
                }

                state_match_arms.push(quote! {
                    #state_ident::#state => {
                        let mut last_guard_failure: Option<#core_path::TransitionError<#state_ident>> = None;
                        #( #transition_blocks_sync )*
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
                        #( #can_blocks_sync )*
                        allowed
                    }
                });
            }

            let event_method = if let Some(payload_ty) = payload_ty {
                let action_call_sync = if let Some(action) = action_method {
                    quote! {
                        if !self.#action(&payload) {
                            let __failed_from = current_state;
                            self.state = current_state;
                            return Err(#core_path::TransitionError {
                                from: __failed_from,
                                event: stringify!(#method_ident),
                                kind: #core_path::TransitionErrorKind::ActionFailed { action: stringify!(#action) },
                            });
                        }
                    }
                } else {
                    quote! {}
                };

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

            let can_method = if let Some(payload_ty) = payload_ty {
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

            sync_event_methods.push(event_method);
            sync_can_methods.push(can_method);
        }
        let machine_definition_items = quote! {
            pub const DEFINITION: #core_path::MachineDefinition<#state_ident> = #core_path::MachineDefinition {
                name: stringify!(#machine_ident),
                states: &[ #( #state_values ),* ],
                initial: #state_ident::#initial_state,
                async_mode: #async_mode_literal,
                events: &[ #( #event_definitions ),* ],
            };

            pub const fn definition() -> &'static #core_path::MachineDefinition<#state_ident> {
                &Self::DEFINITION
            }
        };

        let machine_methods = if async_mode {
            quote! {
                impl #machine_ident {
                    pub const ASYNC_MODE: bool = true;
                    #machine_definition_items
                    #( #async_event_methods )*
                    #( #async_can_methods )*
                }
            }
        } else {
            quote! {
                impl #machine_ident {
                    pub const ASYNC_MODE: bool = false;
                    #machine_definition_items
                    #( #sync_event_methods )*
                    #( #sync_can_methods )*
                }
            }
        };

        let expanded = quote! {
            #state_variant_defs
            #state_impl
            #machine_struct
            #new_method
            #machine_default
            #machine_trait_impl
            #machine_methods
        };

        Ok(expanded)
    }

    fn validate(&self) -> Result<()> {
        if !self.states.iter().any(|state| state == &self.initial) {
            return Err(syn::Error::new(
                self.initial.span(),
                "`initial` must be a member of `states`",
            ));
        }

        let mut seen = HashSet::new();
        for state in &self.states {
            if !seen.insert(state.to_string()) {
                return Err(syn::Error::new(state.span(), "duplicate state"));
            }
        }

        for event in &self.events {
            if event.transitions.is_empty() {
                return Err(syn::Error::new(
                    event.name.span(),
                    "event must declare at least one transition",
                ));
            }

            for transition in &event.transitions {
                if transition.sources.is_empty() {
                    return Err(syn::Error::new(
                        transition.target.span(),
                        "transition must declare at least one source state",
                    ));
                }

                if !self.states.iter().any(|state| state == &transition.target) {
                    return Err(syn::Error::new(
                        transition.target.span(),
                        "target state not declared in `states`",
                    ));
                }

                for source in &transition.sources {
                    if !self.states.iter().any(|state| state == source) {
                        return Err(syn::Error::new(
                            source.span(),
                            "source state not declared in `states`",
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn build_callback_invocation(
        callback: &TransitionCallback,
        state_ident: &Ident,
        event_ident: &Ident,
        call_async: bool,
    ) -> proc_macro2::TokenStream {
        let callback_ident = &callback.name;

        let from_check = if callback.from.is_empty() {
            quote!(true)
        } else if callback.from.len() == 1 {
            let state = &callback.from[0];
            quote!(current_state == #state_ident::#state)
        } else {
            let patterns = callback
                .from
                .iter()
                .map(|state| quote!(#state_ident::#state));
            quote!(matches!(current_state, #( #patterns )|*))
        };

        let to_check = if callback.to.is_empty() {
            quote!(true)
        } else if callback.to.len() == 1 {
            let state = &callback.to[0];
            quote!(target_state == #state_ident::#state)
        } else {
            let patterns = callback.to.iter().map(|state| quote!(#state_ident::#state));
            quote!(matches!(target_state, #( #patterns )|*))
        };

        let event_check = if callback.on.is_empty() {
            quote!(true)
        } else {
            let names = callback.on.iter().map(|event| quote!(stringify!(#event)));
            quote!(matches!(stringify!(#event_ident), #( #names )|*))
        };

        let call = if call_async {
            quote! { self.#callback_ident().await; }
        } else {
            quote! { self.#callback_ident(); }
        };

        quote! {
            if #from_check && #to_check && #event_check {
                #call
            }
        }
    }
}

struct Event {
    name: Ident,
    transitions: Vec<Transition>,
    guards: Vec<Ident>,
    unless: Vec<Ident>,
    before: Vec<Ident>,
    after: Vec<Ident>,
    payload: Option<EventPayload>,
}

struct Transition {
    sources: Vec<Ident>,
    target: Ident,
    guards: Vec<Ident>,
    unless: Vec<Ident>,
    before: Vec<Ident>,
    after: Vec<Ident>,
}

#[derive(Default)]
struct Callbacks {
    before: Vec<TransitionCallback>,
    after: Vec<TransitionCallback>,
    around: Vec<AroundCallback>,
}

struct TransitionCallback {
    name: Ident,
    from: Vec<Ident>,
    to: Vec<Ident>,
    on: Vec<Ident>,
}

struct AroundCallback {
    name: Ident,
    from: Vec<Ident>,
    to: Vec<Ident>,
    on: Vec<Ident>,
}

struct EventPayload {
    ty: syn::Type,
}

fn parse_ident_list(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    let mut items = Vec::new();
    while !input.is_empty() {
        items.push(input.parse()?);
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }
    Ok(items)
}

fn parse_events(input: &ParseBuffer<'_>) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    while !input.is_empty() {
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut transitions = Vec::new();
        let mut guards = Vec::new();
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut unless = Vec::new();
        let mut payload = None;

        while !content.is_empty() {
            let key: Ident = content.parse()?;
            let key_str = key.to_string();

            match key_str.as_str() {
                "transition" => {
                    content.parse::<Token![:]>()?;
                    let block;
                    braced!(block in content);
                    transitions.push(parse_transition(&block, payload.as_ref())?);
                }
                "guards" => {
                    content.parse::<Token![:]>()?;
                    guards = parse_ident_list_value(&content)?;
                }
                "unless" => {
                    content.parse::<Token![:]>()?;
                    unless = parse_ident_list_value(&content)?;
                }
                "before" => {
                    content.parse::<Token![:]>()?;
                    before = parse_ident_list_value(&content)?;
                }
                "after" => {
                    content.parse::<Token![:]>()?;
                    after = parse_ident_list_value(&content)?;
                }
                "payload" => {
                    content.parse::<Token![:]>()?;
                    let ty: syn::Type = content.parse()?;
                    payload = Some(EventPayload { ty });
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unexpected key `{}`", other),
                    ));
                }
            }

            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        events.push(Event {
            name,
            transitions,
            guards,
            unless,
            before,
            after,
            payload,
        });

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(events)
}

fn parse_transition(input: &ParseBuffer<'_>, event_payload: Option<&EventPayload>) -> Result<Transition> {
    let mut sources = None;
    let mut target = None;
    let mut guards = Vec::new();
    let mut unless = Vec::new();
    let mut before = Vec::new();
    let mut after = Vec::new();

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();
        input.parse::<Token![:]>()?;

        match key_str.as_str() {
            "from" => {
                sources = Some(parse_state_set(input)?);
            }
            "to" => {
                target = Some(input.parse()?);
            }
            "guards" => {
                guards = parse_ident_list_value(input)?;
            }
            "unless" => {
                unless = parse_ident_list_value(input)?;
            }
            "before" => {
                before = parse_ident_list_value(input)?;
            }
            "after" => {
                after = parse_ident_list_value(input)?;
            }
            other => {
                return Err(syn::Error::new(
                    key.span(),
                    format!("unexpected key `{}`", other),
                ));
            }
        }

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(Transition {
        sources: sources
            .ok_or_else(|| syn::Error::new(Span::call_site(), "transition missing `from`"))?,
        target: target
            .ok_or_else(|| syn::Error::new(Span::call_site(), "transition missing `to`"))?,
        guards,
        unless,
        before,
        after,
    })
}

fn parse_state_set(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    if input.peek(syn::token::Bracket) {
        let content;
        bracketed!(content in input);
        parse_ident_list(&content)
    } else {
        Ok(vec![input.parse()?])
    }
}

fn parse_ident_list_value(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    if input.peek(syn::token::Bracket) {
        let content;
        bracketed!(content in input);
        parse_ident_list(&content)
    } else {
        Ok(vec![input.parse()?])
    }
}

fn parse_callbacks(input: &ParseBuffer<'_>) -> Result<Callbacks> {
    let mut callbacks = Callbacks::default();

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();

        match key_str.as_str() {
            "before_transition" => {
                let list_content;
                bracketed!(list_content in input);
                callbacks.before = parse_callback_list(&list_content)?;
            }
            "after_transition" => {
                let list_content;
                bracketed!(list_content in input);
                callbacks.after = parse_callback_list(&list_content)?;
            }
            "around_transition" => {
                let list_content;
                bracketed!(list_content in input);
                callbacks.around = parse_around_callback_list(&list_content)?;
            }
            other => {
                return Err(syn::Error::new(
                    key.span(),
                    format!("unexpected key `{}`", other),
                ));
            }
        }

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(callbacks)
}

fn parse_callback_list(input: &ParseBuffer<'_>) -> Result<Vec<TransitionCallback>> {
    let mut callbacks = Vec::new();

    while !input.is_empty() {
        let content;
        braced!(content in input);
        callbacks.push(parse_callback(&content)?);

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(callbacks)
}

fn parse_around_callback_list(input: &ParseBuffer<'_>) -> Result<Vec<AroundCallback>> {
    let mut callbacks = Vec::new();

    while !input.is_empty() {
        let content;
        braced!(content in input);
        callbacks.push(parse_around_callback(&content)?);

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(callbacks)
}

fn parse_callback(input: &ParseBuffer<'_>) -> Result<TransitionCallback> {
    let mut name = None;
    let mut from = Vec::new();
    let mut to = Vec::new();
    let mut on = Vec::new();

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();
        input.parse::<Token![:]>()?;

        match key_str.as_str() {
            "name" => {
                name = Some(input.parse()?);
            }
            "from" => {
                from = parse_state_set(input)?;
            }
            "to" => {
                to = parse_state_set(input)?;
            }
            "on" => {
                on = parse_ident_list_value(input)?;
            }
            other => {
                return Err(syn::Error::new(
                    key.span(),
                    format!("unexpected key `{}`", other),
                ));
            }
        }

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(TransitionCallback {
        name: name.ok_or_else(|| syn::Error::new(Span::call_site(), "callback missing `name`"))?,
        from,
        to,
        on,
    })
}

fn parse_around_callback(input: &ParseBuffer<'_>) -> Result<AroundCallback> {
    let mut name = None;
    let mut from = Vec::new();
    let mut to = Vec::new();
    let mut on = Vec::new();

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();
        input.parse::<Token![:]>()?;

        match key_str.as_str() {
            "name" => {
                name = Some(input.parse()?);
            }
            "from" => {
                from = parse_state_set(input)?;
            }
            "to" => {
                to = parse_state_set(input)?;
            }
            "on" => {
                on = parse_ident_list_value(input)?;
            }
            other => {
                return Err(syn::Error::new(
                    key.span(),
                    format!("unexpected key `{}`", other),
                ));
            }
        }

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(AroundCallback {
        name: name.ok_or_else(|| syn::Error::new(Span::call_site(), "callback missing `name`"))?,
        from,
        to,
        on,
    })
}
