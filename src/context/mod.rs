// MIT/Apache2 License

use super::{config::GlConfig, display::GlDisplay};
use breadx::{
    auto::glx,
    display::{Connection, Display},
    Drawable,
};
use std::{mem, sync::Arc};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

mod attrib;
pub use attrib::*;
pub(crate) mod dispatch;
pub(crate) use dispatch::ContextDispatch;

#[cfg(feature = "async")]
use async_lock::{RwLock, RwLockReadGuard};
#[cfg(not(feature = "async"))]
use parking_lot::{lock_api::RawRwLock as _, RawRwLock, RwLock, RwLockReadGuard};

/// The context in which OpenGL functions are executed.
#[repr(transparent)]
#[derive(Clone)]
pub struct GlContext {
    pub(crate) inner: Arc<InnerGlContext>,
}

pub(crate) struct InnerGlContext {
    // xid relating to the current contex
    xid: glx::Context,
    // the screen associated with this context
    screen: usize,
    // framebuffer config associated with this context
    fbconfig: GlConfig,
    // inner mechanism
    inner: dispatch::ContextDispatch,
}

pub(crate) trait GlInternalContext {
    /// Bind this context to the given drawable.
    fn bind<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>>(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<()>;

    #[cfg(feature = "async")]
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
        'b: 'future;

    fn unbind(&self) -> breadx::Result<()>;

    #[cfg(feature = "async")]
    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result<()>>;
}

impl GlContext {
    #[inline]
    pub(crate) fn dispatch(&self) -> &dispatch::ContextDispatch {
        &self.inner.inner
    }

    #[inline]
    pub(crate) fn new(xid: glx::Context, screen: usize, fbconfig: GlConfig) -> Self {
        Self {
            inner: Arc::new(InnerGlContext {
                xid,
                screen,
                fbconfig,
                inner: ContextDispatch::Placeholder,
            }),
        }
    }

    #[inline]
    pub(crate) fn set_dispatch(&mut self, disp: ContextDispatch) {
        Arc::get_mut(&mut self.inner)
            .expect("Infallible Arc::get_mut()")
            .inner = disp;
    }

    #[inline]
    fn bind_internal<Conn: Connection, Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>>(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<Option<GlContext>> {
        // get this and unbind the old
        let old_gc = get_current_context();

        if let Some(old_gc) = &*old_gc {
            if Arc::ptr_eq(&self.inner, &old_gc.inner) {
                log::warn!("Attempted to set currently active GlContext as active.");
                return Ok(None);
            }
        }

        self.inner.inner.bind(dpy, read, draw)?;
        if let Some(old_gc) = &*old_gc {
            old_gc.inner.inner.unbind()?;
        }

        // bind the current gc to the old one
        mem::drop(old_gc);
        let old_gc = set_current_context(self.clone());
        Ok(old_gc)
    }

    #[cfg(feature = "async")]
    #[inline]
    async fn bind_internal_async<
        Conn: Connection,
        Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>> + Send,
    >(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<Option<GlContext>> {
        // get this and unbind the old
        let old_gc = get_current_context_async().await;

        if let Some(old_gc) = &*old_gc {
            if Arc::ptr_eq(&self.inner, &old_gc.inner) {
                log::warn!("Attempted to set currently active GlContext as active.");
                return Ok(None);
            }
        }

        self.inner.inner.bind_async(dpy, read, draw).await?;
        if let Some(old_gc) = &*old_gc {
            old_gc.inner.inner.unbind_async().await?;
        }

        // bind the current gc to the old one
        mem::drop(old_gc);
        let old_gc = set_current_context_async(self.clone()).await;
        Ok(old_gc)
    }

    #[inline]
    pub fn bind<
        Conn: Connection,
        Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>>,
        Target: Into<Drawable>,
    >(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        draw: Target,
    ) -> breadx::Result<Option<GlContext>> {
        let draw = draw.into();
        self.bind_internal(dpy, Some(draw), Some(draw))
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn bind_async<
        Conn: Connection,
        Dpy: AsRef<Display<Conn>> + AsMut<Display<Conn>> + Send,
        Target: Into<Drawable>,
    >(
        &self,
        dpy: &mut GlDisplay<Conn, Dpy>,
        draw: Target,
    ) -> breadx::Result<Option<GlContext>> {
        let draw = draw.into();
        self.bind_internal_async(dpy, Some(draw), Some(draw)).await
    }
}

/// A static memory location containing the currently active GlContext.
/// GL calls are global (unfortunately). All GL calls should be made onto
/// this context.
/// TODO: there HAS to be a better way of doing this
static CURRENT_CONTEXT: RwLock<Option<GlContext>> = {
    #[cfg(feature = "async")]
    {
        RwLock::new(None)
    }

    #[cfg(not(feature = "async"))]
    {
        RwLock::const_new(RawRwLock::INIT, None)
    }
};

#[inline]
pub(crate) fn get_current_context() -> RwLockReadGuard<'static, Option<GlContext>> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            async_io::block_on(get_current_context_async())
        } else {
            CURRENT_CONTEXT.read()
        }
    }
}

#[inline]
pub(crate) fn set_current_context(ctx: GlContext) -> Option<GlContext> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            async_io::block_on(set_current_context_async(ctx))
        } else {
            let mut ctx = Some(ctx);
            mem::swap(
                &mut *CURRENT_CONTEXT.write(),
                &mut ctx,
            );
            ctx
        }
    }
}

#[inline]
pub(crate) fn take_current_context() -> Option<GlContext> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            async_io::block_on(take_current_context_async())
        } else {
            mem::take(&mut *CURRENT_CONTEXT.write())
        }
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn get_current_context_async() -> RwLockReadGuard<'static, Option<GlContext>> {
    CURRENT_CONTEXT.read().await
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn set_current_context_async(ctx: GlContext) -> Option<GlContext> {
    let mut ctx = Some(ctx);
    mem::swap(&mut *CURRENT_CONTEXT.write().await, &mut ctx);
    ctx
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn take_current_context_async() -> Option<GlContext> {
    mem::take(&mut *CURRENT_CONTEXT.write().await)
}
