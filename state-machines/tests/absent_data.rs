use state_machines::state_machine;

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Owned(std::string::String);

state_machine! {
    name: InitiallyEmpty, dynamic: true, snapshot: true,
    initial: Empty, states: [Empty(Owned)],
    events { ping { transition: { from: Empty, internal: true } } }
}

#[test]
fn non_default_non_clone_initial_data_is_fallible() {
    let mut machine = InitiallyEmpty::new(());
    assert!(machine.empty_data().is_none());
    assert!(machine.empty_data_mut().is_none());
    let mut machine = machine.into_dynamic();
    machine.set_empty_data(Owned("resource".into())).unwrap();
    assert_eq!(
        machine.into_empty().unwrap().empty_data().unwrap().0,
        "resource"
    );
}

#[test]
#[cfg(feature = "serde")]
fn restoring_missing_active_data_does_not_panic() {
    let snapshot = DynamicInitiallyEmpty::new(()).into_snapshot();
    let mut machine = DynamicInitiallyEmpty::from_snapshot(snapshot)
        .unwrap()
        .into_empty()
        .unwrap();
    assert!(machine.empty_data().is_none());
    assert!(machine.empty_data_mut().is_none());
}
