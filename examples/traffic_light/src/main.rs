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
    // Create dynamic machine in a caller-selected state
    // let mut light = DynamicTrafficLight::new_init_state((), TrafficLightState::Yellow);
    let mut light = DynamicTrafficLight::new(());

    let _a = TrafficLightState::Yellow;

    let _b = TrafficLightEvent::Next;

    let a = light.current_state();
    println!("{:?}", a);

    let c = light.get_available_events();
    println!("{:?}", c);

    // Runtime event dispatch
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Green);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Yellow);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Red);
}
