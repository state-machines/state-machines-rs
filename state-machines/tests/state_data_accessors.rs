use state_machines::state_machine;

#[derive(Default, Debug, Clone, PartialEq)]
struct ConfigData {
    version: u32,
}

#[derive(Default, Debug, Clone, PartialEq)]
struct ActiveData {
    connection_id: u64,
}

state_machine! {
    name: DataMachine,
    initial: Idle,
    states: [
        Idle,
        Configured(ConfigData),
        Active(ActiveData),
    ],
    events {
        configure {
            transition: { from: Idle, to: Configured }
        }
        activate {
            transition: { from: Configured, to: Active }
        }
    }
}

#[test]
fn state_specific_data_accessors_work() {
    let machine = DataMachine::new(());

    // Transition to Configured state
    let mut machine = machine.configure().expect("configure should work");

    // Generic accessors return Option (available on all states)
    // Data is now automatically initialized with Default::default()
    assert!(machine.state_data_configured().is_some());
    assert_eq!(machine.state_data_configured().unwrap().version, 0);

    // State-specific data() accessor provides guaranteed access
    let config_data: &ConfigData = machine.data();
    assert_eq!(config_data.version, 0);

    // Mutable accessor also works
    let config_data_mut: &mut ConfigData = machine.data_mut();
    config_data_mut.version = 42;

    // Verify the mutation worked
    assert_eq!(machine.data().version, 42);
    assert_eq!(machine.state_data_configured().unwrap().version, 42);

    // The key is that these methods ONLY exist on DataMachine<C, Configured>
    // and NOT on DataMachine<C, Idle> or DataMachine<C, Active>
    // This is enforced at compile time by the typestate pattern
}

#[test]
fn data_persists_across_transitions() {
    let machine = DataMachine::new(());
    let mut machine = machine.configure().expect("configure");

    // Modify the data
    machine.data_mut().version = 100;
    assert_eq!(machine.data().version, 100);

    // Transition to Active state
    let machine = machine.activate().expect("activate");

    // Active state has its own data (initialized with default)
    assert_eq!(machine.data().connection_id, 0);

    // Configured data should be cleared (not in that state anymore)
    assert!(machine.state_data_configured().is_none());
    // Active data should be present
    assert!(machine.state_data_active().is_some());
}
