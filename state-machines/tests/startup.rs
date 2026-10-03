use state_machines::state_machine;
use std::assert_matches;
use std::cell::RefCell;

#[derive(Debug, Default)]
struct Context {
    log: RefCell<Vec<&'static str>>,
    fail: bool,
}
#[derive(Debug)]
struct Resource(String);
#[derive(Debug)]
pub struct Request(Option<Resource>);
state_machine! {
    name: Startup, dynamic: true, context: Context, error: &'static str,
    initial: Idle,
    states: [superstate Region { state Idle, state Active(Resource) }],
    lifecycle: { Region { enter: [enter_region] }, Idle { enter: [enter_idle] } },
    events {
        activate { payload: Request, transition: { from: Idle, to: Active, data: resource } }
    }
}
impl<S> Startup<S> {
    fn enter_region(&self) {
        self.ctx.log.borrow_mut().push("region");
    }
    fn enter_idle(&self) -> Result<(), &'static str> {
        self.ctx.log.borrow_mut().push("idle");
        if self.ctx.fail {
            Err("startup")
        } else {
            Ok(())
        }
    }
    fn resource(&self, request: &mut Request) -> Resource {
        request.0.take().unwrap()
    }
}
#[test]
fn startup_is_explicit_ordered_and_fallible() {
    let machine = Startup::new(Context::default());
    assert!(machine.ctx.log.borrow().is_empty());
    let machine = machine.initialize().unwrap();
    assert_eq!(*machine.ctx.log.borrow(), ["region", "idle"]);
    let (machine, error) = Startup::new(Context {
        fail: true,
        ..Context::default()
    })
    .initialize()
    .unwrap_err();
    assert_eq!(*machine.ctx.log.borrow(), ["region", "idle"]);
    assert_matches!(error, state_machines::EventError::Callback(_));
    assert!(
        !DynamicStartup::initialize(Context::default())
            .unwrap()
            .is_poisoned()
    );
}
#[test]
fn entry_moves_a_non_default_non_clone_resource() {
    let machine = Startup::new(Context::default())
        .activate(Request(Some(Resource("owned".into()))))
        .unwrap();
    assert_eq!(machine.active_data().unwrap().0, "owned");
    let machine = machine.with_active_data(Resource("replacement".into()));
    assert_eq!(machine.active_data().unwrap().0, "replacement");
}

#[cfg(feature = "async")]
mod asynchronous {
    use super::*;
    state_machine! {
        name: AsyncStartup, dynamic: true, async: true,
        initial: Initial, states: [Initial],
        lifecycle: { Initial { enter: [enter] } },
        events { tick { transition: { from: Initial, internal: true } } }
    }
    impl<C, S> AsyncStartup<C, S> {
        async fn enter(&self) {}
    }
    #[test]
    fn async_startup_works_in_both_modes() {
        pollster::block_on(AsyncStartup::new(()).initialize()).unwrap();
        assert!(
            !pollster::block_on(DynamicAsyncStartup::initialize(()))
                .unwrap()
                .is_poisoned()
        );
    }
}
