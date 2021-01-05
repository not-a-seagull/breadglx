// MIT/Apache2 License

use crate::dri::{ffi, ExtensionContainer};
use std::{
    os::raw::{c_int, c_uint, c_void},
    ptr::{self, raw_mut},
};

fn unimpl(name: &'static str) -> ! {
    log::error!("fill in later: {}", name);
    std::process::abort();
}

/* Implementation of Image Loader Extension functions */
unsafe extern "C" fn get_buffers(
    dri_drawable: *mut ffi::__DRIdrawable,
    format: c_uint,
    stamp: *mut u32,
    loader: *mut c_void,
    buffer_mask: u32,
    buffers: *mut ffi::__DRIimageList,
) -> c_int {
    // write some of our initial values to the buffers variable
    unimpl("get_buffers")
}

unsafe extern "C" fn flush_front_buffer(
    dri_drawable: *mut ffi::__DRIdrawable,
    loader: *mut c_void,
) {
    unimpl("flush_front_buffer")
}

unsafe extern "C" fn flush_swap_buffers(
    dri_drawable: *mut ffi::__DRIdrawable,
    loader: *mut c_void,
) {
    unimpl("flush_swap_buffers")
}

// Loader extensions
const IMAGE_LOADER_EXTENSION: ffi::__DRIimageLoaderExtension = ffi::__DRIimageLoaderExtension {
    base: ffi::__DRIextension {
        name: ffi::__DRI_IMAGE_LOADER.as_ptr() as *const _,
        version: 3,
    },
    getBuffers: Some(get_buffers),
    flushFrontBuffer: Some(flush_front_buffer),
    flushSwapBuffers: Some(flush_swap_buffers),
    getCapability: None,
    destroyLoaderImageState: None,
};

/* Implementation of Invalidate Extension */
const INVALIDATE_EXTENSION: ffi::__DRIuseInvalidateExtension = ffi::__DRIuseInvalidateExtension {
    base: ffi::__DRIextension {
        name: ffi::__DRI_USE_INVALIDATE.as_ptr() as *const _,
        version: 1,
    },
};

/* Implementation of Background Callable extension functions */
unsafe extern "C" fn set_background_context(loader: *mut c_void) {
    unimpl("set_background_context")
}

unsafe extern "C" fn is_thread_safe(_loader: *mut c_void) -> ffi::GLboolean {
    1
}

const BACKGROUND_CALLABLE_EXTENSION: ffi::__DRIbackgroundCallableExtension =
    ffi::__DRIbackgroundCallableExtension {
        base: ffi::__DRIextension {
            name: ffi::__DRI_BACKGROUND_CALLABLE.as_ptr() as *const _,
            version: 2,
        },
        setBackgroundContext: Some(set_background_context),
        isThreadSafe: Some(is_thread_safe),
    };

pub(crate) static LOADER_EXTENSIONS: &[ExtensionContainer; 4] = &[
    ExtensionContainer(&IMAGE_LOADER_EXTENSION.base),
    ExtensionContainer(&INVALIDATE_EXTENSION.base),
    ExtensionContainer(&BACKGROUND_CALLABLE_EXTENSION.base),
    ExtensionContainer(ptr::null()),
];
