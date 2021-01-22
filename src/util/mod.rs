// MIT/Apache2 License

use std::{
    env,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "async")]
use std::{future::Future, pin::Pin};

/*#[cfg(feature = "dri")]
mod fence;
#[cfg(feature = "dri")]
pub use fence::*;*/

/// Convert an environment variable to a boolean.
pub(crate) fn env_to_boolean(name: &str, default_val: bool) -> bool {
    match env::var(name) {
        Err(_) => default_val,
        Ok(s) => match s.to_lowercase().as_str() {
            "1" | "true" | "y" | "yes" => true,
            "0" | "false" | "n" | "no" => false,
            _ => default_val,
        },
    }
}

/// Generic result type.
#[cfg(feature = "async")]
pub type GenericFuture<'future, T> = Pin<Box<dyn Future<Output = T> + Send + 'future>>;

/// Call a function on drop.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub(crate) struct CallOnDrop<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> CallOnDrop<F> {
    #[inline]
    pub fn new(f: F) -> Self {
        Self(Some(f))
    }
}

impl<F: FnOnce()> Drop for CallOnDrop<F> {
    #[inline]
    fn drop(&mut self) {
        (self.0.take().unwrap())()
    }
}

/// Offload an async function on drop.
#[cfg(feature = "async")]
pub(crate) struct OffloadOnDrop<Fut, F = fn() -> Fut>(Option<F>, PhantomData<Fut>);

#[cfg(feature = "async")]
impl<Fut, F> OffloadOnDrop<F> {
    #[inline]
    pub fn new(f: F) -> Self {
        Self(Some(f), PhantomData)
    }
}

#[cfg(feature = "async")]
impl<Fut: Future<Output = ()> + Send, F: FnOnce() -> Fut> Drop for OffloadOnDrop<Fut, F> {
    #[inline]
    fn drop(&mut self) {
        offload::offload((self.0.take().unwrap())());
    }
}

/// Mark an object that usually isn't thread safe as thread safe.
#[derive(Debug, Copy, Clone, Default)]
#[repr(transparent)]
pub(crate) struct ThreadSafe<T: ?Sized>(T);

// SAFETY: This is safe because we only allow unsafe code to construct instances of ThreadSafe
unsafe impl<T: ?Sized> Send for ThreadSafe<T> {}
unsafe impl<T: ?Sized> Sync for ThreadSafe<T> {}

impl<T> ThreadSafe<T> {
    /// Create a new instance of a thread safe object.
    #[inline]
    pub unsafe fn new(obj: T) -> Self {
        // SAFETY: The caller must verify that we don't do anything non thread safe with the
        //         interior object.
        Self(obj)
    }

    /// Get the interior object of this one.
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: ?Sized> Deref for ThreadSafe<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> DerefMut for ThreadSafe<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}
