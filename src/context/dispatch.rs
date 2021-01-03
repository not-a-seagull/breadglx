// MIT/Apache2 License

use super::GlInternalContext;
use crate::{
    display::GlDisplay,
    dri::{dri2, dri3},
    indirect,
};
use breadx::{
    display::{Connection, Display},
    Drawable,
};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

pub enum ContextDispatch {
    Indirect(indirect::IndirectContext),
    Placeholder,
    #[cfg(feature = "dri")]
    Dri2(dri2::Dri2Context),
    #[cfg(feature = "dri3")]
    Dri3(dri3::Dri3Context),
}

impl From<indirect::IndirectContext> for ContextDispatch {
    #[inline]
    fn from(i: indirect::IndirectContext) -> Self {
        Self::Indirect(i)
    }
}

#[cfg(feature = "dri")]
impl From<dri2::Dri2Context> for ContextDispatch {
    #[inline]
    fn from(d2: dri2::Dri2Context) -> Self {
        Self::Dri2(d2)
    }
}

#[cfg(feature = "dri3")]
impl From<dri3::Dri3Context> for ContextDispatch {
    #[inline]
    fn from(d3: dri3::Dri3Context) -> Self {
        Self::Dri3(d3)
    }
}

impl GlInternalContext for ContextDispatch {
    #[inline]
    fn bind<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>>(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
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
    ) -> GenericFuture<'future, breadx::Result<()>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async move {
            match self {
                Self::Placeholder => unreachable!("Invalid placeholder"),
                Self::Indirect(i) => i.bind_async(dpy, read, draw).await,
                #[cfg(feature = "dri")]
                Self::Dri2(d2) => d2.bind_async(dpy, read, draw).await,
                #[cfg(feature = "dri3")]
                Self::Dri3(d3) => d3.bind_async(dpy, read, draw).await,
            }
        })
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

    #[cfg(feature = "async")]
    #[inline]
    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result<()>> {
        Box::pin(async move {
            match self {
                Self::Placeholder => unreachable!("Invalid placeholder"),
                Self::Indirect(i) => i.unbind_async().await,
                #[cfg(feature = "dri")]
                Self::Dri2(d2) => d2.unbind_async().await,
                #[cfg(feature = "dri3")]
                Self::Dri3(d3) => d3.unbind_async().await,
            }
        })
    }
}
