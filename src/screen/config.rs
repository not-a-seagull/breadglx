// MIT/Apache2 License

use crate::config::{GlConfig, RGBA_BIT, WINDOW_BIT};
use breadx::display::{Connection, Display};
use std::{convert::TryInto, os::raw::c_int};

#[inline]
pub(crate) fn get_visuals_and_fbconfigs<Conn: Connection>(
    dpy: &mut Display<Conn>,
    screen: usize,
) -> breadx::Result<(Vec<GlConfig>, Vec<GlConfig>)> {
    // send requests
    let vis_tok = dpy.get_visual_configs(screen)?;
    let fbs_tok = dpy.get_fb_configs(screen)?;

    // resolve requests
    let vis = dpy.resolve_request(vis_tok)?;
    let fbs = dpy.resolve_request(fbs_tok)?;

    // create the configs
    Ok((
        create_configs(
            &vis.property_list,
            vis.num_properties as _,
            vis.num_visuals as _,
            screen,
            false,
        ),
        create_configs(
            &fbs.property_list,
            fbs.num_properties as _,
            fbs.num_fb_configs as _,
            screen,
            true,
        ),
    ))
}

#[cfg(feature = "async")]
#[inline]
pub(crate) async fn get_visuals_and_fbconfigs_async<Conn: Connection>(
    dpy: &mut Display<Conn>,
    screen: usize,
) -> breadx::Result<(Vec<GlConfig>, Vec<GlConfig>)> {
    // send requests
    let vis_tok = dpy.get_visual_configs_async(screen).await?;
    let fbs_tok = dpy.get_fb_configs_async(screen).await?;

    // resolve requests
    // async note: not sure which one should resolve first but they should resolve at around the
    //             same time, so we shouldn't have to worry about extensive pending
    let vis = dpy.resolve_request_async(vis_tok).await?;
    let fbs = dpy.resolve_request_async(fbs_tok).await?;

    // create the configs
    Ok((
        create_configs(
            &vis.property_list,
            vis.num_properties as _,
            vis.num_visuals as _,
            screen,
            false,
        ),
        create_configs(
            &fbs.property_list,
            fbs.num_properties as _,
            fbs.num_fb_configs as _,
            screen,
            true,
        ),
    ))
}

#[inline]
fn create_configs(
    props: &[u32],
    propsize: usize,
    num_configs: usize,
    screen: usize,
    tagged_only: bool,
) -> Vec<GlConfig> {
    props
        .chunks(propsize)
        .map(|props| {
            let mut config = create_config(props, tagged_only, true);
            config.screen = screen
                .try_into()
                .expect("Screen index doesn't fit in c_int");
            config
        })
        .take(num_configs)
        .collect()
}

/// Initialize a configuration from a set of properties.
#[inline]
fn create_config(mut props: &[u32], tagged_only: bool, fbconfig_style_tags: bool) -> GlConfig {
    let mut config: GlConfig = Default::default();
    config.drawable_type |= WINDOW_BIT;

    if !tagged_only {
        config.visual_id = props[0] as _;
        config.visual_type = props[1] as _;
        config.render_type = props[2] as _;
        config.red_bits = props[3] as _;
        config.green_bits = props[4] as _;
        config.blue_bits = props[5] as _;
        config.alpha_bits = props[6] as _;
        config.accum_red_bits = props[7] as _;
        config.accum_green_bits = props[8] as _;
        config.accum_blue_bits = props[9] as _;
        config.accum_alpha_bits = props[10] as _;
        config.double_buffer_mode = props[11];
        config.stereo_mode = props[12];
        config.rgb_bits = props[13] as _;
        config.depth_bits = props[14] as _;
        config.stencil_bits = props[15] as _;
        config.num_aux_buffers = props[16] as _;
        config.level = props[17] as _;

        props = &props[18..];
    }

    let mut i = 0;
    while i < props.len() {
        let tag = props[i];
        i += 1;
        let val = props.get(i + 1).copied().unwrap_or(0) as c_int;

        let val_used = match tag {
            // GLX_RGBA
            4 => {
                config.render_type = RGBA_BIT;
                false
            }
            // GLX_BUFFER_SIZE
            2 => {
                config.rgb_bits = val;
                true
            }
            // GLX_LEVEL
            3 => {
                config.level = val;
                true
            }
            // GLX_DOUBLEBUFFER
            5 => {
                if fbconfig_style_tags {
                    config.double_buffer_mode = val as _;
                    true
                } else {
                    config.double_buffer_mode = 1;
                    false
                }
            }
            // GLX_STEREO
            6 => {
                if fbconfig_style_tags {
                    config.stereo_mode = val as _;
                    true
                } else {
                    config.stereo_mode = 1;
                    false
                }
            }
            // GLX_AUX_BUFFERS
            7 => {
                config.num_aux_buffers = val as _;
                true
            }
            // GLX_RED_SIZE
            8 => {
                config.red_bits = val as _;
                true
            }
            // GLX_GREEN_SIZE
            9 => {
                config.green_bits = val as _;
                true
            }
            // GLX_BLUE_SIZE
            10 => {
                config.blue_bits = val as _;
                true
            }
            // GLX_ALPHA_SIZE
            11 => {
                config.alpha_bits = val as _;
                true
            }
            // GLX_DEPTH_SIZE
            12 => {
                config.depth_bits = val as _;
                true
            }
            // GLX_STENCIL_SIZE
            13 => {
                config.stencil_bits = val as _;
                true
            }
            // GLX_ACCUM_RED_SIZE
            14 => {
                config.accum_red_bits = val as _;
                true
            }
            // GLX_ACCUM_GREEN_SIZE
            15 => {
                config.accum_green_bits = val as _;
                true
            }
            // GLX_ACCUM_BLUE_SIZE
            16 => {
                config.accum_blue_bits = val as _;
                true
            }
            // GLX_ACCUM_ALPHA_SIZE
            17 => {
                config.accum_alpha_bits = val as _;
                true
            }
            // GLX_VISUAL_CAVEAT_EXT
            0x20 => {
                config.visual_rating = val as _;
                true
            }
            // GLX_X_VISUAL_TYPE
            0x22 => {
                config.visual_type = val as _;
                true
            }
            // GLX_TRANSPARENT_TYPE
            0x23 => {
                config.transparent_pixel = val as _;
                true
            }
            // GLX_TRANSPARENT_INDEX_VALUE
            0x24 => {
                config.transparent_index = val as _;
                true
            }
            // GLX_TRANSPARENT_RED_VALUE
            0x25 => {
                config.transparent_red = val as _;
                true
            }
            // GLX_TRANSPARENT_GREEN_VALUE
            0x26 => {
                config.transparent_green = val as _;
                true
            }
            // GLX_TRANSPARENT_BLUE_VALUE
            0x27 => {
                config.transparent_blue = val as _;
                true
            }
            // GLX_TRANSPARENT_ALPHA_VALUE
            0x28 => {
                config.transparent_alpha = val as _;
                true
            }
            // GLX_VISUAL_ID
            0x800B => {
                config.visual_id = val as _;
                true
            }
            // GLX_DRAWABLE_TYPE
            0x8010 => {
                config.drawable_type = val as _;
                true
            }
            // GLX_RENDER_TYPE
            0x8011 => {
                config.render_type = val as _;
                true
            }
            // GLX_X_RENDERABLE
            0x8012 => {
                config.x_renderable = val as _;
                true
            }
            // GLX_FBCONFIG_ID
            0x8013 => {
                config.fbconfig_id = val as _;
                true
            }
            // GLX_MAX_PBUFFER_WIDTH
            0x8016 => {
                config.max_pbuffer_width = val as _;
                true
            }
            // GLX_MAX_PBUFFER_HEIGHT
            0x8017 => {
                config.max_pbuffer_height = val as _;
                true
            }
            // GLX_MAX_PBUFFER_PIXELS
            0x8018 => {
                config.max_pbuffer_pixels = val as _;
                true
            }
            // GLX_OPTIMAL_PBUFFER_WIDTH_SGIX
            0x8019 => {
                config.optimal_pbuffer_width = val as _;
                true
            }
            // GLX_OPTIMAL_PBUFFER_HEIGHT_SGIX
            0x801A => {
                config.optimal_pbuffer_height = val as _;
                true
            }
            // GLX_VISUAL_SELECT_GROUP_SGIX
            0x8028 => {
                config.visual_select_group = val as _;
                true
            }
            // GLX_SWAP_METHOD_OML
            0x8060 => {
                config.swap_method = match val {
                    // GLX_SWAP_COPY_OML or GLX_SWAP_EXCHANGE_OML
                    0x8061 | 0x8062 => val as _,
                    val => {
                        const GLX_SWAP_UNDEFINED_OML: u32 = 0x8063;
                        GLX_SWAP_UNDEFINED_OML as _
                    }
                };

                true
            }
            // GLX_SAMPLE_BUFFERS_SGIS
            100000 => {
                config.sample_buffers = val as _;
                true
            }
            // GLX_SAMPLES_SGIS
            100001 => {
                config.samples = val as _;
                true
            }
            // GLX_BIND_TO_TEXTURE_RGB_EXT
            0x20D0 => {
                config.bind_to_texture_rgb = val as _;
                true
            }
            // GLX_BIND_TO_TEXTURE_RGBA_EXT
            0x20D1 => {
                config.bind_to_texture_rgba = val as _;
                true
            }
            // GLX_BIND_TO_MIPMAP_TEXTURE_EXT
            0x20D2 => {
                config.bind_to_mipmap_texture = val as _;
                true
            }
            // GLX_BIND_TO_TEXTURE_TARGETS_EXT
            0x20D3 => {
                config.bind_to_texture_targets = val as _;
                true
            }
            // GLX_Y_INVERTED_EXT
            0x20D4 => {
                config.y_inverted = val as _;
                true
            }
            // GLX_FRAMEBUFFER_SRGB_CAPABLE_EXT
            0x20B2 => {
                config.srgb_capable = val as _;
                true
            }
            // GLX_USE_GL
            1 => fbconfig_style_tags,
            0 => true,
            _ => {
                log::debug!("Unrecognized GLX tag: {} - {}", tag, val);

                // ignore tag value
                true
            }
        };

        if val_used {
            i += 1;
        }
    }

    config
}
