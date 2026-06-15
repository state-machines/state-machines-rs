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

    // Check initial state. Keep the concrete type explicit so public API
    // changes are caught at compile time.
    let state: TrafficLightState = light.current_state();
    assert_eq!(state, TrafficLightState::Red);

    // Trigger events dynamically
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Green);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Yellow);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Red);
}

#[test]
fn test_get_available_events_tracks_current_state() {
    let mut light = DynamicTrafficLight::new(());

    let events = light.get_available_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name(), "next");

    light.handle(TrafficLightEvent::Next).unwrap();
    let events = light.get_available_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name(), "next");
}

#[test]
fn test_dynamic_dispatch_with_selected_initial_state() {
    let mut light = DynamicTrafficLight::new_init_state((), TrafficLightState::Yellow);

    assert_eq!(light.current_state(), TrafficLightState::Yellow);
    assert_eq!(light.current_state().name(), "Yellow");
    assert_eq!(light.current_state().to_string(), "Yellow");
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Red);
}

// Regression coverage for dynamic construction with state-associated data on
// the declared initial state. `DynamicInitialData::new(ctx)` must keep working
// even when the associated data does not implement `Default`, because the
// existing constructor initializes state storage as `None`.
#[derive(Debug)]
struct InitialDataWithoutDefault {
    _value: u8,
}

state_machine! {
    name: InitialData,
    dynamic: true,
    initial: InitialDataReady,
    states: [
        InitialDataReady(InitialDataWithoutDefault),
        InitialDataDone,
    ],
    events {
        finish {
            transition: { from: InitialDataDone, to: InitialDataDone }
        }
    }
}

#[test]
fn test_dynamic_new_allows_non_default_initial_state_data() {
    let machine = DynamicInitialData::new(());

    assert_eq!(machine.current_state(), InitialDataState::InitialDataReady);
    assert!(machine.initial_data_ready_data().is_none());
}

#[test]
fn test_typestate_to_dynamic_conversion() {
    // Start in typestate mode
    let light = TrafficLight::new(());

    // Convert to dynamic
    let mut dynamic_light = light.into_dynamic();
    assert_eq!(dynamic_light.current_state(), TrafficLightState::Red);

    // Use dynamic dispatch
    dynamic_light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(dynamic_light.current_state(), TrafficLightState::Green);
}

#[test]
fn test_dynamic_to_typestate_conversion() {
    // Start in dynamic mode
    let mut light = DynamicTrafficLight::new(());

    // Transition to Green
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Green);

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
#[cfg(feature = "async")]
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

#[cfg(feature = "async")]
#[test]
fn test_async_dynamic_dispatch() {
    use pollster::block_on;

    block_on(async {
        let mut processor = DynamicAsyncProcessor::new(());
        assert_eq!(processor.current_state(), AsyncProcessorState::Idle);
        assert_eq!(processor.get_available_events().await[0].name(), "start");

        processor.handle(AsyncProcessorEvent::Start).await.unwrap();
        assert_eq!(processor.current_state(), AsyncProcessorState::Processing);
        assert_eq!(processor.get_available_events().await[0].name(), "finish");

        processor.handle(AsyncProcessorEvent::Finish).await.unwrap();
        assert_eq!(processor.current_state(), AsyncProcessorState::Done);
    });
}

// Guard failure test
use std::sync::atomic::{AtomicBool, Ordering};

static GUARD_ALLOWED: AtomicBool = AtomicBool::new(false);
static GUARD_BLOCKED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: Guarded,
    dynamic: true,
    initial: Start,
    states: [Start, End],
    events {
        proceed {
            guards: [is_allowed],
            unless: [is_blocked],
            transition: { from: Start, to: End }
        }
    }
}

impl<C, S> Guarded<C, S> {
    fn is_allowed(&self, _ctx: &C) -> bool {
        GUARD_ALLOWED.load(Ordering::SeqCst)
    }

    fn is_blocked(&self, _ctx: &C) -> bool {
        GUARD_BLOCKED.load(Ordering::SeqCst)
    }
}

#[test]
fn test_guard_failure() {
    use state_machines::DynamicError;

    GUARD_ALLOWED.store(false, Ordering::SeqCst);
    GUARD_BLOCKED.store(false, Ordering::SeqCst);
    let mut machine = DynamicGuarded::new(());
    assert!(machine.get_available_events().is_empty());

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
    assert_eq!(machine.current_state(), GuardedState::Start);

    // Now allow the guard to pass
    GUARD_ALLOWED.store(true, Ordering::SeqCst);
    GUARD_BLOCKED.store(true, Ordering::SeqCst);
    assert!(machine.get_available_events().is_empty());

    GUARD_BLOCKED.store(false, Ordering::SeqCst);
    let events = machine.get_available_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name(), "proceed");
    machine.handle(GuardedEvent::Proceed).unwrap();
    assert_eq!(machine.current_state(), GuardedState::End);
    assert!(machine.get_available_events().is_empty());
}

#[derive(Debug)]
pub struct Payload;

state_machine! {
    name: PayloadAvailability,
    dynamic: true,
    initial: Ready,
    states: [Ready, Submitted],
    events {
        submit {
            payload: Payload,
            transition: { from: Ready, to: Submitted }
        }
        cancel {
            transition: { from: Ready, to: Submitted }
        }
    }
}

#[test]
fn test_get_available_events_omits_payload_events() {
    let machine = DynamicPayloadAvailability::new(());
    let events = machine.get_available_events();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name(), "cancel");
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
    assert_eq!(counter.current_state(), CounterState::Running);

    // Set data using setter
    counter.set_running_data(CounterData { count: 42 }).unwrap();

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
    assert_eq!(counter.current_state(), CounterState::Stopped);

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
fn test_dynamic_selected_state_starts_with_empty_data() {
    // `new_init_state` mirrors `new()`: state-associated storage starts empty
    // (so data types need not implement `Default`). Callers populate it via the
    // generated setter once constructed.
    let mut counter = DynamicCounter::new_init_state((), CounterState::Running);

    assert_eq!(counter.current_state(), CounterState::Running);
    assert!(counter.running_data().is_none());

    counter.set_running_data(CounterData { count: 7 }).unwrap();
    assert_eq!(counter.running_data().unwrap().count, 7);
}

#[test]
fn test_dynamic_state_data_with_typestate_conversion() {
    let mut counter = DynamicCounter::new(());

    // Transition and set data
    counter.handle(CounterEvent::Start).unwrap();
    counter.set_running_data(CounterData { count: 50 }).unwrap();

    // Convert to typestate
    let typed = counter.into_running().unwrap();

    // Access data via typestate-specific accessor (guaranteed non-null)
    assert_eq!(typed.running_data().count, 50);

    // Convert back to dynamic
    let mut dynamic = typed.into_dynamic();
    assert_eq!(dynamic.current_state(), CounterState::Running);
    assert_eq!(dynamic.running_data().unwrap().count, 50);

    // Mutate via dynamic accessor
    dynamic.running_data_mut().unwrap().count = 75;
    assert_eq!(dynamic.running_data().unwrap().count, 75);
}
