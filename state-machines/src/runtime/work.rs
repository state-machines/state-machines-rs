//! Shared visit leases and reservation bookkeeping for timers and activities.
use super::Machine;
use alloc::vec::Vec;

#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) struct Visit<S> {
    state: S,
    epoch: u64,
}
impl<S: Copy + Eq> Visit<S> {
    pub fn capture<M: Machine<State = S>>(machine: &M) -> Option<Self> {
        (!machine.is_poisoned()).then(|| Self {
            state: machine.state(),
            epoch: machine.epoch(),
        })
    }
}

pub(super) struct Entry<I, S, W> {
    pub id: I,
    visit: Visit<S>,
    // None means output is already queued; the lease survives until delivery.
    pub pending: Option<W>,
}
pub(super) struct Registry<I, S, W> {
    pub entries: Vec<Entry<I, S, W>>,
}
impl<I: Copy + Eq, S: Copy + Eq, W> Registry<I, S, W> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
    pub fn insert(&mut self, id: I, visit: Visit<S>, work: W) {
        self.entries.push(Entry {
            id,
            visit,
            pending: Some(work),
        });
    }
    pub fn contains(&self, id: I) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }
    /// Some(true) releases an unqueued reservation; queued output releases at delivery.
    pub fn cancel(&mut self, id: I) -> Option<bool> {
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        Some(self.entries.remove(index).pending.is_some())
    }
    pub fn retire(&mut self, id: I) {
        let _ = self.cancel(id);
    }
    /// Drop work whose visit ended, returning the number of unqueued reservations.
    pub fn reconcile(&mut self, visit: Option<Visit<S>>) -> usize {
        let mut released = 0;
        self.entries.retain(|entry| {
            let live = Some(entry.visit) == visit;
            if !live && entry.pending.is_some() {
                released += 1;
            }
            live
        });
        released
    }
}
