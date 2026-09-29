//! Utility functions for code generation.
//!
//! ## A Note on Naming Conventions
//!
//! At first, Rust made me believe I had superpowers. I didn't see the warnings,
//! so I brought my Ruby `snake_case` traditions into event names.
//! Then I shipped an early release, and Rust was like:
//! "Congratulations, Citizen! Now you need to follow the rules of the land."
//!
//! And thus, this module was born, converting between PascalCase and snake_case
//! so both the compiler and developers can live in harmony.

use crate::types::StateMachine;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{ToTokens, quote};

/// Convert PascalCase or camelCase to snake_case.
///
/// Examples:
/// - `Trip` → `trip`
/// - `EnterHalfOpen` → `enter_half_open`
/// - `HTTPRequest` → `http_request`
/// - `XMLParser` → `xml_parser`
///
/// This is used to generate snake_case method names from PascalCase event names,
/// following Rust naming conventions while preserving event enum variants in PascalCase.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() {
            let prev_is_lowercase = i > 0 && chars.get(i - 1).is_some_and(|c| c.is_lowercase());
            let prev_is_underscore = i > 0 && chars.get(i - 1) == Some(&'_');
            let next_is_lowercase = chars.get(i + 1).is_some_and(|c| c.is_lowercase());

            // Insert underscore before uppercase if:
            // 1. Not at start and not right after underscore
            // 2. Previous was lowercase (camelCase boundary: fooBar → foo_bar)
            // 3. In middle of acronym with lowercase following (HTTPRequest → http_request)
            if i > 0 && !prev_is_underscore && (prev_is_lowercase || next_is_lowercase) {
                // Special case: don't add underscore between consecutive uppercase
                // unless next char is lowercase (end of acronym)
                let prev_is_upper = i > 0 && chars.get(i - 1).is_some_and(|c| c.is_uppercase());
                if !prev_is_upper || next_is_lowercase {
                    result.push('_');
                }
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

/// Create a snake_case identifier from a PascalCase identifier.
///
/// Preserves the original span for better error messages.
pub fn to_snake_case_ident(ident: &Ident) -> Ident {
    let snake = to_snake_case(&ident.to_string());
    Ident::new(&snake, ident.span())
}

/// Convert snake_case to PascalCase.
///
/// Examples:
/// - `next` → `Next`
/// - `enter_half_open` → `EnterHalfOpen`
/// - `set_threshold` → `SetThreshold`
///
/// This is used to generate PascalCase enum variants from snake_case event names.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// The PascalCase event-enum variant for a snake_case event name.
pub fn event_pascal(event: &Ident) -> Ident {
    Ident::new(&to_pascal_case(&event.to_string()), event.span())
}

/// `<C>` when the context type is generic, nothing when it is concrete.
pub fn ctx_generics(machine: &StateMachine) -> TokenStream2 {
    if machine.context.is_some() {
        quote! {}
    } else {
        quote! { <C> }
    }
}

/// The type of the machine's `ctx` field: the concrete context, or `C`.
pub fn ctx_ty(machine: &StateMachine) -> TokenStream2 {
    match &machine.context {
        Some(ctx) => quote! { #ctx },
        None => quote! { C },
    }
}

/// Type arguments of the machine struct in `state`: `<state>` with a
/// concrete context, `<C, state>` with a generic one.
pub fn machine_params(machine: &StateMachine, state: impl ToTokens) -> TokenStream2 {
    if machine.context.is_some() {
        quote! { <#state> }
    } else {
        quote! { <C, #state> }
    }
}

/// `async` in async mode, nothing otherwise.
pub fn maybe_async(is_async: bool) -> TokenStream2 {
    if is_async {
        quote! { async }
    } else {
        quote! {}
    }
}

/// `.await` in async mode, nothing otherwise.
pub fn maybe_await(is_async: bool) -> TokenStream2 {
    if is_async {
        quote! { .await }
    } else {
        quote! {}
    }
}

/// Field initialisers that start every state-data slot empty.
pub fn empty_storage_inits(machine: &StateMachine) -> Vec<TokenStream2> {
    machine
        .state_storage
        .iter()
        .map(|spec| {
            let field = &spec.field;
            quote! { #field: ::core::option::Option::None }
        })
        .collect()
}

#[cfg(test)]
mod tests;
