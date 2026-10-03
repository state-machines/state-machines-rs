use state_machines::{CompletionEvent, state_machine};
use std::{assert_matches, cell::RefCell};

#[derive(Debug, Default)]
pub struct Context {
    approved: bool,
    log: RefCell<Vec<&'static str>>,
}

state_machine! {
    name: DeepCompletion, dynamic: true, context: Context, initial: Working,
    states: [superstate Outer { superstate Inner { state Working, state Done } }, Finished],
    final_states: [Done, Inner, Finished],
    // Deliberately reverse declaration order: completion is bottom-up.
    lifecycle: { Outer { complete: [outer] }, Inner { complete: [inner] } },
    events {
        finish { transition: { from: Working, to: Done } }
        complete { completion: Outer, transition: { from: Outer, to: Finished, guards: [approved] } }
        ping { transition: { from: Outer, internal: true } }
    }
}
impl<S> DeepCompletion<S> {
    fn approved(&self, ctx: &Context) -> bool {
        ctx.approved
    }
    fn inner(&self) {
        self.ctx.log.borrow_mut().push("inner");
    }
    fn outer(&self) {
        self.ctx.log.borrow_mut().push("outer");
    }
}

#[test]
fn final_composite_propagates_to_ancestor_and_guard_can_delay_exit() {
    let mut machine = DynamicDeepCompletion::new(Context::default());
    machine.handle(DeepCompletionEvent::Finish).unwrap();
    assert_eq!(machine.current_state(), DeepCompletionState::Done);
    assert!(
        !machine.is_finished(),
        "nonfinal Outer stops root propagation"
    );
    assert_eq!(
        machine.take_completion_events(),
        [
            CompletionEvent::Superstate("Inner"),
            CompletionEvent::Superstate("Outer"),
        ]
    );
    machine.handle(DeepCompletionEvent::Ping).unwrap();
    assert!(
        machine.take_completion_events().is_empty(),
        "internal edges don't complete twice"
    );
    let mut typed = machine.into_done().unwrap();
    assert_eq!(*typed.ctx.log.borrow(), ["inner", "outer"]);
    typed.ctx.approved = true;
    let mut machine = typed.into_dynamic();
    assert_matches!(machine.stabilize(2), Ok(1));
    assert!(machine.is_finished());
    assert_eq!(machine.take_completion_events(), [CompletionEvent::Machine]);
    #[cfg(feature = "inspect")]
    assert!(DeepCompletion::<Working>::schema().validate().is_empty());
}

mod root {
    use super::*;
    state_machine! {
        name: TerminalTree, dynamic: true, initial: Busy,
        states: [superstate Root { superstate Child { state Busy, state Final } }],
        final_states: [Final, Child, Root],
        events { finish { transition: { from: Busy, to: Final } } }
    }
    #[test]
    fn root_completes_only_when_its_declared_final_subtree_completes() {
        let machine = TerminalTree::new(()).finish().unwrap();
        assert!(machine.is_finished());
        assert_eq!(
            machine.completion_events(),
            [
                CompletionEvent::Superstate("Child"),
                CompletionEvent::Superstate("Root"),
                CompletionEvent::Machine,
            ]
        );
        let mut machine = DynamicTerminalTree::new(());
        machine.handle(TerminalTreeEvent::Finish).unwrap();
        assert!(machine.is_finished());
        assert_eq!(
            machine.take_completion_events(),
            machine.completion_events()
        );
        let mut machine = machine.into_final().unwrap().into_dynamic();
        assert!(machine.is_finished());
        assert!(
            machine.take_completion_events().is_empty(),
            "conversion is inert"
        );
        #[cfg(feature = "inspect")]
        assert!(TerminalTree::<(), Busy>::schema().validate().is_empty());
    }
}
