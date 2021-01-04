// MIT/Apache2 License

use super::ffi;
use crate::{
    config::{
        GlConfig, GlSwapMethod, COLOR_INDEX_BIT, CONFIG_NONE, DONT_CARE, NON_CONFORMANT_CONFIG,
        RGBA_BIT, RGBA_FLOAT_BIT_ARB, RGBA_UNSIGNED_FLOAT_BIT_EXT, SLOW_CONFIG, TEXTURE_1D_BIT_EXT,
        TEXTURE_2D_BIT_EXT, TEXTURE_RECTANGLE_BIT_EXT,
    },
    dri::ExtensionContainer,
};
use ahash::AHasher;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    os::raw::c_uint,
    ptr::{self, NonNull},
};

pub(crate) unsafe fn convert_configs(
    core_extension: ExtensionContainer,
    configs: &[GlConfig],
    driver_configs: *mut *const ffi::__DRIconfig,
) -> impl Iterator<Item = (u64, NonNull<ffi::__DRIconfig>)> + '_ {
    #[repr(transparent)]
    struct DriverConfigsIterator(*mut *const ffi::__DRIconfig);

    impl Iterator for DriverConfigsIterator {
        type Item = NonNull<ffi::__DRIconfig>;

        #[inline]
        fn next(&mut self) -> Option<NonNull<ffi::__DRIconfig>> {
            let current: *const ffi::__DRIconfig = unsafe { ptr::read(self.0) };
            if current.is_null() {
                None
            } else {
                self.0 = unsafe { self.0.offset(1) };
                // safe because we just null-checked
                Some(unsafe { NonNull::new_unchecked(current as *mut _) })
            }
        }
    }

    configs.iter().filter_map(move |c| {
        DriverConfigsIterator(driver_configs).find_map(|dc| {
            if configs_equal(core_extension, c, dc.as_ptr() as *const _) {
                Some((
                    {
                        let mut hasher = AHasher::default();
                        c.hash(&mut hasher);
                        hasher.finish()
                    },
                    dc,
                ))
            } else {
                None
            }
        })
    })
}

#[inline]
fn configs_equal(
    core_extension: ExtensionContainer,
    config: &GlConfig,
    driver_config: *const ffi::__DRIconfig,
) -> bool {
    let mut i = 0;

    // we can iterate over the driver config's values pretty easily
    let (mut attrib, mut value): (c_uint, c_uint) = (0, 0);

    let core_ext = core_extension.0 as *const ffi::__DRIcoreExtension;
    while unsafe {
        ((*core_ext)
            .indexConfigAttrib
            .expect("indexConfigAttrib not present"))(
            driver_config,
            {
                let old = i;
                i += 1;
                old
            },
            &mut attrib,
            &mut value,
        )
    } != 0
    {
        if !config_seg_equal(config, attrib, value) {
            return false;
        }
    }

    true
}

#[inline]
fn config_seg_equal(config: &GlConfig, attrib: c_uint, value: c_uint) -> bool {
    if attrib == ffi::__DRI_ATTRIB_RENDER_TYPE {
        let equivalent = if value & ffi::__DRI_ATTRIB_RENDER_TYPE != 0 {
            RGBA_BIT
        } else {
            0
        } | if value & ffi::__DRI_ATTRIB_COLOR_INDEX_BIT != 0 {
            COLOR_INDEX_BIT
        } else {
            0
        } | if value & ffi::__DRI_ATTRIB_FLOAT_BIT != 0 {
            RGBA_FLOAT_BIT_ARB
        } else {
            0
        } | if value & ffi::__DRI_ATTRIB_UNSIGNED_FLOAT_BIT != 0 {
            RGBA_UNSIGNED_FLOAT_BIT_EXT
        } else {
            0
        };

        let res = config.render_type == equivalent;
        if !res {
            log::debug!(
                "Config of ID {} failed on __DRI_ATTRIB_RENDER_TYPE",
                config.fbconfig_id
            );
        }
        res
    } else if attrib == ffi::__DRI_ATTRIB_CONFIG_CAVEAT {
        let equivalent = if value & ffi::__DRI_ATTRIB_NON_CONFORMANT_CONFIG != 0 {
            NON_CONFORMANT_CONFIG
        } else if value & ffi::__DRI_ATTRIB_SLOW_BIT != 0 {
            SLOW_CONFIG
        } else {
            CONFIG_NONE
        };

        let res = config.visual_rating == equivalent;
        if !res {
            log::debug!(
                "Config of ID {:X} failed on __DRI_ATTRIB_CONFIG_CAVEAT",
                config.fbconfig_id
            );
        }
        res
    } else if attrib == ffi::__DRI_ATTRIB_BIND_TO_TEXTURE_TARGETS {
        let equivalent = if value & ffi::__DRI_ATTRIB_TEXTURE_1D_BIT != 0 {
            TEXTURE_1D_BIT_EXT
        } else {
            0
        } | if value & ffi::__DRI_ATTRIB_TEXTURE_2D_BIT != 0 {
            TEXTURE_2D_BIT_EXT
        } else {
            0
        } | if value & ffi::__DRI_ATTRIB_TEXTURE_RECTANGLE_BIT != 0 {
            TEXTURE_RECTANGLE_BIT_EXT
        } else {
            0
        };

        let res = config.bind_to_texture_targets == DONT_CARE
            || config.bind_to_texture_targets == equivalent;
        if !res {
            log::debug!(
                "Config of ID {:X} failed on __DRI_ATTRIB_BIND_TO_TEXTURE_TARGETS",
                config.fbconfig_id
            );
        }
        res
    } else if attrib == ffi::__DRI_ATTRIB_SWAP_METHOD {
        let equivalent = if value == ffi::__DRI_ATTRIB_SWAP_EXCHANGE {
            GlSwapMethod::Exchange
        } else if value == ffi::__DRI_ATTRIB_SWAP_COPY {
            GlSwapMethod::Copy
        } else {
            GlSwapMethod::Undefined
        };

        config.swap_method == GlSwapMethod::DontCare || config.swap_method == equivalent
    } else {
        raw_compare(config, attrib, value)
    }
}

#[inline]
fn raw_compare(config: &GlConfig, attrib: c_uint, value: c_uint) -> bool {
    let res = ATTRIB_CONVERTERS
        .iter()
        .find_map(|a| {
            if a.attrib == attrib {
                let cfg_val = *(a.indexer)(config);
                Some(cfg_val == DONT_CARE as c_uint || cfg_val == value)
            } else {
                None
            }
        })
        .unwrap_or(true);
    if !res {
        log::debug!(
            "Config of ID 0x{:X} failed on 0x{:X}",
            config.fbconfig_id,
            attrib
        );
    } else if res && attrib == 1 {
        println!("Config succeeded on 1");
    }
    res
}

type ConfigIndexer = for<'a> fn(&'a GlConfig) -> &'a c_uint;

struct AttribConverter {
    attrib: c_uint,
    indexer: ConfigIndexer,
}

const ATTRIB_CONVERTERS: [AttribConverter; 28] = {
    macro_rules! attrib_converter {
        ($attrib: expr, $field: ident) => {{
            AttribConverter {
                attrib: ($attrib) as c_uint,
                indexer: |c| unsafe { &*(&c.$field as *const _ as *const c_uint) },
            }
        }};
    }

    [
        attrib_converter!(ffi::__DRI_ATTRIB_BUFFER_SIZE, rgb_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_LEVEL, level),
        attrib_converter!(ffi::__DRI_ATTRIB_RED_SIZE, red_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_GREEN_SIZE, green_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_BLUE_SIZE, blue_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_ALPHA_SIZE, alpha_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_DEPTH_SIZE, depth_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_STENCIL_SIZE, stencil_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_ACCUM_RED_SIZE, accum_red_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_ACCUM_GREEN_SIZE, accum_green_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_ACCUM_BLUE_SIZE, accum_blue_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_ACCUM_ALPHA_SIZE, accum_alpha_bits),
        attrib_converter!(ffi::__DRI_ATTRIB_SAMPLE_BUFFERS, sample_buffers),
        attrib_converter!(ffi::__DRI_ATTRIB_SAMPLES, samples),
        attrib_converter!(ffi::__DRI_ATTRIB_DOUBLE_BUFFER, double_buffer_mode),
        attrib_converter!(ffi::__DRI_ATTRIB_STEREO, stereo_mode),
        attrib_converter!(ffi::__DRI_ATTRIB_AUX_BUFFERS, num_aux_buffers),
        attrib_converter!(ffi::__DRI_ATTRIB_MAX_PBUFFER_WIDTH, max_pbuffer_width),
        attrib_converter!(ffi::__DRI_ATTRIB_MAX_PBUFFER_HEIGHT, max_pbuffer_height),
        attrib_converter!(ffi::__DRI_ATTRIB_MAX_PBUFFER_PIXELS, max_pbuffer_pixels),
        attrib_converter!(
            ffi::__DRI_ATTRIB_OPTIMAL_PBUFFER_WIDTH,
            optimal_pbuffer_width
        ),
        attrib_converter!(
            ffi::__DRI_ATTRIB_OPTIMAL_PBUFFER_HEIGHT,
            optimal_pbuffer_height
        ),
        attrib_converter!(ffi::__DRI_ATTRIB_SWAP_METHOD, swap_method),
        attrib_converter!(ffi::__DRI_ATTRIB_BIND_TO_TEXTURE_RGB, bind_to_texture_rgb),
        attrib_converter!(ffi::__DRI_ATTRIB_BIND_TO_TEXTURE_RGBA, bind_to_texture_rgba),
        attrib_converter!(
            ffi::__DRI_ATTRIB_BIND_TO_MIPMAP_TEXTURE,
            bind_to_mipmap_texture
        ),
        attrib_converter!(ffi::__DRI_ATTRIB_YINVERTED, y_inverted),
        attrib_converter!(ffi::__DRI_ATTRIB_FRAMEBUFFER_SRGB_CAPABLE, srgb_capable),
    ]
};
