// MIT/Apache2 License

use crate::{
    config::{GlConfig, GlConfigRule},
    context::{
        dispatch::ContextDispatch, GlContext, GlContextRule, GlInternalContext, InnerGlContext,
    },
    display::{DisplayLike, GlDisplay},
    dri::{dri2, dri3},
    indirect,
};
use breadx::{
    auto::glx::{self, Context},
    display::{Connection, Display},
    XidType,
};
use std::{convert::TryInto, sync::Arc};

#[cfg(feature = "async")]
use crate::util::GenericFuture;

mod dispatch;

/// The screen used by the GL system.
#[derive(Debug)]
pub struct GlScreen<Dpy> {
    // the screen number
    screen: usize,
    // the internal dispatch mechanism
    disp: dispatch::ScreenDispatch<Dpy>,

    fbconfigs: Arc<[GlConfig]>,
    visuals: Arc<[GlConfig]>,
}

pub(crate) trait GlInternalScreen<Dpy> {
    /// Create a new gl context for this screen.
    fn create_context(
        &self,
        base: &mut Arc<InnerGlContext<Dpy>>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext<Dpy>>,
    ) -> breadx::Result<ContextDispatch<Dpy>>;
}

pub(crate) trait AsyncGlInternalScreen<Dpy> {
    /// Async redox
    #[cfg(feature = "async")]
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
        'e: 'future;
}

impl<Dpy> GlScreen<Dpy> {
    #[inline]
    pub fn screen_index(&self) -> usize {
        self.screen
    }

    #[inline]
    pub(crate) fn from_indirect(
        screen: usize,
        fbconfigs: Arc<[GlConfig]>,
        visuals: Arc<[GlConfig]>,
        i: indirect::IndirectScreen<Dpy>,
    ) -> Self {
        Self {
            screen,
            disp: i.into(),
            fbconfigs,
            visuals,
        }
    }

    #[cfg(feature = "dri")]
    #[inline]
    pub(crate) fn from_dri2(
        screen: usize,
        fbconfigs: Arc<[GlConfig]>,
        visuals: Arc<[GlConfig]>,
        d2: dri2::Dri2Screen<Dpy>,
    ) -> Self {
        Self {
            screen,
            disp: d2.into(),
            fbconfigs,
            visuals,
        }
    }

    #[cfg(feature = "dri3")]
    #[inline]
    pub(crate) fn from_dri3(
        screen: usize,
        fbconfigs: Arc<[GlConfig]>,
        visuals: Arc<[GlConfig]>,
        d3: dri3::Dri3Screen<Dpy>,
    ) -> Self {
        Self {
            screen,
            disp: d3.into(),
            fbconfigs,
            visuals,
        }
    }

    /// Get the framebuffer configs associated with this screen.
    #[inline]
    pub fn fbconfigs(&self) -> &[GlConfig] {
        &self.fbconfigs
    }

    /// Get the framebuffer configs matching a certain set of rules.
    #[inline]
    pub fn choose_fbconfigs(&self, rules: &[GlConfigRule]) -> Vec<GlConfig> {
        self.fbconfigs
            .iter()
            .filter(|fb| fb.fulfills_rules(rules))
            .cloned()
            .collect()
    }
}

impl<Dpy: DisplayLike> GlScreen<Dpy>
where
    Dpy::Conn: Connection,
{
    /// Create an OpenGL context.
    #[inline]
    pub fn create_context(
        &self,
        dpy: &GlDisplay<Dpy>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext<Dpy>>,
    ) -> breadx::Result<GlContext<Dpy>> {
        log::trace!("Creating context...");

        // create the base
        let mut ctx = GlContext::new(Context::from_xid(0), self.screen, fbconfig.clone());
        // create the dispatch
        let disp = self
            .disp
            .create_context(&mut ctx.inner, fbconfig, rules, share)?;
        // set the dispatch
        ctx.set_dispatch(disp);
        // create the attribs
        let attribs = GlContextRule::convert_ctx_attrib_to_classic(rules)
            .into_iter()
            .map(|c| c as u32)
            .collect();
        // get xid from server
        let xid = dpy.display().create_context_attribs_arb(
            glx::Fbconfig::const_from_xid(fbconfig.fbconfig_id as _),
            self.screen,
            match share {
                Some(share) => share.xid(),
                None => Context::default(),
            },
            ctx.dispatch().is_direct(),
            attribs,
        )?;
        Arc::get_mut(&mut ctx.inner)
            .expect("Infallible Arc::get_mut()")
            .xid = xid;

        log::trace!("Created context.");
        Ok(ctx)
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> GlScreen<Dpy>
where
    Dpy::Conn: AsyncConnection + Send,
{
    /// Create an OpenGL context, async redox.
    #[inline]
    pub async fn create_context_async(
        &self,
        dpy: &GlDisplay<Dpy>,
        fbconfig: &GlConfig,
        rules: &[GlContextRule],
        share: Option<&GlContext<Dpy>>,
    ) -> breadx::Result<GlContext<Dpy>> {
        // as above, so below
        let mut ctx = GlContext::new(Context::from_xid(0), self.screen, fbconfig.clone());
        let disp = self
            .disp
            .create_context_async(&mut ctx.inner, fbconfig, rules, share)
            .await?;
        ctx.set_dispatch(disp);
        let attribs = GlContextRule::convert_ctx_attrib_to_classic(rules)
            .into_iter()
            .map(|c| c as u32)
            .collect();
        let xid = dpy
            .display_async()
            .await
            .create_context_attribs_arb_async(
                glx::Fbconfig::const_from_xid(fbconfig.fbconfig_id as _),
                self.screen,
                match share {
                    Some(share) => share.xid(),
                    None => Context::default(),
                },
                ctx.dispatch().is_direct(),
                attribs,
            )
            .await?;
        Arc::get_mut(&mut ctx.inner)
            .expect("Infallible Arc::get_mut()")
            .xid = xid;
        Ok(ctx)
    }
}
