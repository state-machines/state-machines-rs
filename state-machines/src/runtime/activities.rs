use super::{
    Envelope, EventSink, Machine, RunError, Runner, RuntimeFuture, RuntimeValue, Visit, WorkScope,
};
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
    InactiveScope,
}
impl fmt::Display for InvokeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full => "mailbox is full",
            Self::Poisoned => "machine is poisoned",
            Self::Overflow => "activity id overflow",
            Self::ZeroBudget => "child step budget is zero",
            Self::InactiveScope => "scope is not active",
        })
    }
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
impl<F> fmt::Display for InvokeError<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "activity rejected: {}", self.reason)
    }
}
impl<F> core::error::Error for InvokeError<F> {}
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
impl<C: Machine> fmt::Display for ChildInvokeError<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "child invocation rejected: {}", self.reason)
    }
}
impl<C: Machine> core::error::Error for ChildInvokeError<C> {}

cfg_select! {
    feature = "runtime-send" => {
        pub(super) type Task<E> = Pin<Box<dyn Future<Output = E> + Send + 'static>>;
    }
    _ => {
        pub(super) type Task<E> = Pin<Box<dyn Future<Output = E> + 'static>>;
    }
}

impl<M: Machine> Runner<M> {
    pub(super) fn reserve_activity(
        &mut self,
        scope: WorkScope,
    ) -> Result<(ActivityId, Visit<M::State>), InvokeFailure> {
        self.reconcile_work();
        if self.machine().is_poisoned() {
            return Err(InvokeFailure::Poisoned);
        }
        let visit = Visit::capture(self.machine(), scope).ok_or(InvokeFailure::InactiveScope)?;
        let next = self
            .next_activity
            .checked_add(1)
            .ok_or(InvokeFailure::Overflow)?;
        self.sink
            .inbox
            .borrow_mut()
            .reserve()
            .ok_or(InvokeFailure::Full)?;
        self.next_activity = next;
        Ok((ActivityId(next), visit))
    }

    pub(super) fn store_activity(
        &mut self,
        id: ActivityId,
        visit: Visit<M::State>,
        future: Task<M::Event>,
    ) {
        self.activities.insert(id, visit, future);
    }

    /// Invoke an owned future for the current leaf visit. It reserves one mailbox slot.
    /// Map success/failure to parent events inside the future; completion is raised once.
    /// Cancellation drops the future, it does not abort detached executor tasks.
    pub fn invoke_future<F>(&mut self, future: F) -> Result<ActivityId, InvokeError<F>>
    where
        F: RuntimeFuture<Output = M::Event> + 'static,
    {
        self.invoke_future_in(WorkScope::Leaf, future)
    }
    pub fn invoke_future_in<F>(
        &mut self,
        scope: WorkScope,
        future: F,
    ) -> Result<ActivityId, InvokeError<F>>
    where
        F: RuntimeFuture<Output = M::Event> + 'static,
    {
        let (id, visit) = match self.reserve_activity(scope) {
            Ok(value) => value,
            Err(reason) => return Err(InvokeError { reason, future }),
        };
        self.store_activity(id, visit, Box::pin(future));
        Ok(id)
    }

    /// Cancel an owned activity. Queued completions are skipped at delivery.
    pub fn cancel_activity(&mut self, id: ActivityId) -> bool {
        let result = self.activities.cancel(id);
        self.apply_cancellation(result)
    }

    pub fn activity_count(&self) -> usize {
        self.activities.entries.len()
    }

    /// Poll each pending activity once with the caller's actual waker. Never busy-waits.
    /// Completed futures are dropped; their visit leases survive until event delivery.
    pub fn poll_activities(&mut self, cx: &mut Context<'_>) -> usize {
        self.reconcile_work();
        let mut ready = 0;
        for activity in &mut self.activities.entries {
            let result = match activity.pending.as_mut() {
                Some(future) => future.as_mut().poll(cx),
                None => continue,
            };
            if let Poll::Ready(event) = result {
                activity.pending = None;
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
                self.reconcile_work();
                return Poll::Ready(Err(RunError::Poisoned));
            }
            // Register before polling activities, whose callbacks may enqueue input.
            // Waker clone/drop callbacks may themselves access this mailbox.
            let waker = cx.waker().clone();
            let previous = self.sink.inbox.borrow_mut().waker.replace(waker);
            drop(previous);
            self.poll_activities(cx);
            let (ready, waker) = {
                let mut inbox = self.sink.inbox.borrow_mut();
                let ready = !inbox.internal.is_empty() || !inbox.external.is_empty();
                (ready, if ready { inbox.waker.take() } else { None })
            };
            drop(waker);
            if ready {
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
        child: Runner<C>,
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
        Done: FnOnce(C) -> M::Event + RuntimeValue + 'static,
        Failed: FnOnce(RunError<C::Error>, C) -> M::Event + RuntimeValue + 'static,
    {
        self.invoke_child_in(WorkScope::Leaf, child, max_steps, done, failed)
    }

    #[allow(clippy::result_large_err)]
    pub fn invoke_child_in<C, Done, Failed>(
        &mut self,
        scope: WorkScope,
        child: Runner<C>,
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
        Done: FnOnce(C) -> M::Event + RuntimeValue + 'static,
        Failed: FnOnce(RunError<C::Error>, C) -> M::Event + RuntimeValue + 'static,
    {
        if max_steps == 0 {
            return Err(ChildInvokeError {
                reason: InvokeFailure::ZeroBudget,
                child,
            });
        }
        let (id, visit) = match self.reserve_activity(scope) {
            Ok(value) => value,
            Err(reason) => return Err(ChildInvokeError { reason, child }),
        };
        let sink = child.sink();
        self.store_activity(
            id,
            visit,
            Box::pin(async move {
                match run_child(child, max_steps).await {
                    Ok(child) => done(child),
                    Err((error, child)) => failed(error, child),
                }
            }),
        );
        Ok((id, sink))
    }
}

/// Drive an owned child cooperatively, recovering it on completion or failure.
/// Shared by explicit invocation and declarative activity factories.
pub async fn run_child<C: Machine>(
    mut child: Runner<C>,
    max_steps: usize,
) -> Result<C, (RunError<C::Error>, C)> {
    if max_steps == 0 {
        return Err((RunError::StepLimit { limit: 0 }, child.into_machine()));
    }
    loop {
        if child.machine().is_finished() {
            return Ok(child.into_machine());
        }
        match child.drain(max_steps).await {
            Ok(_) => {
                if child.machine().is_finished() {
                    return Ok(child.into_machine());
                }
                if let Err(error) = child.wait_for_work().await {
                    return Err((error, child.into_machine()));
                }
            }
            Err(RunError::StepLimit { .. }) => {
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
            Err(error) => return Err((error, child.into_machine())),
        }
    }
}
