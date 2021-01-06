// MIT/Apache2 License

use crate::{config::GlConfig, dri, indirect, mesa, screen::GlScreen, util::env_to_boolean};
use breadx::{
    display::{Connection, Display},
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

mod dispatch;

// We stuff this inside of a GlDisplay.
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
    context: dispatch::DisplayDispatch,

    // cache that maps the drawables to a map of their properties
    drawable_properties: DashMap<Drawable, HashMap<u32, u32>>,
}

/// Represents a lock on the inner display.
#[repr(transparent)]
pub struct DisplayLock<'a, Conn, Dpy> {
    #[cfg(not(feature = "async"))]
    base: sync::MutexGuard<'a, Dpy>,
    #[cfg(feature = "async")]
    base: async_lock::MutexGuard<'a, Dpy>,
    _phantom: PhantomData<&'a Conn>,
}

impl<'a, Conn: Connection, Dpy: AsRef<Display<Conn>>> Deref for DisplayLock<'a, Conn, Dpy> {
    type Target = Display<Conn>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.base.as_ref()
    }
}

impl<'a, Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>> DerefMut
    for DisplayLock<'a, Conn, Dpy>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.base.as_mut()
    }
}

impl<'a, Conn, Dpy> Drop for DisplayLock<'a, Conn, Dpy> {
    #[inline]
    fn drop(&mut self) {
        log::trace!("Dropping display lock...");
    }
}

/// The OpenGL context acting as a wrapper.
#[repr(transparent)]
pub struct GlDisplay<Conn, Dpy> {
    inner: Arc<InnerGlDisplay<Dpy>>,

    // needed to satisfy type constraints
    _phantom: PhantomData<Conn>,
}

/// The underlying OpenGL context.
pub(crate) trait GlInternalDisplay {
    fn create_screen<Conn: Connection>(
        &self,
        dpy: &mut Display<Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen>;

    #[cfg(feature = "async")]
    fn create_screen_async<'future, 'a, 'b, Conn: Connection>(
        &'a self,
        dpy: &'b mut Display<Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen>>
    where
        'a: 'future,
        'b: 'future;
}

impl<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>> GlDisplay<Conn, Dpy> {
    /// Lock the mutex containing the internal display.
    #[inline]
    pub fn display(&self) -> DisplayLock<'_, Conn, Dpy> {
        log::trace!("Creating display lock...");

        #[cfg(not(feature = "async"))]
        let base = self
            .inner
            .display
            .lock()
            .expect("Failed to acquire display lock");
        #[cfg(feature = "async")]
        let base = async_io::block_on(self.inner.display.lock());

        DisplayLock {
            base,
            _phantom: PhantomData,
        }
    }

    /// Lock the mutex contained the internal display, async redox.
    #[cfg(feature = "async")]
    #[inline]
    pub async fn display_async(&self) -> DisplayLock<'_, Conn, Dpy> {
        DisplayLock {
            base: self.inner.display.lock().await,
            _phantom: PhantomData,
        }
    }
}

pub(crate) struct GlStats {
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

impl<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>> GlDisplay<Conn, Dpy> {
    #[inline]
    pub fn create_screen(&self, screen: usize) -> breadx::Result<GlScreen> {
        log::trace!("Creating screen...");
        let scr = self.inner
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

    #[inline]
    pub fn new(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

        // get the major and minor version
        let (major_version, minor_version) = dpy.as_mut().query_glx_version_immediate(1, 1)?;

        let mut context: Option<dispatch::DisplayDispatch> = None;

        // try to get DRI
        #[cfg(feature = "dri")]
        if stats.direct && stats.accel {
            #[cfg(feature = "dri3")]
            if !stats.no_dri3 {
                context = dri::dri3::Dri3Display::new(dpy.as_mut())
                    .ok()
                    .map(|x| x.into());
            }

            // try again with dri2 if we can't do dri3
            if context.is_none() && !stats.no_dri2 {
                context = dri::dri2::Dri2Display::new(dpy.as_mut())
                    .ok()
                    .map(|x| x.into());
            }
        }

        let context = match context {
            Some(context) => context,
            None => indirect::IndirectDisplay::new(dpy.as_mut())?.into(),
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
            _phantom: PhantomData,
        })
    }
}

// TODO: make not send, differentiate, etc
impl<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>> + Send>
    GlDisplay<Conn, Dpy>
{
    #[cfg(feature = "async")]
    #[inline]
    pub async fn create_screen_async(&mut self, index: usize) -> breadx::Result<GlScreen> {
        self.inner
            .context
            .create_screen_async(&mut *self.display_async().await, index)
            .await
    }

    /// Load a drawable's property, async redox.
    #[cfg(feature = "async")]
    #[inline]
    pub(crate) async fn load_drawable_property_async(
        &self,
        drawable: Drawable,
        property: u32,
    ) -> breadx::Result<Option<u32>> {
        // dashmap has a chance of blocking, so we clone ourselves a few times to get around the
        // blocking calls
        let this = self.clone();

        let map =
            match blocking::unblock(move || this.inner.drawable_properties.get(&drawable)).await {
                Some(map) => map,
                None => {
                    let repl = self
                        .display_async()
                        .await
                        .get_drawable_properties_immediate_async(drawable.into())
                        .await?;

                    let this = self.clone();
                    blocking::unblock(move || {
                        this.inner
                            .drawable_properties
                            .insert(drawable, repl.chunks(2).map(|kv| (kv[0], kv[1])).collect());
                        this.inner
                            .drawable_properties
                            .get(&drawable)
                            .expect("Infallible HashMap::get()")
                    });
                }
            };

        Ok(map.get(&property).copied())
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

        let (major_version, minor_version) =
            dpy.as_mut().query_glx_version_immediate_async(1, 1).await?;

        let mut context: Option<dispatch::DisplayDispatch> = None;

        // try to get DRI
        #[cfg(feature = "dri")]
        if stats.direct && stats.accel {
            #[cfg(feature = "dri3")]
            if !stats.no_dri3 {
                context = dri::dri3::Dri3Display::new_async(dpy.as_mut())
                    .await
                    .ok()
                    .map(|x| x.into());
            }

            // try again with dri2 if we can't do dri3
            if context.is_none() && !stats.no_dri2 {
                context = dri::dri2::Dri2Display::new_async(dpy.as_mut())
                    .await
                    .ok()
                    .map(|x| x.into());
            }
        }

        let context = match context {
            Some(context) => context,
            None => indirect::IndirectDisplay::new_async(dpy.as_mut())
                .await?
                .into(),
        };

        let this = Self {
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
            _phantom: PhantomData,
        })
    }
}
