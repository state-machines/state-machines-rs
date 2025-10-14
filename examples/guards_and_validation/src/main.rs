//! # Guards and Validation: Conditional Transitions
//!
//! This example teaches **guards** - functions that determine whether a transition
//! should be allowed - using a cargo bay door system.
//!
//! ## The Problem: Safety-Critical Operations
//!
//! Opening cargo bay doors in a spacecraft is dangerous:
//! - ❌ Can't open while in atmosphere (explosive decompression)
//! - ❌ Can't open while engines are firing (debris ejection)
//! - ✓ Safe only in stable orbit
//! - ⚠️ Emergency override exists (with safety check)
//!
//! Guards let you encode these safety rules as **code that runs before transitions**.
//!
//! ## Ruby Comparison
//!
//! Ruby's state_machines uses similar guard syntax:
//!
//! ```ruby
//! state_machine :door do
//!   event :open do
//!     transition closed: :open, if: :in_orbit?
//!   end
//! end
//! ```
//!
//! Rust's version is similar but **type-safe**:
//!
//! ```rust,ignore
//! events {
//!     open {
//!         guards: [in_orbit],  // Must return bool
//!         transition: { from: Closed, to: Open }
//!     }
//! }
//! ```
//!
//! ## Key Concepts Demonstrated
//!
//! 1. **Guards** - Boolean checks that run before transitions
//! 2. **Error Recovery** - Getting the machine back when guards fail
//! 3. **Context Usage** - Passing data through the machine
//! 4. **Multiple Guards** - All must pass for transition to succeed
//!
//! ## Caveats & Flow
//!
//! ### Caveat 1: Guards Return bool, Not Result
//!
//! ```rust,ignore
//! fn in_orbit(&self, ctx: &SpaceshipContext) -> bool {
//!     ctx.altitude > 100_000  // Just return true/false
//! }
//! ```
//!
//! **Why:** Guards are *checks*, not operations. If your guard needs to do I/O
//! or can fail, use a `before` callback instead (which can return Result).
//!
//! ### Caveat 2: Failed Guards Return the Old Machine
//!
//! ```rust,ignore
//! let result = bay.open();
//! match result {
//!     Ok(bay) => { /* Got new state */ }
//!     Err((bay, err)) => {
//!         // bay is back in Closed state - you can retry or handle error
//!     }
//! }
//! ```
//!
//! This is **critical** - you don't lose the machine on guard failure!
//!
//! ### Caveat 3: Multiple Guards Use AND Logic
//!
//! ```rust,ignore
//! guards: [in_orbit, engines_off, not_atmospheric]
//! // ALL three must return true
//! ```
//!
//! If you need OR logic, write a single guard that does the check.
//!
//! ### Caveat 4: Context Flexibility - Generic vs Concrete
//!
//! **Option 1: Generic Context (C type parameter)**
//! ```rust,ignore
//! state_machine! {
//!     name: Machine,
//!     // No context specified = generic over C
//! }
//!
//! impl<C, S> Machine<C, S> {
//!     fn guard(&self, _ctx: &C) -> bool {
//!         // C is generic - can't access its fields
//!     }
//! }
//! ```
//!
//! **Option 2: Concrete Context (specified type)**
//! ```rust,ignore
//! state_machine! {
//!     name: Machine,
//!     context: SpacecraftSensors,  // ← Concrete type
//! }
//!
//! impl<S> Machine<S> {
//!     fn guard(&self, ctx: &SpacecraftSensors) -> bool {
//!         ctx.altitude_km >= 100  // ✓ Direct field access!
//!     }
//! }
//! ```
//!
//! **This example demonstrates Option 2** - concrete context for embedded/hardware systems.
//!
//! ## The Flow: Cargo Bay Door Safety System
//!
//! ```text
//! Closed ──open──> Open ──close──> Closed
//!   │               │
//!   │ (guards)      │
//!   │ - in_orbit?   │
//!   │ - engines_off? │
//!   │               │
//!   └─emergency_open─┘ (override with confirmation)
//! ```
//!
//! **Realistic Scenario:**
//! - Normal operations: Must be in orbit with engines off
//! - Emergency: Can override safety checks with confirmation flag
//! - Failed guards: Machine stays in current state, returns error details

use state_machines::{core::TransitionErrorKind, state_machine};

/// Spacecraft telemetry data (concrete context type)
///
/// In real spacecraft systems, these would be sensor readings from hardware.
/// Using a concrete context type allows guards to directly access fields.
#[derive(Debug, Clone, Default)]
pub struct SpacecraftTelemetry {
    pub altitude_km: u32,
    pub engines_active: bool,
    pub emergency_override: bool,
}

impl SpacecraftTelemetry {
    fn new() -> Self {
        Self::default()
    }

    fn with_altitude(mut self, km: u32) -> Self {
        self.altitude_km = km;
        self
    }

    fn with_engines(mut self, active: bool) -> Self {
        self.engines_active = active;
        self
    }

    fn with_override(mut self, enabled: bool) -> Self {
        self.emergency_override = enabled;
        self
    }
}

// Define cargo bay door state machine with safety guards
state_machine! {
    name: CargoBay,
    context: SpacecraftTelemetry,  // ← Concrete context type for direct field access

    initial: Closed,
    states: [Closed, Open],

    events {
        // Normal opening requires two guards
        open {
            // ALL guards must return true for transition to succeed
            guards: [in_orbit, engines_off],
            transition: { from: Closed, to: Open }
        }

        // Emergency override checks different guard
        emergency_open {
            guards: [has_override_authorization],
            transition: { from: Closed, to: Open }
        }

        // Closing has no guards (always safe)
        close {
            transition: { from: Open, to: Closed }
        }
    }
}

// Implement guard logic
//
// Guards receive:
// - &self: Reference to the machine (can access state)
// - &SpacecraftTelemetry: Reference to concrete context (can access fields!)
//
// They must return bool:
// - true: Transition is allowed
// - false: Transition is blocked (machine stays in current state)
//
// IMPORTANT: With concrete context, guards can directly access telemetry fields.
// This is ideal for embedded systems where hardware types are known at compile time.
impl<S> CargoBay<S> {
    /// Guard: Check if spacecraft is in stable orbit
    ///
    /// Below 100km is considered atmosphere (dangerous decompression)
    fn in_orbit(&self, ctx: &SpacecraftTelemetry) -> bool {
        println!("  [Guard] Checking orbit altitude: {} km", ctx.altitude_km);
        ctx.altitude_km >= 100
    }

    /// Guard: Check if engines are off
    ///
    /// Firing engines with cargo bay open risks debris damage
    fn engines_off(&self, ctx: &SpacecraftTelemetry) -> bool {
        println!(
            "  [Guard] Checking engine status: {}",
            if ctx.engines_active { "active" } else { "off" }
        );
        !ctx.engines_active
    }

    /// Guard: Check emergency override authorization
    ///
    /// Emergency situations may require opening bay despite safety concerns
    fn has_override_authorization(&self, ctx: &SpacecraftTelemetry) -> bool {
        println!(
            "  [Guard] Checking override authorization: {}",
            ctx.emergency_override
        );
        ctx.emergency_override
    }
}

fn main() {
    println!("=== Cargo Bay Safety System Demo ===\n");

    // Scenario 1: Unsafe conditions (on ground, engines off)
    println!("--- Scenario 1: Attempting to open on ground ---");
    let telemetry = SpacecraftTelemetry::new()
        .with_altitude(0) // On ground!
        .with_engines(false)
        .with_override(false);

    let bay = CargoBay::new(telemetry);
    let result = bay.open();

    match result {
        Ok(_) => println!("✗ ERROR: Should not have opened!"),
        Err((_bay, err)) => {
            println!("✓ Guards prevented opening:");
            println!("  Failed guard: {}", err.guard);

            // Check the error kind
            if let TransitionErrorKind::GuardFailed { guard } = err.kind {
                println!("  Error type: Guard '{}' returned false", guard);
            }

            // IMPORTANT: We still have the machine in Closed state!
            // We can retry after fixing the condition
            println!("  Machine returned safely in Closed state\n");

            // Scenario 2: Fix altitude, try again
            println!("--- Scenario 2: Ascending to orbit ---");
            let telemetry = SpacecraftTelemetry::new()
                .with_altitude(150) // Now in orbit
                .with_engines(false)
                .with_override(false);

            let bay = CargoBay::new(telemetry);
            match bay.open() {
                Ok(bay) => {
                    println!("✓ Both guards passed! Bay is now Open");

                    // Close it for next test
                    let _bay = bay.close().unwrap();
                    println!("✓ Closed safely\n");
                }
                Err((_, err)) => {
                    println!("✗ Unexpected guard failure: {}", err.guard);
                }
            }
        }
    }

    // Scenario 3: Engines firing (unsafe even in orbit)
    println!("--- Scenario 3: Engines active (in orbit) ---");
    let telemetry = SpacecraftTelemetry::new()
        .with_altitude(200)
        .with_engines(true) // Unsafe!
        .with_override(false);

    let bay = CargoBay::new(telemetry);
    match bay.open() {
        Ok(_) => println!("✗ ERROR: Should not have opened with engines firing!"),
        Err((_bay, err)) => {
            println!("✓ Guard '{}' prevented opening", err.guard);
            println!("  Even though we're in orbit, engines must be off\n");

            // Scenario 4: Emergency override
            println!("--- Scenario 4: Emergency override ---");
            let telemetry = SpacecraftTelemetry::new()
                .with_altitude(200)
                .with_engines(true)
                .with_override(true);

            let bay = CargoBay::new(telemetry);
            match bay.emergency_open() {
                Ok(bay) => {
                    println!("⚠️  Emergency override successful!");
                    println!("  Bay opened despite safety concerns");

                    let _bay = bay.close().unwrap();
                    println!("✓ Closed after emergency operation\n");
                }
                Err((_, err)) => {
                    println!("✗ Override failed: {}", err.guard);
                }
            }
        }
    }

    // Scenario 5: All conditions nominal
    println!("--- Scenario 5: Nominal operations ---");
    let telemetry = SpacecraftTelemetry::new()
        .with_altitude(250)
        .with_engines(false)
        .with_override(false);

    let bay = CargoBay::new(telemetry);
    match bay.open() {
        Ok(bay) => {
            println!("✓ All guards passed!");
            println!("  in_orbit: ✓");
            println!("  engines_off: ✓");
            println!("✓ Bay is Open - safe for cargo operations");

            let _bay = bay.close().unwrap();
            println!("✓ Operations complete, bay closed\n");
        }
        Err((_, err)) => {
            println!("✗ Unexpected failure: {}", err.guard);
        }
    }

    println!("=== Key Takeaways ===");
    println!("✓ Guards prevent unsafe transitions at runtime");
    println!("✓ Failed guards return (machine, error) - no data loss");
    println!("✓ Multiple guards all must pass (AND logic)");
    println!("✓ Context provides read-only access to system state");
    println!("✓ Guards are pure checks - no side effects");

    println!("\n=== Common Patterns ===");
    println!("• Guards for safety-critical checks (can't open bay in atmosphere)");
    println!("• Guards for precondition validation (must be logged in)");
    println!("• Guards for business rules (can't ship without payment)");
    println!("• Emergency overrides with separate events/guards");

    println!("\n=== Error Handling Pattern ===");
    println!("match machine.event() {{");
    println!("    Ok(new_machine) => {{ /* Transition succeeded */ }}");
    println!("    Err((old_machine, error)) => {{");
    println!("        // old_machine is still valid - can retry");
    println!("        // error.guard tells you which check failed");
    println!("    }}");
    println!("}}");
}
