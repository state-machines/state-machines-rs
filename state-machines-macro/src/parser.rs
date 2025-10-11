//! Parsing logic for the state machine macro.
//!
//! This module handles converting the macro input tokens into our
//! internal data structures. It includes parsers for:
//! - The main StateMachine structure (via syn::Parse trait)
//! - States section (including nested superstates)
//! - Events and transitions
//! - Callbacks
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
        let mut states = None;
        let mut events = None;
        let mut async_mode = false;
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
                    "name" => {
                        input.parse::<Token![:]>()?;
                        name = Some(input.parse()?);
                    }
                    "initial" => {
                        input.parse::<Token![:]>()?;
                        initial = Some(input.parse()?);
                    }
                    "context" => {
                        input.parse::<Token![:]>()?;
                        context = Some(input.parse()?);
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
                    // Legacy fields - parse but ignore
                    "state" | "action" | "callbacks" => {
                        input.parse::<Token![:]>()?;
                        if input.peek(syn::token::Brace) {
                            let _content;
                            braced!(_content in input);
                            // Consume but ignore
                        } else {
                            let _: Ident = input.parse()?;
                        }
                    }
                    other => {
                        return Err(syn::Error::new(
                            key.span(),
                            format!("unexpected key `{}`", other),
                        ));
                    }
                }
            }

            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        // Build the StateMachine, returning errors for missing required fields
        let mut machine = Self {
            name: name.ok_or_else(|| syn::Error::new(Span::call_site(), "missing `name` field"))?,
            initial: initial
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `initial` field"))?,
            context,
            states: states
                .ok_or_else(|| syn::Error::new(Span::call_site(), "missing `states` field"))?,
            state_storage,
            hierarchy,
            events: events.unwrap_or_default(),
            async_mode,
            transition_graph: TransitionGraph::default(),
        };

        // Build the transition graph from events
        machine.build_transition_graph();

        Ok(machine)
    }
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
            let superstate_ty = if input.peek(syn::token::Paren) {
                let ty_content;
                parenthesized!(ty_content in input);
                Some(ty_content.parse()?)
            } else {
                None
            };

            // If the superstate has data, create a storage spec for it
            if let Some(ty) = superstate_ty.clone() {
                let field = storage_field_ident(&superstate_name);
                storage_specs.push(StateStorageSpec { field, ty });
            }

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
            let data_ty = if input.peek(syn::token::Paren) {
                let ty_content;
                parenthesized!(ty_content in input);
                Some(ty_content.parse()?)
            } else {
                None
            };

            let state_ident = ident;

            // Register this leaf state (no ancestors at top level)
            hierarchy.register_leaf(&state_ident, &[]);
            leaves.push(state_ident.clone());

            // If the state has data, create a storage spec for it
            if let Some(ty) = data_ty {
                let field = storage_field_ident(&state_ident);
                storage_specs.push(StateStorageSpec { field, ty });
            }
        }

        // Optional trailing comma
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
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
                let data_ty = if content.peek(syn::token::Paren) {
                    let ty_content;
                    parenthesized!(ty_content in content);
                    Some(ty_content.parse()?)
                } else {
                    None
                };

                // Register this leaf with its ancestor chain
                hierarchy.register_leaf(&state_ident, ancestors);
                leaves.push(state_ident.clone());
                descendants.push(state_ident.clone());

                // If the state has data, create a storage spec for it
                if let Some(ty) = data_ty {
                    let field = storage_field_ident(&state_ident);
                    storage.push(StateStorageSpec { field, ty });
                }
            }
            "superstate" => {
                // Parse a nested superstate
                let nested_name: Ident = content.parse()?;

                // Check for optional data type
                let super_data_ty = if content.peek(syn::token::Paren) {
                    let ty_content;
                    parenthesized!(ty_content in content);
                    Some(ty_content.parse()?)
                } else {
                    None
                };

                // If the superstate has data, create a storage spec for it
                if let Some(ty) = super_data_ty.clone() {
                    let field = storage_field_ident(&nested_name);
                    storage.push(StateStorageSpec { field, ty });
                }

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
            other => {
                return Err(syn::Error::new(
                    entry.span(),
                    format!("unexpected key `{}`", other),
                ));
            }
        }

        // Optional trailing comma
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
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

/// Parse the events section.
///
/// Each event has:
/// - A name
/// - One or more transitions
/// - Optional guards, callbacks, and payload type
pub fn parse_events(input: &ParseBuffer<'_>) -> Result<Vec<Event>> {
    let mut events = Vec::new();

    while !input.is_empty() {
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);

        let mut transitions = Vec::new();
        let mut guards = Vec::new();
        let mut unless = Vec::new();
        let mut before = Vec::new();
        let mut after = Vec::new();
        let mut payload = None;

        // Parse each field in the event block
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            let key_str = key.to_string();

            match key_str.as_str() {
                "transition" => {
                    content.parse::<Token![:]>()?;
                    let block;
                    braced!(block in content);
                    transitions.push(parse_transition(&block)?);
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
                    payload = Some(content.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unexpected key `{}`", other),
                    ));
                }
            }

            // Optional trailing comma
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        events.push(Event {
            name,
            payload,
            transitions,
            guards,
            unless,
            before,
            after,
        });

        // Optional trailing comma
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(events)
}

/// Parse a single transition block.
///
/// A transition must have:
/// - from: One or more source states
/// - to: A target state
///
/// And can optionally have guards and callbacks.
pub fn parse_transition(input: &ParseBuffer<'_>) -> Result<Transition> {
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

        // Optional trailing comma
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

/// Parse the callbacks section.
///
/// Callbacks can be:
/// - before_transition: Runs before any transition
/// - after_transition: Runs after any transition
/// - around_transition: Wraps transitions with before/after hooks
#[allow(dead_code)]
pub fn parse_callbacks(input: &ParseBuffer<'_>) -> Result<Callbacks> {
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
                callbacks.around = parse_callback_list(&list_content)?;
            }
            other => {
                return Err(syn::Error::new(
                    key.span(),
                    format!("unexpected key `{}`", other),
                ));
            }
        }

        // Optional trailing comma
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(callbacks)
}

/// Parse a list of callback definitions.
///
/// Each callback is a block like:
/// `{ name: log_transition, from: [StateA, StateB], to: StateC, on: event_name }`
#[allow(dead_code)]
pub fn parse_callback_list(input: &ParseBuffer<'_>) -> Result<Vec<TransitionCallback>> {
    let mut callbacks = Vec::new();

    while !input.is_empty() {
        let content;
        braced!(content in input);
        callbacks.push(parse_callback(&content)?);

        // Optional trailing comma
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
    }

    Ok(callbacks)
}

/// Parse a single callback definition.
///
/// Must have a name, and can optionally filter by from/to/on.
#[allow(dead_code)]
pub fn parse_callback(input: &ParseBuffer<'_>) -> Result<TransitionCallback> {
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

        // Optional trailing comma
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

// ========== Helper Functions ==========

/// Parse a comma-separated list of identifiers.
///
/// Used for parsing lists like `StateA, StateB, StateC`.
pub fn parse_ident_list(input: &ParseBuffer<'_>) -> Result<Vec<Ident>> {
    let mut items = Vec::new();
    while !input.is_empty() {
        items.push(input.parse()?);
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
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
    if input.peek(syn::token::Bracket) {
        let content;
        bracketed!(content in input);
        parse_ident_list(&content)
    } else {
        Ok(vec![input.parse()?])
    }
}

/// Generate the storage field identifier for a state.
///
/// Converts a state name like `ConnectionActive` to a field name
/// like `__state_data_connection_active`.
pub fn storage_field_ident(name: &Ident) -> Ident {
    let snake = to_snake_case(&name.to_string());
    format_ident!("__state_data_{}", snake)
}

/// Convert a string from PascalCase to snake_case.
///
/// Used for generating field names from state names.
/// Example: "ConnectionActive" -> "connection_active"
pub fn to_snake_case(input: &str) -> String {
    let mut result = String::new();
    for (i, ch) in input.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                result.push('_');
            }
            for lower in ch.to_lowercase() {
                result.push(lower);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

impl StateMachine {
    /// Build the transition graph from the parsed events.
    ///
    /// This populates the transition_graph field by extracting all
    /// transitions from events and creating edges in the graph.
    pub fn build_transition_graph(&mut self) {
        for event in &self.events {
            for transition in &event.transitions {
                // Expand source states (handle superstates)
                for source in &transition.sources {
                    let expanded_sources = self.hierarchy.expand_state(source, &self.states);
                    let resolved_target = self
                        .hierarchy
                        .resolve_target(&transition.target)
                        .unwrap_or_else(|| transition.target.clone());

                    for actual_source in expanded_sources {
                        // Merge event-level and transition-level guards/callbacks
                        let mut all_guards = event.guards.clone();
                        all_guards.extend(transition.guards.clone());

                        let mut all_unless = event.unless.clone();
                        all_unless.extend(transition.unless.clone());

                        let mut all_before = event.before.clone();
                        all_before.extend(transition.before.clone());

                        let mut all_after = event.after.clone();
                        all_after.extend(transition.after.clone());

                        self.transition_graph.add_edge(
                            &actual_source,
                            resolved_target.clone(),
                            event.name.clone(),
                            all_guards,
                            all_unless,
                            all_before,
                            all_after,
                            event.payload.clone(),
                        );
                    }
                }
            }
        }
    }
}
