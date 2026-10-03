use state_machines::state_machine;

#[derive(Debug)]
pub struct Configuration(Box<str>);
state_machine! {
    name: Configured,
    dynamic: true,
    context: Configuration,
    initial: Ready,
    states: [Ready, Done],
    final_states: [Done],
    events { finish { transition: { from: Ready, to: Done } } }
}

#[test]
fn a_concrete_owned_context_does_not_require_default_or_clone() {
    assert_eq!(ConfiguredState::default(), ConfiguredState::Ready);
    let machine = Configured::new(Configuration("explicit".into()));
    assert_eq!(machine.ctx.0.as_ref(), "explicit");
    let mut dynamic = machine.into_dynamic();
    dynamic.handle(ConfiguredEvent::Finish).unwrap();
    assert!(dynamic.is_finished());
}
