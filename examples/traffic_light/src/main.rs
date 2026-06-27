//! Dynamic dispatch: the runtime state enum, `new_init_state`, and
//! `get_available_events`.
//!
//! `dynamic: true` generates a `Dynamic*` wrapper plus a `*State` enum so the
//! current state is a typed value (with `Display` and `name()`) rather than a
//! bare string.

use state_machines::state_machine;

state_machine! {
    name: TrafficLight,
    dynamic: true,
    initial: Red,
    states: [Red, Yellow, Green],
    events {
        next {
            transition: { from: Red, to: Green }
            transition: { from: Green, to: Yellow }
            transition: { from: Yellow, to: Red }
        }
    }
}

fn main() {
    // `new` starts in the declared initial state; `new_init_state` lets the
    // caller pick one (handy for restoring a persisted state).
    let mut light = DynamicTrafficLight::new_init_state((), TrafficLightState::Yellow);
    assert_eq!(light.current_state(), TrafficLightState::Yellow);

    // current_state() is a typed enum: Display + name() when you need text.
    println!("starting at {}", light.current_state());

    // Events enabled from the current state (after guards/unless).
    let available = light.get_available_events();
    println!("available: {:?}", available);

    let is_available = light.is_available_event(&available[0]);
    assert!(is_available);

    // Runtime event dispatch.
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Red);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Green);

    println!("ended at {}", light.current_state());

    let is_available = light.is_available_event(&TrafficLightEvent::Next);
    println!("is_available: {:?}", is_available);
}
