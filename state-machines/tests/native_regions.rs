#![cfg(feature = "runtime")]
use state_machines::{
    runtime::{Clock, Machine, ParallelError, RegionError},
    state_machine,
};
use std::{assert_matches, cell::Cell};

#[derive(Debug)]
pub struct Owned(String);
#[derive(Debug, Default)]
pub struct Log(Cell<usize>);
state_machine! {
    name: WorkerRegion, dynamic: true, context: Log, initial: Waiting,
    states: [Waiting, Done], final_states: [Done],
    runtime: { Waiting { after: [{ delay: 3, event: make_timeout }] } },
    events {
        finish { payload: Owned, transition: { from: Waiting, to: Done, before: [record] } }
        timeout { transition: { from: Waiting, to: Done } }
    }
}
impl<S> WorkerRegion<S> {
    fn make_timeout(&self) -> WorkerRegionEvent {
        WorkerRegionEvent::Timeout
    }
    fn record(&self, payload: &Owned) {
        self.ctx.0.set(payload.0.len());
    }
}
state_machine! {
    name: Team,
    regions: { primary: DynamicWorkerRegion, backup: DynamicWorkerRegion, observer: DynamicWorkerRegion },
    events {
        all {
            payload: (Owned, Owned, Owned),
            routes: {
                // Declaration order, not route spelling order, controls dispatch.
                observer: WorkerRegionEvent::Finish(payload.2),
                primary: WorkerRegionEvent::Finish(payload.0),
                backup: WorkerRegionEvent::Finish(payload.1),
            }
        }
        one { payload: Owned, routes: { backup: WorkerRegionEvent::Finish(payload) } }
    }
}
state_machine! {
    name: NestedTeam,
    regions: { team: Team, solo: DynamicWorkerRegion },
    events {
        all {
            payload: ((Owned, Owned, Owned), Owned),
            routes: { team: TeamEvent::All(payload.0), solo: WorkerRegionEvent::Finish(payload.1) }
        }
    }
}
struct Time(u64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0
    }
}
fn team() -> Team {
    Team::new(
        DynamicWorkerRegion::new(Log::default()),
        DynamicWorkerRegion::new(Log::default()),
        DynamicWorkerRegion::new(Log::default()),
        8,
    )
}
fn owned(value: &str) -> Owned {
    Owned(value.into())
}

#[test]
fn native_owned_fork_and_named_configuration_join_once() {
    let mut team = team();
    assert!(team.take_join().is_none());
    assert_eq!(team.scope_epoch("primary/Waiting"), Some(0));
    assert_matches!(
        pollster::block_on(team.handle(TeamEvent::All((owned("a"), owned("bb"), owned("ccc"))))),
        Err(ParallelError::Left(RegionError::Run(_))),
        "timer declarations require an explicitly observed clock"
    );
    team.start(&Time(0)).unwrap();
    // The event stayed queued across failed initial setup; drain retries it.
    pollster::block_on(team.drain(8)).unwrap();
    assert_eq!(team.current_state().primary, WorkerRegionState::Done);
    pollster::block_on(team.handle(TeamEvent::One(owned("bb")))).unwrap();
    assert!(!team.is_finished());
    team.observer()
        .runner()
        .enqueue(WorkerRegionEvent::Finish(owned("ccc")))
        .unwrap();
    pollster::block_on(team.drain(8)).unwrap();
    assert!(team.is_finished());
    assert_matches!(
        team.take_join(),
        Some(TeamState {
            primary: WorkerRegionState::Done,
            backup: WorkerRegionState::Done,
            observer: WorkerRegionState::Done
        })
    );
    assert!(team.take_join().is_none());
    assert_eq!(team.scope_epoch("primary/Waiting"), None);
    assert_eq!(team.scope_epoch("primary"), Some(0));
    #[cfg(feature = "inspect")]
    {
        let schema = Team::schema();
        assert_eq!(schema.regions.len(), 3);
        assert_eq!(
            schema.region_events[0].regions,
            ["observer", "primary", "backup"]
        );
        assert!(schema.validate().is_empty());
        assert!(schema.to_mermaid().contains("--"));
    }
}

#[test]
fn timers_are_driven_across_all_regions_and_restore_is_not_constructor_entry() {
    let mut team = team();
    team.start(&Time(7)).unwrap();
    team.tick(&Time(10)).unwrap();
    assert_eq!(pollster::block_on(team.drain(8)).unwrap(), 3);
    assert!(team.is_finished());
    assert!(team.take_join().is_some());
    assert!(team.take_join().is_none());
}

#[test]
fn nested_native_regions_reuse_recursive_lifecycle_and_partial_errors() {
    let mut nested = NestedTeam::new(team(), DynamicWorkerRegion::new(Log::default()), 8);
    nested.start(&Time(0)).unwrap();
    pollster::block_on(nested.handle(NestedTeamEvent::All((
        (owned("a"), owned("b"), owned("c")),
        owned("d"),
    ))))
    .unwrap();
    assert!(nested.is_finished());
    assert!(nested.take_join().is_some());
    assert_eq!(nested.scope_epoch("team/primary/Done"), Some(0));
    assert_matches!(
        pollster::block_on(nested.handle(NestedTeamEvent::All((
            (owned("a"), owned("b"), owned("c")),
            owned("d")
        )))),
        Err(ParallelError::Left(_))
    );
    assert!(nested.take_join().is_none());
}
