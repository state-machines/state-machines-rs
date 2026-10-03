//! Optional, executor-independent event processing (`no_std` + `alloc`).
//! Queued events are owned; no payload Clone bounds or hidden threads.
use alloc::{collections::VecDeque, rc::Rc, vec::Vec};
use core::{cell::RefCell, fmt, task::Waker};

mod timers;
pub use timers::{Clock, ClockError, ScheduleError, TimerId};
mod activities;
pub use activities::{ActivityId, ChildInvokeError, InvokeError, InvokeFailure};
mod parallel;
pub use parallel::{Parallel, ParallelError, ParallelEvent};
mod region;
pub use region::{Region, RegionError};
mod work;
pub use work::WorkScope;
mod lifecycle;
pub use activities::run_child;
pub use lifecycle::{ActivityFuture, LifecycleRule, SetupError, SetupFailure};
use work::{Registry, Visit};

/// Implemented by generated dynamic machines when the facade's runtime feature is enabled.
/// Implementers must advance `epoch` on every committed external transition, even
/// self-re-entry or a commit followed by a failed automatic step. Internal edges
/// must retain it. State/epoch must not change through shared references.
#[allow(async_fn_in_trait)]
pub trait Machine {
    type Event;
    type Error;
    type State: Copy + Eq;
    fn state(&self) -> Self::State;
    fn epoch(&self) -> u64;
    /// Optional named hierarchical scope visits. Defaults to no named scopes.
    fn scope_epoch(&self, _scope: &str) -> Option<u64> {
        None
    }
    fn runtime_rules() -> Vec<LifecycleRule<Self>>
    where
        Self: Sized,
    {
        Vec::new()
    }
    fn is_finished(&self) -> bool;
    fn is_poisoned(&self) -> bool;
    async fn dispatch(&mut self, event: Self::Event) -> Result<(), Self::Error>;
    async fn dispatch_one(&mut self, event: Self::Event) -> Result<(), Self::Error> {
        self.dispatch(event).await
    }
    async fn automatic_step(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    async fn automatic_enabled(&self) -> bool {
        false
    }
    /// Host-driven lifecycle operations for composed regions. Ordinary machines
    /// use their enclosing Runner; compositions forward these to region runners.
    fn start_regions(&mut self, _clock: &impl Clock) -> Result<(), Self::Error> {
        Ok(())
    }
    fn tick_regions(&mut self, _clock: &impl Clock) -> Result<(), Self::Error> {
        Ok(())
    }
    fn poll_regions(&mut self, _cx: &mut core::task::Context<'_>) -> usize {
        0
    }
    async fn drive_regions(&mut self, _max_steps: usize) -> Result<usize, Self::Error> {
        Ok(0)
    }
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
impl<E> Inbox<E> {
    fn reserve(&mut self) -> bool {
        if self.closed || self.pending == self.capacity {
            return false;
        }
        self.pending += 1;
        true
    }
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
            if !inbox.reserve() {
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
    Setup(SetupError),
    AutomaticStepLimit { limit: usize },
}

enum Scope<S> {
    State(S),
    Predicate(fn(S) -> bool),
    Named(&'static str),
}
impl<S: Copy + Eq> Scope<S> {
    fn active<M: Machine<State = S>>(&self, machine: &M) -> bool {
        match self {
            Self::State(expected) => *expected == machine.state(),
            Self::Predicate(predicate) => predicate(machine.state()),
            Self::Named(scope) => machine.scope_epoch(scope).is_some(),
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
    timers: Registry<TimerId, M::State, timers::Timeout<M::Event>>,
    next_timer: u64,
    last_time: Option<u64>,
    activities: Registry<ActivityId, M::State, activities::Task<M::Event>>,
    next_activity: u64,
    lifecycle: Vec<lifecycle::Configured<M>>,
}
impl<M: Machine> Runner<M> {
    pub fn new(machine: M, capacity: usize) -> Self {
        let declarations = M::runtime_rules();
        let rules = declarations
            .iter()
            .filter_map(|rule| match rule {
                LifecycleRule::Defer { scope, matches } => Some(Deferral {
                    scope: Scope::Named(scope),
                    matches: *matches,
                }),
                _ => None,
            })
            .collect();
        Self {
            machine: Some(machine),
            sink: EventSink::new(capacity),
            rules,
            deferred: VecDeque::new(),
            timers: Registry::new(),
            next_timer: 0,
            last_time: None,
            activities: Registry::new(),
            next_activity: 0,
            lifecycle: declarations
                .into_iter()
                .map(|rule| lifecycle::Configured { rule, epoch: None })
                .collect(),
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
    /// Recover setup backpressure without losing queued events or repeating factories.
    pub fn set_capacity(&mut self, capacity: usize) -> bool {
        let mut inbox = self.sink.inbox.borrow_mut();
        if capacity < inbox.pending {
            return false;
        }
        inbox.capacity = capacity;
        true
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
    fn reconcile_work(&mut self) {
        let machine = self.machine.as_ref().unwrap();
        let released = self.timers.reconcile(machine) + self.activities.reconcile(machine);
        self.release(released);
    }
    fn release(&self, count: usize) {
        self.sink.inbox.borrow_mut().pending -= count;
    }
    fn apply_cancellation(&self, result: Option<bool>) -> bool {
        if let Some(pending) = result {
            self.release(usize::from(pending));
            true
        } else {
            false
        }
    }
    fn live_delivery(&self, event: &Envelope<M::Event>) -> bool {
        event.timer.is_none_or(|id| self.timers.contains(id))
            && event.activity.is_none_or(|id| self.activities.contains(id))
    }
    fn recall(&mut self) {
        let mut recalled = VecDeque::new();
        for _ in 0..self.deferred.len() {
            let (rule, event) = self.deferred.pop_front().unwrap();
            if !self.live_delivery(&event) {
                self.release(1);
            } else if self.rules[rule].scope.active(self.machine()) {
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
        self.reconcile_work();
        self.recall();
        if self.machine().is_poisoned() {
            return Err(RunError::Poisoned);
        }
        self.setup_entries().map_err(RunError::Setup)?;
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
            if !self.live_delivery(&event) {
                self.release(1);
                steps += 1;
                continue;
            }
            if let Some(rule) = self
                .rules
                .iter()
                .position(|rule| rule.scope.active(self.machine()) && (rule.matches)(&event.event))
            {
                self.deferred.push_back((rule, event));
            } else {
                self.release(1);
                if let Some(id) = event.timer {
                    self.timers.retire(id);
                }
                if let Some(id) = event.activity {
                    self.activities.retire(id);
                }
                let result = self
                    .machine
                    .as_mut()
                    .unwrap()
                    .dispatch_one(event.event)
                    .await;
                // A failed automatic microstep may follow a committed external edge.
                self.reconcile_work();
                self.recall();
                result.map_err(RunError::Dispatch)?;
                self.stabilize(64).await?;
            }
            steps += 1;
        }
    }
    /// Entry setup surrounds every automatic microstep, including transient scopes.
    pub async fn stabilize(&mut self, max_steps: usize) -> Result<usize, RunError<M::Error>> {
        self.reconcile_work();
        self.recall();
        if self.machine().is_poisoned() {
            return Err(RunError::Poisoned);
        }
        self.setup_entries().map_err(RunError::Setup)?;
        let mut steps = 0;
        loop {
            if steps == max_steps {
                return if self.machine().automatic_enabled().await {
                    Err(RunError::AutomaticStepLimit { limit: max_steps })
                } else {
                    Ok(steps)
                };
            }
            let result = self.machine.as_mut().unwrap().automatic_step().await;
            self.reconcile_work();
            self.recall();
            let changed = result.map_err(RunError::Dispatch)?;
            self.setup_entries().map_err(RunError::Setup)?;
            if !changed {
                return Ok(steps);
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
