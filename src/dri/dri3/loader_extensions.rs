// MIT/Apache2 License

use crate::{
    display::DisplayLike,
    dri::{dri3::Dri3Drawable, ffi, ExtensionContainer},
};
use breadx::display::Connection;
use std::{
    os::raw::{c_int, c_uint, c_void},
    panic::catch_unwind,
    ptr::{self, raw_mut},
};

#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

fn unimpl(name: &'static str) -> ! {
    log::error!("fill in later: {}", name);
    std::process::abort();
}

/* Implementation of Image Loader Extension functions */
unsafe extern "C" fn get_buffers<Dpy: DisplayLike>(
    dri_drawable: *mut ffi::__DRIdrawable,
    format: c_uint,
    stamp: *mut u32,
    loader: *mut c_void,
    buffer_mask: u32,
    buffers: *mut ffi::__DRIimageList,
) -> c_int {
    // write some of our initial values to the buffers variable
    ptr::write(raw_mut!((*buffers).image_mask), 0);
    ptr::write(raw_mut!((*buffers).front), ptr::null_mut());
    ptr::write(raw_mut!((*buffers).back), ptr::null_mut());

    // "loader" should be a *const Dri3Drawable, let's load that
    let drawable = &*(loader as *const c_void as *const Dri3Drawable<Dpy>);

    // wrap the rest of this function in a catch_unwind, it is very unsafe to pass a panic across
    // the FFI boundary
    match catch_unwind::<_, breadx::Result<c_int>>(move || {
        drawable.update();
        drawable.update_max_back();

        Ok(0)
    }) {
        Err(_) => {
            log::error!("get_buffers panicked during catch_unwind!");
            0
        }
        Ok(Err(e)) => {
            log::error!("get_buffers resolved to error: {}", e);
            0
        }
        Ok(Ok(res)) => res,
    }
}

unsafe extern "C" fn flush_front_buffer<Dpy: DisplayLike>(
    dri_drawable: *mut ffi::__DRIdrawable,
    loader: *mut c_void,
) {
    unimpl("flush_front_buffer")
}

unsafe extern "C" fn flush_swap_buffers<Dpy: DisplayLike>(
    dri_drawable: *mut ffi::__DRIdrawable,
    loader: *mut c_void,
) {
    unimpl("flush_swap_buffers")
}

/* Implementation of Background Callable extension functions */
unsafe extern "C" fn set_background_context<Dpy: DisplayLike>(loader: *mut c_void) {
    unimpl("set_background_context")
}

unsafe extern "C" fn is_thread_safe(_loader: *mut c_void) -> ffi::GLboolean {
    1
}

struct LoaderExtensions<Dpy>(Dpy);

impl<Dpy: DisplayLike> LoaderExtensions<Dpy>
where
    Dpy::Conn: Connection,
{
    // Loader extensions
    const IMAGE_LOADER_EXTENSION: ffi::__DRIimageLoaderExtension = ffi::__DRIimageLoaderExtension {
        base: ffi::__DRIextension {
            name: ffi::__DRI_IMAGE_LOADER.as_ptr() as *const _,
            version: 3,
        },
        getBuffers: Some(get_buffers::<Dpy>),
        flushFrontBuffer: Some(flush_front_buffer::<Dpy>),
        flushSwapBuffers: Some(flush_swap_buffers::<Dpy>),
        getCapability: None,
        destroyLoaderImageState: None,
    };

    /* Implementation of Invalidate Extension */
    const INVALIDATE_EXTENSION: ffi::__DRIuseInvalidateExtension =
        ffi::__DRIuseInvalidateExtension {
            base: ffi::__DRIextension {
                name: ffi::__DRI_USE_INVALIDATE.as_ptr() as *const _,
                version: 1,
            },
        };

    const BACKGROUND_CALLABLE_EXTENSION: ffi::__DRIbackgroundCallableExtension =
        ffi::__DRIbackgroundCallableExtension {
            base: ffi::__DRIextension {
                name: ffi::__DRI_BACKGROUND_CALLABLE.as_ptr() as *const _,
                version: 2,
            },
            setBackgroundContext: Some(set_background_context::<Dpy>),
            isThreadSafe: Some(is_thread_safe),
        };

    const LOADER_EXTENSIONS: &'static [ExtensionContainer; 4] = &[
        ExtensionContainer(&LoaderExtensions::<Dpy>::IMAGE_LOADER_EXTENSION.base),
        ExtensionContainer(&LoaderExtensions::<Dpy>::INVALIDATE_EXTENSION.base),
        ExtensionContainer(&LoaderExtensions::<Dpy>::BACKGROUND_CALLABLE_EXTENSION.base),
        ExtensionContainer(ptr::null()),
    ];
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> LoaderExtensions<Dpy>
where
    Dpy::Conn: AsyncConnection + Send,
{
    // Loader extensions
    const IMAGE_LOADER_EXTENSION_ASYNC: ffi::__DRIimageLoaderExtension =
        ffi::__DRIimageLoaderExtension {
            base: ffi::__DRIextension {
                name: ffi::__DRI_IMAGE_LOADER.as_ptr() as *const _,
                version: 3,
            },
            getBuffers: Some(get_buffers_async::<Dpy>),
            flushFrontBuffer: Some(flush_front_buffer_async::<Dpy>),
            flushSwapBuffers: Some(flush_swap_buffers_async::<Dpy>),
            getCapability: None,
            destroyLoaderImageState: None,
        };

    /* Implementation of Invalidate Extension */
    const INVALIDATE_EXTENSION_ASYNC: ffi::__DRIuseInvalidateExtension =
        ffi::__DRIuseInvalidateExtension {
            base: ffi::__DRIextension {
                name: ffi::__DRI_USE_INVALIDATE.as_ptr() as *const _,
                version: 1,
            },
        };

    const BACKGROUND_CALLABLE_EXTENSION_ASYNC: ffi::__DRIbackgroundCallableExtension =
        ffi::__DRIbackgroundCallableExtension {
            base: ffi::__DRIextension {
                name: ffi::__DRI_BACKGROUND_CALLABLE.as_ptr() as *const _,
                version: 2,
            },
            setBackgroundContext: Some(set_background_context_async::<Dpy>),
            isThreadSafe: Some(is_thread_safe),
        };

    const LOADER_EXTENSIONS_ASYNC: &'static [ExtensionContainer; 4] = &[
        ExtensionContainer(&LoaderExtensions::<Dpy>::IMAGE_LOADER_EXTENSION_ASYNC.base),
        ExtensionContainer(&LoaderExtensions::<Dpy>::INVALIDATE_EXTENSION_ASYNC.base),
        ExtensionContainer(&LoaderExtensions::<Dpy>::BACKGROUND_CALLABLE_EXTENSION_ASYNC.base),
        ExtensionContainer(ptr::null()),
    ];
}

pub(crate) fn loader_extensions<Dpy: DisplayLike>() -> &'static [ExtensionContainer; 4]
where
    Dpy::Conn: Connection,
{
    LoaderExtensions::<Dpy>::LOADER_EXTENSIONS
}

#[cfg(feature = "async")]
pub(crate) fn loader_extensions_async<Dpy: DisplayLike>() -> &'static [ExtensionContainer; 4]
where
    Dpy::Conn: AsyncConnection + Send,
{
    LoaderExtensions::<Dpy>::LOADER_EXTENSIONS_ASYNC
}
