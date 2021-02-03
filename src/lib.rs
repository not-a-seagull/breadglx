// MIT/Apache2 License

#![feature(const_fn)] // need this for creating PCI tables for DRI
#![feature(new_uninit)] // for DRI3 we need to initialize buffers without putting stuff in them
#![feature(raw_ref_macros)]
// need this for partial initialization of uninit memory
// makes things about 100 times more convenient, could be removed
// but since we're already pinned to nightly, why not?
#![feature(trait_alias)]
#![cfg(all(not(target_os = "macos"), unix))]
#![allow(non_snake_case, unused_unsafe)]
#![deny(clippy::future_not_send)]

pub(crate) mod auto;
pub(crate) mod cstr;
pub(crate) mod dll;
pub(crate) mod indirect;
pub(crate) mod mesa;
pub(crate) mod util;

#[cfg(feature = "async")]
pub(crate) mod offload;

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
