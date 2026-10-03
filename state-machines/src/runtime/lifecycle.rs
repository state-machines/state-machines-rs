//! Declarative entry setup uses the existing scoped timer/activity registries.
use super::{Clock, InvokeFailure, Machine, Runner, ScheduleError, WorkScope, activities::Task};

pub type ActivityFuture<E> = Task<E>;

pub enum LifecycleRule<M: Machine> {
    After {
        scope: &'static str,
        delay: u64,
        event: fn(&M) -> M::Event,
    },
    Invoke {
        scope: &'static str,
        start: fn(&M) -> ActivityFuture<M::Event>,
    },
    Defer {
        scope: &'static str,
        matches: fn(&M::Event) -> bool,
    },
}
impl<M: Machine> Copy for LifecycleRule<M> {}
impl<M: Machine> Clone for LifecycleRule<M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M: Machine> LifecycleRule<M> {
    fn scope(self) -> &'static str {
        match self {
            Self::After { scope, .. } | Self::Invoke { scope, .. } | Self::Defer { scope, .. } => {
                scope
            }
        }
    }
}
pub(super) struct Configured<M: Machine> {
    pub rule: LifecycleRule<M>,
    pub epoch: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetupFailure {
    Poisoned,
    ClockRequired,
    Schedule(ScheduleError<()>),
    Invoke(InvokeFailure),
}
#[derive(Debug, PartialEq, Eq)]
pub struct SetupError {
    pub scope: &'static str,
    pub reason: SetupFailure,
}
pub(super) struct ObservedClock(pub u64);
impl Clock for ObservedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

impl<M: Machine> Runner<M> {
    /// Explicit initial entry setup. Later committed scope entries are wired by drain.
    /// Constructors/restore remain inert; the host supplies monotonic logical time.
    pub fn start(&mut self, clock: &impl Clock) -> Result<(), SetupError> {
        if self.machine().is_poisoned() {
            return Err(SetupError {
                scope: "",
                reason: SetupFailure::Poisoned,
            });
        }
        self.tick(clock).map_err(|_| SetupError {
            scope: "",
            reason: SetupFailure::Schedule(ScheduleError::Backwards(())),
        })?;
        self.setup_entries()
    }

    pub(super) fn setup_entries(&mut self) -> Result<(), SetupError> {
        for index in 0..self.lifecycle.len() {
            let rule = self.lifecycle[index].rule;
            let scope = rule.scope();
            let epoch = self.machine().scope_epoch(scope);
            if epoch.is_none() {
                self.lifecycle[index].epoch = None;
                continue;
            }
            if self.lifecycle[index].epoch == epoch {
                continue;
            }
            let result = match rule {
                LifecycleRule::After { delay, event, .. } => {
                    let now = self.last_time.ok_or(SetupError {
                        scope,
                        reason: SetupFailure::ClockRequired,
                    })?;
                    match self.reserve_timer(WorkScope::Named(scope), &ObservedClock(now), delay) {
                        Err(error) => Err(SetupFailure::Schedule(error)),
                        Ok((id, visit, deadline)) => {
                            let event = event(self.machine());
                            self.store_timer(id, visit, deadline, event);
                            self.tick(&ObservedClock(now))
                                .expect("reserved timer uses the observed monotonic clock");
                            Ok(())
                        }
                    }
                }
                LifecycleRule::Invoke { start, .. } => {
                    match self.reserve_activity(WorkScope::Named(scope)) {
                        Err(error) => Err(SetupFailure::Invoke(error)),
                        Ok((id, visit)) => {
                            let future = start(self.machine());
                            self.store_activity(id, visit, future);
                            Ok(())
                        }
                    }
                }
                LifecycleRule::Defer { .. } => Ok(()),
            };
            result.map_err(|reason| SetupError { scope, reason })?;
            self.lifecycle[index].epoch = epoch;
        }
        Ok(())
    }
}
