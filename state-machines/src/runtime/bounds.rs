//! The Send runtime strengthens the same driver; it does not add an executor.
use core::future::Future;

#[cfg(feature = "runtime-send")]
pub trait RuntimeValue: Send {}
#[cfg(feature = "runtime-send")]
impl<T: Send + ?Sized> RuntimeValue for T {}
#[cfg(not(feature = "runtime-send"))]
pub trait RuntimeValue {}
#[cfg(not(feature = "runtime-send"))]
impl<T: ?Sized> RuntimeValue for T {}

#[cfg(feature = "runtime-send")]
pub trait RuntimeFuture: Future + Send {}
#[cfg(feature = "runtime-send")]
impl<F: Future + Send> RuntimeFuture for F {}
#[cfg(not(feature = "runtime-send"))]
pub trait RuntimeFuture: Future {}
#[cfg(not(feature = "runtime-send"))]
impl<F: Future> RuntimeFuture for F {}
