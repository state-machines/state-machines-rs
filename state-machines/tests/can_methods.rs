#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::state_machine;

static HATCH_SEALED: AtomicBool = AtomicBool::new(false);
static ALARM_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct Cargo {
    mass_kg: u32,
}

state_machine! {
    name: CargoBay,
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
    }
}

impl<C, S> CargoBay<C, S> {
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
}
