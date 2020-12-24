// MIT/Apache2 License

use std::env;

#[cfg(feature = "async")]
use std::{future::Future, pin::Pin};

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
pub type GenericFuture<'future, T> = Pin<Box<dyn Future<Output = T> + Send + Sync + 'future>>;
