//! # Async Patterns: Not Just Adding .await
//!
//! This example teaches **async state machines** - how async/await integrates with
//! typestate patterns - using a drone scout communication system.
//!
//! ## The Problem: I/O-Bound State Transitions
//!
//! Scout drones must:
//! - ✓ Scan for signals (async I/O operation)
//! - ✓ Lock onto signal with delay (network latency)
//! - ✓ Transmit data with retries (async operation)
//! - ⚠️ Handle timeouts gracefully
//!
//! Async state machines let you use `.await` in guards and callbacks without blocking.
//! They require enabling the crate's `async` feature in `Cargo.toml`.
//!
//! ## Sync vs Async: The Core Difference
//!
//! **Sync State Machine:**
//! ```rust,ignore
//! state_machine! {
//!     name: Drone,
//!     async: false,  // or omit (default)
//!     // ...
//! }
//!
//! fn guard(&self, ctx: &C) -> bool {
//!     // Synchronous - can't use .await
//!     check_local_state()
//! }
//! ```
//!
//! **Async State Machine:**
//! ```rust,ignore
//! state_machine! {
//!     name: Drone,
//!     async: true,  // ← Enable async
//!     // ...
//! }
//!
//! async fn guard(&self, ctx: &C) -> bool {
//!     // Asynchronous - can use .await
//!     check_network_status().await
//! }
//! ```
//!
//! ## Key Concepts Demonstrated
//!
//! 1. **Enabling Async** - `async: true` in macro
//! 2. **Async Guards** - Guard functions that can await
//! 3. **Async Callbacks** - Before/after callbacks with async operations
//! 4. **Blocking vs Async** - When to use which pattern
//! 5. **Error Handling** - Fallible callbacks can return `Result<(), E>` and prevent state advancement
//!
//! ## Caveats & Flow
//!
//! ### Caveat 1: All or Nothing - Can't Mix Sync and Async
//!
//! ```rust,ignore
//! state_machine! {
//!     name: Machine,
//!     async: true,
//!     // ...
//! }
//!
//! impl<C, S> Machine<C, S> {
//!     async fn guard(&self, ctx: &C) -> bool { }  // ✓ Async
//!     fn other_guard(&self, ctx: &C) -> bool { }  // ✗ Won't work!
//! }
//! ```
//!
//! **Why:** Once `async: true`, ALL guards/callbacks become async. You must
//! use `async fn` for all of them (even if some don't need `.await`).
//!
//! ### Caveat 2: Context Must Be Send for Async
//!
//! ```rust,ignore
//! struct Context {
//!     data: Rc<String>  // ✗ Not Send!
//! }
//!
//! let machine = AsyncMachine::new(context);
//! machine.event().await;  // Won't compile - context not Send
//! ```
//!
//! **Why:** Async state machines might be sent across thread boundaries by the
//! executor. Use `Arc` instead of `Rc`, or ensure all fields are `Send`.
//!
//! ### Caveat 3: Don't Block in Async Context
//!
//! ```rust,ignore
//! async fn bad_guard(&self, ctx: &C) -> bool {
//!     std::thread::sleep(Duration::from_secs(1));  // ✗ Blocks entire executor!
//!     true
//! }
//!
//! async fn good_guard(&self, ctx: &C) -> bool {
//!     tokio::time::sleep(Duration::from_secs(1)).await;  // ✓ Yields to executor
//!     true
//! }
//! ```
//!
//! Common mistake: Using blocking I/O in async guards.
//!
//! ### Caveat 4: Every Transition Becomes Async
//!
//! ```rust,ignore
//! let machine = AsyncMachine::new(());
//! let machine = machine.event().await;  // ← Must .await every transition
//! //                            ^^^^^^
//! ```
//!
//! Can't forget `.await` - compiler will error if you do.
//!
//! ## The Flow: Drone Scout Communication
//!
//! ```text
//! Idle ──scan──> Scanning ──lock──> SignalLock ──transmit──> Transmitting ──complete──> DataReceived
//!         │         ↓                    ↓
//!         │         └──timeout───────────┘ (back to Idle)
//!         │
//!         └─ (async delays simulate network latency)
//! ```
//!
//! **Realistic Scenario:**
//! - Scanning takes time (async wait for signal detection)
//! - Signal lock requires handshake (network round-trip)
//! - Transmission happens in chunks (async I/O)
//! - Timeouts can occur at any stage

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use state_machines::{EventError, state_machine};
use std::task::Poll;

mod invocation;

/// Simulated signal strength (0-100)
static SIGNAL_STRENGTH: AtomicU64 = AtomicU64::new(0);

/// Simulated network available flag
static NETWORK_AVAILABLE: AtomicBool = AtomicBool::new(true);
static PREP_SHOULD_FAIL: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DroneError {
    PreparationFailed,
}

/// Helper for signal system
struct SignalSystem;

impl SignalSystem {
    fn set_strength(strength: u64) {
        SIGNAL_STRENGTH.store(strength, Ordering::Relaxed);
    }

    async fn scan_for_signal() -> u64 {
        println!("  [SignalSystem] Scanning for signals...");
        // Simulate async I/O delay
        simulated_io().await;
        let strength = SIGNAL_STRENGTH.load(Ordering::Relaxed);
        println!("  [SignalSystem] Scan complete: {} strength", strength);
        strength
    }

    async fn establish_lock() -> bool {
        println!("  [SignalSystem] Establishing signal lock...");
        // Simulate network round-trip
        simulated_io().await;
        let available = NETWORK_AVAILABLE.load(Ordering::Relaxed);
        println!("  [SignalSystem] Lock status: {}", available);
        available
    }

    async fn transmit_data() {
        println!("  [SignalSystem] Transmitting data packets...");
        for i in 1..=3 {
            simulated_io().await;
            println!("  [SignalSystem]   Packet {} sent", i);
        }
        println!("  [SignalSystem] Transmission complete");
    }
}

/// Deterministic simulated I/O yields once without blocking or sleeping.
/// Real hardware/network futures should use their driver's actual wakeups.
async fn simulated_io() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}

// Define async drone scout state machine
state_machine! {
    name: DroneScout,

    async: true,  // ← Enable async mode
    error: DroneError,

    initial: Idle,
    states: [Idle, Scanning, SignalLock, Transmitting, DataReceived],

    events {
        // Start scanning for signals
        scan {
            guards: [has_power],
            before: [log_scan_start],
            transition: { from: Idle, to: Scanning }
        }

        // Lock onto signal
        lock {
            guards: [signal_detected],
            after: [log_signal_locked],
            transition: { from: Scanning, to: SignalLock }
        }

        // Begin data transmission
        transmit {
            guards: [network_ready],
            before: [prepare_transmission],
            transition: { from: SignalLock, to: Transmitting }
        }

        // Complete transmission
        complete {
            after: [log_transmission_complete],
            transition: { from: Transmitting, to: DataReceived }
        }

        // Timeout - reset to idle
        timeout {
            transition: { from: [Scanning, SignalLock], to: Idle }
        }
    }
}

impl<C, S> DroneScout<C, S> {
    // === ASYNC GUARDS ===
    // Guards that perform async operations

    /// Async guard: Check if drone has power
    ///
    /// In a real system, this might query battery status over I2C bus
    async fn has_power(&self, _ctx: &C) -> bool {
        println!("[Guard] Checking power status...");
        // Simulate async hardware query
        simulated_io().await;
        let has_power = true; // Always have power in this demo
        println!("[Guard] Power status: {}", has_power);
        has_power
    }

    /// Async guard: Check if signal is detected
    ///
    /// Performs actual async scan operation
    async fn signal_detected(&self, _ctx: &C) -> bool {
        println!("[Guard] Checking for signal...");
        let strength = SignalSystem::scan_for_signal().await;
        let detected = strength >= 50; // Need at least 50 strength
        println!(
            "[Guard] Signal detected: {} (strength: {})",
            detected, strength
        );
        detected
    }

    /// Async guard: Check if network is ready for transmission
    async fn network_ready(&self, _ctx: &C) -> bool {
        println!("[Guard] Verifying network readiness...");
        let ready = SignalSystem::establish_lock().await;
        println!("[Guard] Network ready: {}", ready);
        ready
    }

    // === ASYNC CALLBACKS ===
    // Before/after callbacks that can perform async operations

    /// Async before callback: Log scan start
    async fn log_scan_start(&self) {
        println!("[Before] Starting scan sequence...");
        // Simulate logging to remote telemetry
        simulated_io().await;
        println!("[Before] Scan sequence logged to mission control");
    }

    /// Async after callback: Log signal locked
    async fn log_signal_locked(&self) {
        println!("[After] Signal lock acquired!");
        // Simulate async logging
        simulated_io().await;
        println!("[After] Lock status transmitted to base");
    }

    /// Async before callback: Prepare transmission
    async fn prepare_transmission(&self) -> Result<(), DroneError> {
        println!("[Before] Preparing data transmission...");
        println!("[Before]   Compressing data...");
        simulated_io().await;
        if PREP_SHOULD_FAIL.load(Ordering::Relaxed) {
            println!("[Before]   Compression worker unavailable!");
            return Err(DroneError::PreparationFailed);
        }
        println!("[Before]   Encryption enabled...");
        simulated_io().await;
        println!("[Before] Ready to transmit");
        Ok(())
    }

    /// Async after callback: Log transmission complete
    async fn log_transmission_complete(&self) {
        println!("[After] Transmission complete!");
        SignalSystem::transmit_data().await;
        println!("[After] All data received by base");
    }
}

fn print_async_error(err: EventError<DroneError>) {
    match err {
        EventError::Guard(err) => {
            println!("  Guard failed: {}", err.guard);
        }
        EventError::Callback(err) => {
            println!("  Callback failed: {}", err.action);
            println!("  Source error: {:?}", err.source);
        }
    }
}

type StepResult<From, To> =
    Result<DroneScout<(), To>, (DroneScout<(), From>, EventError<DroneError>)>;

fn nominal_step<From, To>(result: StepResult<From, To>, state: &str) -> DroneScout<(), To> {
    match result {
        Ok(drone) => {
            println!("✓ Transitioned to {state}\n");
            drone
        }
        Err((_drone, error)) => {
            print_async_error(error);
            panic!("nominal transition to {state} failed");
        }
    }
}

fn main() {
    println!("=== Drone Scout Communication Demo ===\n");
    println!("This example demonstrates async state machine patterns.\n");

    // Scenario 1: Successful communication sequence
    println!("--- Scenario 1: Nominal Communication ---");
    SignalSystem::set_strength(75); // Good signal
    NETWORK_AVAILABLE.store(true, Ordering::Relaxed);
    PREP_SHOULD_FAIL.store(false, Ordering::Relaxed);

    let drone = DroneScout::new(());
    println!("Drone created in Idle state\n");

    // IMPORTANT: Notice .await on every transition
    println!("Calling drone.scan().await...\n");
    let drone = nominal_step(pollster::block_on(drone.scan()), "Scanning");

    println!("Calling drone.lock().await...\n");
    let drone = nominal_step(pollster::block_on(drone.lock()), "SignalLock");

    println!("Calling drone.transmit().await...\n");
    let drone = nominal_step(pollster::block_on(drone.transmit()), "Transmitting");

    println!("Calling drone.complete().await...\n");
    let _drone = nominal_step(pollster::block_on(drone.complete()), "DataReceived");

    // Scenario 2: Weak signal - guard failure
    println!("\n--- Scenario 2: Weak Signal (Guard Failure) ---");
    SignalSystem::set_strength(30); // Too weak!

    let drone = DroneScout::new(());
    let drone = pollster::block_on(async { drone.scan().await.unwrap() });

    println!("Signal strength too weak (30), attempting lock...\n");
    pollster::block_on(async {
        match drone.lock().await {
            Ok(_) => println!("✗ ERROR: Should have failed!"),
            Err((_drone, err)) => {
                println!("✓ Guard correctly prevented transition");
                print_async_error(err);
                println!("  Drone remains in Scanning state");
                println!("  Can retry after signal improves or timeout\n");
            }
        }
    });

    // Scenario 3: Callback failure - before callback returns a real error
    println!("\n--- Scenario 3: Callback Failure (No State Advance) ---");
    SignalSystem::set_strength(80);
    NETWORK_AVAILABLE.store(true, Ordering::Relaxed);
    PREP_SHOULD_FAIL.store(true, Ordering::Relaxed);

    let drone = DroneScout::new(());
    let drone = pollster::block_on(async { drone.scan().await.unwrap() });
    let drone = pollster::block_on(async { drone.lock().await.unwrap() });

    println!("Transmission prep will fail, attempting transmit...\n");
    let drone = pollster::block_on(async {
        match drone.transmit().await {
            Ok(_) => {
                println!("✗ ERROR: Should have failed!");
                Err(())
            }
            Err((drone, err)) => {
                println!("✓ Callback error prevented state advance");
                print_async_error(err);
                println!("  Drone remains in SignalLock and can retry\n");
                Ok(drone)
            }
        }
    })
    .unwrap();

    PREP_SHOULD_FAIL.store(false, Ordering::Relaxed);
    let _drone = pollster::block_on(async { drone.transmit().await.unwrap() });
    println!("✓ Retry succeeded once the callback stopped failing");

    println!("\n=== Key Takeaways ===");
    println!("✓ async: true enables async mode for entire state machine");
    println!("✓ error: DroneError enables fallible before/after callbacks");
    println!("✓ All guards and callbacks become async fn");
    println!("✓ Must .await every transition");
    println!("✓ Can use async I/O in guards (scan_for_signal, etc.)");
    println!("✓ Callback errors return the original machine for retry");
    println!("✓ Executor choice determines whether a future/context must be Send");
    println!("✓ Don't use blocking operations in async guards");

    println!("\n=== Sync vs Async Decision Guide ===");
    println!("Use SYNC when:");
    println!("  • All checks are in-memory (no I/O)");
    println!("  • Performance critical hot path");
    println!("  • Embedded systems without async runtime");

    println!("\nUse ASYNC when:");
    println!("  • Guards need to perform I/O (network, disk)");
    println!("  • Callbacks involve async operations");
    println!("  • Natural fit for event-driven systems");

    println!("\n=== Common Mistakes ===");
    println!("✗ Mixing sync and async guards (all must be async)");
    println!("✗ Moving a non-Send future to a multi-threaded executor");
    println!("✗ Blocking with thread::sleep instead of async sleep");
    println!("✗ Forgetting .await on transitions");

    pollster::block_on(invocation::run_demo());
}
