//! Optional, executor-independent event processing (`no_std` + `alloc`).
//! Queued events are owned; no payload Clone bounds or hidden threads.
use alloc::{collections::VecDeque, rc::Rc, vec::Vec};
use core::{cell::RefCell, fmt, task::Waker};

mod timers;
pub use timers::{Clock, ClockError, ScheduleError, TimerId};
mod activities;
pub use activities::{ActivityId, ChildInvokeError, InvokeError, InvokeFailure};

/// Implemented by generated dynamic machines when the facade's runtime feature is enabled.
#[allow(async_fn_in_trait)]
pub trait Machine {
    type Event;
    type Error;
    type State: Copy + Eq;
    fn state(&self) -> Self::State;
    fn epoch(&self) -> u64;
    fn is_finished(&self) -> bool;
    fn is_poisoned(&self) -> bool;
    async fn dispatch(&mut self, event: Self::Event) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum QueueError<E> {
    Full(E),
    Closed(E),
}
impl<E> QueueError<E> {
    pub fn into_event(self) -> E {
        match self {
            Self::Full(event) | Self::Closed(event) => event,
        }
    }
}

struct Envelope<E> {
    event: E,
    timer: Option<TimerId>,
    activity: Option<ActivityId>,
}
struct Inbox<E> {
    internal: VecDeque<Envelope<E>>,
    external: VecDeque<Envelope<E>>,
    pending: usize,
    capacity: usize,
    closed: bool,
    waker: Option<Waker>,
}

/// A clonable single-executor mailbox. Cloning this handle never clones events.
pub struct EventSink<E> {
    inbox: Rc<RefCell<Inbox<E>>>,
}
impl<E> Clone for EventSink<E> {
    fn clone(&self) -> Self {
        Self {
            inbox: self.inbox.clone(),
        }
    }
}
impl<E> fmt::Debug for EventSink<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inbox = self.inbox.borrow();
        f.debug_struct("EventSink")
            .field("pending", &inbox.pending)
            .field("closed", &inbox.closed)
            .finish()
    }
}
impl<E> EventSink<E> {
    fn new(capacity: usize) -> Self {
        Self {
            inbox: Rc::new(RefCell::new(Inbox {
                internal: VecDeque::new(),
                external: VecDeque::new(),
                pending: 0,
                capacity,
                closed: false,
                waker: None,
            })),
        }
    }
    fn push(&self, event: E, internal: bool) -> Result<(), QueueError<E>> {
        let waker = {
            let mut inbox = self.inbox.borrow_mut();
            if inbox.closed {
                return Err(QueueError::Closed(event));
            }
            if inbox.pending == inbox.capacity {
                return Err(QueueError::Full(event));
            }
            let event = Envelope {
                event,
                timer: None,
                activity: None,
            };
            if internal {
                inbox.internal.push_back(event);
            } else {
                inbox.external.push_back(event);
            }
            inbox.pending += 1;
            inbox.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }
    pub fn enqueue(&self, event: E) -> Result<(), QueueError<E>> {
        self.push(event, false)
    }
    pub fn raise(&self, event: E) -> Result<(), QueueError<E>> {
        self.push(event, true)
    }
    /// Includes deferred events, not the currently executing event.
    pub fn pending(&self) -> usize {
        self.inbox.borrow().pending
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunError<E> {
    Dispatch(E),
    StepLimit { limit: usize },
    Poisoned,
}

enum Scope<S> {
    State(S),
    Predicate(fn(S) -> bool),
}
impl<S: Copy + Eq> Scope<S> {
    fn active(&self, state: S) -> bool {
        match self {
            Self::State(expected) => *expected == state,
            Self::Predicate(predicate) => predicate(state),
        }
    }
}
struct Deferral<S, E> {
    scope: Scope<S>,
    matches: fn(&E) -> bool,
}

/// Explicit FIFO deferral rules override dispatch while their scope is active.
/// Internal raised events precede external events; recalled events retain FIFO order.
pub struct Runner<M: Machine> {
    machine: Option<M>,
    sink: EventSink<M::Event>,
    rules: Vec<Deferral<M::State, M::Event>>,
    deferred: VecDeque<(usize, Envelope<M::Event>)>,
    timers: Vec<timers::Timer<M::State, M::Event>>,
    next_timer: u64,
    last_time: Option<u64>,
    activities: Vec<activities::Activity<M::State, M::Event>>,
    next_activity: u64,
}
impl<M: Machine> Runner<M> {
    pub fn new(machine: M, capacity: usize) -> Self {
        Self {
            machine: Some(machine),
            sink: EventSink::new(capacity),
            rules: Vec::new(),
            deferred: VecDeque::new(),
            timers: Vec::new(),
            next_timer: 0,
            last_time: None,
            activities: Vec::new(),
            next_activity: 0,
        }
    }
    pub fn machine(&self) -> &M {
        self.machine.as_ref().expect("runner owns its machine")
    }
    pub fn sink(&self) -> EventSink<M::Event> {
        self.sink.clone()
    }
    pub fn enqueue(&self, event: M::Event) -> Result<(), QueueError<M::Event>> {
        self.sink.enqueue(event)
    }
    pub fn raise(&self, event: M::Event) -> Result<(), QueueError<M::Event>> {
        self.sink.raise(event)
    }
    pub fn pending(&self) -> usize {
        self.sink.pending()
    }
    pub fn deferred(&self) -> usize {
        self.deferred.len()
    }
    pub fn defer_in(&mut self, state: M::State, matches: fn(&M::Event) -> bool) {
        self.rules.push(Deferral {
            scope: Scope::State(state),
            matches,
        });
    }
    pub fn defer_while(&mut self, scope: fn(M::State) -> bool, matches: fn(&M::Event) -> bool) {
        self.rules.push(Deferral {
            scope: Scope::Predicate(scope),
            matches,
        });
    }
    fn recall(&mut self) {
        let state = self.machine().state();
        let mut recalled = VecDeque::new();
        for _ in 0..self.deferred.len() {
            let (rule, event) = self.deferred.pop_front().unwrap();
            if !self.live_timer(&event) || !self.live_activity(&event) {
                self.sink.inbox.borrow_mut().pending -= 1;
            } else if self.rules[rule].scope.active(state) {
                self.deferred.push_back((rule, event));
            } else {
                recalled.push_back(event);
            }
        }
        let mut inbox = self.sink.inbox.borrow_mut();
        recalled.append(&mut inbox.external);
        inbox.external = recalled;
    }
    pub async fn drain(&mut self, max_steps: usize) -> Result<usize, RunError<M::Error>> {
        self.reconcile_timers();
        self.reconcile_activities();
        self.recall();
        if self.machine().is_poisoned() {
            return Err(RunError::Poisoned);
        }
        let mut steps = 0;
        loop {
            core::future::poll_fn(|cx| {
                self.poll_activities(cx);
                core::task::Poll::Ready(())
            })
            .await;
            let runnable = {
                let inbox = self.sink.inbox.borrow();
                !inbox.internal.is_empty() || !inbox.external.is_empty()
            };
            if !runnable {
                return Ok(steps);
            }
            if steps == max_steps {
                return Err(RunError::StepLimit { limit: max_steps });
            }
            let event = {
                let mut inbox = self.sink.inbox.borrow_mut();
                inbox
                    .internal
                    .pop_front()
                    .or_else(|| inbox.external.pop_front())
                    .unwrap()
            };
            let state = self.machine().state();
            if !self.live_timer(&event) || !self.live_activity(&event) {
                self.sink.inbox.borrow_mut().pending -= 1;
                steps += 1;
                continue;
            }
            if let Some(rule) = self
                .rules
                .iter()
                .position(|rule| rule.scope.active(state) && (rule.matches)(&event.event))
            {
                self.deferred.push_back((rule, event));
            } else {
                self.sink.inbox.borrow_mut().pending -= 1;
                if let Some(id) = event.timer {
                    self.timers.retain(|timer| timer.id != id);
                }
                if let Some(id) = event.activity {
                    self.activities.retain(|activity| activity.id != id);
                }
                let result = self.machine.as_mut().unwrap().dispatch(event.event).await;
                // A failed automatic microstep may follow a committed external edge.
                self.reconcile_timers();
                self.reconcile_activities();
                self.recall();
                result.map_err(RunError::Dispatch)?;
            }
            steps += 1;
        }
    }
    /// Extracting the machine closes the mailbox and discards runner-only queues.
    pub fn into_machine(mut self) -> M {
        self.machine.take().unwrap()
    }
}
impl<M: Machine> Drop for Runner<M> {
    fn drop(&mut self) {
        let (waker, internal, external) = {
            let mut inbox = self.sink.inbox.borrow_mut();
            inbox.closed = true;
            inbox.pending = 0;
            (
                inbox.waker.take(),
                core::mem::take(&mut inbox.internal),
                core::mem::take(&mut inbox.external),
            )
        };
        // User event destructors may use a sink; never run them under its borrow.
        drop((internal, external));
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
