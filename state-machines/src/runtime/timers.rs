use super::{Envelope, Machine, Runner};
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
}
impl<E> ScheduleError<E> {
    pub fn into_event(self) -> E {
        match self {
            Self::Full(e) | Self::Poisoned(e) | Self::Backwards(e) | Self::Overflow(e) => e,
        }
    }
}

pub(super) struct Timer<S, E> {
    pub id: TimerId,
    deadline: u64,
    state: S,
    epoch: u64,
    event: Option<E>,
}

impl<M: Machine> Runner<M> {
    /// Reserve capacity and bind a timeout to the current leaf visit.
    /// Internal transitions preserve it; any external transition cancels it.
    pub fn schedule_after(
        &mut self,
        clock: &impl Clock,
        delay: u64,
        event: M::Event,
    ) -> Result<TimerId, ScheduleError<M::Event>> {
        self.reconcile_timers();
        if self.machine().is_poisoned() {
            return Err(ScheduleError::Poisoned(event));
        }
        let now = clock.now();
        if self.last_time.is_some_and(|last| now < last) {
            return Err(ScheduleError::Backwards(event));
        }
        let Some(deadline) = now.checked_add(delay) else {
            return Err(ScheduleError::Overflow(event));
        };
        let Some(next) = self.next_timer.checked_add(1) else {
            return Err(ScheduleError::Overflow(event));
        };
        {
            let mut inbox = self.sink.inbox.borrow_mut();
            if inbox.pending == inbox.capacity {
                return Err(ScheduleError::Full(event));
            }
            inbox.pending += 1;
        }
        self.last_time = Some(now);
        self.next_timer = next;
        let id = TimerId(next);
        self.timers.push(Timer {
            id,
            deadline,
            state: self.machine().state(),
            epoch: self.machine().epoch(),
            event: Some(event),
        });
        Ok(id)
    }

    /// Move due timers to the external FIFO. Call this from the host's clock driver.
    /// A backwards clock is rejected without delivering any timers.
    pub fn tick(&mut self, clock: &impl Clock) -> Result<usize, ClockError> {
        let now = clock.now();
        if self.last_time.is_some_and(|last| now < last) {
            return Err(ClockError::Backwards);
        }
        self.last_time = Some(now);
        self.reconcile_timers();
        let mut due: Vec<_> = self
            .timers
            .iter()
            .enumerate()
            .filter(|(_, timer)| timer.event.is_some() && timer.deadline <= now)
            .map(|(index, _)| index)
            .collect();
        due.sort_by_key(|index| (self.timers[*index].deadline, self.timers[*index].id.0));
        let count = due.len();
        let waker = {
            let mut inbox = self.sink.inbox.borrow_mut();
            for index in due {
                let timer = &mut self.timers[index];
                inbox.external.push_back(Envelope {
                    event: timer.event.take().unwrap(),
                    timer: Some(timer.id),
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
            .iter()
            .filter(|timer| timer.event.is_some())
            .map(|timer| timer.deadline)
            .min()
    }

    /// Queued/deferred cancelled timeouts release capacity when skipped by `drain`.
    pub fn cancel_timer(&mut self, id: TimerId) -> bool {
        let Some(index) = self.timers.iter().position(|timer| timer.id == id) else {
            return false;
        };
        if self.timers.remove(index).event.is_some() {
            self.sink.inbox.borrow_mut().pending -= 1;
        }
        true
    }

    pub(super) fn reconcile_timers(&mut self) {
        let state = self.machine().state();
        let epoch = self.machine().epoch();
        let poisoned = self.machine().is_poisoned();
        let mut released = 0;
        self.timers.retain(|timer| {
            let live = !poisoned && timer.state == state && timer.epoch == epoch;
            if !live && timer.event.is_some() {
                released += 1;
            }
            live
        });
        self.sink.inbox.borrow_mut().pending -= released;
    }

    pub(super) fn live_timer(&self, event: &Envelope<M::Event>) -> bool {
        event
            .timer
            .is_none_or(|id| self.timers.iter().any(|timer| timer.id == id))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ClockError {
    Backwards,
}
