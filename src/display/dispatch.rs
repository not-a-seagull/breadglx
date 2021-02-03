// MIT/Apache2 License

use super::DisplayLike;
use crate::{
    display::DisplayLock,
    dri::{dri2, dri3},
    indirect,
    screen::GlScreen,
};
use breadx::{Connection, Display};

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;
#[cfg(feature = "async")]
use std::boxed::Box;

/// Dispatch for contexts.
#[derive(Debug)]
pub(crate) enum DisplayDispatch<Dpy> {
    Indirect(indirect::IndirectDisplay<Dpy>),
    #[cfg(feature = "dri")]
    Dri2(dri2::Dri2Display<Dpy>),
    #[cfg(feature = "dri3")]
    Dri3(dri3::Dri3Display<Dpy>),
}

impl<Dpy> From<indirect::IndirectDisplay<Dpy>> for DisplayDispatch<Dpy> {
    #[inline]
    fn from(i: indirect::IndirectDisplay<Dpy>) -> Self {
        Self::Indirect(i)
    }
}

#[cfg(feature = "dri")]
impl<Dpy> From<dri2::Dri2Display<Dpy>> for DisplayDispatch<Dpy> {
    #[inline]
    fn from(d2: dri2::Dri2Display<Dpy>) -> Self {
        Self::Dri2(d2)
    }
}

#[cfg(feature = "dri3")]
impl<Dpy> From<dri3::Dri3Display<Dpy>> for DisplayDispatch<Dpy> {
    #[inline]
    fn from(d3: dri3::Dri3Display<Dpy>) -> Self {
        Self::Dri3(d3)
    }
}

impl<Dpy: DisplayLike> super::GlInternalDisplay<Dpy> for DisplayDispatch<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn create_screen(
        &self,
        dpy: &mut Display<Dpy::Connection>,
        index: usize,
    ) -> breadx::Result<GlScreen<Dpy>> {
        match self {
            Self::Indirect(i) => i.create_screen(dpy, index),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.create_screen(dpy, index),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.create_screen(dpy, index),
        }
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> super::AsyncGlInternalDisplay<Dpy> for DisplayDispatch<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    fn create_screen_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b mut Display<Dpy::Connection>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen<Dpy>>>
    where
        'a: 'future,
        'b: 'future,
    {
        match self {
            Self::Indirect(i) => i.create_screen_async(dpy, index),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.create_screen_async(dpy, index),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.create_screen_async(dpy, index),
        }
    }
}
