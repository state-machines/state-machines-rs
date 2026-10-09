//! The Send runtime strengthens the same driver; it does not add an executor.
use core::future::Future;

cfg_select! {
    feature = "runtime-send" => {
        pub trait RuntimeValue: Send {}
        impl<T: Send + ?Sized> RuntimeValue for T {}

        pub trait RuntimeFuture: Future + Send {}
        impl<F: Future + Send> RuntimeFuture for F {}
    }
    _ => {
        pub trait RuntimeValue {}
        impl<T: ?Sized> RuntimeValue for T {}

        pub trait RuntimeFuture: Future {}
        impl<F: Future> RuntimeFuture for F {}
    }
}
