// MIT/Apache2 License

#![feature(const_fn)]
#![feature(raw_ref_macros)]
#![cfg(all(not(target_os = "macos"), unix))]
#![allow(non_snake_case)]

pub(crate) mod auto;
pub(crate) mod cstr;
pub(crate) mod dll;
pub(crate) mod indirect;
pub(crate) mod mesa;
pub(crate) mod util;

pub mod config;
pub mod context;
pub mod display;
pub mod drawable;
pub mod screen;

pub use config::*;
pub use context::*;
pub use display::*;
pub use screen::*;

#[cfg(feature = "dri")]
pub(crate) mod dri;
