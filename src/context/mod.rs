// MIT/Apache2 License

use std::{mem, sync::Arc};

#[cfg(feature = "async")]
use async_lock::{RwLock, RwLockReadGuard};
#[cfg(not(feature = "async"))]
use parking_lot::{lock_api::RawRwLock as _, RawRwLock, RwLock, RwLockReadGuard};

/// The context in which OpenGL functions are executed.
#[repr(transparent)]
#[derive(Clone)]
pub struct GlContext {
    inner: Arc<InnerGlContext>,
}

#[derive(Default)]
pub(crate) struct InnerGlContext {}

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
