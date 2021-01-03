// MIT/Apache2 License

use crate::{
    config::GlConfig,
    context::{dispatch::ContextDispatch, GlContext, GlContextRule, InnerGlContext},
    screen::GlInternalScreen,
};
use std::sync::Arc;

#[cfg(feature = "async")]
use crate::util::GenericFuture;

#[derive(Debug)]
pub struct Dri2Screen {}

impl GlInternalScreen for Dri2Screen {
    #[inline]
    fn create_context(
        &self,
        base: &mut Arc<InnerGlContext>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext>,
    ) -> breadx::Result<ContextDispatch> {
        unimplemented!()
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
        Box::pin(async { unimplemented!() })
    }
}
