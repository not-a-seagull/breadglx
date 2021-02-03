// MIT/Apache2 License

use crate::{
    config::GlConfig,
    context::{promote_anyarc_ref, GlContext},
    cstr::{const_cstr, ConstCstr},
    dri, indirect, mesa,
    screen::GlScreen,
    util::env_to_boolean,
};
use breadx::{
    display::{Connection, Display, DisplayLike as DpyLikeBase},
    Drawable, Visualtype,
};
use dashmap::DashMap;
use std::{
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    marker::PhantomData,
    ops::{Deref, DerefMut, Range},
    os::raw::c_char,
    ptr,
    sync::Arc,
};

#[cfg(not(feature = "async"))]
use std::sync;

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;
#[cfg(feature = "async")]
use futures_lite::future;

mod dispatch;

pub(crate) use dispatch::DisplayDispatch;

/// Things that can go inside of a GlDisplay. Since it is shoved into a static variable
/// at one point, it needs to be Send + Sync + 'static.
pub trait DisplayLike = DpyLikeBase + Send + Sync + 'static;

// We stuff this inside of a GlDisplay.
#[derive(Debug)]
struct InnerGlDisplay<Dpy> {
    #[cfg(not(feature = "async"))]
    display: sync::Mutex<Dpy>,
    #[cfg(feature = "async")]
    display: async_lock::Mutex<Dpy>,

    // major and minor versions of GLX
    major_version: u32,
    minor_version: u32,

    // if the rendering should be direct or hardware accelerated
    direct: bool,
    accel: bool,

    // the underlying GL rendering method
    context: dispatch::DisplayDispatch<Dpy>,

    // cache that maps the drawables to a map of their properties
    drawable_properties: DashMap<Drawable, HashMap<u32, u32>>,
}

/// Represents a lock on the inner display.
#[repr(transparent)]
pub struct DisplayLock<'a, Dpy> {
    #[cfg(not(feature = "async"))]
    base: sync::MutexGuard<'a, Dpy>,
    #[cfg(feature = "async")]
    base: async_lock::MutexGuard<'a, Dpy>,
}

impl<'a, Dpy: DisplayLike> Deref for DisplayLock<'a, Dpy> {
    type Target = Display<Dpy::Connection>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.base.display()
    }
}

impl<'a, Dpy: DisplayLike> DerefMut for DisplayLock<'a, Dpy> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.base.display_mut()
    }
}

impl<'a, Dpy> Drop for DisplayLock<'a, Dpy> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        log::trace!("Dropping display lock...");
    }
}

/// The OpenGL context acting as a wrapper.
#[derive(Debug)]
#[repr(transparent)]
pub struct GlDisplay<Dpy> {
    inner: Arc<InnerGlDisplay<Dpy>>,
}

impl<Dpy> Clone for GlDisplay<Dpy> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) trait GlInternalDisplay<Dpy: DisplayLike> {
    fn create_screen(
        &self,
        dpy: &mut Display<Dpy::Connection>,
        index: usize,
    ) -> breadx::Result<GlScreen<Dpy>>;
}

#[cfg(feature = "async")]
pub(crate) trait AsyncGlInternalDisplay<Dpy: DisplayLike> {
    fn create_screen_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b mut Display<Dpy::Connection>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen<Dpy>>>
    where
        'a: 'future,
        'b: 'future;
}

impl<Dpy> GlDisplay<Dpy> {
    #[inline]
    pub fn major_version(&self) -> u32 {
        self.inner.major_version
    }

    #[inline]
    pub fn minor_version(&self) -> u32 {
        self.inner.minor_version
    }

    #[inline]
    pub(crate) fn dispatch(&self) -> &dispatch::DisplayDispatch<Dpy> {
        &self.inner.context
    }
}

impl<Dpy: DisplayLike> GlDisplay<Dpy> {
    /// Lock the mutex containing the internal display.
    #[inline]
    pub fn display(&self) -> DisplayLock<'_, Dpy> {
        #[cfg(debug_assertions)]
        log::trace!("Creating display lock...");

        #[cfg(not(feature = "async"))]
        let base = self
            .inner
            .display
            .lock()
            .expect("Failed to acquire display lock");
        #[cfg(feature = "async")]
        let base = future::block_on(self.inner.display.lock());

        DisplayLock { base }
    }

    /// Lock the mutex contained the internal display, async redox.
    #[cfg(feature = "async")]
    #[inline]
    pub async fn display_async(&self) -> DisplayLock<'_, Dpy> {
        #[cfg(debug_assertions)]
        log::trace!("Creating display lock...");

        DisplayLock {
            base: self.inner.display.lock().await,
        }
    }
}

struct GlStats {
    direct: bool,
    accel: bool,
    no_dri3: bool,
    no_dri2: bool,
}

impl GlStats {
    #[inline]
    fn get() -> GlStats {
        GlStats {
            direct: !env_to_boolean("LIBGL_ALWAYS_INDIRECT", false),
            accel: !env_to_boolean("LIBGL_ALWAYS_SOFTWARE", false),
            no_dri3: env_to_boolean("LIBGL_DRI3_DISABLE", false),
            no_dri2: env_to_boolean("LIBGL_DRI2_DISABLE", false),
        }
    }
}

const GLAPI_GET_PROC_ADDRESS: ConstCstr<'static> = const_cstr(&*b"_glapi_get_proc_address\0");
type GlapiGetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;

impl<Dpy: DisplayLike> GlDisplay<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    pub fn create_screen(&self, screen: usize) -> breadx::Result<GlScreen<Dpy>> {
        log::trace!("Creating screen...");
        let scr = self
            .inner
            .context
            .create_screen(&mut *self.display(), screen)?;
        log::trace!("Created screen.");
        Ok(scr)
    }

    /// Load a drawable's property.
    #[inline]
    pub(crate) fn load_drawable_property(
        &self,
        drawable: Drawable,
        property: u32,
    ) -> breadx::Result<Option<u32>> {
        log::trace!("Loading drawable property");

        let map = match self.inner.drawable_properties.get(&drawable) {
            Some(map) => map,
            None => {
                let repl = self
                    .display()
                    .get_drawable_properties_immediate(drawable.into())?;
                let propmap: HashMap<u32, u32> = repl.chunks(2).map(|kv| (kv[0], kv[1])).collect();
                self.inner.drawable_properties.insert(drawable, propmap);
                self.inner
                    .drawable_properties
                    .get(&drawable)
                    .expect("Infallible HashMap::get()")
            }
        };

        Ok(map.get(&property).copied())
    }

    /// Get the address of the desired function, but takes a C String.
    #[inline]
    pub fn get_proc_address_cstr(&self, function: &CStr) -> breadx::Result<*const c_void> {
        let bytes = function.to_bytes();
        if bytes[0] == b'g' && bytes[1] == b'l' && bytes[2] != b'X' {
            // try to call _glapi_get_proc_address to get the address
            let glapi = mesa::glapi()?;
            let glapi_get_proc_address: GlapiGetProcAddress =
                unsafe { glapi.function(&*GLAPI_GET_PROC_ADDRESS) }
                    .expect("_glapi_proc not present");
            let mut f = unsafe { glapi_get_proc_address(function.as_ptr()) };

            // if that failed, get the current context (if possible)
            if f.is_null() {
                if let Some(ref ctx) = GlContext::<Dpy>::get()
                    .as_ref()
                    .and_then(|m| promote_anyarc_ref::<Dpy>(m))
                {
                    f = match ctx.get_proc_address(function) {
                        Some(p) => p.into_inner().as_ptr(),
                        None => ptr::null_mut(),
                    };
                }
            }

            if f.is_null() {
                Err(breadx::BreadError::Msg(format!(
                    "Unable to find OpenGL function: {:?}",
                    function
                )))
            } else {
                Ok(f as *const _)
            }
        } else {
            Err(breadx::BreadError::StaticMsg("Invalid GL function name"))
        }
    }

    /// Get the address of the desired function.
    #[inline]
    pub fn get_proc_address(&self, function: &str) -> breadx::Result<*const c_void> {
        let function = CString::new(function)
            .map_err(|_| breadx::BreadError::StaticMsg("string has a zero?"))?;
        self.get_proc_address_cstr(&*function)
    }

    #[inline]
    pub fn new(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

        // get the major and minor version
        let (major_version, minor_version) = dpy.display_mut().query_glx_version_immediate(1, 1)?;
        if major_version != 1 || minor_version < 1 {
            return Err(breadx::BreadError::StaticMsg(
                "breadglx is not compatible with GLX v1.0",
            ));
        }

        let mut context: Option<dispatch::DisplayDispatch<Dpy>> = None;

        // try to get DRI
        #[cfg(feature = "dri")]
        if stats.direct && stats.accel {
            #[cfg(feature = "dri3")]
            if !stats.no_dri3 {
                context = match dri::dri3::Dri3Display::new(dpy.display_mut()) {
                    Ok(ctx) => Some(ctx.into()),
                    Err(e) => {
                        log::error!("Unable to create DRI3 context: {:?}", e);
                        None
                    }
                };
            } else {
                log::info!("Skipping DRI3 Initialization");
            }

            // try again with dri2 if we can't do dri3
            if context.is_none() && !stats.no_dri2 {
                context = match dri::dri2::Dri2Display::new(dpy.display_mut()) {
                    Ok(ctx) => Some(ctx.into()),
                    Err(e) => {
                        log::error!("Unable to create DRI2 context: {:?}", e);
                        None
                    }
                };
            }
        } else {
            log::info!("Skipping DRI3/DRI2 Initialization");
        }

        let context = match context {
            Some(context) => context,
            None => indirect::IndirectDisplay::new(dpy.display_mut())?.into(),
        };

        let this = InnerGlDisplay {
            #[cfg(not(feature = "async"))]
            display: sync::Mutex::new(dpy),
            #[cfg(feature = "async")]
            display: async_lock::Mutex::new(dpy),
            direct: stats.direct,
            accel: stats.accel,
            context,
            drawable_properties: DashMap::new(),
            major_version,
            minor_version,
        };

        Ok(Self {
            inner: Arc::new(this),
        })
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> GlDisplay<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    pub async fn create_screen_async(&mut self, index: usize) -> breadx::Result<GlScreen<Dpy>> {
        self.inner
            .context
            .create_screen_async(&mut *self.display_async().await, index)
            .await
    }

    /// Load a drawable's property, async redox.
    #[inline]
    pub(crate) async fn load_drawable_property_async(
        &self,
        drawable: Drawable,
        property: u32,
    ) -> breadx::Result<Option<u32>> {
        // NOTE: DashMap doesn't actually block, it just spins. So we're safe to not push each instance
        //       of DashMap::get() into the blocking threadpool
        let map = match self.inner.drawable_properties.get(&drawable) {
            Some(map) => map,
            None => {
                let repl = self
                    .display_async()
                    .await
                    .get_drawable_properties_immediate_async(drawable.into())
                    .await?;

                self.inner
                    .drawable_properties
                    .insert(drawable, repl.chunks(2).map(|kv| (kv[0], kv[1])).collect());
                self.inner
                    .drawable_properties
                    .get(&drawable)
                    .expect("Infallible DashMap::get()")
            }
        };

        Ok(map.get(&property).copied())
    }

    #[inline]
    pub async fn new_async(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

        let (major_version, minor_version) = dpy
            .display_mut()
            .query_glx_version_immediate_async(1, 1)
            .await?;
        if major_version != 1 || minor_version < 1 {
            return Err(breadx::BreadError::StaticMsg(
                "breadglx is not compatible with GLX v1.0",
            ));
        }

        let mut context: Option<dispatch::DisplayDispatch<Dpy>> = None;

        // try to get DRI
        #[cfg(feature = "dri")]
        if stats.direct && stats.accel {
            #[cfg(feature = "dri3")]
            if !stats.no_dri3 {
                context = dri::dri3::Dri3Display::new_async(dpy.display_mut())
                    .await
                    .ok()
                    .map(|x| x.into());
            }

            // try again with dri2 if we can't do dri3
            if context.is_none() && !stats.no_dri2 {
                context = dri::dri2::Dri2Display::new_async(dpy.display_mut())
                    .await
                    .ok()
                    .map(|x| x.into());
            }
        }

        let context = match context {
            Some(context) => context,
            None => indirect::IndirectDisplay::new_async(dpy.display_mut())
                .await?
                .into(),
        };

        let this = InnerGlDisplay {
            display: async_lock::Mutex::new(dpy),
            direct: stats.direct,
            accel: stats.accel,
            context,
            drawable_properties: DashMap::new(),
            major_version,
            minor_version,
        };

        Ok(Self {
            inner: Arc::new(this),
        })
    }
}
