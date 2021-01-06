// MIT/Apache2 License

use crate::{
    display::{DisplayLock, GlInternalDisplay},
    screen::GlScreen,
};
use breadx::display::{Connection, Display};
use std::fmt;

#[cfg(feature = "async")]
use crate::util::GenericFuture;

pub struct Dri2Display {}

impl fmt::Debug for Dri2Display {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Dri2Display")
    }
}

impl Dri2Display {
    #[inline]
    pub(crate) fn new<Conn: Connection>(dpy: &mut Display<Conn>) -> breadx::Result<Self> {
        unimplemented!()
    }

    #[inline]
    pub(crate) async fn new_async<Conn: Connection>(
        dpy: &mut Display<Conn>,
    ) -> breadx::Result<Self> {
        unimplemented!()
    }
}

impl GlInternalDisplay for Dri2Display {
    #[inline]
    fn create_screen<Conn: Connection>(
        &self,
        dpy: &mut Display<Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen> {
        unimplemented!()
    }

    #[cfg(feature = "async")]
    #[inline]
    fn create_screen_async<'future, 'a, 'b, Conn: Connection>(
        &'a self,
        dpy: &'b mut Display<Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async { unimplemented!() })
    }
}
