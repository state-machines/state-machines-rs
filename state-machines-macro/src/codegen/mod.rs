//! Code generation for the state machine macro.
//!
//! Now uses typestate pattern exclusively for compile-time type safety.

pub mod typestate;

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use syn::Result;

impl StateMachine {
    /// Generate the complete state machine code.
    ///
    /// Now uses typestate pattern exclusively for compile-time type safety.
    /// Each state becomes a distinct type, and transitions consume the machine.
    ///
    /// This is the Rust way - impossible states become unrepresentable!
    pub fn expand(&self) -> Result<TokenStream2> {
        self.validate()?;

        // Generate typestate-based machine
        typestate::generate_typestate_machine(self)
    }
}
