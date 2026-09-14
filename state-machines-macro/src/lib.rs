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
/// - A zero-sized marker type per state (typestate pattern)
/// - A machine struct parameterized by state, with transition methods
///   that consume the machine and return it typed to the target state
/// - Event methods for triggering transitions (e.g., `open()`)
/// - Can methods for checking whether guards would allow a transition
///   (e.g., `can_open()`)
/// - Storage accessors for state- and superstate-associated data
/// - With the `inspect` feature: a `schema()` method and an `Inspectable`
///   impl exposing the machine's structure for JSON/Mermaid rendering
/// - With `dynamic: true` (or the `dynamic` feature): a runtime-dispatch
///   wrapper with an event enum, `handle()`, and availability queries
///
/// # Syntax
///
/// ```ignore
/// state_machine! {
///     name: MachineName,            // Required: name of the generated struct
///     initial: InitialState,        // Required: initial state (a leaf)
///     context: ContextType,         // Optional: concrete context type
///     error: ErrorType,             // Optional: fallible callback error type
///     async: true,                  // Optional: enable async support
///     dynamic: true,                // Optional: also generate dynamic wrapper
///
///     states: [                     // Required: list of states
///         StateA,
///         StateB(DataType),         // States can have associated data
///         superstate Parent(Data) { // Superstates for hierarchical machines
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
///             around: [wrapper],    // Optional: around callbacks
///
///             transition: {
///                 from: SourceState,    // Leaf, superstate, or [list]
///                 to: TargetState,      // Leaf or superstate (initial child)
///                 guards: [guard3], // Optional: transition-level guards
///                 unless: [guard4], // Optional: transition-level unless
///                 before: [cb3],    // Optional: transition-level before
///                 after: [cb4],     // Optional: transition-level after
///                 around: [cb5],    // Optional: transition-level around
///             }
///         }
///     },
///
///     callbacks: {                  // Optional: global filtered callbacks
///         before_transition [
///             { name: log_transition, from: [StateA], to: [StateB], on: [event] }
///         ],
///         after_transition [
///             { name: after_cb }    // No filters: every transition
///         ],
///         around_transition [
///             { name: wrap_cb }
///         ]
///     }
/// }
/// ```
///
/// Global callbacks run outermost: matching `before_transition` entries run
/// before event/transition `before` callbacks, and `after_transition`
/// entries run after the local `after` callbacks. Their filters accept
/// superstates (matching any descendant), and they are always invoked
/// without the event payload. Filtering on `to:` / `from:` provides state
/// enter/exit hooks.
///
/// # Features
///
/// - **Type-safe transitions**: Invalid transitions are compile errors
/// - **Guards**: Conditional transitions with guard methods
/// - **Callbacks**: Execute code before/after/around transitions, locally
///   or machine-wide with filters
/// - **State data**: Associate data with specific states or superstates
/// - **Hierarchical states**: Superstates containing child states
/// - **Async support**: Async guards, callbacks, and transitions
/// - **Introspection**: `schema()` metadata (with the `inspect` feature)
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
