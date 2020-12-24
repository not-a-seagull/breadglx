// MIT/Apache2 License

use crate::{
    dri::{dri2, dri3},
    indirect,
    screen::GlScreen,
};
use breadx::{Connection, Display};

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use std::boxed::Box;

/// Dispatch for contexts.
#[derive(Debug)]
pub(crate) enum DisplayDispatch {
    Indirect(indirect::IndirectDisplay),
    #[cfg(feature = "dri")]
    Dri2(dri2::Dri2Display),
    #[cfg(feature = "dri3")]
    Dri3(dri3::Dri3Display),
}

impl From<indirect::IndirectDisplay> for DisplayDispatch {
    #[inline]
    fn from(i: indirect::IndirectDisplay) -> Self {
        Self::Indirect(i)
    }
}

#[cfg(feature = "dri")]
impl From<dri2::Dri2Display> for DisplayDispatch {
    #[inline]
    fn from(d2: dri2::Dri2Display) -> Self {
        Self::Dri2(d2)
    }
}

#[cfg(feature = "dri3")]
impl From<dri3::Dri3Display> for DisplayDispatch {
    #[inline]
    fn from(d3: dri3::Dri3Display) -> Self {
        Self::Dri3(d3)
    }
}

impl super::GlInternalDisplay for DisplayDispatch {
    #[inline]
    fn create_screen<Conn: Connection>(
        &mut self,
        dpy: &mut Display<Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen> {
        match self {
            Self::Indirect(i) => i.create_screen(dpy, index),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.create_screen(dpy, index),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.create_screen(dpy, index),
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    fn create_screen_async<'future, 'a, 'b, Conn: Connection>(
        &'a mut self,
        dpy: &'b mut Display<Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async move {
            match self {
                Self::Indirect(i) => i.create_screen_async(dpy, index).await,
                #[cfg(feature = "dri")]
                Self::Dri2(d2) => d2.create_screen_async(dpy, index).await,
                #[cfg(feature = "dri3")]
                Self::Dri3(d3) => d3.create_screen_async(dpy, index).await,
            }
        })
    }
}
