//! Short-lived mailbox access. No user callback, poll, or await runs under a guard.
#[cfg(not(feature = "runtime-send"))]
use alloc::rc::Rc;
#[cfg(not(feature = "runtime-send"))]
use core::cell::{Ref, RefCell, RefMut};
#[cfg(feature = "runtime-send")]
use std::sync::{Arc, Mutex, MutexGuard};

pub(super) struct Shared<T> {
    #[cfg(not(feature = "runtime-send"))]
    inner: Rc<RefCell<T>>,
    #[cfg(feature = "runtime-send")]
    inner: Arc<Mutex<T>>,
}
impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self {
            #[cfg(not(feature = "runtime-send"))]
            inner: Rc::new(RefCell::new(value)),
            #[cfg(feature = "runtime-send")]
            inner: Arc::new(Mutex::new(value)),
        }
    }
    #[cfg(not(feature = "runtime-send"))]
    pub fn borrow(&self) -> Ref<'_, T> {
        self.inner.borrow()
    }
    #[cfg(not(feature = "runtime-send"))]
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.inner.borrow_mut()
    }
    #[cfg(feature = "runtime-send")]
    pub fn borrow(&self) -> MutexGuard<'_, T> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
    #[cfg(feature = "runtime-send")]
    pub fn borrow_mut(&self) -> MutexGuard<'_, T> {
        self.borrow()
    }
}
