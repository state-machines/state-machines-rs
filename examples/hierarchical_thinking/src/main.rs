//! # Hierarchical Thinking: Superstates as Capability Groups
//!
//! This example teaches **hierarchical states** (superstates) - grouping related
//! states to enable polymorphic transitions - using a spacecraft life support system.
//!
//! ## The Problem: Shared Transitions Across Related States
//!
//! Life support has multiple active modes:
//! - O2Generation (producing oxygen)
//! - CO2Scrubbing (removing carbon dioxide)
//! - PressureRegulation (maintaining cabin pressure)
//!
//! All three need a **common emergency shutdown** that works from ANY active mode.
//! Without superstates, you'd need 3 separate shutdown transitions:
//!
//! ```text
//! emergency_shutdown { transition: { from: O2Generation, to: Offline } }
//! emergency_shutdown { transition: { from: CO2Scrubbing, to: Offline } }
//! emergency_shutdown { transition: { from: PressureRegulation, to: Offline } }
//! ```
//!
//! With superstates, ONE transition handles all:
//!
//! ```text
//! emergency_shutdown { transition: { from: LifeSupport, to: Offline } }
//! // ↑ Works from ANY substate of LifeSupport!
//! ```
//!
//! ## Superstates: Grouping by Capability
//!
//! A superstate groups states that share common characteristics:
//! - **Common transitions** (emergency shutdown from any active mode)
//! - **Shared behaviors** (all active modes consume power)
//! - **Logical grouping** (all are "life support operational" states)
//!
//! Think of superstates as **trait bounds** for states.
//!
//! ## Ruby Comparison
//!
//! Ruby's state_machines doesn't have formal superstate support. You'd use predicates:
//!
//! ```ruby
//! def life_support_active?
//!   [:o2_generation, :co2_scrubbing, :pressure_regulation].include?(state)
//! end
//!
//! event :emergency_shutdown do
//!   transition any: :offline, if: :life_support_active?
//! end
//! ```
//!
//! Rust's version uses compile-time polymorphism:
//!
//! ```rust,ignore
//! superstate LifeSupport {
//!     state O2Generation,
//!     state CO2Scrubbing,
//!     state PressureRegulation,
//! }
//!
//! events {
//!     emergency_shutdown {
//!         transition: { from: LifeSupport, to: Offline }
//!         // ↑ Generated as trait bound: S: SubstateOf<LifeSupport>
//!     }
//! }
//! ```
//!
//! ## Key Concepts Demonstrated
//!
//! 1. **Polymorphic Transitions** - Single transition works from multiple states
//! 2. **SubstateOf Trait** - Compile-time marker for superstate membership
//! 3. **Initial Substate** - Superstate transitions resolve to first declared child
//! 4. **State Data Storage** - Each substate can have associated data
//! 5. **When to Group States** - Shared transitions/behaviors indicate superstate
//!
//! ## Caveats & Flow
//!
//! ### Caveat 1: Can't Transition Between Substates of Different Superstates
//!
//! ```rust,ignore
//! superstate GroupA { state X, state Y }
//! superstate GroupB { state Z, state W }
//!
//! events {
//!     invalid {
//!         transition: { from: X, to: Z }  // ✗ Won't work directly
//!     }
//! }
//! ```
//!
//! Must transition through a common state or out of superstates first.
//!
//! ### Caveat 2: Superstate Transitions Resolve to Initial Substate
//!
//! ```rust,ignore
//! superstate LifeSupport {
//!     state O2Generation,     // ← First declared = initial substate
//!     state CO2Scrubbing,
//!     state PressureRegulation,
//! }
//!
//! events {
//!     activate {
//!         transition: { from: Offline, to: LifeSupport }
//!         // ↑ Actually goes to O2Generation (first substate)
//!     }
//! }
//! ```
//!
//! Order matters! First substate is the initial/default.
//!
//! ### Caveat 3: State Data Accessors Only for Specific Substates
//!
//! ```rust,ignore
//! state O2Generation(O2Data),
//!
//! // Generated accessor (only works when machine is in O2Generation state):
//! impl<C> Machine<C, O2Generation> {
//!     pub fn o2_generation_data(&self) -> &O2Data { }
//! }
//!
//! // Can't access from other states - compile error
//! ```
//!
//! Type system prevents accessing wrong state's data.
//!
//! ### Caveat 4: Superstates Don't Have Their Own Data
//!
//! ```rust,ignore
//! superstate LifeSupport(SharedData) {  // ✗ Can't do this
//!     state O2Generation(O2Data),
//!     state CO2Scrubbing(CO2Data),
//! }
//! ```
//!
//! Only substates have data. Put shared data in context instead.
//!
//! ## The Flow: Life Support System
//!
//! ```text
//! Offline ──activate──> LifeSupport ──deactivate──> Offline
//!                          │
//!                          ├─ O2Generation ──next──> CO2Scrubbing
//!                          ├─ CO2Scrubbing ──next──> PressureRegulation
//!                          └─ PressureRegulation ──next──> O2Generation
//!
//! emergency_shutdown from ANY LifeSupport substate → Offline
//! ```
//!
//! **Realistic Scenario:**
//! - Start offline (no power consumption)
//! - Activate life support (enters first mode: O2Generation)
//! - Cycle through subsystem modes
//! - Emergency shutdown works from ANY active mode

use state_machines::state_machine;

/// O2 generation system data
#[derive(Default, Debug, Clone)]
struct O2Data {
    production_rate: f32, // liters per minute
}

/// CO2 scrubbing system data
#[derive(Default, Debug, Clone)]
struct CO2Data {
    scrub_efficiency: f32, // percentage
}

/// Pressure regulation system data
#[derive(Default, Debug, Clone)]
struct PressureData {
    target_pressure: f32, // kPa
}

// Define hierarchical life support state machine
state_machine! {
    name: LifeSupportSystem,

    initial: Offline,

    states: [
        Offline,

        // LifeSupport superstate groups all operational modes
        superstate LifeSupport {
            state O2Generation(O2Data),
            state CO2Scrubbing(CO2Data),
            state PressureRegulation(PressureData),
        },
    ],

    events {
        // Activate life support (enters initial substate: O2Generation)
        activate {
            transition: { from: Offline, to: LifeSupport }
        }

        // Cycle to next subsystem mode
        next {
            transition: { from: O2Generation, to: CO2Scrubbing }
            transition: { from: CO2Scrubbing, to: PressureRegulation }
            transition: { from: PressureRegulation, to: O2Generation }
        }

        // Deactivate from any life support mode (polymorphic!)
        deactivate {
            transition: { from: LifeSupport, to: Offline }
        }

        // Emergency shutdown from any life support mode (polymorphic!)
        emergency_shutdown {
            transition: { from: LifeSupport, to: Offline }
        }
    }
}

impl<C, S> LifeSupportSystem<C, S> {
    /// Display current operational status
    fn status(&self, state_name: &str) {
        println!("  Status: {}", state_name);
    }
}

// State-specific implementations
impl<C> LifeSupportSystem<C, O2Generation> {
    /// Access O2 generation data (only available in O2Generation state)
    fn display_o2_status(&self) {
        let data = self.o2_generation_data();
        println!("  O2 Production: {:.1} L/min", data.production_rate);
    }

    /// Modify O2 data
    fn set_production_rate(&mut self, rate: f32) {
        self.o2_generation_data_mut().production_rate = rate;
    }
}

impl<C> LifeSupportSystem<C, CO2Scrubbing> {
    /// Access CO2 scrubbing data (only available in CO2Scrubbing state)
    fn display_co2_status(&self) {
        let data = self.co2_scrubbing_data();
        println!("  CO2 Scrubbing Efficiency: {:.1}%", data.scrub_efficiency);
    }

    fn set_scrub_efficiency(&mut self, efficiency: f32) {
        self.co2_scrubbing_data_mut().scrub_efficiency = efficiency;
    }
}

impl<C> LifeSupportSystem<C, PressureRegulation> {
    /// Access pressure data (only available in PressureRegulation state)
    fn display_pressure_status(&self) {
        let data = self.pressure_regulation_data();
        println!("  Target Pressure: {:.1} kPa", data.target_pressure);
    }

    fn set_target_pressure(&mut self, pressure: f32) {
        self.pressure_regulation_data_mut().target_pressure = pressure;
    }
}

fn main() {
    println!("=== Life Support System Hierarchy Demo ===\n");
    println!("This example demonstrates hierarchical states (superstates).\n");

    // Scenario 1: Activating life support and cycling through modes
    println!("--- Scenario 1: Normal Operations ---");

    let system = LifeSupportSystem::new(());
    println!("System created: Offline");
    println!("  Power consumption: 0 kW\n");

    println!("Activating life support...");
    let mut system = system.activate().unwrap();
    println!("✓ Entered LifeSupport superstate");
    println!("  → Initial substate: O2Generation");
    system.status("O2Generation (Active)");
    system.set_production_rate(20.5);
    system.display_o2_status();
    println!();

    println!("Cycling to next subsystem...");
    let mut system = system.next().unwrap();
    println!("✓ Transitioned: O2Generation → CO2Scrubbing");
    system.status("CO2Scrubbing (Active)");
    system.set_scrub_efficiency(95.2);
    system.display_co2_status();
    println!();

    println!("Cycling to next subsystem...");
    let mut system = system.next().unwrap();
    println!("✓ Transitioned: CO2Scrubbing → PressureRegulation");
    system.status("PressureRegulation (Active)");
    system.set_target_pressure(101.3);
    system.display_pressure_status();
    println!();

    println!("Cycling back to first subsystem...");
    let system = system.next().unwrap();
    println!("✓ Transitioned: PressureRegulation → O2Generation");
    system.status("O2Generation (Active)");
    println!();

    // Scenario 2: Polymorphic deactivate (works from any LifeSupport substate)
    println!("--- Scenario 2: Polymorphic Deactivation ---");
    println!("Current state: O2Generation");
    println!("Calling deactivate() - works because O2Generation ⊆ LifeSupport");

    let _system = system.deactivate().unwrap();
    println!("✓ Deactivated from O2Generation → Offline");
    println!("  Power consumption: 0 kW\n");

    // Scenario 3: Emergency shutdown from different substate
    println!("--- Scenario 3: Emergency Shutdown (Polymorphic) ---");

    let system = LifeSupportSystem::new(());
    let system = system.activate().unwrap();
    let system = system.next().unwrap(); // → CO2Scrubbing
    let system = system.next().unwrap(); // → PressureRegulation

    println!("Current state: PressureRegulation");
    println!("⚠️  HULL BREACH DETECTED");
    println!("Calling emergency_shutdown()...");

    let _system = system.emergency_shutdown().unwrap();
    println!("✓ Emergency shutdown from PressureRegulation → Offline");
    println!("  All systems safed\n");

    // Scenario 4: Demonstrate polymorphism
    println!("--- Scenario 4: Understanding Polymorphism ---");
    println!("The deactivate and emergency_shutdown events work from:");
    println!("  • O2Generation (substate of LifeSupport)");
    println!("  • CO2Scrubbing (substate of LifeSupport)");
    println!("  • PressureRegulation (substate of LifeSupport)");
    println!();
    println!("This is because the macro generates:");
    println!("  impl<C, S: SubstateOf<LifeSupport>> LifeSupportSystem<C, S> {{");
    println!("      pub fn deactivate(self) -> Result<...> {{ ... }}");
    println!("  }}");
    println!();
    println!("The trait bound `S: SubstateOf<LifeSupport>` means:");
    println!("  'S can be ANY state that is a substate of LifeSupport'");
    println!();
    println!("Compiler ensures type safety:");
    println!("  system.deactivate()  // ✓ Works from any LifeSupport substate");
    println!("  // system.deactivate() // ✗ Won't compile from Offline state");

    println!("\n=== Key Takeaways ===");
    println!("✓ Superstates group related states for shared transitions");
    println!("✓ Polymorphic transitions use SubstateOf<T> trait bounds");
    println!("✓ Transition to superstate resolves to first declared substate");
    println!("✓ State data accessors are type-specific (compile-time safe)");
    println!("✓ Superstates don't have their own data (use context instead)");

    println!("\n=== When to Use Superstates ===");
    println!("Use superstates when:");
    println!("  • Multiple states share common transitions (emergency shutdown)");
    println!("  • States represent variations of same concept (active life support modes)");
    println!("  • Need polymorphic behavior (one transition, multiple source states)");
    println!("  • Logical grouping improves code clarity");

    println!("\nDon't use superstates when:");
    println!("  • States are unrelated (just use flat states)");
    println!("  • Only one or two states in group (not worth complexity)");
    println!("  • Transitions are state-specific anyway (no polymorphism needed)");

    println!("\n=== Design Pattern ===");
    println!("Think of superstates as 'capability markers':");
    println!("  • States in LifeSupport can be emergency_shutdown");
    println!("  • States in PowerSaving can be woken_up");
    println!("  • States in Operational can be monitored");
    println!();
    println!("Similar to Rust trait bounds:");
    println!("  fn shutdown<S: SubstateOf<LifeSupport>>(system: System<S>)");
}
