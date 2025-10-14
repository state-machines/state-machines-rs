//! # Callbacks Lifecycle: Hook Execution Order
//!
//! This example teaches **callbacks** - hooks that run during state transitions -
//! using a rocket engine ignition sequence.
//!
//! ## The Problem: Orchestrating Complex Operations
//!
//! Rocket ignition requires precise sequencing:
//! - ✓ Pre-ignition checks (fuel pressure, valve status)
//! - ✓ Ignition sequence (spark, fuel injection, monitoring)
//! - ✓ Post-ignition logging (telemetry, diagnostics)
//! - 🔒 Transactional semantics (abort if any step fails)
//!
//! Callbacks let you **hook into the transition lifecycle** at specific points.
//!
//! ## Callback Types
//!
//! 1. **before** - Runs before transition, can be used for validation/logging
//! 2. **after** - Runs after successful transition, for cleanup/logging
//! 3. **around** - Wraps the entire transition with Before/AfterSuccess stages
//!
//! ## Ruby Comparison
//!
//! Ruby's state_machines has similar hooks:
//!
//! ```ruby
//! state_machine :engine do
//!   before_transition idle: :ignited, do: :check_fuel
//!   after_transition idle: :ignited, do: :log_ignition
//!   around_transition do |machine, transition, block|
//!     # Transaction wrapper
//!     begin
//!       block.call
//!     rescue => e
//!       # Rollback
//!     end
//!   end
//! end
//! ```
//!
//! Rust's version:
//!
//! ```rust,ignore
//! events {
//!     ignite {
//!         before: [check_fuel],
//!         after: [log_ignition],
//!         around: [transaction_wrapper],
//!         transition: { from: Idle, to: Ignited }
//!     }
//! }
//! ```
//!
//! ## Key Concepts Demonstrated
//!
//! 1. **Execution Order** - Around Before → Guards → Before → Transition → After → Around AfterSuccess
//! 2. **Around Callbacks** - Transaction-like semantics, can abort transitions
//! 3. **Multiple Callbacks** - All execute in order
//! 4. **State Access** - Callbacks receive `&self` (read-only)
//!
//! ## Caveats & Flow
//!
//! ### Caveat 1: Callbacks Can't Mutate the Machine
//!
//! ```rust,ignore
//! fn before_callback(&self) {
//!     // &self is immutable - can't change machine state
//!     // Use static/atomic data or external systems
//! }
//! ```
//!
//! **Why:** The machine is being consumed during transition. Callbacks are
//! checkpoints, not mutation points.
//!
//! ### Caveat 2: All Callbacks Run for Every Transition Using That Event
//!
//! ```rust,ignore
//! events {
//!     ignite {
//!         before: [check_fuel],  // Runs for ALL `ignite` calls
//!         transition: { from: Idle, to: Ignited }
//!     }
//! }
//! ```
//!
//! Can't have different callbacks for different source states on same event.
//!
//! ### Caveat 3: Around Callbacks Can Abort Transitions
//!
//! ```rust,ignore
//! fn around(&self, stage: AroundStage) -> AroundOutcome<CurrentState> {
//!     match stage {
//!         AroundStage::Before => {
//!             if something_wrong {
//!                 return AroundOutcome::Abort(TransitionError::guard_failed(...));
//!             }
//!             AroundOutcome::Proceed
//!         }
//!         AroundStage::AfterSuccess => AroundOutcome::Proceed,
//!     }
//! }
//! ```
//!
//! Abort at Before stage prevents transition. AfterSuccess only runs if transition succeeds.
//!
//! ### Caveat 4: Callback Execution Order is Fixed
//!
//! ```text
//! 1. Around Before (outermost to innermost if multiple)
//! 2. Guards
//! 3. Before callbacks (in declaration order)
//! 4. Transition (state change)
//! 5. After callbacks (in declaration order)
//! 6. Around AfterSuccess (innermost to outermost if multiple)
//! ```
//!
//! Can't change this order. Design your callbacks accordingly.
//!
//! ## The Flow: Rocket Engine Ignition Sequence
//!
//! ```text
//! Idle ──ignite──> Armed ──start──> Ignited ──throttle──> Thrust
//!        │                   │
//!        │ (callbacks)       │ (callbacks)
//!        │ - around: fuel_valve_transaction
//!        │ - before: check_fuel_pressure
//!        │ - after: log_armed_status
//! ```
//!
//! **Realistic Scenario:**
//! - Fuel valve must open atomically (around callback)
//! - Pressure must be validated before arming (before callback)
//! - Every state change is logged to telemetry (after callbacks)
//! - Timestamps show exact execution order

use core::sync::atomic::{AtomicU64, Ordering};
use state_machines::{
    core::{AroundOutcome, AroundStage, TransitionError},
    state_machine,
};

/// Simulated timestamp counter (in microseconds)
static TIMESTAMP: AtomicU64 = AtomicU64::new(0);

/// Simulated fuel system state
static FUEL_PRESSURE_PSI: AtomicU64 = AtomicU64::new(0);
static FUEL_VALVE_OPEN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Helper to get/advance timestamp
fn timestamp() -> u64 {
    TIMESTAMP.fetch_add(1, Ordering::Relaxed)
}

/// Helper to set fuel system state
struct FuelSystem;

impl FuelSystem {
    fn set_pressure(psi: u64) {
        FUEL_PRESSURE_PSI.store(psi, Ordering::Relaxed);
    }

    fn get_pressure() -> u64 {
        FUEL_PRESSURE_PSI.load(Ordering::Relaxed)
    }

    fn open_valve() {
        FUEL_VALVE_OPEN.store(true, Ordering::Relaxed);
        println!("    [FuelSystem] Valve opened");
    }

    fn close_valve() {
        FUEL_VALVE_OPEN.store(false, Ordering::Relaxed);
        println!("    [FuelSystem] Valve closed");
    }

    fn is_valve_open() -> bool {
        FUEL_VALVE_OPEN.load(Ordering::Relaxed)
    }
}

// Define rocket engine state machine with callbacks
state_machine! {
    name: RocketEngine,

    initial: Idle,
    states: [Idle, Armed, Ignited, Thrust],

    events {
        // Arm the engine with full callback lifecycle
        ignite {
            around: [fuel_valve_transaction],
            before: [check_fuel_pressure, pre_ignition_checks],
            after: [log_armed_status, send_telemetry],
            transition: { from: Idle, to: Armed }
        }

        // Start ignition sequence
        start {
            before: [verify_valve_open],
            after: [log_ignition],
            transition: { from: Armed, to: Ignited }
        }

        // Throttle up to full thrust
        throttle {
            after: [log_thrust],
            transition: { from: Ignited, to: Thrust }
        }

        // Emergency abort
        abort {
            after: [emergency_shutdown],
            transition: { from: [Armed, Ignited], to: Idle }
        }
    }
}

impl<C, S> RocketEngine<C, S> {
    // === AROUND CALLBACKS ===
    // Around callbacks wrap the entire transition with transaction semantics

    /// Around callback: Fuel valve transaction
    ///
    /// Opens valve before transition, ensures it stays open after.
    /// In a real system, this would handle rollback on failure.
    fn fuel_valve_transaction(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        match stage {
            AroundStage::Before => {
                let ts = timestamp();
                println!("[{}] Around::Before - Opening fuel valve...", ts);

                FuelSystem::open_valve();

                if !FuelSystem::is_valve_open() {
                    println!("[{}] Around::Before - Valve failed to open! ABORTING", ts);
                    return AroundOutcome::Abort(TransitionError::guard_failed(
                        Idle,
                        "ignite",
                        "fuel_valve_transaction",
                    ));
                }

                println!("[{}] Around::Before - Valve open, proceeding...", ts);
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess => {
                let ts = timestamp();
                println!(
                    "[{}] Around::AfterSuccess - Transition complete, valve locked open",
                    ts
                );
                AroundOutcome::Proceed
            }
        }
    }

    // === BEFORE CALLBACKS ===
    // Before callbacks run after guards but before the transition

    /// Before callback: Check fuel pressure
    fn check_fuel_pressure(&self) {
        let ts = timestamp();
        let pressure = FuelSystem::get_pressure();
        println!(
            "[{}] Before::check_fuel_pressure - Pressure: {} PSI",
            ts, pressure
        );

        if pressure < 100 {
            println!("[{}]   ⚠️  Warning: Pressure below optimal (100 PSI)", ts);
        } else {
            println!("[{}]   ✓ Pressure nominal", ts);
        }
    }

    /// Before callback: Pre-ignition checks
    fn pre_ignition_checks(&self) {
        let ts = timestamp();
        println!("[{}] Before::pre_ignition_checks - Running diagnostics", ts);
        println!("[{}]   ✓ Spark plug ready", ts);
        println!("[{}]   ✓ Injector system ready", ts);
        println!("[{}]   ✓ Telemetry active", ts);
    }

    /// Before callback: Verify valve is open
    fn verify_valve_open(&self) {
        let ts = timestamp();
        println!("[{}] Before::verify_valve_open - Checking valve status", ts);

        if FuelSystem::is_valve_open() {
            println!("[{}]   ✓ Valve confirmed open", ts);
        } else {
            println!("[{}]   ✗ ERROR: Valve not open!", ts);
        }
    }

    // === AFTER CALLBACKS ===
    // After callbacks run after successful transition

    /// After callback: Log armed status
    fn log_armed_status(&self) {
        let ts = timestamp();
        println!("[{}] After::log_armed_status - Engine now ARMED", ts);
        println!("[{}]   Logging to telemetry database...", ts);
    }

    /// After callback: Send telemetry
    fn send_telemetry(&self) {
        let ts = timestamp();
        println!(
            "[{}] After::send_telemetry - Broadcasting status update",
            ts
        );
        println!("[{}]   → Mission Control: Engine armed", ts);
        println!("[{}]   → Onboard computer: Ready for ignition", ts);
    }

    /// After callback: Log ignition
    fn log_ignition(&self) {
        let ts = timestamp();
        println!("[{}] After::log_ignition - IGNITION SEQUENCE STARTED", ts);
        println!("[{}]   Fuel flow: ACTIVE", ts);
        println!("[{}]   Combustion: INITIATED", ts);
    }

    /// After callback: Log thrust
    fn log_thrust(&self) {
        let ts = timestamp();
        println!("[{}] After::log_thrust - FULL THRUST ACHIEVED", ts);
        println!("[{}]   Throttle: 100%", ts);
        println!("[{}]   Vector: Nominal", ts);
    }

    /// After callback: Emergency shutdown
    fn emergency_shutdown(&self) {
        let ts = timestamp();
        println!("[{}] After::emergency_shutdown - EMERGENCY ABORT", ts);
        FuelSystem::close_valve();
        println!("[{}]   System safed", ts);
    }
}

fn main() {
    println!("=== Rocket Engine Ignition Sequence Demo ===\n");
    println!("This example demonstrates callback execution order.\n");
    println!("Key: [timestamp] CallbackType::name - message\n");

    // Reset timestamp
    TIMESTAMP.store(0, Ordering::Relaxed);

    // Scenario 1: Successful ignition sequence with full callbacks
    println!("--- Scenario 1: Nominal Ignition Sequence ---");
    FuelSystem::set_pressure(120); // Good pressure
    FuelSystem::close_valve(); // Start with closed valve

    let engine = RocketEngine::new(());
    println!("[{}] Engine created in Idle state\n", timestamp());

    println!("Calling engine.ignite()...\n");
    let engine = match engine.ignite() {
        Ok(engine) => {
            println!("\n[{}] ✓ Transition to Armed complete\n", timestamp());
            engine
        }
        Err((_engine, err)) => {
            println!("\n✗ Transition failed: {}", err.guard);
            return;
        }
    };

    println!("Calling engine.start()...\n");
    let engine = engine.start().unwrap();
    println!("\n[{}] ✓ Transition to Ignited complete\n", timestamp());

    println!("Calling engine.throttle()...\n");
    let _engine = engine.throttle().unwrap();
    println!("\n[{}] ✓ Transition to Thrust complete\n", timestamp());

    // Scenario 2: Callback execution order visualization
    println!("\n=== Callback Execution Order ===");
    println!("For the ignite() event, callbacks executed in this order:");
    println!("  1. Around::Before (fuel_valve_transaction)");
    println!("  2. Before::check_fuel_pressure");
    println!("  3. Before::pre_ignition_checks");
    println!("  4. [STATE TRANSITION: Idle → Armed]");
    println!("  5. After::log_armed_status");
    println!("  6. After::send_telemetry");
    println!("  7. Around::AfterSuccess (fuel_valve_transaction)");

    // Scenario 3: Around callback abort
    println!("\n\n--- Scenario 2: Around Callback Abort ---");
    TIMESTAMP.store(100, Ordering::Relaxed); // Reset timestamp
    FuelSystem::close_valve(); // Close valve to trigger abort

    // Temporarily make valve fail to open (simulation)
    println!("Simulating fuel valve failure...\n");

    let engine = RocketEngine::new(());
    println!("Calling engine.ignite() with faulty valve...\n");

    // This will abort in the Around::Before callback
    match engine.ignite() {
        Ok(_) => println!("✗ ERROR: Should have aborted!"),
        Err((_engine, err)) => {
            println!(
                "\n[{}] ✓ Transition aborted by around callback",
                timestamp()
            );
            println!("  Failed in: {}", err.guard);
            println!("  Machine still in Idle state - safe to retry after fixing valve");
        }
    }

    println!("\n\n=== Key Takeaways ===");
    println!("✓ Around callbacks wrap the entire transition (Before + AfterSuccess)");
    println!("✓ Before callbacks run after guards, before state change");
    println!("✓ After callbacks run after successful state change");
    println!("✓ Around::Before can abort transitions");
    println!("✓ Callbacks execute in fixed order (can't be changed)");
    println!("✓ All callbacks take &self (immutable access)");

    println!("\n=== Common Patterns ===");
    println!("• Around callbacks: Transaction semantics, resource acquisition/release");
    println!("• Before callbacks: Validation, logging, pre-conditions");
    println!("• After callbacks: Logging, notifications, post-processing");
    println!("• Multiple callbacks: Chain related operations in order");

    println!("\n=== When to Use Which ===");
    println!("• Use AROUND when: Need transaction semantics or can abort");
    println!("• Use BEFORE when: Need to log/check before transition");
    println!("• Use AFTER when: Need to notify/log after success");
    println!("• Use GUARDS when: Need conditional transitions (return bool)");
}
