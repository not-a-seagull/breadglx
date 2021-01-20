// MIT/Apache2 License

use crate::{
    config::GlConfig,
    context::{dispatch::ContextDispatch, GlContext, GlContextRule, InnerGlContext},
    display::DisplayLike,
    screen::GlInternalScreen,
};
use breadx::Connection;
use std::sync::Arc;

#[cfg(feature = "async")]
use crate::{screen::AsyncGlInternalScreen, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

#[derive(Debug)]
pub struct Dri2Screen<Dpy> {
    p: Dpy,
}

impl<Dpy: DisplayLike> GlInternalScreen<Dpy> for Dri2Screen<Dpy>
where
    Dpy::Conn: Connection,
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
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalScreen<Dpy> for Dri2Screen<Dpy>
where
    Dpy::Conn: AsyncConnection + Send,
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
}
