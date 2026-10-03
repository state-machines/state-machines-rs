use state_machines::state_machine;
use std::cell::Cell;
#[derive(Debug, Default)]
pub struct Policy {
    child: bool,
    checks: Cell<u32>,
}
state_machine! {
    name: Routing, dynamic: true, context: Policy, initial: A,
    states: [superstate Parent { state A, state B }, ChildHandled, ParentHandled],
    events {
        route {
            hierarchical: true,
            transition: { from: Parent, to: ParentHandled }
            transition: { from: A, to: ChildHandled, guards: [allow_child] }
        }
    }
}
impl<S> Routing<S> {
    fn allow_child(&self, ctx: &Policy) -> bool {
        ctx.checks.set(ctx.checks.get() + 1);
        ctx.child
    }
}
fn policy(child: bool) -> Policy {
    Policy {
        child,
        checks: Cell::new(0),
    }
}
#[test]
fn child_wins_regardless_of_declaration_order() {
    let RoutingARouteOutcome::ChildHandled(machine) = Routing::new(policy(true)).route().unwrap()
    else {
        panic!("parent must not shadow child");
    };
    assert_eq!(machine.ctx.checks.get(), 1);
}
#[test]
fn disabled_child_falls_back_to_parent() {
    let RoutingARouteOutcome::ParentHandled(machine) = Routing::new(policy(false)).route().unwrap()
    else {
        panic!("disabled child must fall back");
    };
    assert_eq!(machine.ctx.checks.get(), 1);
    let mut machine = DynamicRouting::new_init_state(policy(true), RoutingState::B);
    machine.handle(RoutingEvent::Route).unwrap();
    assert_eq!(machine.current_state(), RoutingState::ParentHandled);
}
#[test]
fn dynamic_and_schema_use_the_same_specificity_policy() {
    let mut machine = DynamicRouting::new(policy(true));
    machine.handle(RoutingEvent::Route).unwrap();
    assert_eq!(machine.current_state(), RoutingState::ChildHandled);
    #[cfg(feature = "inspect")]
    {
        let schema = Routing::<A>::schema();
        assert!(schema.events[0].hierarchical);
        assert!(
            !schema
                .validate()
                .iter()
                .any(|d| d.level == state_machines::DiagnosticLevel::Error)
        );
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use state_machines::state_machine;
    state_machine! {
        name: AsyncRouting, dynamic: true, async: true, initial: Child,
        states: [superstate Root { state Child }, Success, Fallback],
        events {
            choose { hierarchical: true,
                transition: { from: Root, to: Fallback }
                transition: { from: Child, to: Success, guards: [enabled] }
            }
        }
    }
    impl<C, S> AsyncRouting<C, S> {
        async fn enabled(&self, _ctx: &C) -> bool {
            true
        }
    }
    #[test]
    fn specificity_is_preserved_across_await() {
        let mut machine = DynamicAsyncRouting::new(());
        pollster::block_on(machine.handle(AsyncRoutingEvent::Choose)).unwrap();
        assert_eq!(machine.current_state(), AsyncRoutingState::Success);
    }
}
