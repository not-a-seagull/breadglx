// MIT/Apache2 License

use super::{GlInternalContext, ProcAddress};
use crate::{
    display::{DisplayLike, GlDisplay},
    dri::{dri2, dri3},
    indirect,
};
use breadx::{
    display::{Connection, Display},
    Drawable,
};
use std::ffi::CStr;

#[cfg(feature = "async")]
use crate::{context::AsyncGlInternalContext, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

#[derive(Debug)]
pub enum ContextDispatch<Dpy> {
    Indirect(indirect::IndirectContext<Dpy>),
    Placeholder,
    #[cfg(feature = "dri")]
    Dri2(dri2::Dri2Context<Dpy>),
    #[cfg(feature = "dri3")]
    Dri3(dri3::Dri3Context<Dpy>),
}

impl<Dpy> From<indirect::IndirectContext<Dpy>> for ContextDispatch<Dpy> {
    #[inline]
    fn from(i: indirect::IndirectContext<Dpy>) -> Self {
        Self::Indirect(i)
    }
}

#[cfg(feature = "dri")]
impl<Dpy> From<dri2::Dri2Context<Dpy>> for ContextDispatch<Dpy> {
    #[inline]
    fn from(d2: dri2::Dri2Context<Dpy>) -> Self {
        Self::Dri2(d2)
    }
}

#[cfg(feature = "dri3")]
impl<Dpy> From<dri3::Dri3Context<Dpy>> for ContextDispatch<Dpy> {
    #[inline]
    fn from(d3: dri3::Dri3Context<Dpy>) -> Self {
        Self::Dri3(d3)
    }
}

impl<Dpy> ContextDispatch<Dpy> {
    #[inline]
    pub fn is_direct(&self) -> bool {
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(_) => false,
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => true,
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => true,
        }
    }
}

impl<Dpy: DisplayLike> GlInternalContext<Dpy> for ContextDispatch<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn bind(
        &self,
        dpy: &GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<()> {
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(i) => i.bind(dpy, read, draw),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.bind(dpy, read, draw),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.bind(dpy, read, draw),
        }
    }

    #[inline]
    fn unbind(&self) -> breadx::Result<()> {
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(i) => i.unbind(),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.unbind(),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.unbind(),
        }
    }

    #[inline]
    fn get_proc_address(&self, name: &CStr) -> Option<ProcAddress> {
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(i) => i.get_proc_address(name),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.get_proc_address(name),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.get_proc_address(name),
        }
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalContext<Dpy> for ContextDispatch<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    fn bind_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> GenericFuture<'future, breadx::Result<()>>
    where
        'a: 'future,
        'b: 'future,
    {
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(i) => Box::pin(i.bind_async(dpy, read, draw)),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => Box::pin(d2.bind_async(dpy, read, draw)),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => Box::pin(d3.bind_async(dpy, read, draw)),
        }
    }

    #[inline]
    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result<()>> {
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(i) => i.unbind_async(),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.unbind_async(),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.unbind_async(),
        }
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
        match self {
            Self::Placeholder => unreachable!("Invalid placeholder"),
            Self::Indirect(i) => i.get_proc_address_async(name),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.get_proc_address_async(name),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.get_proc_address_async(name),
        }
    }
}
