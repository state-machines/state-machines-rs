#![cfg(all(feature = "runtime", not(feature = "runtime-send")))]
use state_machines::{
    runtime::{Machine, Parallel, ParallelError, ParallelEvent, Runner},
    state_machine,
};
use std::assert_matches;

#[derive(Debug)]
pub struct Resource(Option<String>);
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Data(String);
state_machine! {
    name: Region, dynamic: true, snapshot: true, initial: Pending,
    states: [Pending, Finished(Data)], final_states: [Finished],
    events {
        finish { payload: Resource, transition: { from: Pending, to: Finished, data: own } }
        heartbeat { transition: { from: Pending, internal: true } }
    }
}
impl<C, S> Region<C, S> {
    fn own(&self, payload: &mut Resource) -> Data {
        Data(payload.0.take().unwrap())
    }
}
fn finish(value: &str) -> RegionEvent {
    RegionEvent::Finish(Resource(Some(value.into())))
}
fn regions() -> Parallel<DynamicRegion<()>, DynamicRegion<()>> {
    Parallel::new(DynamicRegion::new(()), DynamicRegion::new(()))
}

#[test]
fn orthogonal_states_are_independent_and_join_is_emitted_once() {
    let mut machine = regions();
    assert_eq!(
        machine.current_state(),
        (RegionState::Pending, RegionState::Pending)
    );
    assert!(machine.take_join().is_none());
    pollster::block_on(machine.handle(ParallelEvent::Left(finish("left")))).unwrap();
    assert_eq!(
        machine.current_state(),
        (RegionState::Finished, RegionState::Pending)
    );
    assert!(!machine.is_finished());
    assert!(machine.take_join().is_none());
    pollster::block_on(machine.handle(ParallelEvent::Right(finish("right")))).unwrap();
    assert!(machine.is_finished());
    assert_eq!(
        machine.take_join(),
        Some((RegionState::Finished, RegionState::Finished))
    );
    assert!(machine.take_join().is_none());
    let (left, right) = machine.into_regions();
    assert_eq!(left.finished_data().unwrap().0, "left");
    assert_eq!(right.finished_data().unwrap().0, "right");
    #[cfg(feature = "serde")]
    let (left, right) = (
        DynamicRegion::from_snapshot(left.into_snapshot()).unwrap(),
        DynamicRegion::from_snapshot(right.into_snapshot()).unwrap(),
    );
    let mut restored = Parallel::new(left, right);
    assert!(restored.is_finished());
    assert!(
        restored.take_join().is_none(),
        "restore does not synthesize join"
    );
}
#[test]
fn fork_routes_separate_owned_payloads_without_cloning() {
    let mut machine = regions();
    pollster::block_on(machine.fork(finish("one"), finish("two"))).unwrap();
    assert!(machine.is_finished());
    assert_eq!(machine.epoch(), 2);
    assert!(machine.take_join().is_some());
    assert!(machine.take_join().is_none());
}
#[test]
fn second_region_failure_exposes_partial_commit_instead_of_fake_rollback() {
    let mut machine = regions();
    pollster::block_on(machine.handle(ParallelEvent::Right(finish("already done")))).unwrap();
    assert_matches!(
        pollster::block_on(machine.fork(finish("left committed"), finish("rejected"))),
        Err(ParallelError::Right {
            left_committed: true,
            ..
        })
    );
    assert!(machine.is_finished());
    assert_eq!(machine.epoch(), 2);
    assert!(machine.take_join().is_some());
    assert_eq!(machine.left().finished_data().unwrap().0, "left committed");
    assert_eq!(machine.right().finished_data().unwrap().0, "already done");
    assert_matches!(
        pollster::block_on(machine.handle(ParallelEvent::Right(RegionEvent::Heartbeat))),
        Err(ParallelError::Right {
            left_committed: false,
            ..
        })
    );
    assert!(machine.take_join().is_none());
}
#[test]
fn left_failure_does_not_dispatch_right_and_nesting_supports_three_regions() {
    let mut machine = regions();
    pollster::block_on(machine.handle(ParallelEvent::Left(finish("left")))).unwrap();
    assert_matches!(
        pollster::block_on(machine.fork(finish("again"), finish("untouched"))),
        Err(ParallelError::Left(_))
    );
    assert_eq!(machine.right().current_state(), RegionState::Pending);
    let mut nested = Parallel::new(regions(), DynamicRegion::new(()));
    pollster::block_on(nested.fork(
        ParallelEvent::Both {
            left: finish("a"),
            right: finish("b"),
        },
        finish("c"),
    ))
    .unwrap();
    assert_eq!(
        nested.current_state(),
        (
            (RegionState::Finished, RegionState::Finished),
            RegionState::Finished
        )
    );
    assert!(nested.is_finished());
    assert_eq!(nested.epoch(), 3);
    assert!(nested.take_join().is_some());
}

mod invocation {
    use super::*;
    state_machine! {
        name: Coordinator, dynamic: true, initial: Forked,
        states: [Forked, Joined, Failed], final_states: [Joined, Failed],
        events {
            done { transition: { from: Forked, to: Joined } }
            error { transition: { from: Forked, to: Failed } }
        }
    }
    #[test]
    fn fork_join_works_as_an_invoked_child_configuration() {
        let mut runner = Runner::new(DynamicCoordinator::new(()), 1);
        let (_, sink) = runner
            .invoke_child(
                Runner::new(regions(), 2),
                2,
                |child| {
                    assert!(child.is_finished());
                    CoordinatorEvent::Done
                },
                |_, _| CoordinatorEvent::Error,
            )
            .unwrap();
        sink.enqueue(ParallelEvent::Both {
            left: finish("left"),
            right: finish("right"),
        })
        .unwrap();
        assert_eq!(pollster::block_on(runner.drain(1)), Ok(1));
        assert_eq!(runner.machine().current_state(), CoordinatorState::Joined);
    }
}

#[cfg(feature = "async")]
mod asynchronous {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    type Log = Rc<RefCell<Vec<&'static str>>>;
    state_machine! {
        name: AsyncRegion, dynamic: true, async: true, context: Log, initial: Idle,
        states: [Idle, Complete], final_states: [Complete],
        events { finish { payload: &'static str, transition: { from: Idle, to: Complete, before: [record] } } }
    }
    impl<S> AsyncRegion<S> {
        async fn record(&self, payload: &&'static str) {
            self.ctx.borrow_mut().push(*payload);
        }
    }
    #[test]
    fn async_fork_has_deterministic_region_order() {
        let log = Log::default();
        let mut machine = Parallel::new(
            DynamicAsyncRegion::new(log.clone()),
            DynamicAsyncRegion::new(log.clone()),
        );
        pollster::block_on(machine.fork(
            AsyncRegionEvent::Finish("left"),
            AsyncRegionEvent::Finish("right"),
        ))
        .unwrap();
        assert_eq!(&*log.borrow(), &["left", "right"]);
        assert!(machine.is_finished());
    }
    state_machine! {
        name: Suspended, dynamic: true, async: true, initial: Waiting,
        states: [Waiting, Done], final_states: [Done],
        events { finish { transition: { from: Waiting, to: Done, before: [suspend] } } }
    }
    impl<C, S> Suspended<C, S> {
        async fn suspend(&self) {
            std::future::pending::<()>().await;
        }
    }
    #[test]
    fn cancelled_fork_keeps_left_commit_and_reports_right_poisoning() {
        use std::{
            future::Future,
            pin::pin,
            task::{Context, Poll, Waker},
        };
        let mut machine = Parallel::new(DynamicRegion::new(()), DynamicSuspended::new(()));
        {
            let mut fork = pin!(machine.fork(finish("committed"), SuspendedEvent::Finish));
            assert_matches!(
                fork.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending
            );
        }
        assert_eq!(machine.left().current_state(), RegionState::Finished);
        assert!(machine.right().is_poisoned());
        assert!(machine.is_poisoned());
        assert!(!machine.is_finished());
        assert_eq!(machine.epoch(), 1);
        assert!(machine.take_join().is_none());
        assert_matches!(
            pollster::block_on(machine.handle(ParallelEvent::Left(RegionEvent::Heartbeat))),
            Err(ParallelError::Poisoned)
        );
    }
}
