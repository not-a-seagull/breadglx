// MIT/Apache2 License

use crate::{
    context::GlInternalContext,
    display::{DisplayLike, GlDisplay},
};
use breadx::{
    display::{Connection, Display},
    Drawable,
};

#[cfg(feature = "async")]
use crate::{context::AsyncGlInternalContext, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

pub struct IndirectContext<Dpy> {
    p: Dpy,
}

impl<Dpy: DisplayLike> GlInternalContext<Dpy> for IndirectContext<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn bind(
        &self,
        dpy: &GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result {
        unimplemented!()
    }

    #[inline]
    fn unbind(&self) -> breadx::Result {
        unimplemented!()
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalContext<Dpy> for IndirectContext<Dpy>
where
    Dpy::Connection: AsyncConnection,
{
    #[inline]
    fn bind_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> GenericFuture<'future, breadx::Result>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async { unimplemented!() })
    }

    #[inline]
    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result> {
        Box::pin(async { unimplemented!() })
    }
}
