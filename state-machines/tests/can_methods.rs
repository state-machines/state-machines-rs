#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use state_machines::state_machine;

static HATCH_SEALED: AtomicBool = AtomicBool::new(false);
static ALARM_ACTIVE: AtomicBool = AtomicBool::new(false);
static SELECTIONS: AtomicUsize = AtomicUsize::new(0);
static FAILURES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct Cargo {
    mass_kg: u32,
}

state_machine! {
    name: CargoBay,
    dynamic: true,
    initial: Open,
    states: [Open, Sealed, Loaded],
    events {
        seal {
            guards: [hatch_sealed],
            unless: [alarm_active],
            transition: { from: Open, to: Sealed }
        }
        load {
            payload: Cargo,
            guards: [cargo_fits],
            transition: { from: Sealed, to: Loaded }
        }
        reopen {
            transition: { from: Sealed, to: Open }
        }
        route {
            branching: true,
            payload: Cargo,
            guards: [nonempty],
            on_error: [record_failure],
            transition: { from: Open, to: Sealed, guards: [fits_once] }
            transition: { from: Open, to: Loaded, fallback: true }
        }
        classify {
            branching: true,
            payload: Cargo,
            on_error: [record_failure],
            transition: { from: Open, to: Sealed, guards: [cargo_fits] }
            transition: { from: Open, to: Loaded, guards: [heavy] }
        }
    }
}

impl<C, S> CargoBay<C, S> {
    fn record_failure(&self, _error: &state_machines::core::GuardError) {
        FAILURES.fetch_add(1, Ordering::SeqCst);
    }
    fn nonempty(&self, _ctx: &C, cargo: &Cargo) -> bool {
        cargo.mass_kg > 0
    }
    fn heavy(&self, _ctx: &C, cargo: &Cargo) -> bool {
        cargo.mass_kg >= 2000
    }
    fn fits_once(&self, ctx: &C, cargo: &Cargo) -> bool {
        SELECTIONS.fetch_add(1, Ordering::SeqCst);
        self.cargo_fits(ctx, cargo)
    }
    fn hatch_sealed(&self, _ctx: &C) -> bool {
        HATCH_SEALED.load(Ordering::SeqCst)
    }

    fn alarm_active(&self, _ctx: &C) -> bool {
        ALARM_ACTIVE.load(Ordering::SeqCst)
    }

    fn cargo_fits(&self, _ctx: &C, cargo: &Cargo) -> bool {
        cargo.mass_kg <= 1000
    }
}

#[test]
fn can_methods_evaluate_guards_without_consuming() {
    HATCH_SEALED.store(false, Ordering::SeqCst);
    ALARM_ACTIVE.store(false, Ordering::SeqCst);

    let bay = CargoBay::new(());

    // Guard fails -> can_seal is false, machine untouched
    assert!(!bay.can_seal());

    // Guard passes -> true
    HATCH_SEALED.store(true, Ordering::SeqCst);
    assert!(bay.can_seal());

    // unless condition blocks even when the guard passes
    ALARM_ACTIVE.store(true, Ordering::SeqCst);
    assert!(!bay.can_seal());
    ALARM_ACTIVE.store(false, Ordering::SeqCst);

    // Predicate agreed with the real transition
    let bay = bay.seal().expect("seal succeeds when can_seal is true");

    // Unguarded events are always possible from their source state
    assert!(bay.can_reopen());

    // Payload guards receive the payload by reference
    assert!(bay.can_load(&Cargo { mass_kg: 500 }));
    assert!(!bay.can_load(&Cargo { mass_kg: 5000 }));

    let _bay = bay.load(Cargo { mass_kg: 500 }).expect("load fits");

    SELECTIONS.store(0, Ordering::SeqCst);
    let outcome = CargoBay::new(()).route(Cargo { mass_kg: 500 }).unwrap();
    let CargoBayOpenRouteOutcome::Sealed(bay) = outcome else {
        panic!("first matching candidate must win");
    };
    assert_eq!(
        SELECTIONS.load(Ordering::SeqCst),
        1,
        "selection is not re-evaluated"
    );
    let _bay = bay.reopen().unwrap();

    let outcome = CargoBay::new(()).route(Cargo { mass_kg: 5000 }).unwrap();
    let CargoBayOpenRouteOutcome::Loaded(_) = outcome else {
        panic!("fallback");
    };
    let (bay, error) = CargoBay::new(())
        .classify(Cargo { mass_kg: 1500 })
        .unwrap_err();
    assert_eq!(error.guard, "branch_selection");
    assert!(!bay.can_classify(&Cargo { mass_kg: 1500 }));
    let (_, error) = bay.route(Cargo { mass_kg: 0 }).unwrap_err();
    assert_eq!(error.guard, "nonempty");
    assert_eq!(FAILURES.load(Ordering::SeqCst), 2);

    let mut bay = DynamicCargoBay::new(());
    assert!(!bay.is_available_event(&CargoBayEvent::Route(Cargo { mass_kg: 0 })));
    bay.handle(CargoBayEvent::Route(Cargo { mass_kg: 5000 }))
        .unwrap();
    assert_eq!(bay.current_state(), CargoBayState::Loaded);
    assert!(
        bay.handle(CargoBayEvent::Route(Cargo { mass_kg: 1 }))
            .is_err()
    );
    assert_eq!(
        FAILURES.load(Ordering::SeqCst),
        2,
        "invalid events do not run transition hooks"
    );
}
