//! Test concrete context type support.
//!
//! This test verifies that when a concrete context type is specified in the macro,
//! guards can directly access context fields without trait bounds.

use state_machines::state_machine;

/// Concrete context type with hardware/sensor data
#[derive(Debug, Default)]
struct SpacecraftSensors {
    altitude_km: u32,
    fuel_level: u8,
    temperature_c: i16,
}

impl SpacecraftSensors {
    fn new() -> Self {
        Self {
            altitude_km: 0,
            fuel_level: 100,
            temperature_c: 20,
        }
    }

    fn with_altitude(mut self, altitude: u32) -> Self {
        self.altitude_km = altitude;
        self
    }

    fn with_fuel(mut self, fuel: u8) -> Self {
        self.fuel_level = fuel;
        self
    }
}

// Define state machine with concrete context type and dynamic support
state_machine! {
    name: Spacecraft,
    context: SpacecraftSensors,  // ← Concrete context type
    dynamic: true,  // ← Enable dynamic dispatch for testing
    initial: Grounded,
    states: [Grounded, Flying, Orbiting],

    events {
        launch {
            guards: [has_fuel, safe_temperature],
            transition: { from: Grounded, to: Flying }
        }

        reach_orbit {
            guards: [sufficient_altitude],
            transition: { from: Flying, to: Orbiting }
        }

        land {
            transition: { from: [Flying, Orbiting], to: Grounded }
        }
    }
}

// Implement guards that directly access context fields
// This works because context is concrete, not generic
impl<S> Spacecraft<S> {
    /// Guard: Check if we have enough fuel
    ///
    /// Can directly access ctx.fuel_level because context is concrete
    fn has_fuel(&self, ctx: &SpacecraftSensors) -> bool {
        ctx.fuel_level >= 50 // Need at least 50% fuel
    }

    /// Guard: Check if temperature is safe
    ///
    /// Can directly access ctx.temperature_c because context is concrete
    fn safe_temperature(&self, ctx: &SpacecraftSensors) -> bool {
        ctx.temperature_c >= -50 && ctx.temperature_c <= 100
    }

    /// Guard: Check if altitude is sufficient for orbit
    ///
    /// Can directly access ctx.altitude_km because context is concrete
    fn sufficient_altitude(&self, ctx: &SpacecraftSensors) -> bool {
        ctx.altitude_km >= 400 // Need at least 400km for orbit
    }
}

#[test]
fn test_concrete_context_guards() {
    // Test successful launch with sufficient fuel
    let sensors = SpacecraftSensors::new().with_fuel(75);
    let spacecraft = Spacecraft::new(sensors);

    let spacecraft = spacecraft.launch();
    assert!(spacecraft.is_ok(), "Launch should succeed with 75% fuel");

    // Test failed launch with insufficient fuel
    let sensors = SpacecraftSensors::new().with_fuel(30);
    let spacecraft = Spacecraft::new(sensors);

    let result = spacecraft.launch();
    assert!(result.is_err(), "Launch should fail with 30% fuel");
    if let Err((_, err)) = result {
        assert_eq!(err.guard, "has_fuel");
    }
}

#[test]
fn test_concrete_context_altitude_guard() {
    // Launch successfully
    let sensors = SpacecraftSensors::new().with_fuel(100);
    let spacecraft = Spacecraft::new(sensors);
    let spacecraft = spacecraft.launch().unwrap();

    // Try to reach orbit without sufficient altitude (fails)
    let result = spacecraft.reach_orbit();
    assert!(result.is_err(), "Orbit should fail at 0km altitude");
    if let Err((spacecraft, err)) = result {
        assert_eq!(err.guard, "sufficient_altitude");

        // Update altitude and try again
        // Note: We need to access internal state to modify altitude
        // In real code, you'd update the context through proper channels
        // For this test, we'll create a new machine
        let _ = spacecraft; // Explicitly discard
    }

    // Create new machine with sufficient altitude
    let sensors = SpacecraftSensors::new().with_fuel(100).with_altitude(500);
    let spacecraft = Spacecraft::new(sensors);
    let spacecraft = spacecraft.launch().unwrap();
    let spacecraft = spacecraft.reach_orbit();
    assert!(spacecraft.is_ok(), "Orbit should succeed at 500km altitude");
}

#[test]
fn test_concrete_context_temperature_guard() {
    // Test extreme cold (should fail)
    let mut sensors = SpacecraftSensors::new().with_fuel(100);
    sensors.temperature_c = -60;
    let spacecraft = Spacecraft::new(sensors);

    let result = spacecraft.launch();
    assert!(result.is_err(), "Launch should fail at -60°C");
    if let Err((_, err)) = result {
        assert_eq!(err.guard, "safe_temperature");
    }

    // Test extreme heat (should fail)
    let mut sensors = SpacecraftSensors::new().with_fuel(100);
    sensors.temperature_c = 110;
    let spacecraft = Spacecraft::new(sensors);

    let result = spacecraft.launch();
    assert!(result.is_err(), "Launch should fail at 110°C");
    if let Err((_, err)) = result {
        assert_eq!(err.guard, "safe_temperature");
    }

    // Test normal temperature (should succeed)
    let mut sensors = SpacecraftSensors::new().with_fuel(100);
    sensors.temperature_c = 25;
    let spacecraft = Spacecraft::new(sensors);

    let spacecraft = spacecraft.launch();
    assert!(spacecraft.is_ok(), "Launch should succeed at 25°C");
}

#[test]
fn test_concrete_context_type_signature() {
    // Verify that the generated struct has the correct type signature
    let sensors = SpacecraftSensors::new();
    let spacecraft: Spacecraft<Grounded> = Spacecraft::new(sensors);

    // This compiles, proving the context type is concrete, not generic
    let _ = spacecraft;
}

#[test]
fn test_concrete_context_dynamic_dispatch() {
    // Create dynamic machine with concrete context
    let sensors = SpacecraftSensors::new().with_fuel(80).with_altitude(450);
    let mut spacecraft = DynamicSpacecraft::new(sensors);

    assert_eq!(spacecraft.current_state(), "Grounded");

    // Test successful transition via dynamic dispatch
    let result = spacecraft.handle(SpacecraftEvent::Launch);
    assert!(result.is_ok(), "Launch should succeed with 80% fuel");
    assert_eq!(spacecraft.current_state(), "Flying");

    // Test successful orbit transition
    let result = spacecraft.handle(SpacecraftEvent::ReachOrbit);
    assert!(result.is_ok(), "Should reach orbit at 450km");
    assert_eq!(spacecraft.current_state(), "Orbiting");

    // Test landing from orbiting
    let result = spacecraft.handle(SpacecraftEvent::Land);
    assert!(result.is_ok(), "Should land from orbit");
    assert_eq!(spacecraft.current_state(), "Grounded");
}

#[test]
fn test_concrete_context_dynamic_guard_failure() {
    use state_machines::DynamicError;

    // Create dynamic machine with insufficient fuel
    let sensors = SpacecraftSensors::new().with_fuel(30);
    let mut spacecraft = DynamicSpacecraft::new(sensors);

    // Attempt launch with low fuel
    let result = spacecraft.handle(SpacecraftEvent::Launch);
    assert!(result.is_err(), "Launch should fail with 30% fuel");

    match result {
        Err(DynamicError::GuardFailed { guard, event }) => {
            assert_eq!(guard, "has_fuel");
            assert_eq!(event, "launch");
        }
        _ => panic!("Expected GuardFailed error"),
    }

    // Machine should remain in original state after guard failure
    assert_eq!(spacecraft.current_state(), "Grounded");
}

#[test]
fn test_concrete_context_dynamic_conversion() {
    // Test typestate to dynamic conversion
    let sensors = SpacecraftSensors::new().with_fuel(100);
    let spacecraft = Spacecraft::new(sensors);
    let spacecraft = spacecraft.launch().unwrap();

    // Convert to dynamic
    let mut dynamic = spacecraft.into_dynamic();
    assert_eq!(dynamic.current_state(), "Flying");

    // Use dynamic mode
    let result = dynamic.handle(SpacecraftEvent::Land);
    assert!(result.is_ok());
    assert_eq!(dynamic.current_state(), "Grounded");

    // Convert back to typestate
    let typed = dynamic.into_grounded();
    assert!(typed.is_ok(), "Should extract Grounded state");
}
