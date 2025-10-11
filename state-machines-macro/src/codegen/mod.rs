//! Code generation for the state machine macro.
//!
//! Supports both typestate pattern (compile-time safety) and dynamic dispatch
//! (runtime flexibility). Dynamic mode is opt-in via feature flag or explicit config.

pub mod dynamic;
pub mod typestate;
pub mod utils;

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::Result;

impl StateMachine {
    /// Generate the complete state machine code.
    ///
    /// Generates typestate pattern for compile-time type safety.
    /// Optionally also generates dynamic dispatch wrapper when:
    /// - The `dynamic` feature flag is enabled, OR
    /// - The macro explicitly specifies `dynamic: true`
    pub fn expand(&self) -> Result<TokenStream2> {
        self.validate()?;

        // Always generate typestate-based machine
        let typestate_code = typestate::generate_typestate_machine(self)?;

        // Conditionally generate dynamic dispatch wrapper
        let should_generate_dynamic = self.dynamic_mode || cfg!(feature = "dynamic");

        if should_generate_dynamic {
            let dynamic_code = dynamic::generate_dynamic_wrapper(self)?;
            Ok(quote! {
                #typestate_code
                #dynamic_code
            })
        } else {
            Ok(typestate_code)
        }
    }
}
