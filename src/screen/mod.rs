// MIT/Apache2 License

use crate::{
    config::GlConfig,
    dri::{dri2, dri3},
    indirect,
};

mod dispatch;

/// The screen used by the GL system.
#[derive(Debug)]
pub struct GlScreen {
    // the screen number
    screen: usize,
    // the internal dispatch mechanism
    disp: dispatch::ScreenDispatch,

    fbconfigs: Vec<GlConfig>,
    visuals: Vec<GlConfig>,
}

pub(crate) trait GlInternalScreen {}

impl GlScreen {
    #[inline]
    pub(crate) fn from_indirect(
        screen: usize,
        fbconfigs: Vec<GlConfig>,
        visuals: Vec<GlConfig>,
        i: indirect::IndirectScreen,
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
        fbconfigs: Vec<GlConfig>,
        visuals: Vec<GlConfig>,
        d2: dri2::Dri2Screen,
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
        fbconfigs: Vec<GlConfig>,
        visuals: Vec<GlConfig>,
        d3: dri3::Dri3Screen,
    ) -> Self {
        Self {
            screen,
            disp: d3.into(),
            fbconfigs,
            visuals,
        }
    }
}
