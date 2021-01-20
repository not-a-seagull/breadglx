// MIT/Apache2 License

use crate::{
    display::{DisplayLike, DisplayLock, GlInternalDisplay},
    screen::GlScreen,
};
use breadx::display::{Connection, Display};
use std::fmt;

#[cfg(feature = "async")]
use crate::{display::AsyncGlInternalDisplay, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

pub struct Dri2Display<Dpy> {
    p: Dpy,
}

impl<Dpy> fmt::Debug for Dri2Display<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Dri2Display")
    }
}

impl<Dpy: DisplayLike> Dri2Display<Dpy> {
    #[inline]
    pub(crate) fn new(dpy: &mut Display<Dpy::Conn>) -> breadx::Result<Self> {
        unimplemented!()
    }

    #[inline]
    pub(crate) async fn new_async(dpy: &mut Display<Dpy::Conn>) -> breadx::Result<Self> {
        unimplemented!()
    }
}

impl<Dpy: DisplayLike> GlInternalDisplay<Dpy> for Dri2Display<Dpy>
where
    Dpy::Conn: Connection,
{
    #[inline]
    fn create_screen(
        &self,
        dpy: &mut Display<Dpy::Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen<Dpy>> {
        unimplemented!()
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalDisplay<Dpy> for Dri2Display<Dpy>
where
    Dpy::Conn: AsyncConnection + Send,
{
    #[inline]
    fn create_screen_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b mut Display<Dpy::Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen<Dpy>>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async { unimplemented!() })
    }
}
