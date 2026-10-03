//! Parsing logic for the state machine macro.
//!
//! This module handles converting the macro input tokens into our
//! internal data structures. It includes parsers for:
//! - The main StateMachine structure (via syn::Parse trait)
//! - States section (including nested superstates)
//! - Events and transitions
//! - Helper utilities for parsing lists and sets

use crate::types::*;
use proc_macro2::Span;
use quote::format_ident;
use std::collections::HashSet;
use syn::{
    Ident, Result, Token, braced, bracketed, parenthesized,
    parse::{Parse, ParseBuffer, ParseStream},
};

/// Implementation of syn::Parse for StateMachine.
///
/// This allows us to use `syn::parse_macro_input!(input as StateMachine)`
/// to parse the entire macro input in one go.
impl Parse for StateMachine {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        // Initialize all fields with defaults or None
        let mut name = None;
        let mut initial = None;
        let mut context = None;
        let mut error = None;
        let mut states = None;
        let mut events = None;
        let mut callbacks = GlobalCallbacks::default();
        let mut lifecycle = Vec::new();
        let mut runtime_lifecycle = Vec::new();
        let mut final_states = Vec::new();
        let mut snapshot = false;
        let mut async_mode = false;
        let mut dynamic_mode = false;
        let mut state_storage = Vec::new();
        let mut hierarchy = Hierarchy::default();

        // Parse each key-value pair in the macro input
        while !input.is_empty() {
            // Handle the special case of `async` keyword
            if input.peek(Token![async]) {
                let _: Token![async] = input.parse()?;
                input.parse::<Token![:]>()?;
                let value: syn::LitBool = input.parse()?;
                async_mode = value.value();
            } else {
                let key: Ident = input.parse()?;
                let key_str = key.to_string();

                match key_str.as_str() {
                    "runtime" => {
                        input.parse::<Token![:]>()?;
                        let content;
                        braced!(content in input);
                        runtime_lifecycle = parse_runtime_lifecycle(&content)?;
                    }
                    "dynamic" => {
                        input.parse::<Token![:]>()?;
                        let value: syn::LitBool = input.parse()?;
                        dynamic_mode = value.value();
                    }
                    "snapshot" => {
                        input.parse::<Token![:]>()?;
                        snapshot = input.parse::<syn::LitBool>()?.value;
                    }
                    "name" => {
                        input.parse::<Token![:]>()?;
                        name = Some(input.parse()?);
                    }
                    "initial" => {
                        input.parse::<Token![:]>()?;
                        initial = Some(input.parse()?);
                    }
                    "final_states" => {
                        input.parse::<Token![:]>()?;
                        final_states = parse_ident_list_value(input)?;
                    }
                    "context" => {
                        input.parse::<Token![:]>()?;
                        context = Some(input.parse()?);
                    }
                    "error" => {
                        input.parse::<Token![:]>()?;
                        error = Some(input.parse()?);
                    }
                    "states" => {
                        input.parse::<Token![:]>()?;
                        let content;
                        bracketed!(content in input);
                        let parsed_states = parse_states_section(&content)?;
                        states = Some(parsed_states.leaves);
                        hierarchy = parsed_states.hierarchy;
                        state_storage = parsed_states.storage;
                    }
                    "events" => {
                        // Optional colon for backwards compatibility
                        if input.peek(Token![:]) {
                            input.parse::<Token![:]>()?;
                        }
                        let content;
                        braced!(content in input);
                        events = Some(parse_events(&content)?);
                    }
                    "callbacks" => {
                        // Optional colon, mirroring `events`
                        if input.peek(Token![:]) {
                            input.parse::<Token![:]>()?;
                        }
                        let content;
                        braced!(content in input);
                        callbacks = parse_global_callbacks(&content)?;
                    }
                    "lifecycle" => {
                        input.parse::<Token![:]>()?;
                        let content;
                        braced!(content in input);
                        while !content.is_empty() {
                            let state = content.parse()?;
                            let block;
                            braced!(block in content);
                            let mut hooks = StateLifecycle {
                                state,
                                enter: Vec::new(),
                                exit: Vec::new(),
                                complete: Vec::new(),
                            };
                            while !block.is_empty() {
                                let key: Ident = block.parse()?;
                                block.parse::<Token![:]>()?;
                                match key.to_string().as_str() {
                                    "enter" => hooks.enter = parse_ident_list_value(&block)?,
                                    "exit" => hooks.exit = parse_ident_list_value(&block)?,
                                    "complete" => hooks.complete = parse_ident_list_value(&block)?,
                                    _ => return Err(unexpected_key(&key)),
                                }
                                skip_optional_comma(&block)?;
                            }
                            lifecycle.push(hooks);
                            skip_optional_comma(&content)?;
                        }
                    }
                    // Fields from the Ruby-era design that were never
                    // implemented. Error loudly instead of silently
                    // swallowing configuration the user expects to work.
                    "state" => {
                        return Err(syn::Error::new(
                            key.span(),
                            "`state` is not supported: states are types, not an enum; remove this field",
                        ));
                    }
                    "action" => {
                        return Err(syn::Error::new(
                            key.span(),
                            "`action` is not supported: use `callbacks: { before_transition [...] }` for machine-wide hooks",
                        ));
                    }
                    _ => {
                        return Err(unexpected_key(&key));
                    }
                }
            }

            skip_optional_comma(input)?;
        }

        // Build the StateMachine, returning errors for missing required fields
        let mut machine = Self {
            name: name.ok_or_else(|| syn::Error::new(Span::call_site(), "missing `name` field"))?,
            initial: initial
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `initial` field"))?,
            context,
            error,
            states: states
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `states` field"))?,
            state_storage,
            hierarchy,
            events: events.unwrap_or_default(),
            callbacks,
            lifecycle,
            runtime_lifecycle,
            final_states,
            snapshot,
            async_mode,
            dynamic_mode,
            transition_graph: TransitionGraph::default(),
        };

        // Build the transition graph from events
        machine.build_transition_graph();

        Ok(machine)
    }
}

fn parse_runtime_lifecycle(input: ParseStream<'_>) -> Result<Vec<RuntimeLifecycle>> {
    let mut rules = Vec::new();
    while !input.is_empty() {
        let state = input.parse()?;
        let body;
        braced!(body in input);
        let mut rule = RuntimeLifecycle {
            state,
            after: Vec::new(),
            invoke: Vec::new(),
            defer: Vec::new(),
        };
        while !body.is_empty() {
            let key: Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            match key.to_string().as_str() {
                "after" => {
                    let list;
                    bracketed!(list in body);
                    while !list.is_empty() {
                        let deadline;
                        braced!(deadline in list);
                        let mut delay = None;
                        let mut event = None;
                        while !deadline.is_empty() {
                            let key: Ident = deadline.parse()?;
                            deadline.parse::<Token![:]>()?;
                            match key.to_string().as_str() {
                                "delay" => {
                                    delay = Some(deadline.parse::<syn::LitInt>()?.base10_parse()?)
                                }
                                "event" => event = Some(deadline.parse()?),
                                _ => return Err(unexpected_key(&key)),
                            }
                            skip_optional_comma(&deadline)?;
                        }
                        rule.after.push(Deadline {
                            delay: delay.ok_or_else(|| list.error("deadline requires delay"))?,
                            event: event
                                .ok_or_else(|| list.error("deadline requires event factory"))?,
                        });
                        skip_optional_comma(&list)?;
                    }
                }
                "invoke" => rule.invoke = parse_ident_list_value(&body)?,
                "defer" => rule.defer = parse_ident_list_value(&body)?,
                _ => return Err(unexpected_key(&key)),
            }
            skip_optional_comma(&body)?;
        }
        rules.push(rule);
        skip_optional_comma(input)?;
    }
    Ok(rules)
}

/// Parse the states section of the macro input.
///
/// The states section can contain:
/// - Simple leaf states: `StateA, StateB`
/// - States with data: `Active(ConnectionData)`
/// - Superstates: `superstate Running { state Active, state Idle }`
///
/// Returns all leaf states, the hierarchy information, and storage specs.
pub fn parse_states_section(input: &ParseBuffer<'_>) -> Result<ParsedStates> {
    let mut leaves = Vec::new();
    let mut hierarchy = Hierarchy::default();
    let mut seen = HashSet::new();
    let mut storage_specs = Vec::new();

    while !input.is_empty() {
        let ident: Ident = input.parse()?;
        let key = ident.to_string();

        if key == "superstate" {
            // Parse a superstate block
            let superstate_name: Ident = input.parse()?;

            // Check for optional data type
            let superstate_ty = parse_optional_paren_type(input)?;

            // If the superstate has data, create a storage spec for it
            push_storage_spec(&mut storage_specs, &superstate_name, superstate_ty);

            // Parse the superstate's contents
            let mut ancestors = Vec::new();
            let block_content;
            braced!(block_content in input);
            let parsed = parse_superstate_block(
                &superstate_name,
                &block_content,
                &mut hierarchy,
                &mut leaves,
                &mut seen,
                &mut ancestors,
                &mut storage_specs,
            )?;

            // Register this superstate in the hierarchy
            hierarchy.register_superstate(superstate_name, parsed.descendants, parsed.initial);
        } else {
            // Parse a regular leaf state

            // Check for duplicates
            if !seen.insert(key.clone()) {
                return Err(syn::Error::new(ident.span(), "duplicate state"));
            }

            // Check for optional data type
            let data_ty = parse_optional_paren_type(input)?;

            let state_ident = ident;

            // Register this leaf state (no ancestors at top level)
            hierarchy.register_leaf(&state_ident, &[]);
            leaves.push(state_ident.clone());

            // If the state has data, create a storage spec for it
            push_storage_spec(&mut storage_specs, &state_ident, data_ty);
        }

        skip_optional_comma(input)?;
    }

    Ok(ParsedStates {
        leaves,
        hierarchy,
        storage: storage_specs,
    })
}

/// Parse a superstate block.
///
/// A superstate can contain:
/// - Child states: `state Active, state Idle`
/// - Nested superstates: `superstate SubGroup { ... }`
/// - Initial state specification: `initial: Active`
///
/// The `ancestors` parameter tracks the chain of parent superstates,
/// which is used for hierarchical transition resolution.
pub fn parse_superstate_block(
    superstate_name: &Ident,
    content: &ParseBuffer<'_>,
    hierarchy: &mut Hierarchy,
    leaves: &mut Vec<Ident>,
    seen: &mut HashSet<String>,
    ancestors: &mut Vec<Ident>,
    storage: &mut Vec<StateStorageSpec>,
) -> Result<SuperstateParseResult> {
    let mut descendants = Vec::new();
    let mut initial_spec: Option<Ident> = None;

    // Add ourselves to the ancestor chain
    ancestors.push(superstate_name.clone());

    while !content.is_empty() {
        let entry: Ident = content.parse()?;
        let entry_key = entry.to_string();

        match entry_key.as_str() {
            "state" => {
                // Parse a child state
                let state_ident: Ident = content.parse()?;

                // Check for duplicates
                if !seen.insert(state_ident.to_string()) {
                    return Err(syn::Error::new(state_ident.span(), "duplicate state"));
                }

                // Check for optional data type
                let data_ty = parse_optional_paren_type(content)?;

                // Register this leaf with its ancestor chain
                hierarchy.register_leaf(&state_ident, ancestors);
                leaves.push(state_ident.clone());
                descendants.push(state_ident.clone());

                // If the state has data, create a storage spec for it
                push_storage_spec(storage, &state_ident, data_ty);
            }
            "superstate" => {
                // Parse a nested superstate
                let nested_name: Ident = content.parse()?;

                // Check for optional data type
                let super_data_ty = parse_optional_paren_type(content)?;

                // If the superstate has data, create a storage spec for it
                push_storage_spec(storage, &nested_name, super_data_ty);

                // Parse the nested superstate's contents
                let block_content;
                braced!(block_content in content);
                let nested = parse_superstate_block(
                    &nested_name,
                    &block_content,
                    hierarchy,
                    leaves,
                    seen,
                    ancestors,
                    storage,
                )?;

                // Register the nested superstate
                hierarchy.register_superstate(
                    nested_name,
                    nested.descendants.clone(),
                    nested.initial.clone(),
                );

                // Add all nested descendants to our descendants
                descendants.extend(nested.descendants);
            }
            "initial" => {
                // Parse the initial state specification
                content.parse::<Token![:]>()?;
                let initial_ident: Ident = content.parse()?;
                initial_spec = Some(initial_ident);
            }
            _ => {
                return Err(unexpected_key(&entry));
            }
        }

        skip_optional_comma(content)?;
    }

    // Remove ourselves from the ancestor chain
    ancestors.pop();

    // Validate that we have at least one child
    if descendants.is_empty() {
        return Err(syn::Error::new(
            superstate_name.span(),
            "superstate must declare at least one child state",
        ));
    }

    // Determine the initial state
    let initial_ident = if let Some(initial) = initial_spec {
        let initial_name = initial.to_string();
        // Validate that the initial state is a descendant
        if !descendants.iter().any(|leaf| *leaf == initial_name) {
            return Err(syn::Error::new(
                initial.span(),
                "`initial` must reference a descendant state",
            ));
        }
        initial
    } else {
        // Default to first descendant if no initial specified
        descendants[0].clone()
    };

    Ok(SuperstateParseResult {
        descendants,
        initial: initial_ident,
    })
}

pub fn parse_events(input: &ParseBuffer<'_>) -> Result<Vec<Event>> {
    let mut events = Vec::new();

    while !input.is_empty() {
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut transitions = Vec::new();
        let mut hooks = Hooks::default();
        let mut payload = None;
        let mut branching = false;
        let mut hierarchical = false;
        let mut automatic = false;
        let mut completion = None;

        // Parse each field in the event block
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            let key_str = key.to_string();
            content.parse::<Token![:]>()?;

            match key_str.as_str() {
                "transition" => {
                    let block;
                    braced!(block in content);
                    transitions.push(parse_transition(&block)?);
                }
                "payload" => {
                    payload = Some(content.parse()?);
                }
                "branching" => branching = content.parse::<syn::LitBool>()?.value,
                "hierarchical" => hierarchical = content.parse::<syn::LitBool>()?.value,
                "automatic" => automatic = content.parse::<syn::LitBool>()?.value,
                "completion" => completion = Some(content.parse()?),
                other => {
                    if !hooks.parse_field(other, &content)? {
                        return Err(unexpected_key(&key));
                    }
                }
            }

            skip_optional_comma(&content)?;
        }

        events.push(Event {
            automatic: automatic || completion.is_some(),
            completion,
            hierarchical,
            name,
            payload,
            transitions,
            hooks,
            branching,
        });

        skip_optional_comma(input)?;
    }

    Ok(events)
}

/// Parse the `callbacks:` block of global, filterable callbacks.
///
/// ```ignore
/// callbacks: {
///     before_transition [
///         { name: log_exit, from: [Active], on: [shutdown] }
///     ],
///     after_transition [
///         { name: on_enter_ready, to: Ready }
///     ],
///     around_transition [
///         { name: wrap_all }
///     ]
/// }
/// ```
///
/// Each hook key takes a bracketed list of `{ name: ..., from: ..., to: ..., on: ... }`
/// entries. `name` is required; the filters are optional and accept a single
/// identifier or a bracketed list. `from`/`to` may name superstates.
pub fn parse_global_callbacks(input: &ParseBuffer<'_>) -> Result<GlobalCallbacks> {
    let mut callbacks = GlobalCallbacks::default();

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();

        let bucket = match key_str.as_str() {
            "before_transition" => &mut callbacks.before,
            "after_transition" => &mut callbacks.after,
            "around_transition" => &mut callbacks.around,
            "error_transition" => &mut callbacks.on_error,
            _ => {
                return Err(unexpected_key_in(
                    &key,
                    "`callbacks`",
                    "`before_transition`, `after_transition`, `around_transition`, or `error_transition`",
                ));
            }
        };

        // Optional colon before the bracketed list
        if input.peek(Token![:]) {
            input.parse::<Token![:]>()?;
        }

        let list;
        bracketed!(list in input);
        while !list.is_empty() {
            let entry;
            braced!(entry in list);
            bucket.push(parse_global_callback_entry(&entry)?);
            skip_optional_comma(&list)?;
        }

        skip_optional_comma(input)?;
    }

    Ok(callbacks)
}

/// Parse a single `{ name: cb, from: ..., to: ..., on: ... }` entry.
fn parse_global_callback_entry(input: &ParseBuffer<'_>) -> Result<GlobalCallback> {
    let mut name = None;
    let mut from = None;
    let mut to = None;
    let mut on = None;

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();
        input.parse::<Token![:]>()?;

        match key_str.as_str() {
            "name" => name = Some(input.parse()?),
            "from" => from = Some(parse_state_set(input)?),
            "to" => to = Some(parse_state_set(input)?),
            "on" => on = Some(parse_ident_list_value(input)?),
            _ => {
                return Err(unexpected_key_in(
                    &key,
                    "callback entry",
                    "`name`, `from`, `to`, or `on`",
                ));
            }
        }

        skip_optional_comma(input)?;
    }

    Ok(GlobalCallback {
        name: name
            .ok_or_else(|| syn::Error::new(Span::call_site(), "callback entry missing `name`"))?,
        from,
        to,
        on,
    })
}

pub fn parse_transition(input: &ParseBuffer<'_>) -> Result<Transition> {
    let mut kind = None;
    let mut data = None;
    let mut sources = None;
    let mut target = None;
    let mut hooks = Hooks::default();
    let mut internal = false;
    let mut fallback = false;
    let mut history = None;

    while !input.is_empty() {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();
        input.parse::<Token![:]>()?;

        match key_str.as_str() {
            "kind" => {
                let value: Ident = input.parse()?;
                kind = Some(match value.to_string().as_str() {
                    "internal" => TransitionKind::Internal,
                    "local" => TransitionKind::Local,
                    "external" => TransitionKind::External,
                    _ => {
                        return Err(syn::Error::new(
                            value.span(),
                            "kind must be internal, local, or external",
                        ));
                    }
                });
            }
            "data" => data = Some(input.parse()?),
            "from" => {
                sources = Some(parse_state_set(input)?);
            }
            "to" => {
                target = Some(input.parse()?);
            }
            "internal" => {
                internal = input.parse::<syn::LitBool>()?.value;
            }
            "fallback" => fallback = input.parse::<syn::LitBool>()?.value,
            "history" => {
                let mode: Ident = input.parse()?;
                history = Some(match mode.to_string().as_str() {
                    "shallow" => HistoryMode::Shallow,
                    "deep" => HistoryMode::Deep,
                    _ => {
                        return Err(syn::Error::new(
                            mode.span(),
                            "history must be `shallow` or `deep`",
                        ));
                    }
                });
            }
            other => {
                if !hooks.parse_field(other, input)? {
                    return Err(unexpected_key(&key));
                }
            }
        }

        skip_optional_comma(input)?;
    }

    let sources =
        sources.ok_or_else(|| syn::Error::new(Span::call_site(), "transition missing `from`"))?;
    if internal && kind.is_some_and(|kind| kind != TransitionKind::Internal) {
        return Err(syn::Error::new(
            Span::call_site(),
            "`internal` conflicts with transition kind",
        ));
    }
    internal |= kind == Some(TransitionKind::Internal);
    if internal && target.is_some() {
        return Err(syn::Error::new(
            Span::call_site(),
            "internal transitions must omit `to`",
        ));
    }
    let target = if internal {
        sources.first().cloned()
    } else {
        target
    }
    .ok_or_else(|| syn::Error::new(Span::call_site(), "transition missing `to` or source"))?;
    Ok(Transition {
        kind,
        data,
        sources,
        target,
        hooks,
        internal,
        fallback,
        history,
    })
}

impl Hooks {
    /// Parse the value of a hook-list key into its slot.
    ///
    /// Returns `false` without consuming input when `key` is not one of
    /// `guards`, `unless`, `before`, `after`, or `around`.
    fn parse_field(&mut self, key: &str, input: &ParseBuffer<'_>) -> Result<bool> {
        let slot = match key {
            "guards" => &mut self.guards,
            "unless" => &mut self.unless,
            "before" => &mut self.before,
            "after" => &mut self.after,
            "around" => &mut self.around,
            "on_error" => &mut self.on_error,
            _ => return Ok(false),
        };
        *slot = parse_ident_list_value(input)?;
        Ok(true)
    }
}

// ========== Helper Functions ==========

/// Consume an optional trailing comma if one is present.
fn skip_optional_comma(input: &ParseBuffer<'_>) -> Result<()> {
    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }
    Ok(())
}

/// Build the standard "unexpected key" error anchored at the key's span.
fn unexpected_key(key: &Ident) -> syn::Error {
    syn::Error::new(key.span(), format!("unexpected key `{}`", key))
}

/// Like [`unexpected_key`], naming the enclosing block and the accepted keys.
fn unexpected_key_in(key: &Ident, block: &str, expected: &str) -> syn::Error {
    syn::Error::new(
        key.span(),
        format!(
            "unexpected key `{}` in {} (expected {})",
            key, block, expected
        ),
    )
}

/// Parse an optional `(Type)` suffix used to attach data to a state.
fn parse_optional_paren_type(input: &ParseBuffer<'_>) -> Result<Option<syn::Type>> {
    if input.peek(syn::token::Paren) {
        let ty_content;
        parenthesized!(ty_content in input);
        Ok(Some(ty_content.parse()?))
    } else {
        Ok(None)
    }
}

/// Parse a comma-separated list of identifiers.
///
/// Used for parsing lists like `StateA, StateB, StateC`.
pub fn parse_ident_list(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    let mut items = Vec::new();
    while !input.is_empty() {
        items.push(input.parse()?);
        skip_optional_comma(input)?;
    }
    Ok(items)
}

/// Parse a state set (either a single identifier or a bracketed list).
///
/// Examples:
/// - `StateA` -> vec![StateA]
/// - `[StateA, StateB]` -> vec![StateA, StateB]
pub fn parse_state_set(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    if input.peek(syn::token::Bracket) {
        let content;
        bracketed!(content in input);
        parse_ident_list(&content)
    } else {
        Ok(vec![input.parse()?])
    }
}

/// Parse an identifier list value (either a single identifier or a bracketed list).
///
/// This is similar to parse_state_set but used for non-state lists
/// like guards, callbacks, etc.
pub fn parse_ident_list_value(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    parse_state_set(input)
}

/// Push a storage spec for a state that declares associated data.
///
/// No-op when the state has no data type. Centralises the field-name
/// derivation so leaf states and superstates stay consistent.
fn push_storage_spec(
    storage: &mut Vec<StateStorageSpec>,
    state_name: &Ident,
    ty: Option<syn::Type>,
) {
    if let Some(ty) = ty {
        let field = storage_field_ident(state_name);
        storage.push(StateStorageSpec {
            state_name: state_name.clone(),
            field,
            ty,
        });
    }
}

/// Generate the storage field identifier for a state.
///
/// Converts a state name like `ConnectionActive` to a field name
/// like `__state_data_connection_active`.
pub fn storage_field_ident(name: &Ident) -> Ident {
    let snake = crate::codegen::utils::to_snake_case(&name.to_string());
    format_ident!("__state_data_{}", snake)
}

impl StateMachine {
    /// Build the transition graph from the parsed events.
    ///
    /// This populates the transition_graph field by extracting all
    /// transitions from events and creating edges in the graph.
    pub fn build_transition_graph(&mut self) {
        for event in &self.events {
            for (origin, transition) in event.transitions.iter().enumerate() {
                // Event-level guards/callbacks run before transition-level ones
                let hooks = event.hooks.merged(&transition.hooks);

                // Expand source states (handle superstates)
                for source in &transition.sources {
                    let expanded_sources = self.hierarchy.expand_state(source, &self.states);
                    let choices = crate::codegen::history::choices(self, transition);

                    for actual_source in expanded_sources {
                        if let Some(scope) = &event.completion
                            && !crate::codegen::finality::completed(self, &actual_source)
                                .contains(&scope)
                        {
                            continue;
                        }
                        for (resolved_target, history) in &choices {
                            let resolved_target = if transition.internal {
                                actual_source.clone()
                            } else {
                                resolved_target.clone()
                            };
                            // Global callbacks whose filters match this concrete
                            // edge. Around callbacks take no payload, so global
                            // ones can share the around list; they are prepended
                            // so machine-wide wrappers run outermost.
                            let matching_globals = |bucket: &[GlobalCallback]| -> Vec<Ident> {
                                bucket
                                    .iter()
                                    .filter(|cb| {
                                        cb.matches(
                                            &self.hierarchy,
                                            &self.states,
                                            &actual_source,
                                            &resolved_target,
                                            &event.name,
                                        )
                                    })
                                    .map(|cb| cb.name.clone())
                                    .collect()
                            };

                            let global_before = matching_globals(&self.callbacks.before);
                            let global_after = matching_globals(&self.callbacks.after);

                            let mut edge_hooks = hooks.clone();
                            edge_hooks
                                .around
                                .splice(0..0, matching_globals(&self.callbacks.around));
                            edge_hooks
                                .on_error
                                .splice(0..0, matching_globals(&self.callbacks.on_error));

                            self.transition_graph.add_edge(
                                &actual_source,
                                TransitionEdge {
                                    scope: source.clone(),
                                    kind: transition.kind,
                                    data: transition.data.clone(),
                                    target: resolved_target.clone(),
                                    event: event.name.clone(),
                                    hooks: edge_hooks,
                                    global_before,
                                    global_after,
                                    payload: event.payload.clone(),
                                    internal: transition.internal,
                                    selection: transition.hooks.clone(),
                                    fallback: transition.fallback,
                                    history: history.clone(),
                                    origin,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}
