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
//! ### Caveat 4: Guards Can't Access Generic Context Data
//!
//! ```rust,ignore
//! impl<C, S> Machine<C, S> {
//!     fn guard(&self, _ctx: &C) -> bool {
//!         // C is generic - can't access its fields
//!         // Use static/atomic data or callbacks for context-dependent checks
//!     }
//! }
//! ```
//!
//! **Why:** Guards must be generic over all context types. For context-dependent
//! validation, use static state or `before` callbacks instead.
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

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use state_machines::{core::TransitionErrorKind, state_machine};

/// Spacecraft telemetry data (simulated with atomic statics)
///
/// In real spacecraft systems, these would be sensor readings.
/// We use atomics to allow mutation in guards (which take &self).
static ALTITUDE_KM: AtomicU32 = AtomicU32::new(0);
static ENGINES_ACTIVE: AtomicBool = AtomicBool::new(false);
static EMERGENCY_OVERRIDE: AtomicBool = AtomicBool::new(false);

/// Helper struct to set spacecraft state
struct SpacecraftTelemetry;

impl SpacecraftTelemetry {
    fn set_altitude(km: u32) {
        ALTITUDE_KM.store(km, Ordering::Relaxed);
    }

    fn set_engines(active: bool) {
        ENGINES_ACTIVE.store(active, Ordering::Relaxed);
    }

    fn set_override(enabled: bool) {
        EMERGENCY_OVERRIDE.store(enabled, Ordering::Relaxed);
    }
}

// Define cargo bay door state machine with safety guards
state_machine! {
    name: CargoBay,

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
// - &C: Reference to context (generic, can't access fields)
//
// They must return bool:
// - true: Transition is allowed
// - false: Transition is blocked (machine stays in current state)
//
// IMPORTANT: Guards must be generic over C. To access spacecraft state,
// we use static atomics (like real embedded systems would use hardware registers).
impl<C, S> CargoBay<C, S> {
    /// Guard: Check if spacecraft is in stable orbit
    ///
    /// Below 100km is considered atmosphere (dangerous decompression)
    fn in_orbit(&self, _ctx: &C) -> bool {
        let altitude = ALTITUDE_KM.load(Ordering::Relaxed);
        println!("  [Guard] Checking orbit altitude: {} km", altitude);
        altitude >= 100
    }

    /// Guard: Check if engines are off
    ///
    /// Firing engines with cargo bay open risks debris damage
    fn engines_off(&self, _ctx: &C) -> bool {
        let active = ENGINES_ACTIVE.load(Ordering::Relaxed);
        println!(
            "  [Guard] Checking engine status: {}",
            if active { "active" } else { "off" }
        );
        !active
    }

    /// Guard: Check emergency override authorization
    ///
    /// Emergency situations may require opening bay despite safety concerns
    fn has_override_authorization(&self, _ctx: &C) -> bool {
        let override_enabled = EMERGENCY_OVERRIDE.load(Ordering::Relaxed);
        println!(
            "  [Guard] Checking override authorization: {}",
            override_enabled
        );
        override_enabled
    }
}

fn main() {
    println!("=== Cargo Bay Safety System Demo ===\n");

    // Scenario 1: Unsafe conditions (on ground, engines off)
    println!("--- Scenario 1: Attempting to open on ground ---");
    SpacecraftTelemetry::set_altitude(0); // On ground!
    SpacecraftTelemetry::set_engines(false);
    SpacecraftTelemetry::set_override(false);

    let bay = CargoBay::new(());
    let result = bay.open();

    match result {
        Ok(_) => println!("✗ ERROR: Should not have opened!"),
        Err((_bay, err)) => {
            println!("✓ Guards prevented opening:");
            println!("  Failed guard: {}", err.guard);

            // Check the error kind
            match err.kind {
                TransitionErrorKind::GuardFailed { guard } => {
                    println!("  Error type: Guard '{}' returned false", guard);
                }
                _ => {}
            }

            // IMPORTANT: We still have the machine in Closed state!
            // We can retry after fixing the condition
            println!("  Machine returned safely in Closed state\n");

            // Scenario 2: Fix altitude, try again
            println!("--- Scenario 2: Ascending to orbit ---");
            SpacecraftTelemetry::set_altitude(150); // Now in orbit

            let bay = CargoBay::new(());
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
    SpacecraftTelemetry::set_altitude(200);
    SpacecraftTelemetry::set_engines(true); // Unsafe!

    let bay = CargoBay::new(());
    match bay.open() {
        Ok(_) => println!("✗ ERROR: Should not have opened with engines firing!"),
        Err((_bay, err)) => {
            println!("✓ Guard '{}' prevented opening", err.guard);
            println!("  Even though we're in orbit, engines must be off\n");

            // Scenario 4: Emergency override
            println!("--- Scenario 4: Emergency override ---");
            SpacecraftTelemetry::set_override(true);

            let bay = CargoBay::new(());
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
    SpacecraftTelemetry::set_altitude(250);
    SpacecraftTelemetry::set_engines(false);
    SpacecraftTelemetry::set_override(false);

    let bay = CargoBay::new(());
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
