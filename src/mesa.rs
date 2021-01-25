// MIT/Apache2 License

//! This module provides links to the Mesa3D bindings.

use super::dll::Dll;

#[cfg(not(feature = "async"))]
use once_cell::sync::Lazy;

#[cfg(feature = "async")]
use async_lock::Mutex;
#[cfg(feature = "async")]
use futures_lite::future;
#[cfg(feature = "async")]
use std::{
    cell::{Cell, UnsafeCell},
    sync::atomic::{AtomicBool, Ordering},
};

// Basic implementation of a lazy async cell
// cribs from https://github.com/matklad/once_cell/blob/master/src/imp_pl.rs
#[cfg(feature = "async")]
struct Lazy<T, F = fn() -> T> {
    init: Cell<Option<F>>,
    data: UnsafeCell<Option<T>>,
    is_initialized: AtomicBool,
    mutex: Mutex<()>,
}

#[cfg(feature = "async")]
impl<T, F> Lazy<T, F> {
    #[inline]
    const fn new(f: F) -> Self {
        Self {
            init: Cell::new(Some(f)),
            data: UnsafeCell::new(None),
            is_initialized: AtomicBool::new(false),
            mutex: Mutex::new(()),
        }
    }

    #[inline]
    fn is_initialized(&self) -> bool {
        self.is_initialized.load(Ordering::Acquire)
    }

    #[inline]
    unsafe fn get_raw(&self) -> Option<&T> {
        let slot = unsafe { &*self.data.get() };
        slot.as_ref()
    }
}

#[cfg(feature = "async")]
impl<T: Send + 'static, F: FnOnce() -> T + Send + 'static> Lazy<T, F> {
    #[inline]
    async fn get(&self) -> &T {
        // if the value is already initialized, return it
        if let Some(val) = unsafe { self.get_raw() } {
            return val;
        }

        let _guard = self.mutex.lock().await;
        if !self.is_initialized() {
            // initialize the value
            let init = self.init.take().expect("Lazy is already initialized");
            let val = blocking::unblock(move || (init)()).await;

            let slot: &mut Option<T> = unsafe { &mut *self.data.get() };
            *slot = Some(val);
            self.is_initialized.store(true, Ordering::Release);
        }

        unsafe { self.get_raw() }.expect("Literally impossible")
    }
}

#[cfg(feature = "async")]
unsafe impl<T: Send, F: Send> Send for Lazy<T, F> {}
#[cfg(feature = "async")]
unsafe impl<T: Sync + Send, F: Send> Sync for Lazy<T, F> {}

const GL_LIB_NAMES: [&str; 2] = ["libGL.so", "libGL.so.1"];
const DRM_LIB_NAMES: [&str; 2] = ["libdrm.so", "libdrm.so.2"];
const XSHMFENCE_LIB_NAMES: [&str; 2] = ["libxshmfence.so", "libxshmfence.so.1"];
const GLAPI_LIB_NAMES: [&str; 3] = ["libglapi.so", "libglapi.so.0", "libglapi.so.0.0.0"];

static GL: Lazy<breadx::Result<Dll>> = Lazy::new(|| Dll::load("LibGL", &GL_LIB_NAMES));
static DRM: Lazy<breadx::Result<Dll>> = Lazy::new(|| Dll::load("LibDRM", &DRM_LIB_NAMES));
static XSHMFENCE: Lazy<breadx::Result<Dll>> =
    Lazy::new(|| Dll::load("LibXShmFence", &XSHMFENCE_LIB_NAMES));
static GLAPI: Lazy<breadx::Result<Dll>> = Lazy::new(|| Dll::load("LibGLAPI", &GLAPI_LIB_NAMES));

#[inline]
fn unwrap_result(res: &breadx::Result<Dll>) -> breadx::Result<&Dll> {
    match res {
        Ok(ref dll) => Ok(dll),
        Err(e) => Err(e.clone()),
    }
}

#[inline]
pub(crate) fn gl() -> breadx::Result<&'static Dll> {
    #[cfg(feature = "async")]
    {
        future::block_on(gl_async())
    }
    #[cfg(not(feature = "async"))]
    {
        let res = &*GL;

        unwrap_result(res)
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn gl_async() -> breadx::Result<&'static Dll> {
    unwrap_result(GL.get().await)
}

#[inline]
pub(crate) fn drm() -> breadx::Result<&'static Dll> {
    #[cfg(feature = "async")]
    {
        future::block_on(drm_async())
    }
    #[cfg(not(feature = "async"))]
    {
        let res = &*DRM;

        unwrap_result(res)
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn drm_async() -> breadx::Result<&'static Dll> {
    unwrap_result(DRM.get().await)
}

#[inline]
pub(crate) fn xshmfence() -> breadx::Result<&'static Dll> {
    #[cfg(feature = "async")]
    {
        future::block_on(xshmfence_async())
    }
    #[cfg(not(feature = "async"))]
    {
        let res = &*XSHMFENCE;
        unwrap_result(res)
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn xshmfence_async() -> breadx::Result<&'static Dll> {
    unwrap_result(XSHMFENCE.get().await)
}

#[inline]
pub(crate) fn glapi() -> breadx::Result<&'static Dll> {
    #[cfg(feature = "async")]
    {
        future::block_on(glapi_async())
    }
    #[cfg(not(feature = "async"))]
    {
        let res = &*GLAPI;
        unwrap_result(res)
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn glapi_async() -> breadx::Result<&'static Dll> {
    unwrap_result(GLAPI.get().await)
}
