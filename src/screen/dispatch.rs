// MIT/Apache2 License

use super::GlInternalScreen;
use crate::{
    config::GlConfig,
    context::{ContextDispatch, GlContext, GlContextRule, InnerGlContext},
    display::DisplayLike,
    dri::{dri2, dri3},
    indirect,
};
use breadx::display::Connection;
use std::sync::Arc;

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

#[derive(Debug)]
pub(crate) enum ScreenDispatch<Dpy> {
    Indirect(indirect::IndirectScreen<Dpy>),
    #[cfg(feature = "dri")]
    Dri2(dri2::Dri2Screen<Dpy>),
    #[cfg(feature = "dri3")]
    Dri3(dri3::Dri3Screen<Dpy>),
}

impl<Dpy> From<indirect::IndirectScreen<Dpy>> for ScreenDispatch<Dpy> {
    #[inline]
    fn from(i: indirect::IndirectScreen<Dpy>) -> Self {
        Self::Indirect(i)
    }
}

#[cfg(feature = "dri")]
impl<Dpy> From<dri2::Dri2Screen<Dpy>> for ScreenDispatch<Dpy> {
    #[inline]
    fn from(d2: dri2::Dri2Screen<Dpy>) -> Self {
        Self::Dri2(d2)
    }
}

#[cfg(feature = "dri3")]
impl<Dpy> From<dri3::Dri3Screen<Dpy>> for ScreenDispatch<Dpy> {
    #[inline]
    fn from(d3: dri3::Dri3Screen<Dpy>) -> Self {
        Self::Dri3(d3)
    }
}

impl<Dpy: DisplayLike> GlInternalScreen<Dpy> for ScreenDispatch<Dpy>
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
        match self {
            Self::Indirect(is) => is.create_context(base, fbconfig, rules, share),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.create_context(base, fbconfig, rules, share),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.create_context(base, fbconfig, rules, share),
        }
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalScreen<Dpy> for ScreenDispatch<Dpy>
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
        match self {
            Self::Indirect(is) => is.create_context_async(base, fbconfig, rules, share),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.create_context_async(base, fbconfig, rules, share),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.create_context_async(base, fbconfig, rules, share),
        }
    }
}
