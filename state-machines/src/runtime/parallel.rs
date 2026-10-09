use super::{Clock, Machine};
use alloc::collections::VecDeque;
use core::fmt;

/// Explicit region routing; forked payloads are owned separately, never cloned.
#[derive(Debug)]
pub enum ParallelEvent<L, R> {
    Left(L),
    Right(R),
    Both { left: L, right: R },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParallelError<L, R> {
    Left(L),
    /// `left_committed` means the left dispatch succeeded in this fork.
    /// It does not promise that a failed right dispatch had no partial effects.
    Right {
        error: R,
        left_committed: bool,
    },
    Poisoned,
}
impl<L, R> fmt::Display for ParallelError<L, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Left(_) => "left region failed",
            Self::Right {
                left_committed: true,
                ..
            } => "right region failed after the left region committed",
            Self::Right { .. } => "right region failed",
            Self::Poisoned => "a region is poisoned",
        })
    }
}
impl<L, R> core::error::Error for ParallelError<L, R>
where
    L: core::error::Error + 'static,
    R: core::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Left(error) => Some(error),
            Self::Right { error, .. } => Some(error),
            Self::Poisoned => None,
        }
    }
}

/// Two orthogonal regions with deterministic left-then-right fork dispatch.
/// Region effects are not transactional; nesting composes more regions.
///
/// ```
/// use state_machines::{state_machine, runtime::Parallel};
/// # async fn example() {
/// state_machine! {
///     name: Worker, dynamic: true, initial: Ready,
///     states: [Ready, Done], final_states: [Done],
///     events { finish { transition: { from: Ready, to: Done } } }
/// }
/// let mut regions = Parallel::new(DynamicWorker::new(()), DynamicWorker::new(()));
/// regions.fork(WorkerEvent::Finish, WorkerEvent::Finish).await.unwrap();
/// std::assert_matches!(regions.take_join(), Some((WorkerState::Done, WorkerState::Done)));
/// assert!(regions.take_join().is_none());
/// # }
/// ```
pub struct Parallel<L: Machine, R: Machine> {
    left: L,
    right: R,
    was_finished: bool,
    joins: VecDeque<(L::State, R::State)>,
}
impl<L: Machine, R: Machine> Parallel<L, R> {
    /// Construction/restore is inert, including already completed regions.
    pub fn new(left: L, right: R) -> Self {
        let was_finished = left.is_finished() && right.is_finished();
        Self {
            left,
            right,
            was_finished,
            joins: VecDeque::new(),
        }
    }
    pub fn left(&self) -> &L {
        &self.left
    }
    pub fn right(&self) -> &R {
        &self.right
    }
    pub fn left_mut(&mut self) -> &mut L {
        &mut self.left
    }
    pub fn right_mut(&mut self) -> &mut R {
        &mut self.right
    }
    pub fn current_state(&self) -> (L::State, R::State) {
        (self.left.state(), self.right.state())
    }
    pub fn is_finished(&self) -> bool {
        !self.is_poisoned() && self.left.is_finished() && self.right.is_finished()
    }
    pub fn is_poisoned(&self) -> bool {
        self.left.is_poisoned() || self.right.is_poisoned()
    }
    pub fn into_regions(self) -> (L, R) {
        (self.left, self.right)
    }

    /// One owned configuration per transition into all-regions-finished.
    pub fn take_join(&mut self) -> Option<(L::State, R::State)> {
        self.joins.pop_front()
    }

    pub async fn fork(
        &mut self,
        left: L::Event,
        right: R::Event,
    ) -> Result<(), ParallelError<L::Error, R::Error>> {
        self.handle(ParallelEvent::Both { left, right }).await
    }

    pub async fn handle(
        &mut self,
        event: ParallelEvent<L::Event, R::Event>,
    ) -> Result<(), ParallelError<L::Error, R::Error>> {
        if self.is_poisoned() {
            return Err(ParallelError::Poisoned);
        }
        let result = match event {
            ParallelEvent::Left(event) => {
                self.left.dispatch(event).await.map_err(ParallelError::Left)
            }
            ParallelEvent::Right(event) => {
                self.right
                    .dispatch(event)
                    .await
                    .map_err(|error| ParallelError::Right {
                        error,
                        left_committed: false,
                    })
            }
            ParallelEvent::Both { left, right } => match self.left.dispatch(left).await {
                Err(error) => Err(ParallelError::Left(error)),
                Ok(()) => self
                    .right
                    .dispatch(right)
                    .await
                    .map_err(|error| ParallelError::Right {
                        error,
                        left_committed: true,
                    }),
            },
        };
        self.observe_join();
        result
    }

    fn observe_join(&mut self) {
        // Also observe partial progress before a failed automatic microstep.
        let finished = self.is_finished();
        if finished && !self.was_finished {
            self.joins.push_back(self.current_state());
        }
        self.was_finished = finished;
    }
}
impl<L: Machine, R: Machine> Machine for Parallel<L, R> {
    type Event = ParallelEvent<L::Event, R::Event>;
    type Error = ParallelError<L::Error, R::Error>;
    type State = (L::State, R::State);
    fn state(&self) -> Self::State {
        self.current_state()
    }
    fn epoch(&self) -> u64 {
        self.left.epoch().wrapping_add(self.right.epoch())
    }
    fn is_finished(&self) -> bool {
        self.is_finished()
    }
    fn is_poisoned(&self) -> bool {
        self.is_poisoned()
    }
    async fn dispatch(&mut self, event: Self::Event) -> Result<(), Self::Error> {
        self.handle(event).await
    }
    fn start_regions(&mut self, clock: &impl Clock) -> Result<(), Self::Error> {
        self.left
            .start_regions(clock)
            .map_err(ParallelError::Left)?;
        self.right
            .start_regions(clock)
            .map_err(|error| ParallelError::Right {
                error,
                left_committed: true,
            })
    }
    fn tick_regions(&mut self, clock: &impl Clock) -> Result<(), Self::Error> {
        self.left.tick_regions(clock).map_err(ParallelError::Left)?;
        self.right
            .tick_regions(clock)
            .map_err(|error| ParallelError::Right {
                error,
                left_committed: true,
            })
    }
    fn poll_regions(&mut self, cx: &mut core::task::Context<'_>) -> usize {
        self.left.poll_regions(cx) + self.right.poll_regions(cx)
    }
    async fn drive_regions(&mut self, max_steps: usize) -> Result<usize, Self::Error> {
        let result = match self.left.drive_regions(max_steps).await {
            Err(error) => Err(ParallelError::Left(error)),
            Ok(left) => self
                .right
                .drive_regions(max_steps)
                .await
                .map(|right| left + right)
                .map_err(|error| ParallelError::Right {
                    error,
                    left_committed: true,
                }),
        };
        self.observe_join();
        result
    }
}
