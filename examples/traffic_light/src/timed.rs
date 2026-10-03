//! Host-driven deadlines: color timers belong to leaves, while the operating
//! window belongs to their composite and survives sibling transitions.
use state_machines::{
    runtime::{Clock, Runner},
    state_machine,
};
use std::{
    assert_matches,
    cell::{Cell, RefCell},
};

#[derive(Debug, Default)]
pub struct Signals {
    crossed: RefCell<Vec<String>>,
    heartbeats: Cell<usize>,
}
// The request is owned and intentionally not Clone. A callback consumes its
// contents only when the deferred event is eventually dispatched.
#[derive(Debug)]
pub struct CrossingRequest(RefCell<Option<String>>);

state_machine! {
    name: Intersection, dynamic: true, context: Signals, initial: Red,
    states: [superstate Operating { state Red, state Green, state Yellow }, Off],
    runtime: {
        Operating { after: [{ delay: 60, event: close_window }] },
        Red { after: [{ delay: 5, event: next_color }] },
        Green { after: [{ delay: 10, event: next_color }] },
        Yellow { after: [{ delay: 2, event: next_color }], defer: [cross] },
    },
    events {
        next {
            transition: { from: Red, to: Green }
            transition: { from: Green, to: Yellow }
            transition: { from: Yellow, to: Red }
        }
        cross { payload: CrossingRequest, before: [record_crossing],
            transition: { from: Operating, internal: true } }
        heartbeat { before: [record_heartbeat],
            transition: { from: Operating, internal: true } }
        maintenance { transition: { from: Operating, to: Off } }
        reset { transition: { from: [Operating, Off], to: Operating, kind: external } }
    }
}
impl<S> Intersection<S> {
    fn close_window(&self) -> IntersectionEvent {
        IntersectionEvent::Maintenance
    }
    fn next_color(&self) -> IntersectionEvent {
        IntersectionEvent::Next
    }
    fn record_crossing(&self, request: &CrossingRequest) {
        self.ctx.crossed.borrow_mut().push(
            request
                .0
                .borrow_mut()
                .take()
                .expect("dispatch a crossing once"),
        );
    }
    fn record_heartbeat(&self) {
        self.ctx.heartbeats.set(self.ctx.heartbeats.get() + 1);
    }
}
struct Time(u64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0
    }
}
async fn advance(runner: &mut Runner<DynamicIntersection>, now: u64) {
    runner.tick(&Time(now)).unwrap();
    runner.drain(8).await.unwrap();
    println!("tick {now}: {}", runner.machine().current_state());
}

pub async fn run_demo() {
    println!("\n=== Declarative deadlines, deferral and composite reset ===");
    let mut runner = Runner::new(DynamicIntersection::new(Signals::default()), 8);
    // Constructors do not start clocks. No sleeps/executor timer dependency.
    runner.start(&Time(0)).unwrap();
    let visit = runner.machine().scope_epoch("Operating").unwrap();
    assert_eq!(runner.next_deadline(), Some(5));
    advance(&mut runner, 5).await;
    assert_eq!(runner.machine().current_state(), IntersectionState::Green);
    advance(&mut runner, 15).await;
    assert_eq!(runner.machine().current_state(), IntersectionState::Yellow);
    assert_eq!(runner.machine().scope_epoch("Operating"), Some(visit));

    // Raised internal events have priority; Yellow defers the owned crossing.
    runner
        .enqueue(IntersectionEvent::Cross(CrossingRequest(RefCell::new(
            Some("north pedestrian".into()),
        ))))
        .unwrap();
    runner.raise(IntersectionEvent::Heartbeat).unwrap();
    runner.drain(8).await.unwrap();
    assert_eq!(
        runner.pending(),
        3,
        "two timeouts plus one deferred request"
    );
    advance(&mut runner, 17).await;
    assert_eq!(runner.machine().current_state(), IntersectionState::Red);
    assert_eq!(runner.pending(), 2, "crossing recalled on leaving Yellow");

    // Reuse the same runner: a reset invalidates the old operating-window timer,
    // whereas the earlier sibling transitions retained it.
    runner.tick(&Time(18)).unwrap();
    runner.enqueue(IntersectionEvent::Reset).unwrap();
    runner.drain(8).await.unwrap();
    assert_matches!(runner.machine().scope_epoch("Operating"), Some(epoch) if epoch != visit);
    advance(&mut runner, 60).await;
    assert_eq!(
        runner.machine().current_state(),
        IntersectionState::Green,
        "the old parent timeout at 60 was cancelled by reset"
    );
    advance(&mut runner, 78).await;
    assert_eq!(runner.machine().current_state(), IntersectionState::Off);
    let machine = runner.into_machine().into_off().unwrap();
    assert_eq!(*machine.ctx.crossed.borrow(), ["north pedestrian"]);
    assert_eq!(machine.ctx.heartbeats.get(), 1);
    println!("Crossing delivered once; reset restarted the parent deadline.");
}
