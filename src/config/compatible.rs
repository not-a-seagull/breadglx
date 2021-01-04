// MIT/Apache2 License

use super::{
    GlConfig, GlConfigRule, GlSwapMethod, GlVisualType, CONFIG_NONE, DONT_CARE, WINDOW_BIT,
};

impl GlConfig {
    #[inline]
    fn compatible_with(&self, other: &GlConfig) -> bool {
        macro_rules! rmatch {
            ($a: expr, $b: expr, $fname: ident) => {{
                if ($a).$fname != DONT_CARE as _ && ($a).$fname != ($b).$fname {
                    return false;
                }
            }};

            ($a: expr, $b: expr, $fname: ident, $($other: ident),*) => {{
                rmatch!($a, $b, $fname);

                rmatch!($a, $b, $($other),*);
            }};
        }

        macro_rules! rmatch_lt {
            ($a: expr, $b: expr, $fname: ident) => {{
                if ($a).$fname != DONT_CARE as _ && ($a).$fname > ($b).$fname {
                    return false;
                }
            }};

            ($a: expr, $b: expr, $fname: ident, $($other: ident),*) => {{
                rmatch_lt!($a, $b, $fname);

                rmatch_lt!($a, $b, $($other),*);
            }};
        }

        macro_rules! rmatch_mask {
            ($a: expr, $b: expr, $fname: ident) => {{
                if ($a).$fname != DONT_CARE as _ && (($a).$fname & !($b).$fname) != 0 {
                    return false;
                }
            }};

            ($a: expr, $b: expr, $fname: ident, $($other: ident),*) => {{
                rmatch_mask!($a, $b, $fname);

                rmatch_mask!($a, $b, $($other),*);
            }};
        }

        macro_rules! rmatch_exact {
            ($a: expr, $b: expr, $fname: ident) => {{
                if ($a).$fname != ($b).$fname {
                    return false;
                }
            }};

            ($a: expr, $b: expr, $fname: ident, $($other: ident),*) => {{
                rmatch_exact!($a, $b, $fname);

                rmatch_exact!($a, $b, $($other),*);
            }};
        }

        rmatch!(
            self,
            other,
            double_buffer_mode,
            visual_rating,
            x_renderable,
            fbconfig_id
        );

        if self.visual_type != GlVisualType::DontCare && self.visual_type != other.visual_type {
            return false;
        }

        if self.swap_method != GlSwapMethod::DontCare && self.swap_method != other.swap_method {
            return false;
        }

        rmatch_lt!(
            self,
            other,
            rgb_bits,
            num_aux_buffers,
            red_bits,
            green_bits,
            blue_bits,
            alpha_bits,
            depth_bits,
            stencil_bits,
            accum_red_bits,
            accum_green_bits,
            accum_blue_bits,
            accum_alpha_bits,
            sample_buffers,
            max_pbuffer_width,
            max_pbuffer_height,
            max_pbuffer_pixels,
            samples
        );

        rmatch!(self, other, stereo_mode);
        rmatch_exact!(self, other, level);

        rmatch_mask!(self, other, drawable_type, render_type);
        rmatch!(self, other, srgb_capable);

        if self.transparent_pixel != DONT_CARE as _ && self.transparent_pixel != 0 {
            if self.transparent_pixel == CONFIG_NONE as _ {
                if other.transparent_pixel != CONFIG_NONE && other.transparent_pixel != 0 {
                    return false;
                }
            } else {
                rmatch!(self, other, transparent_pixel);
            }

            match self.transparent_pixel {
                // GLX_TRANSPARENT_RGB
                0x8008 => rmatch!(
                    self,
                    other,
                    transparent_red,
                    transparent_green,
                    transparent_blue,
                    transparent_alpha
                ),
                // GLX_TRANSPARENT_INDEX
                0x8009 => rmatch!(self, other, transparent_index),
                _ => (),
            }
        }

        true
    }

    #[inline]
    pub fn fulfills_rules(&self, rules: &[GlConfigRule]) -> bool {
        // apply rules to matcher
        let matcher = rules
            .iter()
            .fold(construct_matching_config(), |matcher, rule| match rule {
                GlConfigRule::DoubleBufferMode(dbm) => GlConfig {
                    double_buffer_mode: *dbm,
                    ..matcher
                },
                GlConfigRule::StereoMode(sm) => GlConfig {
                    stereo_mode: *sm,
                    ..matcher
                },
                GlConfigRule::RedBits(r) => GlConfig {
                    red_bits: *r,
                    ..matcher
                },
                GlConfigRule::GreenBits(g) => GlConfig {
                    green_bits: *g,
                    ..matcher
                },
                GlConfigRule::BlueBits(b) => GlConfig {
                    blue_bits: *b,
                    ..matcher
                },
                GlConfigRule::AlphaBits(a) => GlConfig {
                    alpha_bits: *a,
                    ..matcher
                },
                GlConfigRule::RedMask(r) => GlConfig {
                    red_mask: *r,
                    ..matcher
                },
                GlConfigRule::GreenMask(g) => GlConfig {
                    green_mask: *g,
                    ..matcher
                },
                GlConfigRule::BlueMask(b) => GlConfig {
                    blue_mask: *b,
                    ..matcher
                },
                GlConfigRule::AlphaMask(a) => GlConfig {
                    alpha_mask: *a,
                    ..matcher
                },
                GlConfigRule::RedShift(r) => GlConfig {
                    red_shift: *r,
                    ..matcher
                },
                GlConfigRule::GreenShift(g) => GlConfig {
                    green_shift: *g,
                    ..matcher
                },
                GlConfigRule::BlueShift(b) => GlConfig {
                    blue_shift: *b,
                    ..matcher
                },
                GlConfigRule::AlphaShift(a) => GlConfig {
                    alpha_shift: *a,
                    ..matcher
                },
                GlConfigRule::RgbBits(rgb) => GlConfig {
                    rgb_bits: *rgb,
                    ..matcher
                },
                GlConfigRule::ColorIndex(ci) => GlConfig {
                    color_index: *ci,
                    ..matcher
                },
                GlConfigRule::AccumRedBits(r) => GlConfig {
                    accum_red_bits: *r,
                    ..matcher
                },
                GlConfigRule::AccumGreenBits(g) => GlConfig {
                    accum_green_bits: *g,
                    ..matcher
                },
                GlConfigRule::AccumBlueBits(b) => GlConfig {
                    accum_blue_bits: *b,
                    ..matcher
                },
                GlConfigRule::AccumAlphaBits(a) => GlConfig {
                    accum_alpha_bits: *a,
                    ..matcher
                },
                GlConfigRule::DepthBits(d) => GlConfig {
                    depth_bits: *d,
                    ..matcher
                },
                GlConfigRule::StencilBits(s) => GlConfig {
                    stencil_bits: *s,
                    ..matcher
                },
                GlConfigRule::VisualId(v) => GlConfig {
                    visual_id: *v,
                    ..matcher
                },
                GlConfigRule::VisualType(v) => GlConfig {
                    visual_type: *v,
                    ..matcher
                },
                GlConfigRule::VisualRating(v) => GlConfig {
                    visual_rating: *v,
                    ..matcher
                },
                GlConfigRule::TransparentPixel(t) => GlConfig {
                    transparent_pixel: *t,
                    ..matcher
                },
                GlConfigRule::TransparentRed(r) => GlConfig {
                    transparent_red: *r,
                    ..matcher
                },
                GlConfigRule::TransparentGreen(g) => GlConfig {
                    transparent_green: *g,
                    ..matcher
                },
                GlConfigRule::TransparentBlue(b) => GlConfig {
                    transparent_blue: *b,
                    ..matcher
                },
                GlConfigRule::TransparentAlpha(a) => GlConfig {
                    transparent_alpha: *a,
                    ..matcher
                },
                GlConfigRule::TransparentIndex(i) => GlConfig {
                    transparent_index: *i,
                    ..matcher
                },
                GlConfigRule::SampleBuffers(sb) => GlConfig {
                    sample_buffers: *sb,
                    ..matcher
                },
                GlConfigRule::Samples(s) => GlConfig {
                    samples: *s,
                    ..matcher
                },
                GlConfigRule::DrawableType(d) => GlConfig {
                    drawable_type: *d,
                    ..matcher
                },
                GlConfigRule::RenderType(r) => GlConfig {
                    render_type: *r,
                    ..matcher
                },
                GlConfigRule::XRenderable(xr) => GlConfig {
                    x_renderable: *xr,
                    ..matcher
                },
                GlConfigRule::FbconfigId(f) => GlConfig {
                    fbconfig_id: *f,
                    ..matcher
                },
                GlConfigRule::MaxPbufferWidth(w) => GlConfig {
                    max_pbuffer_width: *w,
                    ..matcher
                },
                GlConfigRule::MaxPbufferHeight(h) => GlConfig {
                    max_pbuffer_height: *h,
                    ..matcher
                },
                GlConfigRule::MaxPbufferPixels(p) => GlConfig {
                    max_pbuffer_pixels: *p,
                    ..matcher
                },
                GlConfigRule::OptimalPbufferWidth(w) => GlConfig {
                    optimal_pbuffer_width: *w,
                    ..matcher
                },
                GlConfigRule::OptimalPbufferHeight(h) => GlConfig {
                    optimal_pbuffer_height: *h,
                    ..matcher
                },
                GlConfigRule::VisualSelectGroup(vsg) => GlConfig {
                    visual_select_group: *vsg,
                    ..matcher
                },
                GlConfigRule::SwapMethod(sm) => GlConfig {
                    swap_method: *sm,
                    ..matcher
                },
                GlConfigRule::Screen(s) => GlConfig {
                    screen: *s,
                    ..matcher
                },
                GlConfigRule::BindToTextureRgb(r) => GlConfig {
                    bind_to_texture_rgb: *r,
                    ..matcher
                },
                GlConfigRule::BindToTextureRgba(r) => GlConfig {
                    bind_to_texture_rgba: *r,
                    ..matcher
                },
                GlConfigRule::BindToMipmapTexture(m) => GlConfig {
                    bind_to_mipmap_texture: *m,
                    ..matcher
                },
                GlConfigRule::BindToTextureTargets(tt) => GlConfig {
                    bind_to_texture_targets: *tt,
                    ..matcher
                },
                GlConfigRule::YInverted(y) => GlConfig {
                    y_inverted: *y,
                    ..matcher
                },
                GlConfigRule::SrgbCapable(s) => GlConfig {
                    srgb_capable: *s,
                    ..matcher
                },
                GlConfigRule::Level(l) => GlConfig {
                    level: *l,
                    ..matcher
                },
                GlConfigRule::NumAuxBuffers(nab) => GlConfig {
                    num_aux_buffers: *nab,
                    ..matcher
                },
            });

        matcher.compatible_with(self)
    }
}

/// Construct a GlConfig to be used to match to other configs.
#[inline]
fn construct_matching_config() -> GlConfig {
    GlConfig {
        double_buffer_mode: DONT_CARE as _,
        render_type: WINDOW_BIT,
        drawable_type: DONT_CARE,
        visual_rating: DONT_CARE,
        swap_method: GlSwapMethod::DontCare,
        ..GlConfig::default()
    }
}
