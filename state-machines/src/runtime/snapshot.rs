//! Persist control/history/owned data, never ephemeral execution resources.
use super::{Machine, Parallel, Region};
use crate::SnapshotError;
use alloc::string::String;

/// Owned persistence for machines and recursively composed regions.
///
/// Implementations must validate the whole envelope without consuming it.
/// A live machine's capture must succeed; poisoned capture returns the original
/// machine. Restoration after successful validation is infallible and inert.
pub trait SnapshotMachine: Machine + Sized {
    type Snapshot;
    fn validate_snapshot(snapshot: &Self::Snapshot) -> Result<(), SnapshotError>;
    /// Capture a live machine; use `try_into_snapshot` if it may be poisoned.
    fn into_snapshot(self) -> Self::Snapshot;
    /// Only call after `validate_snapshot` succeeded for this exact envelope.
    /// `capacity` configures fresh region mailboxes, not persisted queue contents.
    #[doc(hidden)]
    fn from_validated_snapshot(snapshot: Self::Snapshot, capacity: usize) -> Self;

    fn try_into_snapshot(self) -> Result<Self::Snapshot, Self> {
        if self.is_poisoned() {
            Err(self)
        } else {
            Ok(self.into_snapshot())
        }
    }
    /// Validation failure returns the entire original owned envelope.
    fn from_snapshot(
        snapshot: Self::Snapshot,
        capacity: usize,
    ) -> Result<Self, (Self::Snapshot, SnapshotError)> {
        match Self::validate_snapshot(&snapshot) {
            Ok(()) => Ok(Self::from_validated_snapshot(snapshot, capacity)),
            Err(error) => Err((snapshot, error)),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelSnapshot<L, R> {
    pub version: u32,
    pub machine: String,
    pub left: L,
    pub right: R,
}

impl<L: SnapshotMachine, R: SnapshotMachine> SnapshotMachine for Parallel<L, R> {
    type Snapshot = ParallelSnapshot<L::Snapshot, R::Snapshot>;
    fn validate_snapshot(snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        SnapshotError::validate_header(snapshot.version, &snapshot.machine, "Parallel")?;
        L::validate_snapshot(&snapshot.left)?;
        R::validate_snapshot(&snapshot.right)
    }
    fn into_snapshot(self) -> Self::Snapshot {
        assert!(!self.is_poisoned(), "cannot snapshot poisoned regions");
        let (left, right) = self.into_regions();
        ParallelSnapshot {
            version: 1,
            machine: "Parallel".into(),
            left: left.into_snapshot(),
            right: right.into_snapshot(),
        }
    }
    fn from_validated_snapshot(snapshot: Self::Snapshot, capacity: usize) -> Self {
        Self::new(
            L::from_validated_snapshot(snapshot.left, capacity),
            R::from_validated_snapshot(snapshot.right, capacity),
        )
    }
}
impl<M: SnapshotMachine> SnapshotMachine for Region<M> {
    type Snapshot = M::Snapshot;
    fn validate_snapshot(snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        M::validate_snapshot(snapshot)
    }
    fn into_snapshot(self) -> Self::Snapshot {
        self.into_machine().into_snapshot()
    }
    fn from_validated_snapshot(snapshot: Self::Snapshot, capacity: usize) -> Self {
        Self::new(M::from_validated_snapshot(snapshot, capacity), capacity)
    }
}
