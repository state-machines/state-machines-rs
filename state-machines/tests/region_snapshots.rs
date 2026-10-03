#![cfg(all(feature = "runtime", feature = "serde", not(feature = "runtime-send")))]
use state_machines::{
    SnapshotError,
    runtime::{Clock, Machine, Parallel, QueueError, SnapshotMachine},
    state_machine,
};
use std::{
    assert_matches,
    cell::Cell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Owned(String);
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Data(String);
state_machine! {
    name: Persistent, context: Owned, dynamic: true, snapshot: true, initial: Ready,
    states: [superstate Active { state Ready, state Busy(Data) }, Suspended, Finished(Data)],
    final_states: [Finished],
    runtime: { Active { after: [{ delay: 10, event: make_ping }] } },
    events {
        begin { payload: String, transition: { from: Ready, to: Busy, data: own } }
        pause { transition: { from: Active, to: Suspended } }
        resume { transition: { from: Suspended, to: Active, history: deep } }
        finish { payload: String, transition: { from: [Active, Suspended], to: Finished, data: own } }
        ping { transition: { from: Active, internal: true } }
    }
}
impl<S> Persistent<S> {
    fn own(&self, payload: &mut String) -> Data {
        Data(std::mem::take(payload))
    }
    fn make_ping(&self) -> PersistentEvent {
        PersistentEvent::Ping
    }
}
state_machine! {
    name: Pair, snapshot: true,
    regions: { first: DynamicPersistent, second: DynamicPersistent },
    events {
        begin { payload: (String, String), routes: {
            first: PersistentEvent::Begin(payload.0), second: PersistentEvent::Begin(payload.1)
        } }
        pause { routes: { first: PersistentEvent::Pause } }
        resume { routes: { first: PersistentEvent::Resume } }
        finish { payload: (String, String), routes: {
            first: PersistentEvent::Finish(payload.0), second: PersistentEvent::Finish(payload.1)
        } }
    }
}
state_machine! {
    name: Nested, snapshot: true,
    regions: { pair: Pair, extra: DynamicPersistent },
    events { finish { payload: ((String, String), String), routes: {
        pair: PairEvent::Finish(payload.0), extra: PersistentEvent::Finish(payload.1)
    } } }
}
struct Time(u64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0
    }
}
fn persistent(name: &str) -> DynamicPersistent {
    DynamicPersistent::new(Owned(name.into()))
}
fn pair() -> Pair {
    Pair::new(persistent("first-owner"), persistent("second-owner"), 8)
}
fn strings() -> (String, String) {
    ("first-data".into(), "second-data".into())
}

#[test]
fn one_named_envelope_preserves_owned_data_and_each_regions_history() {
    let mut machine = pair();
    machine.start(&Time(0)).unwrap();
    pollster::block_on(machine.handle(PairEvent::Begin(strings()))).unwrap();
    pollster::block_on(machine.handle(PairEvent::Pause)).unwrap();
    let snapshot = machine.into_snapshot();
    assert_eq!(snapshot.first.__sm_history_active.as_deref(), Some("Busy"));
    assert_eq!(
        snapshot.second.__state_data_busy.as_ref().unwrap().0,
        "second-data"
    );
    let json = serde_json::to_string(&snapshot).unwrap();
    assert!(json.contains("\"history_Active\":\"Busy\""));
    let snapshot: PairSnapshot = serde_json::from_str(&json).unwrap();
    let mut restored = Pair::from_snapshot(snapshot, 4).ok().unwrap();
    assert!(restored.take_join().is_none());
    assert_eq!(restored.first().runner().pending(), 0);
    assert_eq!(restored.second().runner().pending(), 0);
    assert_eq!(restored.scope_epoch("second/Busy"), Some(0));
    restored.start(&Time(20)).unwrap();
    pollster::block_on(restored.handle(PairEvent::Resume)).unwrap();
    assert_eq!(restored.current_state().first, PersistentState::Busy);
    assert!(
        restored
            .first()
            .runner()
            .machine()
            .busy_data()
            .unwrap()
            .0
            .is_empty(),
        "history restores control, not suspended resources"
    );
    let (first, second) = restored.into_regions();
    assert_eq!(first.into_busy().unwrap().ctx.0, "first-owner");
    assert_eq!(second.busy_data().unwrap().0, "second-data");
}

#[test]
fn failed_nested_validation_returns_every_owned_region_intact() {
    let nested = Nested::new(pair(), persistent("extra-owner"), 8);
    let mut snapshot = nested.into_snapshot();
    snapshot.pair.second.version = 9;
    let pointer = snapshot.pair.first.ctx.0.as_ptr();
    let (mut snapshot, error) = Nested::from_snapshot(snapshot, 8).err().unwrap();
    assert_matches!(
        error,
        SnapshotError::UnsupportedVersion {
            expected: 1,
            actual: 9
        }
    );
    assert_eq!(
        snapshot.pair.first.ctx.0.as_ptr(),
        pointer,
        "not reconstructed or cloned"
    );
    assert_eq!(snapshot.extra.ctx.0, "extra-owner");
    snapshot.pair.second.version = 1;
    snapshot.pair.first.__sm_history_active = Some("Finished".into());
    let (mut snapshot, error) = Nested::from_snapshot(snapshot, 8).err().unwrap();
    assert_matches!(error, SnapshotError::InvalidHistory { region: "Active" });
    snapshot.pair.first.__sm_history_active = None;
    snapshot.machine = "Wrong".into();
    let (mut snapshot, error) = Nested::from_snapshot(snapshot, 8).err().unwrap();
    assert_matches!(error, SnapshotError::WrongMachine);
    snapshot.machine = "Nested".into();
    let restored = Nested::from_snapshot(snapshot, 8).ok().unwrap();
    assert_eq!(restored.current_state().pair.first, PersistentState::Ready);
}

#[test]
fn completed_nested_restore_is_inert_and_region_names_are_validated_by_serde() {
    let mut machine = Nested::new(pair(), persistent("extra"), 8);
    machine.start(&Time(0)).unwrap();
    pollster::block_on(machine.handle(NestedEvent::Finish((strings(), "extra-data".into()))))
        .unwrap();
    assert!(machine.take_join().is_some());
    let json = serde_json::to_string(&machine.into_snapshot()).unwrap();
    let snapshot: NestedSnapshot = serde_json::from_str(&json).unwrap();
    let mut restored = Nested::from_snapshot(snapshot, 8).ok().unwrap();
    assert!(restored.is_finished());
    assert!(restored.take_join().is_none());
    assert_eq!(restored.epoch(), 0);
    let wrong = json.replacen("\"extra\":", "\"unknown_region\":", 1);
    assert!(serde_json::from_str::<NestedSnapshot>(&wrong).is_err());
}

struct Pending(Rc<Cell<usize>>);
impl Future for Pending {
    type Output = PersistentEvent;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
#[test]
fn capture_drops_ephemeral_work_closes_old_channels_and_does_not_persist_capacity() {
    let mut machine = pair();
    machine.start(&Time(0)).unwrap();
    let dropped = Rc::new(Cell::new(0));
    machine
        .first_mut()
        .runner_mut()
        .invoke_future(Pending(dropped.clone()))
        .unwrap();
    let sink = machine.first().runner().sink();
    sink.enqueue(PersistentEvent::Begin("queued-not-persisted".into()))
        .unwrap();
    let snapshot = machine.into_snapshot();
    assert_eq!(dropped.get(), 1);
    assert_matches!(sink.enqueue(PersistentEvent::Begin("still-owned".into())),
        Err(QueueError::Closed(PersistentEvent::Begin(value))) if value == "still-owned");
    let restored = Pair::from_snapshot(snapshot, 1).ok().unwrap();
    assert_eq!(restored.first().runner().pending(), 0);
    assert!(restored.first().runner().next_deadline().is_none());
}

#[test]
fn recursive_parallel_adapter_uses_the_same_borrowed_validation_protocol() {
    type Tree = Parallel<DynamicPersistent, Parallel<DynamicPersistent, DynamicPersistent>>;
    let machine: Tree = Parallel::new(
        persistent("left"),
        Parallel::new(persistent("middle"), persistent("right")),
    );
    let mut snapshot = machine.into_snapshot();
    snapshot.right.right.state = "Unknown".into();
    let (mut snapshot, error) = Tree::from_snapshot(snapshot, 2).err().unwrap();
    assert_matches!(error, SnapshotError::UnknownState);
    assert_eq!(snapshot.left.ctx.0, "left");
    assert_eq!(snapshot.right.left.ctx.0, "middle");
    snapshot.right.right.state = "Ready".into();
    let mut restored = Tree::from_snapshot(snapshot, 2).ok().unwrap();
    assert!(restored.take_join().is_none());
    assert_eq!(
        restored.current_state(),
        (
            PersistentState::Ready,
            (PersistentState::Ready, PersistentState::Ready)
        )
    );
}

#[cfg(feature = "async")]
mod cancellation {
    use super::*;
    state_machine! {
        name: Interruptible, dynamic: true, snapshot: true, async: true, initial: Idle,
        states: [Idle, End],
        events { wait { transition: { from: Idle, to: End, before: [pending] } } }
    }
    impl<C, S> Interruptible<C, S> {
        async fn pending(&self) {
            std::future::pending::<()>().await;
        }
    }
    #[test]
    fn poisoned_capture_recovers_the_original_partial_configuration() {
        let mut machine = Parallel::new(DynamicInterruptible::new(()), persistent("survivor"));
        let mut future = Box::pin(machine.handle(state_machines::runtime::ParallelEvent::Left(
            InterruptibleEvent::Wait,
        )));
        let mut cx = Context::from_waker(std::task::Waker::noop());
        assert_matches!(future.as_mut().poll(&mut cx), Poll::Pending);
        drop(future);
        let machine = machine.try_into_snapshot().err().unwrap();
        assert!(machine.is_poisoned());
        assert_eq!(machine.right().current_state(), PersistentState::Ready);
        let (_, survivor) = machine.into_regions();
        assert_eq!(survivor.into_ready().unwrap().ctx.0, "survivor");
    }
}
