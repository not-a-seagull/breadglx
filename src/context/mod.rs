// MIT/Apache2 License

use super::{
    config::GlConfig,
    display::{DisplayLike, GlDisplay},
};
use breadx::{
    auto::glx::{self, Context},
    display::{Connection, Display},
    Drawable,
};
use std::{
    any::Any,
    ffi::{c_void, CStr},
    mem,
    ptr::NonNull,
    sync::Arc,
};

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;
#[cfg(feature = "async")]
use core::future::Future;

mod attrib;
pub use attrib::*;
pub(crate) mod dispatch;
pub(crate) use dispatch::ContextDispatch;

#[cfg(feature = "async")]
use async_lock::RwLockReadGuard;
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;
#[cfg(feature = "async")]
use futures_lite::future;

#[cfg(not(feature = "async"))]
use once_cell::sync::OnceCell;
#[cfg(not(feature = "async"))]
use std::sync::{self, RwLockReadGuard};

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub(crate) struct ProcAddress(NonNull<c_void>);

unsafe impl Send for ProcAddress {}
//unsafe impl Sync for ProcAddress {}

impl From<NonNull<c_void>> for ProcAddress {
    #[inline]
    fn from(p: NonNull<c_void>) -> Self {
        Self(p)
    }
}

impl ProcAddress {
    #[inline]
    pub fn into_inner(self) -> NonNull<c_void> {
        self.0
    }
}

/// The context in which OpenGL functions are executed.
#[repr(transparent)]
pub struct GlContext<Dpy> {
    pub(crate) inner: Arc<InnerGlContext<Dpy>>,
}

impl<Dpy> Clone for GlContext<Dpy> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) struct InnerGlContext<Dpy> {
    // xid relating to the current contex
    pub(crate) xid: glx::Context,
    // the screen associated with this context
    screen: usize,
    // framebuffer config associated with this context
    fbconfig: GlConfig,
    // inner mechanism
    inner: ContextDispatch<Dpy>,
}

pub(crate) trait GlInternalContext<Dpy> {
    /// Bind this context to the given drawable.
    fn bind(
        &self,
        dpy: &GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<()>;

    fn unbind(&self) -> breadx::Result<()>;

    /// Get the proc address for the given function.
    fn get_proc_address(&self, name: &CStr) -> Option<ProcAddress>;
}

#[cfg(feature = "async")]
pub(crate) trait AsyncGlInternalContext<Dpy> {
    fn is_direct(&self) -> bool;

    fn bind_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> GenericFuture<'future, breadx::Result<()>>
    where
        'a: 'future,
        'b: 'future;

    fn unbind_async<'future>(&'future self) -> GenericFuture<'future, breadx::Result<()>>;
    fn get_proc_address_async<'future, 'a, 'b>(
        &'a self,
        name: &'b CStr,
    ) -> GenericFuture<'future, Option<ProcAddress>>
    where
        'a: 'future,
        'b: 'future;
}

impl<Dpy> GlContext<Dpy> {
    #[inline]
    pub(crate) fn dispatch(&self) -> &ContextDispatch<Dpy> {
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
    pub(crate) fn set_dispatch(&mut self, disp: ContextDispatch<Dpy>) {
        Arc::get_mut(&mut self.inner)
            .expect("Infallible Arc::get_mut()")
            .inner = disp;
    }

    #[inline]
    pub fn xid(&self) -> Context {
        self.inner.xid
    }

    #[inline]
    pub(crate) fn get() -> RwLockReadGuard<'static, Option<AnyArc>> {
        get_current_context()
    }
}

impl<Dpy: DisplayLike> GlContext<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn bind_internal(
        &self,
        dpy: &GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<Option<GlContext<Dpy>>> {
        // get this and unbind the old
        let old_gc = get_current_context();
        let old_gc_ref = old_gc.as_ref().and_then(|m| promote_anyarc_ref(m));

        if let Some(old_gc) = old_gc_ref {
            if Arc::ptr_eq(&self.inner, &old_gc.inner) {
                log::warn!("Attempted to set currently active GlContext as active.");
                return Ok(None);
            }
        }

        self.inner.inner.bind(dpy, read, draw)?;
        if let Some(old_gc) = old_gc_ref {
            old_gc.inner.inner.unbind()?;
        }

        // bind the current gc to the old one
        mem::drop(old_gc);
        let old_gc = set_current_context(self.clone());

        // try to promote the GC to the proper version
        let old_gc = old_gc.and_then(|old_gc| promote_anyarc(old_gc));

        Ok(old_gc)
    }

    #[inline]
    pub fn bind<Target: Into<Drawable>>(
        &self,
        dpy: &GlDisplay<Dpy>,
        draw: Target,
    ) -> breadx::Result<Option<GlContext<Dpy>>> {
        let draw = draw.into();
        self.bind_internal(dpy, Some(draw), Some(draw))
    }

    #[inline]
    pub(crate) fn get_proc_address(&self, name: &CStr) -> Option<ProcAddress> {
        self.inner.inner.get_proc_address(name)
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> GlContext<Dpy>
where
    Dpy::Connection: AsyncConnection + Send,
{
    #[inline]
    async fn bind_internal_async(
        &self,
        dpy: &GlDisplay<Dpy>,
        read: Option<Drawable>,
        draw: Option<Drawable>,
    ) -> breadx::Result<Option<GlContext<Dpy>>> {
        // get this and unbind the old
        let old_gc = get_current_context_async().await;
        let old_gc_ref = old_gc.as_ref().and_then(|m| promote_anyarc_ref(m));

        if let Some(old_gc) = old_gc_ref {
            if Arc::ptr_eq(&self.inner, &old_gc.inner) {
                log::warn!("Attempted to set currently active GlContext as active.");
                return Ok(None);
            }
        }

        self.inner.inner.bind_async(dpy, read, draw).await?;
        if let Some(old_gc) = old_gc_ref {
            old_gc.inner.inner.unbind_async().await?;
        }

        // bind the current gc to the old one
        mem::drop(old_gc);
        let old_gc = set_current_context_async(self.clone()).await;

        // try to promote the GC to the proper version
        let old_gc = old_gc.and_then(|old_gc| promote_anyarc(old_gc));

        Ok(old_gc)
    }

    #[inline]
    pub fn bind_async<Target: Into<Drawable>>(
        &self,
        dpy: &GlDisplay<Dpy>,
        draw: Target,
    ) -> impl Future<Output = breadx::Result<Option<GlContext<Dpy>>>> {
        let draw = draw.into();
        self.bind_internal_async(dpy, Some(draw), Some(draw))
    }
}

pub(crate) type AnyArc = Arc<dyn Any + Send + Sync + 'static>;

/// A static memory location containing the currently active GlContext.
/// GL calls are global (unfortunately). All GL calls should be made onto
/// this context.
/// Note: The inner context here is always an Arc<InnerGlContext<Conn, Dpy>> of some type. We confirm these
/// generic parameters whenever we load or set the context. In any sane configuration we shouldn't end up
/// with a downcasting error.
/// TODO: there HAS to be a better way of doing this
#[cfg(feature = "async")]
static CURRENT_CONTEXT: async_lock::RwLock<Option<AnyArc>> = async_lock::RwLock::new(None);
#[cfg(not(feature = "async"))]
static CURRENT_CONTEXT: OnceCell<sync::RwLock<Option<AnyArc>>> = OnceCell::new();

#[cfg(not(feature = "async"))]
const FAILED_READ: &str = "Failed to acquire read lock for current context";
#[cfg(not(feature = "async"))]
const FAILED_WRITE: &str = "Failed to acquire write lock for current context";

#[cfg(not(feature = "async"))]
fn current_context() -> &'static sync::RwLock<Option<AnyArc>> {
    CURRENT_CONTEXT.get_or_init(|| sync::RwLock::new(None))
}

/// Try to promote an AnyArc to a GlContext.
#[inline]
pub(crate) fn promote_anyarc<Dpy: Send + Sync + 'static>(a: AnyArc) -> Option<GlContext<Dpy>> {
    match Arc::downcast::<InnerGlContext<Dpy>>(a) {
        Ok(inner) => Some(GlContext { inner }),
        Err(_) => {
            log::error!("Failed to promote GlContext.");
            None
        }
    }
}

#[inline]
pub(crate) fn promote_anyarc_ref<Dpy: Send + Sync + 'static>(
    a: &AnyArc,
) -> Option<&GlContext<Dpy>> {
    if Any::is::<InnerGlContext<Dpy>>(&**a) {
        // SAFETY: GlContext is just a transparent wrapper around Arc<InnerGlContext>, so we can safely use
        //         pointer transmutation once we're certain of its inner type
        Some(unsafe { &*(a as *const AnyArc as *const GlContext<Dpy>) })
    } else {
        log::error!("Failed to promote GlContext.");
        None
    }
}

#[inline]
pub(crate) fn get_current_context() -> RwLockReadGuard<'static, Option<AnyArc>> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            future::block_on(get_current_context_async())
        } else {
            current_context().read().expect(FAILED_READ)
        }
    }
}

#[inline]
pub(crate) fn set_current_context<Dpy: Send + Sync + 'static>(
    ctx: GlContext<Dpy>,
) -> Option<AnyArc> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            future::block_on(set_current_context_async(ctx))
        } else {
            mem::replace(
                &mut *current_context().write().expect(FAILED_WRITE),
                Some(ctx.inner),
            )
        }
    }
}

#[inline]
pub(crate) fn take_current_context() -> Option<AnyArc> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "async")] {
            future::block_on(take_current_context_async())
        } else {
            mem::take(&mut *current_context().write().expect(FAILED_WRITE))
        }
    }
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn get_current_context_async() -> RwLockReadGuard<'static, Option<AnyArc>> {
    CURRENT_CONTEXT.read().await
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn set_current_context_async<Dpy: Send + Sync + 'static>(
    ctx: GlContext<Dpy>,
) -> Option<AnyArc> {
    mem::replace(&mut *CURRENT_CONTEXT.write().await, Some(ctx.inner))
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn take_current_context_async() -> Option<AnyArc> {
    mem::take(&mut *CURRENT_CONTEXT.write().await)
}
