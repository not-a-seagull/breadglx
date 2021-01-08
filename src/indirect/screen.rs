// MIT/Apache2 License

use crate::{
    config::GlConfig,
    context::{dispatch::ContextDispatch, GlContext, GlContextRule, InnerGlContext},
    display::DisplayLike,
    screen::GlInternalScreen,
};
use std::{marker::PhantomData, sync::Arc};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

#[derive(Debug)]
pub struct IndirectScreen<Dpy> {
    p: PhantomData<Dpy>,
}

impl<Dpy: DisplayLike> GlInternalScreen<Dpy> for IndirectScreen<Dpy> {
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

    #[cfg(feature = "async")]
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
