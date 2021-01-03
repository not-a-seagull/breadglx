// MIT/Apache2 License

use super::GlInternalScreen;
use crate::{
    config::GlConfig,
    context::{ContextDispatch, GlContext, GlContextRule, InnerGlContext},
    dri::{dri2, dri3},
    indirect,
};
use std::sync::Arc;

#[cfg(feature = "async")]
use crate::util::GenericFuture;

#[derive(Debug)]
pub(crate) enum ScreenDispatch {
    Indirect(indirect::IndirectScreen),
    #[cfg(feature = "dri")]
    Dri2(dri2::Dri2Screen),
    #[cfg(feature = "dri3")]
    Dri3(dri3::Dri3Screen),
}

impl From<indirect::IndirectScreen> for ScreenDispatch {
    #[inline]
    fn from(i: indirect::IndirectScreen) -> Self {
        Self::Indirect(i)
    }
}

#[cfg(feature = "dri")]
impl From<dri2::Dri2Screen> for ScreenDispatch {
    #[inline]
    fn from(d2: dri2::Dri2Screen) -> Self {
        Self::Dri2(d2)
    }
}

#[cfg(feature = "dri3")]
impl From<dri3::Dri3Screen> for ScreenDispatch {
    #[inline]
    fn from(d3: dri3::Dri3Screen) -> Self {
        Self::Dri3(d3)
    }
}

impl GlInternalScreen for ScreenDispatch {
    #[inline]
    fn create_context(
        &self,
        base: &mut Arc<InnerGlContext>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext>,
    ) -> breadx::Result<ContextDispatch> {
        match self {
            Self::Indirect(is) => is.create_context(base, fbconfig, rules, share),
            #[cfg(feature = "dri")]
            Self::Dri2(d2) => d2.create_context(base, fbconfig, rules, share),
            #[cfg(feature = "dri3")]
            Self::Dri3(d3) => d3.create_context(base, fbconfig, rules, share),
        }
    }

    #[cfg(feature = "async")]
    #[inline]
    fn create_context_async<'future, 'a, 'b, 'c, 'd, 'e>(
        &'a self,
        base: &'b mut Arc<InnerGlContext>,
        fbconfig: &'c GlConfig,
        rules: &'d [GlContextRule],
        share: Option<&'e GlContext>,
    ) -> GenericFuture<'future, breadx::Result<ContextDispatch>>
    where
        'a: 'future,
        'b: 'future,
        'c: 'future,
        'd: 'future,
        'e: 'future,
    {
        Box::pin(async move {
            match self {
                Self::Indirect(is) => is.create_context_async(base, fbconfig, rules, share).await,
                #[cfg(feature = "dri")]
                Self::Dri2(d2) => d2.create_context_async(base, fbconfig, rules, share).await,
                #[cfg(feature = "dri3")]
                Self::Dri3(d3) => d3.create_context_async(base, fbconfig, rules, share).await,
            }
        })
    }
}
