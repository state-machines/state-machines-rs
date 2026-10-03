#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use state_machines::state_machine;

#[derive(Default, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct ConfigData {
    version: u32,
}

#[derive(Default, Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct ActiveData {
    connection_id: u64,
}

state_machine! {
    name: DataMachine,
    dynamic: true,
    snapshot: true,
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
        heartbeat {
            transition: { from: [Configured, Active], internal: true }
        }
        reconfigure {
            transition: { from: Configured, to: Configured }
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

    // State-specific data accessor provides guaranteed access (state-named method)
    let config_data: &ConfigData = machine.configured_data().unwrap();
    assert_eq!(config_data.version, 0);

    // Mutable accessor also works
    let config_data_mut: &mut ConfigData = machine.configured_data_mut().unwrap();
    config_data_mut.version = 42;

    // Verify the mutation worked
    assert_eq!(machine.configured_data().unwrap().version, 42);
    assert_eq!(machine.state_data_configured().unwrap().version, 42);
    let machine = machine.heartbeat().expect("internal transition");
    assert_eq!(machine.configured_data().unwrap().version, 42);
    let machine = machine.reconfigure().expect("external self-transition");
    assert_eq!(machine.configured_data().unwrap().version, 0);

    // The key is that these methods ONLY exist on DataMachine<C, Configured>
    // and NOT on DataMachine<C, Idle> or DataMachine<C, Active>
    // This is enforced at compile time by the typestate pattern
}

#[test]
fn data_persists_across_transitions() {
    let machine = DataMachine::new(());
    let mut machine = machine.configure().expect("configure");

    // Modify the data
    machine.configured_data_mut().unwrap().version = 100;
    assert_eq!(machine.configured_data().unwrap().version, 100);

    // Transition to Active state
    let machine = machine.activate().expect("activate");

    // Active state has its own data (initialized with default)
    assert_eq!(machine.active_data().unwrap().connection_id, 0);

    // Configured data should be cleared (not in that state anymore)
    assert!(machine.state_data_configured().is_none());
    // Active data should be present
    assert!(machine.state_data_active().is_some());
    let mut machine = machine.into_dynamic();
    machine.active_data_mut().unwrap().connection_id = 7;
    machine.handle(DataMachineEvent::Heartbeat).unwrap();
    assert_eq!(machine.active_data().unwrap().connection_id, 7);
    assert_eq!(machine.current_state(), DataMachineState::Active);

    #[cfg(feature = "serde")]
    {
        let json = serde_json::to_string(&machine.into_snapshot()).unwrap();
        let mut snapshot: DataMachineSnapshot<()> = serde_json::from_str(&json).unwrap();
        snapshot.version = 2;
        let (mut snapshot, error) = DynamicDataMachine::from_snapshot(snapshot).unwrap_err();
        assert_eq!(
            error,
            state_machines::SnapshotError::UnsupportedVersion {
                expected: 1,
                actual: 2
            }
        );
        snapshot.version = 1;
        snapshot.machine = "Other".into();
        let (mut snapshot, error) = DynamicDataMachine::from_snapshot(snapshot).unwrap_err();
        assert_eq!(error, state_machines::SnapshotError::WrongMachine);
        snapshot.machine = "DataMachine".into();
        snapshot.state = "Missing".into();
        let data = snapshot.__state_data_active.take();
        let (mut snapshot, error) = DynamicDataMachine::from_snapshot(snapshot).unwrap_err();
        assert_eq!(error, state_machines::SnapshotError::UnknownState);
        snapshot.state = "Active".into();
        snapshot.__state_data_active = data;
        snapshot.__state_data_configured = Some(ConfigData::default());
        let (mut snapshot, error) = DynamicDataMachine::from_snapshot(snapshot).unwrap_err();
        assert_eq!(
            error,
            state_machines::SnapshotError::InactiveData {
                state: "Configured"
            }
        );
        snapshot.__state_data_configured = None;
        let mut restored = DynamicDataMachine::from_snapshot(snapshot).unwrap();
        assert_eq!(restored.active_data().unwrap().connection_id, 7);
        assert_eq!(restored.take_completion_events(), []);
        restored.handle(DataMachineEvent::Heartbeat).unwrap();
        assert_eq!(restored.active_data().unwrap().connection_id, 7);
    }
}
