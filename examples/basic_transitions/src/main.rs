//! # Basic Transitions: Understanding the Typestate Pattern
//!
//! This example teaches the **fundamental mental model** of typestate state machines
//! through a simple airlock pressure cycling system.
//!
//! ## The Ruby Way (Mutation)
//!
//! In Ruby's state_machines, you mutate an object in place:
//!
//! ```ruby
//! airlock = Airlock.new          # state: :pressurized
//! airlock.depressurize           # state: :vacuum (mutated!)
//! airlock.depressurize           # ERROR: invalid transition
//! ```
//!
//! **Problem:** Nothing stops you from calling `depressurize` twice.
//! You only discover the error at runtime.
//!
//! ## The Rust Way (Typestate)
//!
//! Each state is a **different type**. Transitions **consume** the old machine
//! and **return** a new one:
//!
//! ```rust,ignore
//! let airlock = Airlock::new(());       // Type: Airlock<Pressurized>
//! let airlock = airlock.depressurize(); // Type: Airlock<Vacuum>
//! // airlock.depressurize();            // ❌ Compiler error: Vacuum doesn't have depressurize()!
//! ```
//!
//! **The type system prevents invalid transitions at compile time.**
//!
//! ## Key Concepts Demonstrated
//!
//! 1. **Type Safety** - Invalid transitions become compiler errors
//! 2. **Move Semantics** - Old state is consumed, can't be reused
//! 3. **Zero Cost** - Types exist only at compile time (PhantomData)
//! 4. **Explicit Errors** - Result type shows success/failure paths
//!
//! ## Caveats & Flow
//!
//! ### Caveat 1: You Can't Reuse Old States
//!
//! ```rust,ignore
//! let airlock = Airlock::new(());
//! let airlock2 = airlock.depressurize(); // airlock is MOVED here
//! // airlock.repressurize();             // ❌ Error: value moved
//! ```
//!
//! **Why:** State machines consume themselves on transition. This is **intentional** -
//! it prevents you from having multiple versions of the "same" machine in
//! different states.
//!
//! ### Caveat 2: Result Handling is Mandatory
//!
//! ```rust,ignore
//! let result = airlock.depressurize(); // Type: Result<Airlock<Vacuum>, ...>
//! // You MUST handle both Ok and Err cases
//! ```
//!
//! Even if a transition has no guards, it returns Result because the **pattern**
//! requires it. Use `.unwrap()` when you know it can't fail.
//!
//! ### Caveat 3: Generic Parameters Everywhere
//!
//! ```rust,ignore
//! impl<C, S> Airlock<C, S> {
//!     fn common_method(&self) { }  // Works on ANY state
//! }
//!
//! impl<C> Airlock<C, Pressurized> {
//!     fn pressure_specific(&self) { } // Only works on Pressurized
//! }
//! ```
//!
//! `C` is context (usually unit type `()`), `S` is the state marker.
//!
//! ## The Flow: Airlock Pressure Cycle
//!
//! ```text
//! Pressurized ──depressurize──> Vacuum ──repressurize──> Pressurized
//!      │                                                       ▲
//!      └───────────────── (cycle completes) ──────────────────┘
//! ```
//!
//! **Realistic Scenario:**
//! Spacecraft airlock must cycle pressure before EVA (extravehicular activity).
//! - Start: Pressurized (crew can enter)
//! - Depressurize: Pump air out (takes time in real life)
//! - Vacuum: Match space pressure, open outer door
//! - Repressurize: Pump air back in after EVA
//! - End: Pressurized again (crew can exit to ship)
//!
//! **Safety Requirement:** Can't skip steps. Can't depressurize twice.
//! Type system enforces this automatically.

use state_machines::state_machine;

// Define the airlock state machine
//
// This macro generates:
// 1. Zero-sized state marker types: Pressurized, Vacuum
// 2. The main struct: Airlock<C, S>
// 3. Event methods: depressurize(), repressurize()
// 4. Transition logic with type transformations
state_machine! {
    name: Airlock,

    // Initial state when created with Airlock::new()
    initial: Pressurized,

    // All possible states (order doesn't matter)
    states: [Pressurized, Vacuum],

    // Events define what actions are available
    events {
        depressurize {
            // Only valid from Pressurized → Vacuum
            transition: { from: Pressurized, to: Vacuum }
        }
        repressurize {
            // Only valid from Vacuum → Pressurized
            transition: { from: Vacuum, to: Pressurized }
        }
    }
}

fn main() {
    println!("=== Airlock Pressure Cycle Demo ===\n");

    // Create new airlock in Pressurized state
    // Type: Airlock<(), Pressurized>
    let airlock = Airlock::new(());
    println!("1. Airlock created: Pressurized (safe for crew entry)");

    // Depressurize for EVA
    // Type: Result<Airlock<(), Vacuum>, (Airlock<(), Pressurized>, TransitionError)>
    let airlock = airlock.depressurize().unwrap();
    println!("2. Depressurized: Vacuum (outer door can open)");

    // IMPORTANT: Try uncommenting this line:
    // let airlock = airlock.depressurize().unwrap();
    //
    // You'll get a compiler error:
    //   error[E0599]: no method named `depressurize` found for struct `Airlock<(), Vacuum>`
    //
    // This is the power of typestate: invalid transitions are caught at compile time!

    // Complete the cycle by repressurizing
    // Type: Result<Airlock<(), Pressurized>, ...>
    let airlock = airlock.repressurize().unwrap();
    println!("3. Repressurized: Back to safe pressure");

    // Now we can depressurize again (full cycle)
    let _airlock = airlock.depressurize().unwrap();
    println!("4. Cycled again: Ready for next EVA\n");

    println!("=== Key Takeaways ===");
    println!("✓ Each state is a distinct type (Airlock<Pressurized> vs Airlock<Vacuum>)");
    println!("✓ Invalid transitions are compile errors, not runtime errors");
    println!("✓ Move semantics prevent accidental state reuse");
    println!("✓ Zero runtime overhead (PhantomData optimizes away)");
    println!("\n=== Common Mistakes to Avoid ===");
    println!("✗ Trying to reuse old machine after transition (moved!)");
    println!("✗ Calling wrong event for current state (won't compile!)");
    println!("✗ Ignoring Result from transitions (use .unwrap() or match)");

    // Compare to Ruby's approach:
    println!("\n=== Ruby Comparison ===");
    println!("Ruby: airlock.depressurize  # Mutates in place, runtime check");
    println!(
        "Rust: let airlock = airlock.depressurize().unwrap()  # Type change, compile-time check"
    );
}
