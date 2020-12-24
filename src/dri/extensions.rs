// MIT/Apache2 License

use super::ffi;
use crate::{
    cstr::{const_cstr, ConstCstr},
    dll::Dll,
};
use std::{ffi::CString, ptr, slice, str};

const DRI_EXTENSIONS_NAME: ConstCstr<'static> = const_cstr(ffi::__DRI_DRIVER_EXTENSIONS);

#[inline]
pub fn load_extensions<'a, 'b>(
    dll: &'a Dll,
    driver_name: &str,
) -> breadx::Result<&'b [*const ffi::__DRIextension]>
where
    'b: 'a,
{
    let mut get_ext_name: Vec<u8> =
        Vec::with_capacity(ffi::__DRI_DRIVER_GET_EXTENSIONS.len() + driver_name.len() + 1);
    get_ext_name.extend_from_slice(ffi::__DRI_DRIVER_GET_EXTENSIONS);
    get_ext_name.push(b'_');
    get_ext_name.extend_from_slice(driver_name.as_bytes());

    get_ext_name.iter_mut().for_each(|r| {
        if *r == b'-' {
            *r = b'_';
        }
    });

    // try to use the get_extensions method
    let get_ext_name =
        CString::new(get_ext_name).expect("Failed to construct get_extensions method name.");
    let get_extensions: Option<unsafe extern "C" fn() -> *const *const ffi::__DRIextension> =
        unsafe { dll.function(&get_ext_name) };

    let mut extensions: *const *const ffi::__DRIextension = match get_extensions {
        Some(get_extensions) => unsafe { get_extensions() },
        None => ptr::null(),
    };

    // if get_extensions didn't pan out, try loading the extensions directly
    if extensions.is_null() {
        extensions = match unsafe { dll.function(&DRI_EXTENSIONS_NAME) } {
            Some(exts) => exts,
            None => {
                return Err(breadx::BreadError::StaticMsg(
                    "Unable to load extensions from driver.",
                ))
            }
        };
    }

    // since we know extensions is not null, we can make a slice out of it
    // first, figure out the length
    let mut length = 0;
    let mut p = extensions;
    while !(unsafe { *p }.is_null()) {
        p = unsafe { p.offset(1) };
        length += 1;
    }

    let extensions = unsafe { slice::from_raw_parts(extensions, length) };
    Ok(extensions)
}
