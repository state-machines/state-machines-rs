use state_machines::state_machine;

state_machine! {
    name: TrafficLight,
    initial: Red,
    dynamic: true,
    states: [Red, Yellow, Green],
    events {
        Next {
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
    let _ = typed_light.Next();
}

#[test]
fn test_event_enum() {
    let event = TrafficLightEvent::Next;
    assert_eq!(event.name(), "Next");
}

// Async dynamic dispatch test
state_machine! {
    name: AsyncProcessor,
    dynamic: true,
    async: true,
    initial: Idle,
    states: [Idle, Processing, Done],
    events {
        Start {
            transition: { from: Idle, to: Processing }
        }
        Finish {
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
        Proceed {
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
            assert_eq!(event, "Proceed");
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
