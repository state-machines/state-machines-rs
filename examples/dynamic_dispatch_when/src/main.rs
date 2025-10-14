//! # Dynamic Dispatch When: Runtime Flexibility vs Compile-Time Safety
//!
//! This example teaches **when to use dynamic dispatch** vs typestate patterns
//! using a spacecraft computer command system.
//!
//! ## The Problem: External Event Sources
//!
//! Spacecraft computers receive commands from multiple sources:
//! - 📡 Mission Control (network messages)
//! - 👨‍✈️ Pilot (button presses)
//! - 🤖 Autopilot (computed decisions)
//!
//! These events arrive at **runtime** from external sources. You can't know
//! at compile time which sequence of commands will arrive. This is where
//! dynamic dispatch shines.
//!
//! ## Typestate vs Dynamic: The Core Tradeoff
//!
//! **Typestate (Compile-Time):**
//! ```rust,ignore
//! let computer = Computer::new(());        // Type: Computer<Standby>
//! let computer = computer.boot().unwrap(); // Type: Computer<Active>
//! // computer.boot()                       // ✗ Won't compile!
//! ```
//!
//! ✓ Pros: Compile-time safety, zero overhead, impossible states prevented
//! ✗ Cons: Must know sequence at compile time, can't store in collections
//!
//! **Dynamic (Runtime):**
//! ```rust,ignore
//! let mut computer = DynamicComputer::new(());
//! computer.handle(ComputerEvent::Boot).unwrap();  // Runtime dispatch
//! computer.handle(ComputerEvent::Boot).unwrap();  // Runtime error (but compiles)
//! ```
//!
//! ✓ Pros: Runtime flexibility, event-driven, can store in collections
//! ✗ Cons: Runtime errors possible, small overhead (~few ns)
//!
//! ## Key Concepts Demonstrated
//!
//! 1. **Enabling Dynamic Mode** - `dynamic: true` or feature flag
//! 2. **Event Enum** - Auto-generated runtime event representation
//! 3. **Converting Between Modes** - `.into_dynamic()` and `.into_<state>()`
//! 4. **When to Use Which** - Decision framework
//! 5. **Hybrid Pattern** - Typestate setup → Dynamic runtime → Typestate verification
//!
//! ## Caveats & Flow
//!
//! ### Caveat 1: Dynamic Mode Loses Compile-Time State Safety
//!
//! ```rust,ignore
//! let mut computer = DynamicComputer::new(());
//! computer.handle(ComputerEvent::Boot).unwrap();
//! computer.handle(ComputerEvent::Boot).unwrap();  // Compiles but fails at runtime
//! ```
//!
//! Invalid transitions compile but fail at runtime. This is the tradeoff.
//!
//! ### Caveat 2: Can't Use Typestate Methods on Dynamic Machine
//!
//! ```rust,ignore
//! let dynamic = DynamicComputer::new(());
//! // dynamic.boot()  // ✗ Method doesn't exist on dynamic wrapper
//!
//! dynamic.handle(ComputerEvent::Boot)  // ✓ Use handle() instead
//! ```
//!
//! Dynamic wrapper only has `handle()` and `current_state()`.
//!
//! ### Caveat 3: Event Payloads Must Be Clone
//!
//! ```rust,ignore
//! #[derive(Clone)]  // ← Required for dynamic mode
//! struct CommandPayload {
//!     data: Vec<u8>
//! }
//! ```
//!
//! Dynamic mode needs to clone payloads for routing.
//!
//! ### Caveat 4: current_state() Returns String, Not Type
//!
//! ```rust,ignore
//! let state: &str = dynamic.current_state();  // "Active", "Standby", etc.
//! // Not a type - can't use type-specific methods
//! ```
//!
//! State is a runtime string, not a compile-time type marker.
//!
//! ## The Flow: Spacecraft Computer Command System
//!
//! ```text
//! TYPESTATE PHASE (Setup):
//!   Standby → boot() → Active
//!
//! DYNAMIC PHASE (Runtime event loop):
//!   Active → handle(event) → (various states based on commands)
//!
//! TYPESTATE PHASE (Verification):
//!   .into_active() → Extract typed machine for critical operations
//! ```
//!
//! **Realistic Scenario:**
//! - Boot sequence uses typestate (known sequence)
//! - Convert to dynamic for command loop (unknown sequence)
//! - Extract back to typestate for shutdown (safety critical)

use state_machines::{state_machine, DynamicError};

/// Command source identifier
#[derive(Debug, Clone)]
enum CommandSource {
    MissionControl,
    Pilot,
    Autopilot,
}

/// Command payload
#[derive(Debug, Clone)]
struct Command {
    source: CommandSource,
    priority: u8,
}

// Define spacecraft computer state machine with dynamic mode enabled
state_machine! {
    name: SpacecraftComputer,

    dynamic: true,  // ← Enable dynamic dispatch

    initial: Standby,

    states: [
        Standby,
        Active,
        Processing,
        Diagnostics,
        SafeMode,
    ],

    events {
        // Boot computer
        boot {
            transition: { from: Standby, to: Active }
        }

        // Execute command
        execute {
            payload: Command,
            transition: { from: Active, to: Processing }
        }

        // Complete processing
        complete {
            transition: { from: Processing, to: Active }
        }

        // Run diagnostics
        diagnose {
            transition: { from: Active, to: Diagnostics }
        }

        // Return from diagnostics
        resume {
            transition: { from: Diagnostics, to: Active }
        }

        // Enter safe mode
        safe_mode {
            transition: { from: [Active, Processing, Diagnostics], to: SafeMode }
        }

        // Shutdown
        shutdown {
            transition: { from: [Active, SafeMode], to: Standby }
        }
    }
}

fn main() {
    println!("=== Spacecraft Computer Command System Demo ===\n");
    println!("This example demonstrates when to use dynamic vs typestate.\n");

    // === PATTERN 1: Typestate for Known Sequence ===
    println!("--- Pattern 1: Typestate Boot Sequence ---");
    println!("Use typestate when sequence is known at compile time:\n");

    let computer = SpacecraftComputer::new(());
    println!("Computer created: Standby (type: SpacecraftComputer<(), Standby>)");

    let computer = computer.boot().unwrap();
    println!("Boot complete: Active (type: SpacecraftComputer<(), Active>)");

    // Can't boot again - won't compile!
    // computer.boot().unwrap();  // ✗ Error: no method named `boot` found

    println!("✓ Typestate prevents invalid transitions at compile time\n");

    // === PATTERN 2: Dynamic for Runtime Events ===
    println!("--- Pattern 2: Dynamic Event Loop ---");
    println!("Use dynamic when events come from external sources:\n");

    // Convert to dynamic for runtime flexibility
    let mut computer = computer.into_dynamic();
    println!("Converted to dynamic mode");
    println!("Current state: {}\n", computer.current_state());

    // Simulate incoming commands from various sources
    let commands = vec![
        (
            "Mission Control",
            SpacecraftComputerEvent::Execute(Command {
                source: CommandSource::MissionControl,
                priority: 9,
            }),
        ),
        ("Onboard", SpacecraftComputerEvent::Complete),
        ("Pilot", SpacecraftComputerEvent::Diagnose),
        ("Autopilot", SpacecraftComputerEvent::Resume),
    ];

    println!("Processing incoming commands...\n");
    for (source, event) in commands {
        print!("[{}] Sending command: {:?} → ", source, event);

        match computer.handle(event) {
            Ok(()) => {
                println!("Success (state: {})", computer.current_state());
            }
            Err(e) => {
                println!("Failed: {:?}", e);
            }
        }
    }

    println!("\n✓ Dynamic mode allows runtime command dispatch");
    println!("  Current state: {}\n", computer.current_state());

    // === PATTERN 3: Invalid Transition (Runtime Error) ===
    println!("--- Pattern 3: Dynamic Mode Runtime Errors ---");
    println!("Dynamic mode allows invalid transitions to compile:\n");

    println!("Attempting to boot while Active...");
    match computer.handle(SpacecraftComputerEvent::Boot) {
        Ok(()) => println!("✗ ERROR: Should have failed!"),
        Err(DynamicError::InvalidTransition { from, event }) => {
            println!("✓ Runtime error caught:");
            println!("  Cannot '{}' from '{}' state", event, from);
            println!("  Machine remains in: {}", computer.current_state());
        }
        Err(e) => println!("✗ Unexpected error: {:?}", e),
    }

    println!("\n✓ Invalid transitions fail at runtime (not compile time)\n");

    // === PATTERN 4: Hybrid - Back to Typestate for Safety ===
    println!("--- Pattern 4: Hybrid Pattern (Critical Operations) ---");
    println!("Extract typestate for safety-critical operations:\n");

    println!("Current dynamic state: {}", computer.current_state());
    println!("Attempting to extract as Active typestate...");

    match computer.into_active() {
        Ok(typed_computer) => {
            println!("✓ Successfully extracted typed machine");
            println!("  Type: SpacecraftComputer<(), Active>");
            println!("  Can now use type-safe methods\n");

            // Now can use type-safe shutdown
            println!("Performing type-safe shutdown...");
            let _computer = typed_computer.shutdown().unwrap();
            println!("✓ Shutdown complete (type: SpacecraftComputer<(), Standby>)");
        }
        Err(dynamic) => {
            println!("✗ Machine not in Active state");
            println!("  Current state: {}", dynamic.current_state());
            println!("  Cannot extract as Active");
        }
    }

    // === DECISION GUIDE ===
    println!("\n\n=== Decision Guide: When to Use Which ===");
    println!("\nUse TYPESTATE when:");
    println!("  ✓ Sequence known at compile time (boot → init → ready)");
    println!("  ✓ Want compile-time safety guarantees");
    println!("  ✓ Performance critical (zero overhead)");
    println!("  ✓ Building DSLs or configuration pipelines");
    println!("  ✓ State machine is short-lived");

    println!("\nUse DYNAMIC when:");
    println!("  ✓ Events from external sources (network, user input)");
    println!("  ✓ Event sequence unknown at compile time");
    println!("  ✓ Need to store machines in collections");
    println!("  ✓ Building event-driven systems");
    println!("  ✓ Need runtime introspection (current_state())");

    println!("\nUse HYBRID (both) when:");
    println!("  ✓ Known setup phase → Unknown runtime → Known cleanup");
    println!("  ✓ Want safety where possible, flexibility where needed");
    println!("  ✓ Critical operations need type safety");
    println!("  ✓ Most operations can tolerate runtime errors");

    println!("\n=== Performance Comparison ===");
    println!("Typestate: 0 ns overhead (PhantomData optimizes away)");
    println!("Dynamic:   ~few ns overhead (enum match + method call)");
    println!("Overhead is negligible for most applications");

    println!("\n=== Common Patterns ===");
    println!("1. Event Loop Pattern:");
    println!("   let mut machine = DynamicMachine::new(());");
    println!("   loop {{ machine.handle(receive_event()).ok(); }}");
    println!();
    println!("2. Hybrid Boot Pattern:");
    println!("   let m = Machine::new(()).boot().unwrap();  // Typestate");
    println!("   let m = m.into_dynamic();                  // Switch to dynamic");
    println!("   // ... runtime event loop ...");
    println!("   let m = m.into_active().unwrap();          // Back to typestate");
    println!();
    println!("3. Collection Pattern:");
    println!("   let machines: Vec<DynamicMachine> = vec![");
    println!("       m1.into_dynamic(),");
    println!("       m2.into_dynamic(),");
    println!("   ];");

    println!("\n=== Key Takeaways ===");
    println!("✓ Typestate = compile-time safety, zero cost");
    println!("✓ Dynamic = runtime flexibility, minimal cost");
    println!("✓ Can convert between modes with .into_dynamic() / .into_<state>()");
    println!("✓ Invalid transitions: compile error (typestate) vs runtime error (dynamic)");
    println!("✓ Use hybrid pattern for best of both worlds");
    println!("✓ Decision based on whether event sequence is known at compile time");
}
