use state_machines::state_machine;
use std::cell::RefCell;
#[derive(Debug, Default)]
pub struct Log(RefCell<Vec<&'static str>>);
#[derive(Debug, Default)]
struct RegionData(u32);
state_machine! {
    name: Composite, dynamic: true, context: Log, error: &'static str, initial: Outside,
    states: [Outside, superstate Running(RegionData) { state A, state B }],
    lifecycle: { Running { enter: [enter], exit: [exit] } },
    events {
        begin { transition: { from: Outside, to: Running } }
        local { transition: { from: Running, to: B, kind: local } }
        reset { transition: { from: Running, to: Running, kind: external } }
        broken { after: [reject], transition: { from: Running, to: Running, kind: external } }
        legacy { transition: { from: Running, to: Running } }
    }
}
impl<S> Composite<S> {
    fn enter(&self) {
        self.ctx.0.borrow_mut().push("enter");
    }
    fn exit(&self) {
        self.ctx.0.borrow_mut().push("exit");
    }
    fn reject(&self) -> Result<(), &'static str> {
        Err("after")
    }
}
#[test]
fn local_preserves_parent_but_external_resets_it() {
    let mut machine = Composite::new(Log::default()).begin().unwrap();
    machine.running_data_mut().unwrap().0 = 42;
    let machine = machine.local().unwrap();
    assert_eq!(machine.running_data().unwrap().0, 42);
    assert_eq!(*machine.ctx.0.borrow(), ["enter"]);
    let (machine, _) = machine.broken().unwrap_err();
    assert_eq!(
        machine.running_data().unwrap().0,
        42,
        "rollback preserves old data"
    );
    let machine = machine.reset().unwrap();
    assert_eq!(machine.running_data().unwrap().0, 0);
    assert_eq!(
        *machine.ctx.0.borrow(),
        ["enter", "exit", "enter", "exit", "enter"]
    );
}
#[test]
fn legacy_semantics_and_dynamic_dispatch_remain_consistent() {
    let mut machine = Composite::new(Log::default()).begin().unwrap();
    machine.running_data_mut().unwrap().0 = 8;
    let machine = machine.legacy().unwrap();
    assert_eq!(machine.running_data().unwrap().0, 8);
    let mut machine = machine.into_dynamic();
    machine.handle(CompositeEvent::Reset).unwrap();
    assert_eq!(machine.running_data().unwrap().0, 0);
}
