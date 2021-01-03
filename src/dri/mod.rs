// MIT/Apache2 License

#![cfg(feature = "dri")]

pub(crate) mod config;
pub(crate) mod dri2;
pub(crate) mod dri3;
pub(crate) mod extensions;
pub(crate) mod ffi;
pub(crate) mod load;

mod rules;
pub(crate) use rules::*;

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub(crate) struct ExtensionContainer(pub *const ffi::__DRIextension);

unsafe impl Send for ExtensionContainer {}
unsafe impl Sync for ExtensionContainer {}
