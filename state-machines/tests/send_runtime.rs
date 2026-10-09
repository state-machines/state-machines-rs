#![cfg(feature = "runtime-send")]

use state_machines::{
    runtime::{Clock, EventSink, Machine, QueueError, Runner, WorkScope},
    state_machine,
};
use std::{
    assert_matches,
    future::pending,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Debug)]
pub struct Owned(Box<str>);
state_machine! {
    name: SendWorker, dynamic: true, async: true, initial: Waiting,
    states: [Waiting, Done(Owned)], final_states: [Done],
    events {
        finish { payload: Option<Owned>, transition: { from: Waiting, to: Done, data: own } }
        reset { transition: { from: Waiting, to: Waiting } }
        retain { transition: { from: Waiting, internal: true } }
    }
}
impl<C, S> SendWorker<C, S> {
    async fn own(&self, payload: &mut Option<Owned>) -> Owned {
        payload.take().unwrap()
    }
}
state_machine! {
    name: SendTeam,
    regions: { left: DynamicSendWorker<()>, right: DynamicSendWorker<()> },
    events {
        finish {
            payload: (Owned, Owned),
            routes: {
                left: SendWorkerEvent::Finish(Some(payload.0)),
                right: SendWorkerEvent::Finish(Some(payload.1)),
            }
        }
    }
}
struct Time(u64);
impl Clock for Time {
    fn now(&self) -> u64 {
        self.0
    }
}
fn event(value: &str) -> SendWorkerEvent {
    SendWorkerEvent::Finish(Some(Owned(value.into())))
}
fn assert_send<T: Send>(_: &T) {}
fn assert_send_sync<T: Send + Sync>(_: &T) {}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .unwrap()
}

#[test]
fn spawned_driver_wakes_from_cross_thread_owned_ingress() {
    runtime().block_on(async {
        let mut runner = Runner::new(DynamicSendWorker::new(()), 2);
        assert_send(&runner);
        let sink = runner.sink();
        assert_send_sync(&sink);
        let task = tokio::spawn(async move {
            runner.wait_for_work().await.unwrap();
            runner.drain(4).await.unwrap();
            runner.into_machine()
        });
        std::thread::spawn(move || sink.enqueue(event("owned")).unwrap())
            .join()
            .unwrap();
        let machine = task.await.unwrap();
        assert_eq!(machine.done_data().unwrap().0.as_ref(), "owned");
    });
}

state_machine! {
    name: SendOnlyWorker, dynamic: true, initial: SendOnlyWaiting,
    states: [SendOnlyWaiting, SendOnlyDone], final_states: [SendOnlyDone],
    events { finish { transition: { from: SendOnlyWaiting, to: SendOnlyDone } } }
}

#[test]
fn synchronous_generated_adapters_accept_send_but_not_sync_contexts() {
    runtime().block_on(async {
        let mut runner = Runner::new(DynamicSendOnlyWorker::new(std::cell::Cell::new(0_u32)), 2);
        assert_send(&runner);
        tokio::spawn(async move {
            runner.enqueue(SendOnlyWorkerEvent::Finish).unwrap();
            runner.drain(2).await.unwrap();
            assert!(runner.machine().is_finished());
        })
        .await
        .unwrap();
    });
}

state_machine! {
    name: AsyncSendOnlyWorker, dynamic: true, async: true, initial: SendOnlyReady,
    states: [SendOnlyReady(std::cell::Cell<u32>), SendOnlyFinished],
    final_states: [SendOnlyFinished],
    events { finish { transition: { from: SendOnlyReady, to: SendOnlyFinished } } }
}

#[test]
fn asynchronous_generated_adapters_accept_send_but_not_sync_state_data() {
    runtime().block_on(async {
        let mut machine = DynamicAsyncSendOnlyWorker::new(());
        machine
            .set_send_only_ready_data(std::cell::Cell::new(7))
            .unwrap();
        let automatic = Machine::automatic_enabled(&machine);
        assert_send(&automatic);
        assert!(!automatic.await);

        let mut runner = Runner::new(machine, 2);
        assert_send(&runner);
        tokio::spawn(async move {
            assert_eq!(runner.machine().send_only_ready_data().unwrap().get(), 7);
            runner.enqueue(AsyncSendOnlyWorkerEvent::Finish).unwrap();
            runner.drain(2).await.unwrap();
            assert!(runner.machine().is_finished());
        })
        .await
        .unwrap();
    });
}

#[test]
fn capacity_and_close_return_the_original_owned_event() {
    let runner = Runner::new(DynamicSendWorker::new(()), 1);
    let sink = runner.sink();
    sink.enqueue(event("first")).unwrap();
    let QueueError::Full(SendWorkerEvent::Finish(Some(owned))) =
        sink.enqueue(event("full")).unwrap_err()
    else {
        panic!("expected full");
    };
    assert_eq!(owned.0.as_ref(), "full");
    drop(runner);
    assert_matches!(sink.enqueue(event("closed")), Err(QueueError::Closed(_)));
}

#[test]
fn concurrent_producers_share_one_exact_capacity_limit() {
    let runner = Runner::new(DynamicSendWorker::new(()), 64);
    let producers: Vec<_> = (0..8)
        .map(|_| {
            let sink = runner.sink();
            std::thread::spawn(move || {
                (0..32)
                    .filter(|_| sink.enqueue(event("owned")).is_ok())
                    .count()
            })
        })
        .collect();
    let accepted: usize = producers.into_iter().map(|task| task.join().unwrap()).sum();
    assert_eq!(accepted, 64);
    assert_eq!(runner.pending(), 64);
}

#[test]
fn spawned_nested_regions_and_child_invocation_are_send() {
    runtime().block_on(async {
        let mut team = SendTeam::new(DynamicSendWorker::new(()), DynamicSendWorker::new(()), 4);
        team.start(&Time(0)).unwrap();
        let mut parent = Runner::new(DynamicSendWorker::new(()), 4);
        let (_, child_sink) = parent
            .invoke_child(
                Runner::new(team, 4),
                8,
                |_| event("joined"),
                |error, _| panic!("child failed: {error:?}"),
            )
            .unwrap();
        let task = tokio::spawn(async move {
            parent.wait_for_work().await.unwrap();
            parent.drain(8).await.unwrap();
            assert!(parent.machine().is_finished());
        });
        child_sink
            .enqueue(SendTeamEvent::Finish((
                Owned("a".into()),
                Owned("b".into()),
            )))
            .unwrap();
        task.await.unwrap();
    });
}

struct OnDrop(Arc<AtomicUsize>);
impl Drop for OnDrop {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn external_reset_cancels_owned_send_work_but_internal_transition_retains_it() {
    runtime().block_on(async {
        let dropped = Arc::new(AtomicUsize::new(0));
        let resource = OnDrop(dropped.clone());
        let mut runner = Runner::new(DynamicSendWorker::new(()), 4);
        runner
            .invoke_future(async move {
                let _resource = resource;
                pending::<SendWorkerEvent>().await
            })
            .unwrap();
        runner.schedule_after(&Time(0), 5, event("stale")).unwrap();
        runner.enqueue(SendWorkerEvent::Retain).unwrap();
        runner.drain(4).await.unwrap();
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        runner.enqueue(SendWorkerEvent::Reset).unwrap();
        runner.drain(4).await.unwrap();
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        runner.tick(&Time(6)).unwrap();
        assert_eq!(runner.pending(), 0);
        assert_eq!(runner.activity_count(), 0);
    });
}

#[test]
fn send_activity_completion_and_named_scope_deadline_use_the_shared_driver() {
    runtime().block_on(async {
        let mut runner = Runner::new(DynamicSendWorker::new(()), 4);
        runner.invoke_future(async { event("activity") }).unwrap();
        runner
            .schedule_after_in(WorkScope::Named("Waiting"), &Time(0), 5, event("timeout"))
            .unwrap();
        tokio::spawn(async move {
            runner.drain(4).await.unwrap();
            assert!(runner.machine().is_finished());
            runner.tick(&Time(5)).unwrap();
            assert_eq!(runner.pending(), 0);
        })
        .await
        .unwrap();
    });
}

#[test]
fn event_destructors_can_reenter_the_closed_sink_without_holding_its_lock() {
    #[derive(Debug)]
    struct Reentrant(Option<EventSink<Reentrant>>, Arc<AtomicUsize>);
    impl Drop for Reentrant {
        fn drop(&mut self) {
            if let Some(sink) = self.0.take() {
                assert_matches!(
                    sink.enqueue(Reentrant(None, self.1.clone())),
                    Err(QueueError::Closed(_))
                );
            }
            self.1.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct Receiver;
    impl Machine for Receiver {
        type Event = Reentrant;
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
        async fn dispatch(&mut self, _: Reentrant) -> Result<(), ()> {
            Ok(())
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let runner = Runner::new(Receiver, 2);
    runner
        .enqueue(Reentrant(Some(runner.sink()), drops.clone()))
        .unwrap_or_else(|_| panic!("enqueue"));
    drop(runner);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}

#[test]
fn replacing_waiters_drops_the_previous_waker_outside_the_mailbox_lock() {
    use std::{
        future::Future,
        task::{Context, Poll, Wake, Waker},
        time::Duration,
    };
    struct ReentrantWake {
        sink: EventSink<SendWorkerEvent>,
        drops: Arc<AtomicUsize>,
    }
    // Waker::noop has no owned data/destructor; this test needs the last Arc drop.
    #[allow(clippy::manual_noop_waker)]
    impl Wake for ReentrantWake {
        fn wake(self: Arc<Self>) {}
    }
    impl Drop for ReentrantWake {
        fn drop(&mut self) {
            assert_eq!(self.sink.pending(), 0);
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    let (completed, result) = std::sync::mpsc::channel();
    let task = std::thread::spawn(move || {
        let mut runner = Runner::new(DynamicSendWorker::new(()), 2);
        let drops = Arc::new(AtomicUsize::new(0));
        let waker = Waker::from(Arc::new(ReentrantWake {
            sink: runner.sink(),
            drops: drops.clone(),
        }));
        {
            let mut wait = std::pin::pin!(runner.wait_for_work());
            assert_matches!(
                wait.as_mut().poll(&mut Context::from_waker(&waker)),
                Poll::Pending,
            );
        }
        drop(waker);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        {
            let mut wait = std::pin::pin!(runner.wait_for_work());
            assert_matches!(
                wait.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending,
            );
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        completed.send(()).unwrap();
    });
    result
        .recv_timeout(Duration::from_secs(5))
        .expect("waker destruction must not deadlock");
    task.join().unwrap();
}

#[test]
fn sink_debug_formatting_does_not_hold_the_mailbox_lock() {
    use std::{fmt::Write, time::Duration};
    struct ReentrantWriter(EventSink<SendWorkerEvent>);
    impl std::fmt::Write for ReentrantWriter {
        fn write_str(&mut self, _: &str) -> std::fmt::Result {
            assert_eq!(self.0.pending(), 0);
            Ok(())
        }
    }
    let (completed, result) = std::sync::mpsc::channel();
    let task = std::thread::spawn(move || {
        let runner = Runner::new(DynamicSendWorker::new(()), 2);
        write!(ReentrantWriter(runner.sink()), "{:?}", runner.sink()).unwrap();
        completed.send(()).unwrap();
    });
    result
        .recv_timeout(Duration::from_secs(5))
        .expect("debug formatting must not deadlock");
    task.join().unwrap();
}

#[cfg(feature = "serde")]
mod snapshots {
    use super::*;
    state_machine! {
        name: SnapshotWorker, dynamic: true, snapshot: true,
        initial: Ready, states: [Ready, Done(Box<str>)], final_states: [Done],
        events {
            finish {
                payload: Option<Box<str>>,
                transition: { from: Ready, to: Done, data: own }
            }
        }
    }
    impl<C, S> SnapshotWorker<C, S> {
        fn own(&self, value: &mut Option<Box<str>>) -> Box<str> {
            value.take().unwrap()
        }
    }
    state_machine! {
        name: SnapshotTeam, snapshot: true,
        regions: { left: DynamicSnapshotWorker<()>, right: DynamicSnapshotWorker<()> },
        events {
            finish {
                payload: (Box<str>, Box<str>),
                routes: {
                    left: SnapshotWorkerEvent::Finish(Some(payload.0)),
                    right: SnapshotWorkerEvent::Finish(Some(payload.1)),
                }
            }
        }
    }
    #[test]
    fn restored_owned_regions_remain_send_and_do_not_synthesize_a_join() {
        runtime().block_on(async {
            let mut team = SnapshotTeam::new(
                DynamicSnapshotWorker::new(()),
                DynamicSnapshotWorker::new(()),
                4,
            );
            team.start(&Time(0)).unwrap();
            team.handle(SnapshotTeamEvent::Finish(("left".into(), "right".into())))
                .await
                .unwrap();
            assert!(team.take_join().is_some());
            let snapshot = team.try_into_snapshot().ok().unwrap();
            let json = serde_json::to_string(&snapshot).unwrap();
            let restored = SnapshotTeam::from_snapshot(
                serde_json::from_str::<SnapshotTeamSnapshot>(&json).unwrap(),
                4,
            )
            .ok()
            .unwrap();
            assert_send(&restored);
            tokio::spawn(async move {
                let mut restored = restored;
                assert!(restored.is_finished());
                assert!(restored.take_join().is_none());
                assert_eq!(
                    restored
                        .left()
                        .runner()
                        .machine()
                        .done_data()
                        .unwrap()
                        .as_ref(),
                    "left"
                );
                assert_eq!(
                    restored
                        .right()
                        .runner()
                        .machine()
                        .done_data()
                        .unwrap()
                        .as_ref(),
                    "right"
                );
            })
            .await
            .unwrap();
        });
    }
}
