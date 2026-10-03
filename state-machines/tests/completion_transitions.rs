use state_machines::{CompletionEvent, state_machine};
use std::cell::RefCell;
#[derive(Debug, Default)]
pub struct Log(RefCell<Vec<&'static str>>);
state_machine! {
    name: Workflow, dynamic: true, context: Log, initial: Work,
    states: [
        superstate Outer {
            superstate Inner { state Work, state InnerDone },
            state OuterDone,
        },
        Finished, Cancelled,
    ],
    final_states: [InnerDone, OuterDone, Finished],
    lifecycle: { Inner { complete: [inner_complete] }, Outer { complete: [outer_complete] } },
    events {
        finish { transition: { from: Work, to: InnerDone } }
        done_inner { completion: Inner, transition: { from: Inner, to: OuterDone } }
        done_outer { completion: Outer, transition: { from: Outer, to: Finished } }
        cancel { transition: { from: Outer, to: Cancelled } }
    }
}
impl<S> Workflow<S> {
    fn inner_complete(&self) {
        self.ctx.0.borrow_mut().push("inner");
    }
    fn outer_complete(&self) {
        self.ctx.0.borrow_mut().push("outer");
    }
}
#[test]
fn nested_completion_advances_parents_in_one_macrostep() {
    let mut machine = DynamicWorkflow::new(Log::default());
    assert!(
        machine.handle(WorkflowEvent::DoneInner).is_err(),
        "no completion before a final child"
    );
    machine.handle(WorkflowEvent::Finish).unwrap();
    assert_eq!(machine.current_state(), WorkflowState::Finished);
    assert!(machine.is_finished());
    assert_eq!(
        machine.take_completion_events(),
        [
            CompletionEvent::Superstate("Inner"),
            CompletionEvent::Superstate("Outer"),
            CompletionEvent::Machine,
        ]
    );
    assert_eq!(
        *machine.into_finished().unwrap().ctx.0.borrow(),
        ["inner", "outer"]
    );
}
#[test]
fn parent_exits_are_legal_from_a_final_child() {
    let machine = Workflow::new(Log::default()).finish().unwrap();
    assert_eq!(
        machine.completion_events(),
        [CompletionEvent::Superstate("Inner")]
    );
    let machine = machine.cancel().unwrap().into_dynamic();
    assert_eq!(machine.current_state(), WorkflowState::Cancelled);
    #[cfg(feature = "inspect")]
    assert!(
        !Workflow::<Work>::schema()
            .validate()
            .iter()
            .any(|d| d.level == state_machines::DiagnosticLevel::Error)
    );
}
#[test]
fn typed_completion_methods_can_be_chained_explicitly() {
    let machine = Workflow::new(Log::default())
        .finish()
        .unwrap()
        .done_inner()
        .unwrap()
        .done_outer()
        .unwrap();
    assert!(machine.is_finished());
}

#[cfg(feature = "inspect")]
mod unary {
    use super::*;
    state_machine! {
        name: Unary, dynamic: true, initial: Working,
        states: [superstate Outer { superstate Inner { state Working, state Done } }, Finished],
        final_states: [Done, Finished],
        events {
            finish { transition: { from: Working, to: Done } }
            complete { completion: Inner, transition: { from: Inner, to: Finished } }
        }
    }
    #[test]
    fn schema_retains_parent_identity_for_equal_descendant_sets() {
        let schema = Unary::<(), Working>::schema();
        assert_eq!(
            schema
                .superstates
                .iter()
                .find(|s| s.name == "Inner")
                .unwrap()
                .parent
                .as_deref(),
            Some("Outer")
        );
        assert!(schema.validate().is_empty());
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use super::*;
    state_machine! {
        name: AsyncCompletion, dynamic: true, async: true, context: bool, initial: Work,
        states: [superstate Parent { state Work, state Done }, Next],
        final_states: [Done],
        events {
            finish { transition: { from: Work, to: Done } }
            advance { completion: Parent, transition: { from: Parent, to: Next, guards: [approved] } }
        }
    }
    impl<S> AsyncCompletion<S> {
        async fn approved(&self, ctx: &bool) -> bool {
            *ctx
        }
    }
    #[test]
    fn disabled_completion_waits_and_can_be_resumed() {
        let mut machine = DynamicAsyncCompletion::new(false);
        pollster::block_on(machine.handle(AsyncCompletionEvent::Finish)).unwrap();
        assert_eq!(machine.current_state(), AsyncCompletionState::Done);
        assert_eq!(
            machine.take_completion_events(),
            [CompletionEvent::Superstate("Parent")]
        );
        let mut typed = machine.into_done().unwrap();
        typed.ctx = true;
        let mut machine = typed.into_dynamic();
        assert_eq!(pollster::block_on(machine.stabilize(2)), Ok(1));
        assert_eq!(machine.current_state(), AsyncCompletionState::Next);
    }
}
