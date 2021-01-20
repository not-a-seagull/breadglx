// MIT/Apache2 License

use std::env;

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
pub(crate) struct CallOnDrop<F>(Option<F>);

impl<F> CallOnDrop<F> {
    #[inline]
    pub fn new(f: F) -> Self {
        Self(Some(f))
    }
}

impl<F: FnOnce()> Drop for CallonDrop<F> {
    #[inline]
    fn drop(&mut self) {
        (self.0.take().unwrap())()
    }
}

/// Offload an async function on drop.
#[cfg(feature = "async")]
struct OffloadOnDrop<Fut, F = fn() -> Fut>(Option<F>, PhantomData<Fut>);

#[cfg(feature = "async")]
impl<Fut, F> OffloadOnDrop<F> {
    #[inline]
    fn new(f: F) -> Self {
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
