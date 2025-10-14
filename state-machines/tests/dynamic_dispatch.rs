#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;

state_machine! {
    name: TrafficLight,
    initial: Red,
    dynamic: true,
    states: [Red, Yellow, Green],
    events {
        next {
            transition: { from: Red, to: Green }
            transition: { from: Green, to: Yellow }
            transition: { from: Yellow, to: Red }
        }
    }
}

#[test]
fn test_dynamic_dispatch_basic() {
    // Create a dynamic machine with unit context
    let mut light = DynamicTrafficLight::new(());

    // Check initial state
    assert_eq!(light.current_state(), "Red");

    // Trigger events dynamically
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), "Green");

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), "Yellow");

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), "Red");
}

#[test]
fn test_typestate_to_dynamic_conversion() {
    // Start in typestate mode
    let light = TrafficLight::new(());

    // Convert to dynamic
    let mut dynamic_light = light.into_dynamic();
    assert_eq!(dynamic_light.current_state(), "Red");

    // Use dynamic dispatch
    dynamic_light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(dynamic_light.current_state(), "Green");
}

#[test]
fn test_dynamic_to_typestate_conversion() {
    // Start in dynamic mode
    let mut light = DynamicTrafficLight::new(());

    // Transition to Green
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), "Green");

    // Convert back to typestate
    let typed_light = light.into_green().unwrap();

    // Now we have compile-time type safety
    let _ = typed_light.next();
}

#[test]
fn test_event_enum() {
    // Event enum variant is PascalCase (Next), but name() returns snake_case (next)
    let event = TrafficLightEvent::Next;
    assert_eq!(event.name(), "next");
}

// Async dynamic dispatch test
state_machine! {
    name: AsyncProcessor,
    dynamic: true,
    async: true,
    initial: Idle,
    states: [Idle, Processing, Done],
    events {
        start {
            transition: { from: Idle, to: Processing }
        }
        finish {
            transition: { from: Processing, to: Done }
        }
    }
}

#[test]
fn test_async_dynamic_dispatch() {
    use pollster::block_on;

    block_on(async {
        let mut processor = DynamicAsyncProcessor::new(());
        assert_eq!(processor.current_state(), "Idle");

        processor.handle(AsyncProcessorEvent::Start).await.unwrap();
        assert_eq!(processor.current_state(), "Processing");

        processor.handle(AsyncProcessorEvent::Finish).await.unwrap();
        assert_eq!(processor.current_state(), "Done");
    });
}

// Guard failure test
use std::sync::atomic::{AtomicBool, Ordering};

static GUARD_ALLOWED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: Guarded,
    dynamic: true,
    initial: Start,
    states: [Start, End],
    events {
        proceed {
            guards: [is_allowed],
            transition: { from: Start, to: End }
        }
    }
}

impl<C, S> Guarded<C, S> {
    fn is_allowed(&self, _ctx: &C) -> bool {
        GUARD_ALLOWED.load(Ordering::SeqCst)
    }
}

#[test]
fn test_guard_failure() {
    use state_machines::DynamicError;

    GUARD_ALLOWED.store(false, Ordering::SeqCst);
    let mut machine = DynamicGuarded::new(());

    let result = machine.handle(GuardedEvent::Proceed);
    assert!(result.is_err());

    match result.unwrap_err() {
        DynamicError::GuardFailed { guard, event } => {
            assert_eq!(guard, "is_allowed");
            assert_eq!(event, "proceed"); // Event name is snake_case
        }
        _ => panic!("Expected GuardFailed error"),
    }

    // Machine stays in Start state after guard failure
    assert_eq!(machine.current_state(), "Start");

    // Now allow the guard to pass
    GUARD_ALLOWED.store(true, Ordering::SeqCst);
    machine.handle(GuardedEvent::Proceed).unwrap();
    assert_eq!(machine.current_state(), "End");
}

// State storage test - demonstrates accessing and mutating state data
#[derive(Debug, Clone, Default, PartialEq)]
struct CounterData {
    count: u32,
}

state_machine! {
    name: Counter,
    dynamic: true,
    initial: Stopped,
    states: [
        Stopped,
        Running(CounterData),
    ],
    events {
        start {
            transition: { from: Stopped, to: Running }
        }
        stop {
            transition: { from: Running, to: Stopped }
        }
    }
}

#[test]
fn test_dynamic_state_data_accessors() {
    use state_machines::DynamicError;

    let mut counter = DynamicCounter::new(());

    // No data in Stopped state
    assert!(counter.running_data().is_none());
    assert!(counter.running_data_mut().is_none());

    // Transition to Running
    counter.handle(CounterEvent::Start).unwrap();
    assert_eq!(counter.current_state(), "Running");

    // Set data using setter
    counter
        .set_running_data(CounterData { count: 42 })
        .unwrap();

    // Read data
    assert_eq!(counter.running_data().unwrap().count, 42);

    // Mutate data via mutable accessor
    counter.running_data_mut().unwrap().count += 1;
    assert_eq!(counter.running_data().unwrap().count, 43);

    // Mutate again
    if let Some(data) = counter.running_data_mut() {
        data.count = 100;
    }
    assert_eq!(counter.running_data().unwrap().count, 100);

    // Transition back to Stopped
    counter.handle(CounterEvent::Stop).unwrap();
    assert_eq!(counter.current_state(), "Stopped");

    // Data accessors return None after transition
    assert!(counter.running_data().is_none());
    assert!(counter.running_data_mut().is_none());

    // Try to set data when in wrong state
    let result = counter.set_running_data(CounterData { count: 99 });
    assert!(result.is_err());

    match result.unwrap_err() {
        DynamicError::WrongState {
            expected,
            actual,
            operation,
        } => {
            assert_eq!(expected, "Running");
            assert_eq!(actual, "Stopped");
            assert_eq!(operation, "set_running_data");
        }
        _ => panic!("Expected WrongState error"),
    }
}

#[test]
fn test_dynamic_state_data_with_typestate_conversion() {
    let mut counter = DynamicCounter::new(());

    // Transition and set data
    counter.handle(CounterEvent::Start).unwrap();
    counter
        .set_running_data(CounterData { count: 50 })
        .unwrap();

    // Convert to typestate
    let typed = counter.into_running().unwrap();

    // Access data via typestate-specific accessor (guaranteed non-null)
    assert_eq!(typed.running_data().count, 50);

    // Convert back to dynamic
    let mut dynamic = typed.into_dynamic();
    assert_eq!(dynamic.current_state(), "Running");
    assert_eq!(dynamic.running_data().unwrap().count, 50);

    // Mutate via dynamic accessor
    dynamic.running_data_mut().unwrap().count = 75;
    assert_eq!(dynamic.running_data().unwrap().count, 75);
}
