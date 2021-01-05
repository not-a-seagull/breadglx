// MIT/Apache2 License

use crate::{context::GlInternalContext, display::GlDisplay};
use breadx::{
    display::{Connection, Display},
    Drawable,
};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

pub struct IndirectContext {}

impl GlInternalContext for IndirectContext {
    #[inline]
    fn is_direct(&self) -> bool {
        false
    }

    #[inline]
    fn bind<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>>(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result {
        unimplemented!()
    }

    #[cfg(feature = "async")]
    #[inline]
    fn bind_async<
        'future,
        'a,
        'b,
        Conn: Connection,
        Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>> + Send,
    >(
        &'a self,
        dpy: &'b mut GlDisplay<Conn, Dpy>,
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
    fn unbind(&self) -> breadx::Result {
        unimplemented!()
    }

    #[cfg(feature = "async")]
    #[inline]
    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result> {
        Box::pin(async { unimplemented!() })
    }
}
