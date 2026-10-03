//! An exclusive statechart: local/external domains, child-first selection,
//! eventless boot and completion propagated through a final composite.
use state_machines::{CompletionEvent, state_machine};
use std::{assert_matches, cell::RefCell};

#[derive(Debug, Default)]
pub struct Audit {
    allow_child: bool,
    release: bool,
    log: RefCell<Vec<&'static str>>,
}
#[derive(Debug, Default)]
struct CycleData {
    samples: usize,
}
state_machine! {
    name: Checkout, context: Audit, dynamic: true, initial: Offline,
    states: [
        Offline, Preparing,
        superstate Operational(CycleData) {
            superstate Diagnostics { state Checking, state Checked },
            state Monitoring,
        },
        Finished,
    ],
    final_states: [Checked, Diagnostics, Finished],
    lifecycle: {
        // Declaration order is not completion order.
        Operational { enter: [entered], exit: [exited], complete: [operational_done] },
        Diagnostics { complete: [diagnostics_done] },
        Finished { complete: [machine_done] },
    },
    events {
        start { transition: { from: Offline, to: Preparing } }
        prepared { automatic: true, transition: { from: Preparing, to: Operational } }
        route {
            hierarchical: true,
            // Parent first in the declaration, child first in selection.
            transition: { from: Operational, to: Monitoring, kind: local }
            transition: { from: Checking, to: Checked, guards: [child_enabled] }
        }
        heartbeat { transition: { from: Operational, internal: true } }
        reset { transition: { from: Operational, to: Operational, kind: external } }
        released {
            completion: Operational,
            transition: { from: Operational, to: Finished, guards: [release_enabled] }
        }
    }
}
impl<S> Checkout<S> {
    fn child_enabled(&self, ctx: &Audit) -> bool {
        ctx.allow_child
    }
    fn release_enabled(&self, ctx: &Audit) -> bool {
        ctx.release
    }
    fn entered(&self) {
        self.ctx.log.borrow_mut().push("enter");
    }
    fn exited(&self) {
        self.ctx.log.borrow_mut().push("exit");
    }
    fn diagnostics_done(&self) {
        self.ctx.log.borrow_mut().push("diagnostics");
    }
    fn operational_done(&self) {
        self.ctx.log.borrow_mut().push("operational");
    }
    fn machine_done(&self) {
        self.ctx.log.borrow_mut().push("machine");
    }
}

pub fn run_demo() {
    println!("\n=== Statechart domains and bottom-up completion ===");
    let mut machine = DynamicCheckout::new(Audit::default());
    machine.handle(CheckoutEvent::Start).unwrap();
    assert_eq!(
        machine.current_state(),
        CheckoutState::Checking,
        "eventless boot settles before returning"
    );
    machine.operational_data_mut().unwrap().samples = 42;

    // Disabled child falls back to the parent without leaving Operational.
    machine.handle(CheckoutEvent::Route).unwrap();
    assert_eq!(machine.current_state(), CheckoutState::Monitoring);
    assert_eq!(machine.operational_data().unwrap().samples, 42);
    println!("Disabled child -> parent fallback; local transition retained 42 samples");

    // Change the policy using the public typestate extraction API. Reuse the
    // same owned context, then demonstrate a real composite reset.
    let mut typed = machine.into_monitoring().unwrap();
    typed.ctx.allow_child = true;
    let mut machine = typed.into_dynamic();
    machine.handle(CheckoutEvent::Reset).unwrap();
    assert_eq!(machine.operational_data().unwrap().samples, 0);
    assert_matches!(machine.scope_epoch("Operational"), Some(1));
    machine.handle(CheckoutEvent::Route).unwrap();
    assert_eq!(machine.current_state(), CheckoutState::Checked);
    assert!(
        !machine.is_finished(),
        "non-final Operational waits for release"
    );
    assert_eq!(
        machine.take_completion_events(),
        [
            CompletionEvent::Superstate("Diagnostics"),
            CompletionEvent::Superstate("Operational"),
        ]
    );

    machine.handle(CheckoutEvent::Heartbeat).unwrap();
    assert!(
        machine.take_completion_events().is_empty(),
        "internal transitions do not repeat completion"
    );
    let mut typed = machine.into_checked().unwrap();
    typed.ctx.release = true;
    let mut machine = typed.into_dynamic();
    assert_matches!(machine.stabilize(8), Ok(1));
    assert!(machine.is_finished());
    assert_eq!(machine.take_completion_events(), [CompletionEvent::Machine]);
    let machine = machine.into_finished().unwrap();
    assert_eq!(
        *machine.ctx.log.borrow(),
        [
            "enter",
            "exit",
            "enter",
            "diagnostics",
            "operational",
            "exit",
            "machine",
        ]
    );
    println!("Child handled first; Diagnostics -> Operational -> machine completed");
}
