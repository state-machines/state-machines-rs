//! Keep the basic pressure-cycle demo simple, then add explicit startup and
//! entry factories for an owned, non-Clone/non-Default resource.
use state_machines::state_machine;
use std::cell::Cell;

#[derive(Debug)]
struct Seal(String);
#[derive(Debug)]
pub struct SealRequest(Option<String>);
#[derive(Debug, Default)]
pub struct Ledger {
    entries: Cell<usize>,
}

state_machine! {
    name: OwnedAirlock, context: Ledger, initial: Pressurized,
    states: [Pressurized(Seal), Vacuum],
    lifecycle: { Pressurized { enter: [inspect_seal] } },
    events {
        depressurize { transition: { from: Pressurized, to: Vacuum } }
        repressurize {
            payload: SealRequest,
            transition: { from: Vacuum, to: Pressurized, data: take_seal }
        }
    }
}
impl<S> OwnedAirlock<S> {
    fn take_seal(&self, request: &mut SealRequest) -> Seal {
        Seal(request.0.take().expect("one owned seal per entry"))
    }
}
impl OwnedAirlock<Pressurized> {
    fn inspect_seal(&self) {
        assert!(
            self.pressurized_data().is_some(),
            "install startup data first"
        );
        self.ctx.entries.set(self.ctx.entries.get() + 1);
    }
}

fn pressure_cycle() -> OwnedAirlock<Pressurized> {
    let machine = OwnedAirlock::new(Ledger::default());
    assert!(machine.pressurized_data().is_none());
    assert_eq!(machine.ctx.entries.get(), 0, "construction is inert");
    let machine = machine
        .with_pressurized_data(Seal("startup gasket".into()))
        .initialize()
        .unwrap();
    assert_eq!(machine.ctx.entries.get(), 1);
    let machine = machine
        .depressurize()
        .unwrap()
        .repressurize(SealRequest(Some("replacement gasket".into())))
        .unwrap();
    assert_eq!(machine.ctx.entries.get(), 2, "entry follows the factory");
    assert_eq!(machine.pressurized_data().unwrap().0, "replacement gasket");
    machine
}

pub fn run_demo() {
    let machine = pressure_cycle();
    println!("\n=== Explicit startup and owned entry ===");
    println!(
        "Installed {}, inspected {} entries without cloning the seal",
        machine.pressurized_data().unwrap().0,
        machine.ctx.entries.get(),
    );
}
