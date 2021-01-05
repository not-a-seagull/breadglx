// MIT/Apache2 License

#![allow(overflowing_literals)]

use std::{
    convert::TryFrom,
    mem::MaybeUninit,
    os::raw::{c_int, c_uint},
    ptr::{self, raw_mut},
};

mod compatible;
mod construct;
mod load;
mod rules;
mod values;
pub use rules::*;
pub use values::*;

/// The values associated with the configuration of a framebuffer or a visual.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlConfig {
    pub double_buffer_mode: c_uint,
    pub stereo_mode: c_uint,

    // bits per comp
    pub red_bits: c_int,
    pub green_bits: c_int,
    pub blue_bits: c_int,
    pub alpha_bits: c_int,

    pub red_mask: c_uint,
    pub green_mask: c_uint,
    pub blue_mask: c_uint,
    pub alpha_mask: c_uint,

    pub red_shift: c_uint,
    pub green_shift: c_uint,
    pub blue_shift: c_uint,
    pub alpha_shift: c_uint,

    /// Total bits for RGB
    pub rgb_bits: c_int,
    /// Total bits for colorindex
    pub color_index: c_int,
    pub num_aux_buffers: c_int,
    pub level: c_int,

    pub accum_red_bits: c_int,
    pub accum_green_bits: c_int,
    pub accum_blue_bits: c_int,
    pub accum_alpha_bits: c_int,

    pub depth_bits: c_int,
    pub stencil_bits: c_int,

    pub visual_id: c_int,
    pub visual_type: GlVisualType,
    pub visual_rating: c_int,

    pub transparent_pixel: c_int,
    pub transparent_red: c_int,
    pub transparent_green: c_int,
    pub transparent_blue: c_int,
    pub transparent_alpha: c_int,
    pub transparent_index: c_int,

    pub sample_buffers: c_int,
    pub samples: c_int,

    pub drawable_type: c_int,
    pub render_type: c_int,
    pub x_renderable: c_int,
    pub fbconfig_id: c_int,

    pub max_pbuffer_width: c_int,
    pub max_pbuffer_height: c_int,
    pub max_pbuffer_pixels: c_int,
    pub optimal_pbuffer_width: c_int,
    pub optimal_pbuffer_height: c_int,

    pub visual_select_group: c_int,
    pub swap_method: GlSwapMethod,
    pub screen: c_int,

    pub bind_to_texture_rgb: c_int,
    pub bind_to_texture_rgba: c_int,
    pub bind_to_mipmap_texture: c_int,
    pub bind_to_texture_targets: c_int,

    pub y_inverted: c_int,
    pub srgb_capable: c_int,
}

impl Default for GlConfig {
    #[inline]
    fn default() -> Self {
        Self {
            visual_id: DONT_CARE,
            visual_type: GlVisualType::DontCare,
            visual_rating: CONFIG_NONE,
            transparent_pixel: CONFIG_NONE,
            transparent_red: DONT_CARE,
            transparent_green: DONT_CARE,
            transparent_blue: DONT_CARE,
            transparent_alpha: DONT_CARE,
            transparent_index: DONT_CARE,
            x_renderable: DONT_CARE,
            fbconfig_id: DONT_CARE,
            swap_method: GlSwapMethod::Undefined,
            bind_to_texture_rgb: DONT_CARE,
            bind_to_texture_rgba: DONT_CARE,
            bind_to_mipmap_texture: DONT_CARE,
            bind_to_texture_targets: DONT_CARE,
            y_inverted: DONT_CARE,
            srgb_capable: DONT_CARE,
            // safe because everything else is a c_int that can be zeroed
            ..unsafe { MaybeUninit::zeroed().assume_init() }
        }
    }
}

/// A visual type for the GlConfig.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GlVisualType {
    TrueColor,
    DirectColor,
    PseudoColor,
    StaticColor,
    GrayScale,
    StaticGray,
    DontCare,
}

impl TryFrom<c_int> for GlVisualType {
    type Error = c_int;

    #[inline]
    fn try_from(i: c_int) -> Result<Self, c_int> {
        Ok(match i {
            // GLX_TRUE_COLOR
            0x8002 => Self::TrueColor,
            // GLX_DIRECT_COLOR
            0x8003 => Self::DirectColor,
            // GLX_PSEUDO_COLOR
            0x8004 => Self::PseudoColor,
            // GLX_STATIC_COLOR
            0x8005 => Self::StaticColor,
            // GLX_GRAY_SCALE
            0x8006 => Self::GrayScale,
            // GLX_STATIC_GRAY
            0x8007 => Self::StaticGray,
            // GLX_DONT_CARE
            0xFFFFFFFF => Self::DontCare,
            i => return Err(i),
        })
    }
}

/// Swap method.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GlSwapMethod {
    Exchange,
    Copy,
    Undefined,
    DontCare,
}

impl TryFrom<c_int> for GlSwapMethod {
    type Error = c_int;

    #[inline]
    fn try_from(i: c_int) -> Result<Self, c_int> {
        Ok(match i {
            // GLX_SWAP_EXCHANGE_OML
            0x8061 => Self::Exchange,
            // GLX_SWAP_COPY_OML
            0x8062 => Self::Copy,
            // GLX_SWAP_UNDEFINED_OML
            0x8063 => Self::Undefined,
            // GLX_DONT_CARE
            0xFFFFFFFF => Self::DontCare,
            _ => return Err(i),
        })
    }
}
