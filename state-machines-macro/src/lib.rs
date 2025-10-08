//! Procedural macros for the state machines crate.
//!
//! This crate provides the `state_machine!` macro for defining
//! type-safe state machines with transitions, guards, and callbacks.
//!
//! # Architecture
//!
//! The macro implementation is split into several modules:
//!
//! - **types**: Data structures representing state machines
//! - **parser**: Parsing macro input into our data structures
//! - **codegen**: Generating Rust code from the parsed structures
//! - **validation**: Validating state machine definitions
//!
//! # Example
//!
//! ```ignore
//! use state_machines::state_machine;
//!
//! state_machine! {
//!     name: Door,
//!     state: DoorState,
//!     initial: Closed,
//!     states: [Open, Closed],
//!     events: {
//!         open {
//!             transition: { from: Closed, to: Open }
//!         }
//!         close {
//!             transition: { from: Open, to: Closed }
//!         }
//!     }
//! }
//! ```

use proc_macro::TokenStream;

// Module declarations
mod codegen;
mod parser;
mod types;
mod validation;

/// Define a state machine with compile-time guarantees.
///
/// This macro generates:
/// - A state enum with all defined states
/// - A state machine struct with transition methods
/// - Event methods for triggering transitions (e.g., `open()`)
/// - Can methods for checking if transitions are allowed (e.g., `can_open()`)
/// - Default implementations
/// - Machine trait implementation for introspection
///
/// # Syntax
///
/// ```ignore
/// state_machine! {
///     name: MachineName,           // Required: name of the generated struct
///     state: StateName,             // Required: name of the state enum
///     initial: InitialState,        // Required: initial state
///     async: true,                  // Optional: enable async support
///     action: action_method,        // Optional: method called on every transition
///
///     states: [                     // Required: list of states
///         StateA,
///         StateB(DataType),         // States can have associated data
///         superstate Parent {       // Superstates for hierarchical machines
///             state Child1,
///             state Child2,
///             initial: Child1,      // Superstate's initial child
///         }
///     ],
///
///     events: {                     // Optional: event definitions
///         event_name {
///             payload: PayloadType, // Optional: event payload type
///             guards: [guard1],     // Optional: event-level guards
///             unless: [guard2],     // Optional: inverted guards
///             before: [callback1],  // Optional: before callbacks
///             after: [callback2],   // Optional: after callbacks
///
///             transition: {
///                 from: SourceState,
///                 to: TargetState,
///                 guards: [guard3], // Optional: transition-level guards
///                 unless: [guard4], // Optional: transition-level unless
///                 before: [cb3],    // Optional: transition-level before
///                 after: [cb4],     // Optional: transition-level after
///             }
///         }
///     },
///
///     callbacks: {                  // Optional: global callbacks
///         before_transition [
///             { name: log_transition, from: [StateA], to: [StateB], on: [event] }
///         ],
///         after_transition [
///             { name: after_cb }
///         ],
///         around_transition [
///             { name: wrap_cb }
///         ]
///     }
/// }
/// ```
///
/// # Generated Code
///
/// The macro generates:
///
/// 1. A state enum with all variants
/// 2. A machine struct with:
///    - `new()` constructor
///    - `state()` accessor
///    - Event methods (e.g., `activate()`)
///    - Can methods (e.g., `can_activate()`)
///    - Storage accessors for state-associated data
/// 3. A `DEFINITION` constant for runtime introspection
/// 4. Trait implementations (Machine, Default, Debug)
///
/// # Features
///
/// - **Type-safe transitions**: Invalid transitions are compile errors
/// - **Guards**: Conditional transitions with guard methods
/// - **Callbacks**: Execute code before/after/around transitions
/// - **State data**: Associate data with specific states
/// - **Hierarchical states**: Superstates containing child states
/// - **Async support**: Async guards, callbacks, and transitions
/// - **Introspection**: Runtime metadata about the machine structure
///
/// # Learning Resource
///
/// This codebase is intentionally over-commented to serve as a learning
/// resource for procedural macro development. Each module contains
/// extensive documentation explaining the what, why, and how.
#[proc_macro]
pub fn state_machine(input: TokenStream) -> TokenStream {
    // Parse the macro input into our StateMachine structure
    // The Parse trait implementation is in parser.rs
    let machine = syn::parse_macro_input!(input as types::StateMachine);

    // Expand the machine into generated code
    // This validates the definition and generates all the code
    // The expand() method is in codegen.rs
    match machine.expand() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
