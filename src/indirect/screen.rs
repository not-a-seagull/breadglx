// MIT/Apache2 License

use crate::{
    config::GlConfig,
    context::{dispatch::ContextDispatch, GlContext, GlContextRule, InnerGlContext},
    display::{DisplayLike, GlDisplay},
    screen::GlInternalScreen,
};
use breadx::{display::Connection, Drawable};
use std::{fmt, marker::PhantomData, sync::Arc};

#[cfg(feature = "async")]
use crate::{screen::AsyncGlInternalScreen, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

pub struct IndirectScreen<Dpy> {
    // As before, the indirect screen doesn't really have any
    // attached data.
    _private: PhantomData<Dpy>,
}

impl<Dpy> fmt::Debug for IndirectScreen<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IndirectScreen")
    }
}

impl<Dpy> IndirectScreen<Dpy> {
    #[inline]
    pub fn new() -> Self {
        Self {
            _private: PhantomData,
        }
    }
}

impl<Dpy: DisplayLike> GlInternalScreen<Dpy> for IndirectScreen<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn create_context(
        &self,
        base: &mut Arc<InnerGlContext<Dpy>>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext<Dpy>>,
    ) -> breadx::Result<ContextDispatch<Dpy>> {
        unimplemented!()
    }

    #[inline]
    fn swap_buffers(
        &self,
        display: &GlDisplay<Dpy>,
        drawable: Drawable,
        target_msc: i64,
        divisor: i64,
        remainder: i64,
        flush: bool,
    ) -> breadx::Result {
        unimplemented!()
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalScreen<Dpy> for IndirectScreen<Dpy>
where
    Dpy::Connection: AsyncConnection,
{
    #[inline]
    fn create_context_async<'future, 'a, 'b, 'c, 'd, 'e>(
        &'a self,
        base: &'b mut Arc<InnerGlContext<Dpy>>,
        fbconfig: &'c GlConfig,
        rules: &'d [GlContextRule],
        share: Option<&'e GlContext<Dpy>>,
    ) -> GenericFuture<'future, breadx::Result<ContextDispatch<Dpy>>>
    where
        'a: 'future,
        'b: 'future,
        'c: 'future,
        'd: 'future,
        'e: 'future,
    {
        Box::pin(async { unimplemented!() })
    }

    #[inline]
    fn swap_buffers_async<'future>(
        &'future self,
        drawable: Drawable,
        target_msc: i64,
        divisor: i64,
        remainder: i64,
        flush: bool,
    ) -> GenericFuture<'future, breadx::Result> {
        Box::pin(async { unimplemented!() })
    }
}
