// MIT/Apache2 License

use crate::{
    display::{DisplayLike, DisplayLock, GlInternalDisplay},
    screen::GlScreen,
};
use breadx::display::{Connection, Display};
use std::{fmt, marker::PhantomData};

#[cfg(feature = "async")]
use crate::{display::AsyncGlInternalDisplay, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

pub struct IndirectDisplay<Dpy> {
    _private: PhantomData<Dpy>,
}

impl<Dpy> fmt::Debug for IndirectDisplay<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IndirectDisplay")
    }
}

impl<Dpy: DisplayLike> IndirectDisplay<Dpy> {
    #[inline]
    pub fn new(_dpy: &mut Display<Dpy::Conn>) -> breadx::Result<Self> {
        Ok(Self {
            _private: PhantomData,
        })
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async(_dpy: &mut Display<Dpy::Conn>) -> breadx::Result<Self> {
        Ok(Self {
            _private: PhantomData,
        })
    }
}

impl<Dpy: DisplayLike> GlInternalDisplay<Dpy> for IndirectDisplay<Dpy>
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
impl<Dpy: DisplayLike> AsyncGlInternalDisplay<Dpy> for IndirectDisplay<Dpy>
where
    Dpy::Conn: AsyncConnection,
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
