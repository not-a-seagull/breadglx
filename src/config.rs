// MIT/Apache2 License

use std::os::raw::{c_int, c_uint};

pub const RGBA_BIT: c_int = 1;
pub const WINDOW_BIT: c_int = 1;
pub const CFG_NONE: c_int = 0x8000;
pub const TRANSPARENT_RGB: c_int = 0x8008;
pub const TRANSPARENT_INDEX: c_int = 0x8009;
pub const TRUE_COLOR: c_int = 0x8002;
pub const DIRECT_COLOR: c_int = 0x8003;
pub const PSEUDO_COLOR: c_int = 0x8004;
pub const STATIC_COLOR: c_int = 0x8005;
pub const GRAY_SCALE: c_int = 0x8006;
pub const STATIC_GRAY: c_int = 0x8007;

/// Configuration of visual or framebuffer info.
#[derive(Default, Debug, Copy, Clone)]
#[repr(C)]
pub struct GlConfig {
    pub double_buffer_mode: c_uint,
    pub stereo_mode: c_uint,
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
    pub rgb_bits: c_int,
    pub index_bits: c_int,
    pub accum_red_bits: c_int,
    pub accum_green_bits: c_int,
    pub accum_blue_bits: c_int,
    pub accum_alpha_bits: c_int,
    pub depth_bits: c_int,
    pub stencil_bits: c_int,
    pub num_aux_buffers: c_int,
    pub level: c_int,
    pub visual_id: c_int,
    pub visual_type: c_int,
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
    pub swap_method: c_int,
    pub screen: c_int,
    pub bind_to_texture_rgb: c_int,
    pub bind_to_texture_rgba: c_int,
    pub bind_to_mipmap_texture: c_int,
    pub bind_to_texture_targets: c_int,
    pub y_inverted: c_int,
    pub srgb_capable: c_int,
}

/// Fields of a GlConfig.
#[derive(Debug, Clone)]
pub enum GlConfigRule {
    DoubleBufferMode(c_uint),
    StereoMode(c_uint),
    RedBits(c_int),
    GreenBits(c_int),
    BlueBits(c_int),
    AlphaBits(c_int),
    RedMask(c_uint),
    GreenMask(c_uint),
    BlueMask(c_uint),
    AlphaMask(c_uint),
    RedShift(c_uint),
    GreenShift(c_uint),
    BlueShift(c_uint),
    AlphaShift(c_uint),
    RgbBits(c_int),
    IndexBits(c_int),
    AccumRedBits(c_int),
    AccumGreenBits(c_int),
    AccumBlueBits(c_int),
    AccumAlphaBits(c_int),
    DepthBits(c_int),
    StencilBits(c_int),
    NumAuxBuffers(c_int),
    Level(c_int),
    VisualId(c_int),
    VisualType(c_int),
    VisualRating(c_int),
    TransparentPixel(c_int),
    TransparentRed(c_int),
    TransparentGreen(c_int),
    TransparentBlue(c_int),
    TransparentAlpha(c_int),
    TransparentIndex(c_int),
    SampleBuffers(c_int),
    Samples(c_int),
    DrawableType(c_int),
    RenderType(c_int),
    XRenderable(c_int),
    FbconfigId(c_int),
    MaxPBufferWidth(c_int),
    MaxPBufferHeight(c_int),
    MaxPBufferPixels(c_int),
    OptimalPBufferWidth(c_int),
    OptimalPBufferHeight(c_int),
    VisualSelectGroup(c_int),
    SwapMethod(c_int),
    Screen(c_int),
    BindToTextureRgb(c_int),
    BindToTextureRgba(c_int),
    BindToMipmapTexture(c_int),
    BindToTextureTargets(c_int),
    YInverted(c_int),
    SrgbCapable(c_int),
}

impl GlConfig {
    #[inline]
    pub fn fulfills_rule(&self, rule: &GlConfigRule, matching_on_rgb: c_int) -> bool {
        match rule {
            GlConfigRule::DoubleBufferMode(dbm) => self.double_buffer_mode == *dbm,
            GlConfigRule::VisualType(vt) => self.visual_type == *vt,
            GlConfigRule::VisualRating(vr) => self.visual_rating == *vr,
            GlConfigRule::XRenderable(xr) => self.x_renderable == *xr,
            GlConfigRule::FbconfigId(fi) => self.fbconfig_id == *fi,
            GlConfigRule::SwapMethod(sm) => self.swap_method == *sm,
            GlConfigRule::RgbBits(b) => self.rgb_bits >= *b,
            GlConfigRule::NumAuxBuffers(nab) => self.num_aux_buffers >= *nab,
            GlConfigRule::RedBits(rb) => self.red_bits >= *rb,
            GlConfigRule::GreenBits(gb) => self.green_bits >= *gb,
            GlConfigRule::BlueBits(bb) => self.blue_bits >= *bb,
            GlConfigRule::AlphaBits(ab) => self.alpha_bits >= *ab,
            GlConfigRule::DepthBits(db) => self.depth_bits >= *db,
            GlConfigRule::StencilBits(sb) => self.stencil_bits >= *sb,
            GlConfigRule::AccumRedBits(arb) => self.accum_red_bits >= *arb,
            GlConfigRule::AccumGreenBits(agb) => self.accum_green_bits >= *agb,
            GlConfigRule::AccumBlueBits(abb) => self.accum_blue_bits >= *abb,
            GlConfigRule::AccumAlphaBits(aab) => self.accum_alpha_bits >= *aab,
            GlConfigRule::SampleBuffers(sb) => self.sample_buffers >= *sb,
            GlConfigRule::MaxPBufferWidth(mpbw) => self.max_pbuffer_width >= *mpbw,
            GlConfigRule::MaxPBufferHeight(mpbh) => self.max_pbuffer_height >= *mpbh,
            GlConfigRule::Samples(s) => self.samples >= *s,
            GlConfigRule::StereoMode(sm) => self.stereo_mode == *sm,
            GlConfigRule::Level(l) => self.level == *l,
            GlConfigRule::DrawableType(dt) => self.drawable_type & *dt != 0,
            GlConfigRule::RenderType(rt) => self.render_type & *rt != 0,
            GlConfigRule::SrgbCapable(sc) => self.srgb_capable == *sc,
            GlConfigRule::TransparentPixel(tp) => {
                if *tp != 0 {
                    if *tp == CFG_NONE
                        && self.transparent_pixel != CFG_NONE
                        && self.transparent_pixel != 0
                    {
                        false
                    } else {
                        *tp == self.transparent_pixel
                    }
                } else {
                    true
                }
            }
            GlConfigRule::TransparentRed(tr) => {
                if matching_on_rgb == TRANSPARENT_RGB {
                    *tr == self.transparent_red
                } else {
                    true
                }
            }
            GlConfigRule::TransparentGreen(tg) => {
                if matching_on_rgb == TRANSPARENT_RGB {
                    *tg == self.transparent_green
                } else {
                    true
                }
            }
            GlConfigRule::TransparentBlue(tb) => {
                if matching_on_rgb == TRANSPARENT_RGB {
                    *tb == self.transparent_blue
                } else {
                    true
                }
            }
            GlConfigRule::TransparentAlpha(ta) => {
                if matching_on_rgb == TRANSPARENT_RGB {
                    *ta == self.transparent_alpha
                } else {
                    true
                }
            }
            GlConfigRule::TransparentIndex(ti) => {
                if matching_on_rgb == TRANSPARENT_INDEX {
                    *ti == self.transparent_index
                } else {
                    true
                }
            }
            _ => true,
        }
    }

    #[inline]
    pub fn fulfills_rules(&self, rules: &[GlConfigRule]) -> bool {
        // first, figure out if we need a transparent pixel
        let matching_on_rgb: c_int = rules
            .iter()
            .find_map(|glr| match glr {
                GlConfigRule::TransparentPixel(tp) => Some(*tp),
                _ => None,
            })
            .unwrap_or(0);

        // then, iterate over the rules
        rules
            .iter()
            .all(|glr| self.fulfills_rule(glr, matching_on_rgb))
    }
}
