use super::{Clock, ClockError, Machine, QueueError, RunError, Runner};
use core::{fmt, task::Context};

#[derive(Debug)]
pub enum RegionError<E, Error> {
    Queue(QueueError<E>),
    Run(RunError<Error>),
    Clock(ClockError),
    Nested(Error),
}
impl<E, Error> fmt::Display for RegionError<E, Error> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Queue(_) => "region mailbox rejected the event",
            Self::Run(_) => "region runner failed",
            Self::Clock(_) => "region clock rejected the tick",
            Self::Nested(_) => "nested region failed",
        })
    }
}
impl<E, Error> core::error::Error for RegionError<E, Error>
where
    E: fmt::Debug + 'static,
    Error: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(match self {
            Self::Queue(error) => error,
            Self::Run(error) => error,
            Self::Clock(error) => error,
            Self::Nested(error) => error,
        })
    }
}

/// A lifecycle-aware orthogonal region. All dispatch goes through the same
/// mailbox/microstep driver as an ordinary Runner, including deferral.
pub struct Region<M: Machine> {
    runner: Runner<M>,
}
impl<M: Machine> Region<M> {
    pub fn new(machine: M, capacity: usize) -> Self {
        Self {
            runner: Runner::new(machine, capacity),
        }
    }
    pub fn runner(&self) -> &Runner<M> {
        &self.runner
    }
    pub fn runner_mut(&mut self) -> &mut Runner<M> {
        &mut self.runner
    }
    pub fn into_machine(self) -> M {
        self.runner.into_machine()
    }
    fn machine_mut(&mut self) -> &mut M {
        self.runner.machine_mut()
    }
}
impl<M: Machine> Machine for Region<M> {
    type Event = M::Event;
    type Error = RegionError<M::Event, M::Error>;
    type State = M::State;
    fn state(&self) -> Self::State {
        self.runner.machine().state()
    }
    fn epoch(&self) -> u64 {
        self.runner.machine().epoch()
    }
    fn scope_epoch(&self, scope: &str) -> Option<u64> {
        self.runner.machine().scope_epoch(scope)
    }
    fn is_finished(&self) -> bool {
        self.runner.machine().is_finished()
    }
    fn is_poisoned(&self) -> bool {
        self.runner.machine().is_poisoned()
    }
    async fn dispatch(&mut self, event: Self::Event) -> Result<(), Self::Error> {
        self.runner.enqueue(event).map_err(RegionError::Queue)?;
        self.drive_regions(64).await.map(|_| ())
    }
    fn start_regions(&mut self, clock: &impl Clock) -> Result<(), Self::Error> {
        self.machine_mut()
            .start_regions(clock)
            .map_err(RegionError::Nested)?;
        self.runner
            .start(clock)
            .map_err(|error| RegionError::Run(RunError::Setup(error)))
    }
    fn tick_regions(&mut self, clock: &impl Clock) -> Result<(), Self::Error> {
        self.machine_mut()
            .tick_regions(clock)
            .map_err(RegionError::Nested)?;
        self.runner
            .tick(clock)
            .map_err(RegionError::Clock)
            .map(|_| ())
    }
    fn poll_regions(&mut self, cx: &mut Context<'_>) -> usize {
        self.machine_mut().poll_regions(cx) + self.runner.poll_activities(cx)
    }
    async fn drive_regions(&mut self, max_steps: usize) -> Result<usize, Self::Error> {
        let nested = self
            .machine_mut()
            .drive_regions(max_steps)
            .await
            .map_err(RegionError::Nested)?;
        self.runner
            .drain(max_steps)
            .await
            .map(|steps| steps + nested)
            .map_err(RegionError::Run)
    }
}
