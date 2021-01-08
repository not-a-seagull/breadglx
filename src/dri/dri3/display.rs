// MIT/Apache2 License

use crate::{
    config::GlConfig,
    display::{DisplayLike, DisplayLock, GlInternalDisplay},
    dll::Dll,
    dri::dri3::Dri3Screen,
    screen::{self, GlScreen},
};
use breadx::{
    display::{Connection, Display},
    error::BreadError,
};
use std::{boxed::Box, fmt, marker::PhantomData, os::raw::c_int, sync::Arc};

#[cfg(feature = "async")]
use crate::util::GenericFuture;
#[cfg(feature = "async")]
use futures_lite::future;

const PRESENT_EXT_NAME: &str = "Present";
const DRI3_EXT_NAME: &str = "DRI3";

const DRI3_MAJOR: u32 = 1;
const DRI3_MINOR: u32 = 0;
const PRESENT_MAJOR: u32 = 1;
const PRESENT_MINOR: u32 = 0;

pub(crate) struct Dri3Display<Dpy> {
    dri3_version_major: u32,
    dri3_version_minor: u32,
    present_version_major: u32,
    present_version_minor: u32,
    _phantom: PhantomData<Dpy>,
}

impl<Dpy> fmt::Debug for Dri3Display<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Dri3Display")
    }
}

impl<Dpy: DisplayLike> Dri3Display<Dpy> {
    #[inline]
    pub(crate) fn new(dpy: &mut Display<Dpy::Conn>) -> breadx::Result<Self> {
        // query whether or not the extension's versions are present
        // note: this automatically triggers ExtensionNotPresent errors
        let dri3iv_tok = dpy.query_dri3_version(DRI3_MAJOR, DRI3_MINOR)?;
        let presentiv_tok = dpy.query_present_version(PRESENT_MAJOR, PRESENT_MINOR)?;
        let dri3iv = dpy.resolve_request(dri3iv_tok)?;
        let presentiv = dpy.resolve_request(presentiv_tok)?;

        Ok(Self {
            dri3_version_major: dri3iv.major_version,
            dri3_version_minor: dri3iv.minor_version,
            present_version_major: presentiv.major_version,
            present_version_minor: presentiv.minor_version,
            _phantom: PhantomData,
        })
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) async fn new_async(dpy: &mut Display<Dpy::Conn>) -> breadx::Result<Self> {
        // query whether or not the extension's versions are present
        // note: this automatically triggers ExtensionNotPresent errors
        let dri3iv_tok = dpy.query_dri3_version_async(DRI3_MAJOR, DRI3_MINOR).await?;
        let presentiv_tok = dpy
            .query_present_version_async(PRESENT_MAJOR, PRESENT_MINOR)
            .await?;
        let dri3iv = dpy.resolve_request_async(dri3iv_tok).await?;
        let presentiv = dpy.resolve_request_async(presentiv_tok).await?;

        Ok(Self {
            dri3_version_major: dri3iv.major_version,
            dri3_version_minor: dri3iv.minor_version,
            present_version_major: presentiv.major_version,
            present_version_minor: presentiv.minor_version,
            _phantom: PhantomData,
        })
    }
}

impl<Dpy: DisplayLike> GlInternalDisplay<Dpy> for Dri3Display<Dpy> {
    #[inline]
    fn create_screen(
        &self,
        dpy: &mut Display<Dpy::Conn>,
        index: usize,
    ) -> breadx::Result<GlScreen<Dpy>> {
        let (visuals, fbconfigs) = GlConfig::get_visuals_and_fbconfigs(dpy, index)?;
        let visuals: Arc<[GlConfig]> = visuals.into_boxed_slice().into();
        let fbconfigs: Arc<[GlConfig]> = fbconfigs.into_boxed_slice().into();
        let screen = Dri3Screen::new(dpy, index, visuals.clone(), fbconfigs.clone())?;

        Ok(GlScreen::from_dri3(index, fbconfigs, visuals, screen))
    }

    #[cfg(feature = "async")]
    #[inline]
    fn create_screen_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b mut Display<Dpy::Conn>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen<Dpy>>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async move {
            // TODO: find a way to zip these futures together
            let (visuals, fbconfigs) =
                GlConfig::get_visuals_and_fbconfigs_async(dpy, index).await?;
            let visuals: Arc<[GlConfig]> = visuals.into_boxed_slice().into();
            let fbconfigs: Arc<[GlConfig]> = fbconfigs.into_boxed_slice().into();
            let dri3_screen =
                Dri3Screen::new_async(dpy, index, visuals.clone(), fbconfigs.clone()).await?;

            Ok(GlScreen::from_dri3(index, fbconfigs, visuals, dri3_screen))
        })
    }
}
