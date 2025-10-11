//! Utility functions for code generation.
//!
//! ## A Note on Naming Conventions
//!
//! At first, Rust made me believe I had superpowers. I didn't see the warnings,
//! so I brought my Ruby `snake_case` traditions into event names.
//! Then I published version 0.3, and Rust was like:
//! "Congratulations, Citizen! Now you need to follow the rules of the land."
//!
//! And thus, this module was born, converting between PascalCase and snake_case
//! so both the compiler and developers can live in harmony.

use proc_macro2::{Ident, Span};

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

/// Create a snake_case identifier with a custom span.
#[allow(dead_code)]
pub fn to_snake_case_ident_with_span(s: &str, span: Span) -> Ident {
    let snake = to_snake_case(s);
    Ident::new(&snake, span)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Trip"), "trip");
        assert_eq!(to_snake_case("trip"), "trip");
        assert_eq!(to_snake_case("EnterHalfOpen"), "enter_half_open");
        assert_eq!(to_snake_case("HTTPRequest"), "http_request");
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("IOError"), "io_error");
        assert_eq!(to_snake_case("parseXML"), "parse_xml");
        assert_eq!(to_snake_case("sendHTTPRequest"), "send_http_request");
        assert_eq!(to_snake_case("A"), "a");
        assert_eq!(to_snake_case("AB"), "ab");
        assert_eq!(to_snake_case("ABC"), "abc");
        assert_eq!(to_snake_case("ABCDef"), "abc_def");
        assert_eq!(to_snake_case("snake_case"), "snake_case");
        assert_eq!(to_snake_case("SCREAMING_SNAKE"), "screaming_snake");
    }

    #[test]
    fn test_to_snake_case_ident() {
        let pascal = Ident::new("EnterHalfOpen", Span::call_site());
        let snake = to_snake_case_ident(&pascal);
        assert_eq!(snake.to_string(), "enter_half_open");
    }
}
