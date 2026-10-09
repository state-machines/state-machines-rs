//! Short-lived mailbox access. No user callback, poll, or await runs under a guard.

cfg_select! {
    feature = "runtime-send" => {
        use std::sync::{Arc, Mutex, MutexGuard};
        type Inner<T> = Arc<Mutex<T>>;
        type Ref<'a, T> = MutexGuard<'a, T>;
        type RefMut<'a, T> = MutexGuard<'a, T>;

        fn wrap<T>(value: T) -> Inner<T> {
            Arc::new(Mutex::new(value))
        }
        fn shared<T>(inner: &Inner<T>) -> Ref<'_, T> {
            inner.lock().unwrap_or_else(|error| error.into_inner())
        }
        fn exclusive<T>(inner: &Inner<T>) -> RefMut<'_, T> {
            shared(inner)
        }
    }
    _ => {
        use alloc::rc::Rc;
        use core::cell::{Ref, RefCell, RefMut};
        type Inner<T> = Rc<RefCell<T>>;

        fn wrap<T>(value: T) -> Inner<T> {
            Rc::new(RefCell::new(value))
        }
        fn shared<T>(inner: &Inner<T>) -> Ref<'_, T> {
            inner.borrow()
        }
        fn exclusive<T>(inner: &Inner<T>) -> RefMut<'_, T> {
            inner.borrow_mut()
        }
    }
}

pub(super) struct Shared<T> {
    inner: Inner<T>,
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
        Self { inner: wrap(value) }
    }
    pub fn borrow(&self) -> Ref<'_, T> {
        shared(&self.inner)
    }
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        exclusive(&self.inner)
    }
}
