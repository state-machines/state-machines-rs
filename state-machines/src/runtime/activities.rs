use super::{Envelope, EventSink, Machine, RunError, Runner};
use alloc::boxed::Box;
use core::{
    fmt,
    future::{Future, poll_fn},
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ActivityId(u64);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum InvokeFailure {
    Full,
    Poisoned,
    Overflow,
    ZeroBudget,
}

/// Rejected futures remain owned by the caller, without being polled.
pub struct InvokeError<F> {
    pub reason: InvokeFailure,
    pub future: F,
}
impl<F> fmt::Debug for InvokeError<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InvokeError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
pub struct ChildInvokeError<C: Machine> {
    pub reason: InvokeFailure,
    pub child: Runner<C>,
}
impl<C: Machine> fmt::Debug for ChildInvokeError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChildInvokeError")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

type Task<E> = Pin<Box<dyn Future<Output = E> + 'static>>;
pub(super) struct Activity<S, E> {
    pub id: ActivityId,
    state: S,
    epoch: u64,
    future: Option<Task<E>>,
}

impl<M: Machine> Runner<M> {
    fn reserve_activity(&mut self) -> Result<ActivityId, InvokeFailure> {
        self.reconcile_activities();
        self.reconcile_timers();
        if self.machine().is_poisoned() {
            return Err(InvokeFailure::Poisoned);
        }
        let next = self
            .next_activity
            .checked_add(1)
            .ok_or(InvokeFailure::Overflow)?;
        {
            let mut inbox = self.sink.inbox.borrow_mut();
            if inbox.pending == inbox.capacity {
                return Err(InvokeFailure::Full);
            }
            inbox.pending += 1;
        }
        self.next_activity = next;
        Ok(ActivityId(next))
    }

    fn store_activity(&mut self, id: ActivityId, future: Task<M::Event>) {
        self.activities.push(Activity {
            id,
            state: self.machine().state(),
            epoch: self.machine().epoch(),
            future: Some(future),
        });
    }

    /// Invoke an owned future for the current leaf visit. It reserves one mailbox slot.
    /// Map success/failure to parent events inside the future; completion is raised once.
    /// Cancellation drops the future, it does not abort detached executor tasks.
    pub fn invoke_future<F>(&mut self, future: F) -> Result<ActivityId, InvokeError<F>>
    where
        F: Future<Output = M::Event> + 'static,
    {
        let id = match self.reserve_activity() {
            Ok(id) => id,
            Err(reason) => return Err(InvokeError { reason, future }),
        };
        self.store_activity(id, Box::pin(future));
        Ok(id)
    }

    /// Cancel an owned activity. Queued completions are skipped at delivery.
    pub fn cancel_activity(&mut self, id: ActivityId) -> bool {
        let Some(index) = self
            .activities
            .iter()
            .position(|activity| activity.id == id)
        else {
            return false;
        };
        if self.activities.remove(index).future.is_some() {
            self.sink.inbox.borrow_mut().pending -= 1;
        }
        true
    }

    pub fn activity_count(&self) -> usize {
        self.activities.len()
    }

    /// Poll each pending activity once with the caller's actual waker. Never busy-waits.
    /// Completed futures are dropped; their visit leases survive until event delivery.
    pub fn poll_activities(&mut self, cx: &mut Context<'_>) -> usize {
        self.reconcile_activities();
        let mut ready = 0;
        for activity in &mut self.activities {
            let result = match activity.future.as_mut() {
                Some(future) => future.as_mut().poll(cx),
                None => continue,
            };
            if let Poll::Ready(event) = result {
                activity.future = None;
                self.sink.inbox.borrow_mut().internal.push_back(Envelope {
                    event,
                    timer: None,
                    activity: Some(activity.id),
                });
                ready += 1;
            }
        }
        let waker = if ready > 0 {
            self.sink.inbox.borrow_mut().waker.take()
        } else {
            None
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        ready
    }

    /// Wait until queued input or an activity completion is available. Does not dispatch.
    /// The host remains responsible for clock driving (cancel this wait before `tick`).
    pub async fn wait_for_work(&mut self) -> Result<(), RunError<M::Error>> {
        poll_fn(|cx| {
            if self.machine().is_poisoned() {
                self.reconcile_activities();
                self.reconcile_timers();
                return Poll::Ready(Err(RunError::Poisoned));
            }
            // Register before polling activities, whose callbacks may enqueue input.
            self.sink.inbox.borrow_mut().waker = Some(cx.waker().clone());
            self.poll_activities(cx);
            let mut inbox = self.sink.inbox.borrow_mut();
            if !inbox.internal.is_empty() || !inbox.external.is_empty() {
                inbox.waker = None;
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
        .await
    }

    /// Invoke a reusable child runner, returning its event channel.
    /// Child dispatch failures map to one parent error event with the owned child.
    /// Step-budget exhaustion yields cooperatively and resumes on the next poll.
    // Preserve the entire child without allocating another box on backpressure.
    #[allow(clippy::result_large_err)]
    pub fn invoke_child<C, Done, Failed>(
        &mut self,
        mut child: Runner<C>,
        max_steps: usize,
        done: Done,
        failed: Failed,
    ) -> Result<(ActivityId, EventSink<C::Event>), ChildInvokeError<C>>
    where
        C: Machine + 'static,
        C::State: 'static,
        C::Event: 'static,
        C::Error: 'static,
        M::Event: 'static,
        Done: FnOnce(C) -> M::Event + 'static,
        Failed: FnOnce(RunError<C::Error>, C) -> M::Event + 'static,
    {
        if max_steps == 0 {
            return Err(ChildInvokeError {
                reason: InvokeFailure::ZeroBudget,
                child,
            });
        }
        let id = match self.reserve_activity() {
            Ok(id) => id,
            Err(reason) => return Err(ChildInvokeError { reason, child }),
        };
        let sink = child.sink();
        self.store_activity(
            id,
            Box::pin(async move {
                loop {
                    if child.machine().is_finished() {
                        return done(child.into_machine());
                    }
                    match child.drain(max_steps).await {
                        Ok(_) => {
                            if child.machine().is_finished() {
                                return done(child.into_machine());
                            }
                            if let Err(error) = child.wait_for_work().await {
                                return failed(error, child.into_machine());
                            }
                        }
                        Err(RunError::StepLimit { .. }) => {
                            // Limit work per parent poll, including infinitely raised child events.
                            let mut yielded = false;
                            poll_fn(|cx| {
                                if yielded {
                                    Poll::Ready(())
                                } else {
                                    yielded = true;
                                    cx.waker().wake_by_ref();
                                    Poll::Pending
                                }
                            })
                            .await;
                        }
                        Err(error) => return failed(error, child.into_machine()),
                    }
                }
            }),
        );
        Ok((id, sink))
    }

    pub(super) fn reconcile_activities(&mut self) {
        let state = self.machine().state();
        let epoch = self.machine().epoch();
        let poisoned = self.machine().is_poisoned();
        let mut released = 0;
        self.activities.retain(|activity| {
            let live = !poisoned && activity.state == state && activity.epoch == epoch;
            if !live && activity.future.is_some() {
                released += 1;
            }
            live
        });
        self.sink.inbox.borrow_mut().pending -= released;
    }

    pub(super) fn live_activity(&self, event: &Envelope<M::Event>) -> bool {
        event
            .activity
            .is_none_or(|id| self.activities.iter().any(|activity| activity.id == id))
    }
}
