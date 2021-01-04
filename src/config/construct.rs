// MIT/Apache2 License

use super::*;
use breadx::{AsByteSequence, VisualClass};
use std::{convert::TryFrom, slice};

impl GlConfig {
    #[inline]
    pub fn set_from_properties(
        props: &[u32],
        num_props: usize,
        num_configs: usize,
        tagged_only: bool,
    ) -> Vec<GlConfig> {
        props
            .chunks_exact(num_props)
            .map(|props| Self::from_properties(props, tagged_only, true))
            .take(num_configs)
            .collect()
    }

    #[inline]
    pub fn from_properties(
        props: &[u32],
        tagged_only: bool,
        fbconfig_style_tags: bool,
    ) -> GlConfig {
        let mut glconfig = GlConfig::default();
        glconfig.set_properties(props, tagged_only, fbconfig_style_tags);
        glconfig
    }

    #[inline]
    pub fn set_properties(&mut self, props: &[u32], tagged_only: bool, fbconfig_style_tags: bool) {
        const NONTAGGED_CONFIG_PROPS: usize = 18;

        let props = if tagged_only {
            props
        } else {
            let (h, props) = props.split_at(NONTAGGED_CONFIG_PROPS);

            self.visual_id = h[0] as _;
            self.visual_type = match VisualClass::from_bytes(unsafe {
                slice::from_raw_parts(&h[1] as *const u32 as *const u8, 4)
            }) {
                Some((VisualClass::TrueColor, _)) => GlVisualType::TrueColor,
                Some((VisualClass::DirectColor, _)) => GlVisualType::DirectColor,
                Some((VisualClass::PseudoColor, _)) => GlVisualType::PseudoColor,
                Some((VisualClass::StaticColor, _)) => GlVisualType::StaticColor,
                Some((VisualClass::GrayScale, _)) => GlVisualType::GrayScale,
                Some((VisualClass::StaticGray, _)) => GlVisualType::StaticGray,
                None => GlVisualType::DontCare,
            };
            self.render_type = if h[2] != 0 { RGBA_BIT } else { COLOR_INDEX_BIT };
            self.red_bits = h[3] as _;
            self.green_bits = h[4] as _;
            self.blue_bits = h[5] as _;
            self.alpha_bits = h[6] as _;
            self.accum_red_bits = h[7] as _;
            self.accum_green_bits = h[8] as _;
            self.accum_blue_bits = h[9] as _;
            self.accum_alpha_bits = h[10] as _;

            self.double_buffer_mode = h[11];
            self.stereo_mode = h[12];

            self.rgb_bits = h[13] as _;
            self.depth_bits = h[14] as _;
            self.stencil_bits = h[15] as _;
            self.num_aux_buffers = h[16] as _;
            self.level = h[17] as _;

            props
        };

        // iterate over the properties
        let mut i = 0;
        for _ in 0..props.len() / 2 {
            // match on the tag and the value
            let tag = props[i];
            let val = props.get(i + 1).copied().unwrap_or(0) as c_int;
            let used_val = match tag {
                // GLX_RGBA
                4 => {
                    if fbconfig_style_tags {
                        self.render_type = if val != 0 { RGBA_BIT } else { COLOR_INDEX_BIT };
                        true
                    } else {
                        self.render_type = RGBA_BIT;
                        false
                    }
                }
                // GLX_BUFFER_SIZE
                2 => {
                    self.rgb_bits = val;
                    true
                }
                // GLX_LEVEL
                3 => {
                    self.level = val;
                    true
                }
                // GLX_DOUBLEBUFFER
                5 => {
                    if fbconfig_style_tags {
                        self.double_buffer_mode = val as _;
                        true
                    } else {
                        self.double_buffer_mode = 1;
                        false
                    }
                }
                // GLX_STEREO
                6 => {
                    if fbconfig_style_tags {
                        self.stereo_mode = val as _;
                        true
                    } else {
                        self.stereo_mode = 1;
                        false
                    }
                }
                // GLX_AUX_BUFFERS
                7 => {
                    self.num_aux_buffers = val;
                    true
                }
                // GLX_RED_SIZE
                8 => {
                    self.red_bits = val;
                    true
                }
                // GLX_GREEN_SIZE
                9 => {
                    self.green_bits = val;
                    true
                }
                // GLX_BLUE_SIZE
                10 => {
                    self.blue_bits = val;
                    true
                }
                // GLX_ALPHA_SIZE
                11 => {
                    self.alpha_bits = val;
                    true
                }
                // GLX_DEPTH_SIZE
                12 => {
                    self.depth_bits = val;
                    true
                }
                // GLX_STENCIL_SIZE
                13 => {
                    self.stencil_bits = val;
                    true
                }
                // GLX_ACCUM_RED_SIZE
                14 => {
                    self.accum_red_bits = val;
                    true
                }
                // GLX_ACCUM_GREEN_SIZE
                15 => {
                    self.accum_green_bits = val;
                    true
                }
                // GLX_ACCUM_BLUE_SIZE
                16 => {
                    self.accum_blue_bits = val;
                    true
                }
                // GLX_ACCUM_ALPHA_SIZE
                17 => {
                    self.accum_alpha_bits = val;
                    true
                }
                // GLX_VISUAL_CAVEAT_EXT
                0x20 => {
                    self.visual_rating = val;
                    true
                }
                // GLX_X_VISUAL_TYPE
                0x22 => {
                    self.visual_type =
                        GlVisualType::try_from(val).unwrap_or(GlVisualType::DontCare);
                    true
                }
                // GLX_TRANSPARENT_TYPE
                0x23 => {
                    self.transparent_pixel = val;
                    true
                }
                // GLX_TRANSPARENT_INDEX_VALUE
                0x24 => {
                    self.transparent_index = val;
                    true
                }
                // GLX_TRANSPARENT_RED_VALUE
                0x25 => {
                    self.transparent_red = val;
                    true
                }
                // GLX_TRANSPARENT_GREEN_VALUE
                0x26 => {
                    self.transparent_green = val;
                    true
                }
                // GLX_TRANSPARENT_BLUE_VALUE
                0x27 => {
                    self.transparent_blue = val;
                    true
                }
                // GLX_TRANSPARENT_ALPHA_VALUE
                0x28 => {
                    self.transparent_alpha = val;
                    true
                }
                // GLX_VISUAL_ID
                0x800B => {
                    self.visual_id = val;
                    true
                }
                // GLX_DRAWABLE_TYPE
                0x8010 => {
                    self.drawable_type = val;
                    true
                }
                // GLX_RENDER_TYPE
                0x8011 => {
                    self.render_type = val;
                    true
                }
                // GLX_X_RENDERABLE
                0x8012 => {
                    self.x_renderable = val;
                    true
                }
                // GLX_FBCONFIG_ID
                0x8013 => {
                    self.fbconfig_id = val;
                    true
                }
                // GLX_MAX_PBUFFER_WIDTH
                0x8016 => {
                    self.max_pbuffer_width = val;
                    true
                }
                // GLX_MAX_PBUFFER_HEIGHT
                0x8017 => {
                    self.max_pbuffer_height = val;
                    true
                }
                // GLX_MAX_PBUFFER_PIXELS
                0x8018 => {
                    self.max_pbuffer_pixels = val;
                    true
                }
                // GLX_OPTIMAL_PBUFFER_WIDTH_SGIX
                0x8019 => {
                    self.optimal_pbuffer_width = val;
                    true
                }
                // GLX_OPTIMAL_PBUFFER_HEIGHT_SGIX
                0x801A => {
                    self.optimal_pbuffer_height = val;
                    true
                }
                // GLX_VISUAL_SELECT_GROUP_SGIX
                0x8028 => {
                    self.visual_select_group = val;
                    true
                }
                // GLX_SWAP_METHOD_OML
                0x8060 => {
                    self.swap_method =
                        GlSwapMethod::try_from(val).unwrap_or(GlSwapMethod::DontCare);
                    true
                }
                // GLX_SAMPLE_BUFFERS_SGIS
                100000 => {
                    self.sample_buffers = val;
                    true
                }
                // GLX_SAMPLES_SGIS
                100001 => {
                    self.samples = val;
                    true
                }
                // GLX_BIND_TO_TEXTURE_RGB_EXT
                0x20D0 => {
                    self.bind_to_texture_rgb = val;
                    true
                }
                // GLX_BIND_TO_TEXTURE_RGBA_EXT
                0x20D1 => {
                    self.bind_to_texture_rgba = val;
                    true
                }
                // GLX_BIND_TO_MIPMAP_TEXTURE_EXT
                0x20D2 => {
                    self.bind_to_mipmap_texture = val;
                    true
                }
                // GLX_BIND_TO_TEXTURE_TAGETES_EXT
                0x20D3 => {
                    self.bind_to_texture_targets = val;
                    true
                }
                // GLX_Y_INVERTED_EXT
                0x20D4 => {
                    self.y_inverted = val;
                    true
                }
                // GLX_FRAMEBUFFER_SRGB_CAPABLE_EXT
                0x20B2 => {
                    self.srgb_capable = val;
                    true
                }
                // GLX_USE_GL
                1 => fbconfig_style_tags,
                // None
                0 => break,
                // Anything else is ignored
                tag => {
                    log::debug!("Unknown config values: 0x{:X} - 0x{:X}", tag, val);
                    true
                }
            };

            i += if used_val { 2 } else { 1 };
        }
    }
}
