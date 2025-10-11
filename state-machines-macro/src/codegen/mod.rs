//! Code generation for the state machine macro.
//!
//! Now uses typestate pattern exclusively for compile-time type safety.
//! The code is split across smaller modules:
//! - `callbacks`: filtering logic for global callbacks (legacy, may be removed)
//! - `event`: event method generation (legacy, may be removed)
//! - `storage`: helpers for state data storage handling (legacy, may be removed)
//! - `typestate`: typestate pattern code generation (ACTIVE)

mod callbacks;
mod event;
mod storage;
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
