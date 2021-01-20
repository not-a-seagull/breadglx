// MIT/Apache2 License

use crate::{config::GlConfig, dri, indirect, mesa, screen::GlScreen, util::env_to_boolean};
use breadx::{
    display::{Connection, Display, DisplayLike as DpyLikeBase},
    Drawable, Visualtype,
};
use dashmap::DashMap;
use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut, Range},
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
    drawable_properties: DashMap<Drawable, Arc<HashMap<u32, u32>>>,
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
    type Target = Display<Dpy::Conn>;

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
        dpy: &mut Display<Dpy::Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen<Dpy>>;
}

#[cfg(feature = "async")]
pub(crate) trait AsyncGlInternalDisplay<Dpy: DisplayLike> {
    fn create_screen_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b mut Display<Dpy::Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen<Dpy>>>
    where
        'a: 'future,
        'b: 'future;
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

impl<Dpy: DisplayLike> GlDisplay<Dpy>
where
    Dpy::Conn: Connection,
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
                self.inner
                    .drawable_properties
                    .insert(drawable, Arc::new(propmap));
                self.inner
                    .drawable_properties
                    .get(&drawable)
                    .expect("Infallible HashMap::get()")
            }
        };

        Ok(map.get(&property).copied())
    }

    #[inline]
    pub fn new(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

        // get the major and minor version
        let (major_version, minor_version) = dpy.display_mut().query_glx_version_immediate(1, 1)?;

        let mut context: Option<dispatch::DisplayDispatch<Dpy>> = None;

        // try to get DRI
        #[cfg(feature = "dri")]
        if stats.direct && stats.accel {
            #[cfg(feature = "dri3")]
            if !stats.no_dri3 {
                context = dri::dri3::Dri3Display::new(dpy.display_mut())
                    .ok()
                    .map(|x| x.into());
            }

            // try again with dri2 if we can't do dri3
            if context.is_none() && !stats.no_dri2 {
                context = dri::dri2::Dri2Display::new(dpy.display_mut())
                    .ok()
                    .map(|x| x.into());
            }
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
    Dpy::Conn: AsyncConnection + Send,
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
        // dashmap has a chance of blocking, so we clone ourselves a few times to get around the
        // blocking calls
        let this = self.clone();

        let map = match blocking::unblock(move || {
            this.inner
                .drawable_properties
                .get(&drawable)
                .as_deref()
                .cloned()
        })
        .await
        {
            Some(map) => map,
            None => {
                let repl = self
                    .display_async()
                    .await
                    .get_drawable_properties_immediate_async(drawable.into())
                    .await?;

                let this = self.clone();
                blocking::unblock(move || {
                    this.inner.drawable_properties.insert(
                        drawable,
                        Arc::new(repl.chunks(2).map(|kv| (kv[0], kv[1])).collect()),
                    );
                    this.inner
                        .drawable_properties
                        .get(&drawable)
                        .expect("Infallible HashMap::get()")
                        .clone()
                })
                .await
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
