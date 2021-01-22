// MIT/Apache2 License

use crate::{
    display::DisplayLike,
    dri::{
        dri3::{free_buffer_arc, BufferType, Dri3Drawable, MAX_BACK},
        ffi, ExtensionContainer,
    },
};
use breadx::display::Connection;
use std::{
    mem,
    os::raw::{c_int, c_uint, c_void},
    panic::catch_unwind,
    process::abort,
    ptr::{self, raw_mut},
    sync::Arc,
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
    mut buffer_mask: u32,
    buffers: *mut ffi::__DRIimageList,
) -> c_int
where
    Dpy::Connection: Connection,
{
    // write some of our initial values to the buffers variable
    ptr::write(raw_mut!((*buffers).image_mask), 0);
    ptr::write(raw_mut!((*buffers).front), ptr::null_mut());
    ptr::write(raw_mut!((*buffers).back), ptr::null_mut());

    // wrap the rest of this function in a catch_unwind, it is very unsafe to pass a panic across
    // the FFI boundary
    match catch_unwind::<_, breadx::Result<c_int>>(move || {
        log::debug!("Entering scope for get_buffers panic catcher");

        // SAFETY: "loader" should semantically be an Arc<Dri3Drawable<Dpy>>. Here, we increment the reference
        //         count and return an Arc
        let drawable = Arc::from_raw(loader as *const c_void as *const Dri3Drawable<Dpy>);
        // SAFETY: Since the only copies of this function that exist are the one in the map
        //         and the one here, and the reference count would logically only read as "1",
        //         we need to create a dummy copy of the drawable so that other threads know
        //         we're using it
        mem::forget(drawable.clone());

        drawable.update()?;
        drawable.update_max_back();

        // free back buffers we no longer need
        drawable.free_back_buffers()?;
        if drawable.is_pixmap() || drawable.swap_method() == ffi::__DRI_ATTRIB_SWAP_EXCHANGE as _ {
            buffer_mask |= ffi::__DRI_IMAGE_BUFFER_FRONT as u32;
        }

        let buffers = unsafe { &mut *buffers };

        if buffer_mask & ffi::__DRI_IMAGE_BUFFER_FRONT as u32 == 0 {
            // we don't need a front buffer
            drawable.free_buffers(BufferType::Front)?;
            drawable.set_have_fake_front(false);
        } else {
            let not_fake_front = drawable.is_pixmap() && !drawable.is_different_gpu();
            let buffer = if not_fake_front {
                drawable.get_pixmap_buffer(BufferType::Front, format)?
            } else {
                drawable.get_buffer(BufferType::Front, format)?
            };

            buffers.image_mask |= ffi::__DRI_IMAGE_BUFFER_FRONT as u32;
            buffers.front = buffer.image.as_ptr();
            drawable.set_have_fake_front(!not_fake_front);
        }

        if buffer_mask & ffi::__DRI_IMAGE_BUFFER_BACK as u32 == 0 {
            drawable.free_buffers(BufferType::Back)?;
            drawable.set_have_back(false);
        } else {
            let back = drawable.get_buffer(BufferType::Back, format)?;
            drawable.set_have_back(true);
            buffers.image_mask |= ffi::__DRI_IMAGE_BUFFER_BACK as u32;
            buffers.back = back.image.as_ptr();
        }

        Ok(1)
    }) {
        Err(_) => {
            log::error!("get_buffers panicked during catch_unwind!");
            abort()
        }
        Ok(Err(e)) => {
            log::error!("get_buffers resolved to error: {:?}", e);
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
    Dpy::Connection: Connection,
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
    Dpy::Connection: AsyncConnection + Send,
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
    Dpy::Connection: Connection,
{
    LoaderExtensions::<Dpy>::LOADER_EXTENSIONS
}

#[cfg(feature = "async")]
pub(crate) fn loader_extensions_async<Dpy: DisplayLike>() -> &'static [ExtensionContainer; 4]
where
    Dpy::Connection: AsyncConnection + Send,
{
    LoaderExtensions::<Dpy>::LOADER_EXTENSIONS_ASYNC
}
