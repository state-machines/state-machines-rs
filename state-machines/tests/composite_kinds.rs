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
    const fn reject(&self) -> Result<(), &'static str> {
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

#[test]
fn dynamic_machine_trait_carries_the_declared_error_type() {
    fn assert_error<
        M: state_machines::DynamicMachine<Error = state_machines::DynamicError<&'static str>>,
    >() {
    }
    assert_error::<DynamicComposite>();
}

state_machine! {
    name: Nested, context: Log, initial: X,
    states: [superstate Outer { superstate Inner { state X, state Y }, state Z }],
    lifecycle: {
        Outer { enter: [enter_outer] },
        Inner { enter: [enter_inner], exit: [exit_inner] },
    },
    events {
        local { transition: { from: Outer, to: Y, kind: local } }
        legacy { transition: { from: Outer, to: X } }
    }
}
impl<S> Nested<S> {
    fn enter_outer(&self) {
        self.ctx.0.borrow_mut().push("enter_outer");
    }
    fn enter_inner(&self) {
        self.ctx.0.borrow_mut().push("enter_inner");
    }
    fn exit_inner(&self) {
        self.ctx.0.borrow_mut().push("exit_inner");
    }
}

#[test]
fn local_exits_every_active_state_below_its_source() {
    let machine = Nested::new(Log::default()).local().unwrap();
    assert_eq!(*machine.ctx.0.borrow(), ["exit_inner", "enter_inner"]);
    let machine = machine.legacy().unwrap();
    assert_eq!(*machine.ctx.0.borrow(), ["exit_inner", "enter_inner"]);
}
