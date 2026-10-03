use super::{Envelope, Machine, Runner, Visit, WorkScope};
use alloc::vec::Vec;

/// Executor-independent monotonic logical ticks. The host chooses the tick unit.
pub trait Clock {
    fn now(&self) -> u64;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct TimerId(u64);

#[derive(Debug, PartialEq, Eq)]
pub enum ScheduleError<E> {
    Full(E),
    Poisoned(E),
    Backwards(E),
    Overflow(E),
    InactiveScope(E),
}
impl<E> ScheduleError<E> {
    pub(super) fn with_event<T>(self, event: T) -> ScheduleError<T> {
        match self {
            Self::Full(_) => ScheduleError::Full(event),
            Self::Poisoned(_) => ScheduleError::Poisoned(event),
            Self::Backwards(_) => ScheduleError::Backwards(event),
            Self::Overflow(_) => ScheduleError::Overflow(event),
            Self::InactiveScope(_) => ScheduleError::InactiveScope(event),
        }
    }
    pub fn into_event(self) -> E {
        match self {
            Self::Full(e)
            | Self::Poisoned(e)
            | Self::Backwards(e)
            | Self::Overflow(e)
            | Self::InactiveScope(e) => e,
        }
    }
}

pub(super) struct Timeout<E> {
    deadline: u64,
    event: E,
}

type TimerReservation<S> = (TimerId, Visit<S>, u64);

impl<M: Machine> Runner<M> {
    /// Reserve capacity and bind a timeout to the current leaf visit.
    /// Internal transitions preserve it; any external transition cancels it.
    pub fn schedule_after(
        &mut self,
        clock: &impl Clock,
        delay: u64,
        event: M::Event,
    ) -> Result<TimerId, ScheduleError<M::Event>> {
        self.schedule_after_in(WorkScope::Leaf, clock, delay, event)
    }
    pub fn schedule_after_in(
        &mut self,
        scope: WorkScope,
        clock: &impl Clock,
        delay: u64,
        event: M::Event,
    ) -> Result<TimerId, ScheduleError<M::Event>> {
        let (id, visit, deadline) = match self.reserve_timer(scope, clock, delay) {
            Ok(value) => value,
            Err(error) => return Err(error.with_event(event)),
        };
        self.store_timer(id, visit, deadline, event);
        Ok(id)
    }
    pub(super) fn reserve_timer(
        &mut self,
        scope: WorkScope,
        clock: &impl Clock,
        delay: u64,
    ) -> Result<TimerReservation<M::State>, ScheduleError<()>> {
        self.reconcile_work();
        if self.machine().is_poisoned() {
            return Err(ScheduleError::Poisoned(()));
        }
        let Some(visit) = Visit::capture(self.machine(), scope) else {
            return Err(ScheduleError::InactiveScope(()));
        };
        let now = clock.now();
        if self.last_time.is_some_and(|last| now < last) {
            return Err(ScheduleError::Backwards(()));
        }
        let Some(deadline) = now.checked_add(delay) else {
            return Err(ScheduleError::Overflow(()));
        };
        let Some(next) = self.next_timer.checked_add(1) else {
            return Err(ScheduleError::Overflow(()));
        };
        if !self.sink.inbox.borrow_mut().reserve() {
            return Err(ScheduleError::Full(()));
        }
        self.last_time = Some(now);
        self.next_timer = next;
        let id = TimerId(next);
        Ok((id, visit, deadline))
    }
    pub(super) fn store_timer(
        &mut self,
        id: TimerId,
        visit: Visit<M::State>,
        deadline: u64,
        event: M::Event,
    ) {
        self.timers.insert(id, visit, Timeout { deadline, event });
    }

    /// Move due timers to the external FIFO. Call this from the host's clock driver.
    /// A backwards clock is rejected without delivering any timers.
    pub fn tick(&mut self, clock: &impl Clock) -> Result<usize, ClockError> {
        let now = clock.now();
        if self.last_time.is_some_and(|last| now < last) {
            return Err(ClockError::Backwards);
        }
        self.last_time = Some(now);
        self.reconcile_work();
        let mut due: Vec<_> = self
            .timers
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry
                    .pending
                    .as_ref()
                    .is_some_and(|timer| timer.deadline <= now)
            })
            .map(|(index, _)| index)
            .collect();
        due.sort_by_key(|index| {
            let entry = &self.timers.entries[*index];
            (entry.pending.as_ref().unwrap().deadline, entry.id.0)
        });
        let count = due.len();
        let waker = {
            let mut inbox = self.sink.inbox.borrow_mut();
            for index in due {
                let entry = &mut self.timers.entries[index];
                inbox.external.push_back(Envelope {
                    event: entry.pending.take().unwrap().event,
                    timer: Some(entry.id),
                    activity: None,
                });
            }
            if count > 0 { inbox.waker.take() } else { None }
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(count)
    }

    /// The next deadline not yet queued; clocks/queues are intentionally not snapshots.
    pub fn next_deadline(&self) -> Option<u64> {
        self.timers
            .entries
            .iter()
            .filter_map(|entry| entry.pending.as_ref().map(|timer| timer.deadline))
            .min()
    }

    /// Queued/deferred cancelled timeouts release capacity when skipped by `drain`.
    pub fn cancel_timer(&mut self, id: TimerId) -> bool {
        let result = self.timers.cancel(id);
        self.apply_cancellation(result)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ClockError {
    Backwards,
}
