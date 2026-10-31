use super::{Envelope, Machine, Runner, Visit, WorkScope};
use alloc::vec::Vec;
use core::fmt;

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
impl<E> fmt::Display for ScheduleError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full(_) => "timer rejected: mailbox is full",
            Self::Poisoned(_) => "timer rejected: machine is poisoned",
            Self::Backwards(_) => "timer rejected: clock moved backwards",
            Self::Overflow(_) => "timer rejected: deadline or timer id overflow",
            Self::InactiveScope(_) => "timer rejected: scope is not active",
        })
    }
}
impl<E: fmt::Debug> core::error::Error for ScheduleError<E> {}

pub(super) struct Timeout<E> {
    deadline: u64,
    event: E,
}

type TimerReservation<S> = (TimerId, Visit<S>, u64);

impl<M: Machine> Runner<M> {
    /// Reserve capacity and bind a timeout to the current leaf visit.
    /// Internal transitions preserve it; any external transition cancels it.
    ///
    /// # Errors
    ///
    /// The event is handed back inside the [`ScheduleError`] variant that names the reason:
    /// `Poisoned` when the machine is poisoned, `InactiveScope` when the leaf visit
    /// cannot be captured, `Backwards` when `clock.now()` is earlier than the last
    /// observed time, `Overflow` when `now + delay` or the timer id counter would wrap,
    /// or `Full` when no mailbox slot is free for the timeout.
    pub fn schedule_after(
        &mut self,
        clock: &impl Clock,
        delay: u64,
        event: M::Event,
    ) -> Result<TimerId, ScheduleError<M::Event>> {
        self.schedule_after_in(WorkScope::Leaf, clock, delay, event)
    }
    /// Like [`schedule_after`](Self::schedule_after), bound to `scope` instead of the leaf.
    ///
    /// # Errors
    ///
    /// The event is handed back inside the [`ScheduleError`] variant that names the reason:
    /// `Poisoned` when the machine is poisoned, `InactiveScope` when `scope` is not
    /// currently active, `Backwards` when `clock.now()` is earlier than the last
    /// observed time, `Overflow` when `now + delay` or the timer id counter would wrap,
    /// or `Full` when no mailbox slot is free for the timeout.
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
        self.sink
            .inbox
            .borrow_mut()
            .reserve()
            .ok_or(ScheduleError::Full(()))?;
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
    ///
    /// # Errors
    ///
    /// Returns [`ClockError::Backwards`] when `clock.now()` is earlier than the
    /// time observed by the previous `tick`, `start`, or schedule call.
    pub fn tick(&mut self, clock: &impl Clock) -> Result<usize, ClockError> {
        let now = clock.now();
        if self.last_time.is_some_and(|last| now < last) {
            return Err(ClockError::Backwards);
        }
        self.last_time = Some(now);
        self.reconcile_work();
        // Timer ids are unique, so ordering by (deadline, id) is total.
        let mut due: Vec<_> = self
            .timers
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                entry
                    .pending
                    .as_ref()
                    .filter(|timer| timer.deadline <= now)
                    .map(|timer| (timer.deadline, entry.id.0, index))
            })
            .collect();
        due.sort_unstable();
        let mut count = 0;
        let waker = {
            let mut inbox = self.sink.inbox.borrow_mut();
            for (_, _, index) in due {
                let entry = &mut self.timers.entries[index];
                let Some(timer) = entry.pending.take() else {
                    continue;
                };
                inbox.external.push_back(Envelope {
                    event: timer.event,
                    timer: Some(entry.id),
                    activity: None,
                });
                count += 1;
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
impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("clock moved backwards")
    }
}
impl core::error::Error for ClockError {}
