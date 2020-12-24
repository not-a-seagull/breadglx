// MIT/Apache2 License

use crate::{dri, indirect, mesa, screen::GlScreen, util::env_to_boolean};
use breadx::display::{Connection, Display};
use std::{marker::PhantomData, ops::Range};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

mod dispatch;

/// The OpenGL context acting as a wrapper.
pub struct GlDisplay<Conn, Dpy> {
    // the display that this acts as a wrapper around
    display: Dpy,

    // if the rendering should be direct or hardware accelerated
    direct: bool,
    accel: bool,

    // the underlying GL rendering method
    context: dispatch::DisplayDispatch,

    // needed to satisfy type constraints
    _phantom: PhantomData<Conn>,
}

/// The underlying OpenGL context.
pub(crate) trait GlInternalDisplay {
    fn create_screen<Conn: Connection>(
        &mut self,
        dpy: &mut Display<Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen>;

    #[cfg(feature = "async")]
    fn create_screen_async<'future, 'a, 'b, Conn: Connection>(
        &'a mut self,
        dpy: &'b mut Display<Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen>>
    where
        'a: 'future,
        'b: 'future;
}

impl<Conn: Connection, Dpy: AsRef<Display<Conn>>> GlDisplay<Conn, Dpy> {
    #[inline]
    pub fn display(&self) -> &Display<Conn> {
        self.display.as_ref()
    }
}

impl<Conn: Connection, Dpy: AsRef<Display<Conn>>> AsRef<Display<Conn>> for GlDisplay<Conn, Dpy> {
    #[inline]
    fn as_ref(&self) -> &Display<Conn> {
        self.display()
    }
}

impl<Conn: Connection, Dpy: AsMut<Display<Conn>>> GlDisplay<Conn, Dpy> {
    #[inline]
    pub fn display_mut(&mut self) -> &mut Display<Conn> {
        self.display.as_mut()
    }
}

impl<Conn: Connection, Dpy: AsMut<Display<Conn>>> AsMut<Display<Conn>> for GlDisplay<Conn, Dpy> {
    #[inline]
    fn as_mut(&mut self) -> &mut Display<Conn> {
        self.display_mut()
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
    pub fn create_screen(&mut self, screen: usize) -> breadx::Result<GlScreen> {
        self.context.create_screen(self.display.as_mut(), screen)
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn create_screen_async(&mut self, index: usize) -> breadx::Result<GlScreen> {
        self.context
            .create_screen_async(self.display.as_mut(), index)
            .await
    }

    #[inline]
    pub fn new(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

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

        let mut this = Self {
            display: dpy,
            direct: stats.direct,
            accel: stats.accel,
            context,
            _phantom: PhantomData,
        };

        Ok(this)
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async(mut dpy: Dpy) -> breadx::Result<Self> {
        // create the basic display
        let stats = GlStats::get();

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

        let mut this = Self {
            display: dpy,
            direct: stats.direct,
            accel: stats.accel,
            context,
            _phantom: PhantomData,
        };

        Ok(this)
    }
}
