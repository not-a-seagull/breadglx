// MIT/Apache2 License

use crate::{
    context::{GlInternalContext, ProcAddress, Profile},
    display::{DisplayLike, GlDisplay},
};
use breadx::{
    display::{Connection, Display},
    Drawable,
};
use std::{ffi::CStr, fmt};

#[cfg(feature = "async")]
use crate::{context::AsyncGlInternalContext, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

pub struct IndirectContext<Dpy> {
    // hold a reference to the display so we can call commands
    display: GlDisplay<Dpy>,
    // the buffer to hold GLX commands in before we flush them
    glx_buffer: Vec<u8>,
    // attributes we take from the rules sections
    render_type: u32,
    major_version: u32,
    minor_version: u32,
    profile: Profile,
}

impl<Dpy> fmt::Debug for IndirectContext<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IndirectContext")
    }
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

    #[inline]
    fn get_proc_address(&self, name: &CStr) -> Option<ProcAddress> {
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

    #[inline]
    fn get_proc_address_async<'future, 'a, 'b>(
        &'a self,
        name: &'b CStr,
    ) -> GenericFuture<'future, Option<ProcAddress>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async { unimplemented!() })
    }
}
