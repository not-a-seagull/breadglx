// MIT/Apache2 License

use crate::{
    dri::{dri2, dri3},
    indirect,
};

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

impl super::GlInternalScreen for ScreenDispatch {}
