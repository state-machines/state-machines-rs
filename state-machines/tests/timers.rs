#![cfg(feature = "runtime")]
use state_machines::{
    runtime::{Clock, ClockError, Runner, ScheduleError},
    state_machine,
};
use std::{cell::Cell, rc::Rc};

struct VirtualClock(Cell<u64>);
impl Clock for VirtualClock {
    fn now(&self) -> u64 {
        self.0.get()
    }
}
#[derive(Debug)]
pub struct Resource(Rc<Cell<usize>>);
impl Drop for Resource {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
state_machine! {
    name: Timed, dynamic: true, initial: Waiting, states: [Waiting, Done],
    events {
        heartbeat { transition: { from: Waiting, internal: true } }
        reset { transition: { from: Waiting, to: Waiting } }
        timeout { payload: Resource, transition: { from: Waiting, to: Done } }
    }
}
fn resource(drops: &Rc<Cell<usize>>) -> TimedEvent {
    TimedEvent::Timeout(Resource(drops.clone()))
}
#[test]
fn due_delivery_is_ordered_and_internal_heartbeat_preserves_lease() {
    let clock = VirtualClock(Cell::new(10));
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(DynamicTimed::new(()), 3);
    runner.schedule_after(&clock, 5, resource(&drops)).unwrap();
    runner.enqueue(TimedEvent::Heartbeat).unwrap();
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(1));
    assert_eq!(runner.next_deadline(), Some(15));
    clock.0.set(14);
    assert_eq!(runner.tick(&clock), Ok(0));
    clock.0.set(15);
    assert_eq!(runner.tick(&clock), Ok(1));
    assert_eq!(runner.tick(&clock), Ok(0));
    assert_eq!(runner.pending(), 1);
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.machine().current_state(), TimedState::Done);
    assert_eq!(drops.get(), 1);
}
#[test]
fn queued_timeout_is_rejected_after_external_self_reentry() {
    let clock = VirtualClock(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(DynamicTimed::new(()), 2);
    runner.enqueue(TimedEvent::Reset).unwrap();
    runner.schedule_after(&clock, 0, resource(&drops)).unwrap();
    assert_eq!(runner.tick(&clock), Ok(1));
    assert_eq!(pollster::block_on(runner.drain(2)), Ok(2));
    assert_eq!(runner.machine().current_state(), TimedState::Waiting);
    assert_eq!(runner.pending(), 0);
    assert_eq!(drops.get(), 1);
}
#[test]
fn external_exit_cancels_unqueued_and_deferred_timers() {
    let clock = VirtualClock(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(DynamicTimed::new(()), 3);
    runner.defer_in(TimedState::Waiting, |e| matches!(e, TimedEvent::Timeout(_)));
    runner.schedule_after(&clock, 0, resource(&drops)).unwrap();
    runner
        .schedule_after(&clock, 100, resource(&drops))
        .unwrap();
    runner.tick(&clock).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    runner.enqueue(TimedEvent::Reset).unwrap();
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(runner.deferred(), 0);
    assert_eq!(runner.next_deadline(), None);
    assert_eq!(runner.pending(), 0);
    assert_eq!(drops.get(), 2);
}
#[test]
fn cancellation_backpressure_and_errors_preserve_owned_payloads() {
    let clock = VirtualClock(Cell::new(0));
    let drops = Rc::new(Cell::new(0));
    let mut runner = Runner::new(DynamicTimed::new(()), 1);
    let id = runner.schedule_after(&clock, 10, resource(&drops)).unwrap();
    let rejected = runner
        .schedule_after(&clock, 1, resource(&drops))
        .unwrap_err();
    assert!(matches!(rejected, ScheduleError::Full(_)));
    assert_eq!(drops.get(), 0);
    drop(rejected.into_event());
    assert!(runner.cancel_timer(id));
    assert!(!runner.cancel_timer(id));
    assert_eq!(drops.get(), 2);
    assert_eq!(runner.pending(), 0);
    let id = runner.schedule_after(&clock, 0, resource(&drops)).unwrap();
    runner.tick(&clock).unwrap();
    assert!(runner.cancel_timer(id));
    assert_eq!(runner.pending(), 1);
    assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
    assert_eq!(drops.get(), 3);
    clock.0.set(u64::MAX);
    assert!(matches!(
        runner.schedule_after(&clock, 1, resource(&drops)),
        Err(ScheduleError::Overflow(_))
    ));
    runner.tick(&clock).unwrap();
    clock.0.set(1);
    assert_eq!(runner.tick(&clock), Err(ClockError::Backwards));
    assert!(matches!(
        runner.schedule_after(&clock, 1, resource(&drops)),
        Err(ScheduleError::Backwards(_))
    ));
}
#[test]
fn closing_runner_releases_queued_resources_even_with_live_sink() {
    let drops = Rc::new(Cell::new(0));
    let runner = Runner::new(DynamicTimed::new(()), 1);
    let sink = runner.sink();
    runner.enqueue(resource(&drops)).unwrap();
    drop(runner);
    assert_eq!(drops.get(), 1);
    assert_eq!(sink.pending(), 0);
}

#[test]
fn deadlines_then_registration_order_determine_delivery_order() {
    struct Recorder(Vec<u8>);
    impl state_machines::runtime::Machine for Recorder {
        type Event = u8;
        type Error = ();
        type State = ();
        fn state(&self) {}
        fn epoch(&self) -> u64 {
            0
        }
        fn is_finished(&self) -> bool {
            false
        }
        fn is_poisoned(&self) -> bool {
            false
        }
        async fn dispatch(&mut self, event: u8) -> Result<(), ()> {
            self.0.push(event);
            Ok(())
        }
    }
    let clock = VirtualClock(Cell::new(0));
    let mut runner = Runner::new(Recorder(Vec::new()), 4);
    runner.schedule_after(&clock, 20, 3).unwrap();
    runner.schedule_after(&clock, 10, 1).unwrap();
    runner.schedule_after(&clock, 10, 2).unwrap();
    runner.enqueue(0).unwrap();
    clock.0.set(20);
    runner.tick(&clock).unwrap();
    assert_eq!(pollster::block_on(runner.drain(4)), Ok(4));
    assert_eq!(runner.into_machine().0, vec![0, 1, 2, 3]);
}

mod partial_failure {
    use super::*;
    #[derive(Debug)]
    pub enum Failure {
        Rejected,
    }
    state_machine! {
        name: Partial, dynamic: true, error: Failure, initial: A, states: [A, B, End],
        events {
            begin { transition: { from: A, to: B } }
            advance {
                automatic: true,
                transition: { from: B, to: End, before: [reject] }
            }
        }
    }
    impl<Ctx, S> Partial<Ctx, S> {
        fn reject(&self) -> Result<(), Failure> {
            Err(Failure::Rejected)
        }
    }
    #[test]
    fn failed_microstep_cleans_up_a_committed_exit() {
        let clock = VirtualClock(Cell::new(0));
        let mut runner = Runner::new(DynamicPartial::new(()), 2);
        runner
            .schedule_after(&clock, 100, PartialEvent::Begin)
            .unwrap();
        runner.enqueue(PartialEvent::Begin).unwrap();
        assert!(pollster::block_on(runner.drain(1)).is_err());
        assert_eq!(runner.machine().current_state(), PartialState::B);
        assert_eq!(runner.next_deadline(), None);
        assert_eq!(runner.pending(), 0);
    }
}
