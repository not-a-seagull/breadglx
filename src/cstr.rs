// MIT/Apache2 License

use std::{
    cmp::{Eq, PartialEq},
    ffi::CStr,
    ops::Deref,
};

/// Internal use utility structure for a const CStr.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub(crate) struct ConstCstr<'a> {
    bytes: &'a [u8],
}

impl<'a> PartialEq<CStr> for ConstCstr<'a> {
    #[inline]
    fn eq(&self, other: &CStr) -> bool {
        self.deref() == other
    }
}

impl<'a, 'b> PartialEq<&'a CStr> for ConstCstr<'b> {
    #[inline]
    fn eq(&self, other: &&'a CStr) -> bool {
        self == *other
    }
}

impl<'a> Deref for ConstCstr<'a> {
    type Target = CStr;

    #[inline]
    fn deref(&self) -> &CStr {
        CStr::from_bytes_with_nul(self.bytes).expect("Unable to construct CStr")
    }
}

/// Create a constant c-string.
pub(crate) const fn const_cstr<'a>(t: &'a [u8]) -> ConstCstr<'a> {
    ConstCstr { bytes: t }
}
