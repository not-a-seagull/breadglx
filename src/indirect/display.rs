// MIT/Apache2 License

use super::IndirectScreen;
use crate::{
    config::GlConfig,
    display::{DisplayLike, DisplayLock, GlInternalDisplay},
    screen::GlScreen,
};
use breadx::display::{Connection, Display};
use std::{fmt, marker::PhantomData, sync::Arc};

#[cfg(feature = "async")]
use crate::{display::AsyncGlInternalDisplay, util::GenericFuture};
#[cfg(feature = "async")]
use breadx::display::AsyncConnection;
#[cfg(feature = "async")]
use futures_lite::future;

pub struct IndirectDisplay<Dpy> {
    // The indirect display data isn't actually important.
    _private: PhantomData<Dpy>,
}

impl<Dpy> fmt::Debug for IndirectDisplay<Dpy> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IndirectDisplay")
    }
}

impl<Dpy: DisplayLike> IndirectDisplay<Dpy> {
    #[inline]
    pub fn new(_dpy: &mut Display<Dpy::Connection>) -> breadx::Result<Self> {
        /*Ok(Self {
            _private: PhantomData,
        })*/
        Err(breadx::BreadError::StaticMsg(
            "Indirect displays are currently unavailable",
        ))
    }

    #[cfg(feature = "async")]
    #[inline]
    pub async fn new_async(_dpy: &mut Display<Dpy::Connection>) -> breadx::Result<Self> {
        /*Ok(Self {
            _private: PhantomData,
        })*/
        Err(breadx::BreadError::StaticMsg(
            "Indirect displays are currently unavailable",
        ))
    }
}

impl<Dpy: DisplayLike> GlInternalDisplay<Dpy> for IndirectDisplay<Dpy>
where
    Dpy::Connection: Connection,
{
    #[inline]
    fn create_screen(
        &self,
        dpy: &mut Display<Dpy::Connection>,
        index: usize,
    ) -> breadx::Result<GlScreen<Dpy>> {
        let (visuals, fbconfigs) = GlConfig::get_visuals_and_fbconfigs(dpy, index)?;
        let visuals: Arc<[GlConfig]> = visuals.into_boxed_slice().into();
        let fbconfigs: Arc<[GlConfig]> = fbconfigs.into_boxed_slice().into();

        Ok(GlScreen::from_indirect(
            index,
            fbconfigs,
            visuals,
            IndirectScreen::new(),
        ))
    }
}

#[cfg(feature = "async")]
impl<Dpy: DisplayLike> AsyncGlInternalDisplay<Dpy> for IndirectDisplay<Dpy>
where
    Dpy::Connection: AsyncConnection,
{
    #[inline]
    fn create_screen_async<'future, 'a, 'b>(
        &'a self,
        dpy: &'b mut Display<Dpy::Connection>,
        index: usize,
    ) -> GenericFuture<'future, breadx::Result<GlScreen<Dpy>>>
    where
        'a: 'future,
        'b: 'future,
    {
        Box::pin(async move {
            let (visuals, fbconfigs) =
                GlConfig::get_visuals_and_fbconfigs_async(dpy, index).await?;
            let visuals: Arc<[GlConfig]> = visuals.into_boxed_slice().into();
            let fbconfigs: Arc<[GlConfig]> = fbconfigs.into_boxed_slice().into();

            Ok(GlScreen::from_indirect(
                index,
                fbconfigs,
                visuals,
                IndirectScreen::new(),
            ))
        })
    }
}
