// MIT/Apache2 License

use super::GlConfig;
use breadx::display::{Connection, Display};
use std::convert::TryInto;

const U32_NO_FIT: &str = "u32 doesn't fit in usize";

#[cfg(feature = "async")]
use breadx::display::AsyncConnection;

impl GlConfig {
    #[inline]
    pub(crate) fn get_visuals_and_fbconfigs<Conn: Connection>(
        dpy: &mut Display<Conn>,
        screen: usize,
    ) -> breadx::Result<(Vec<GlConfig>, Vec<GlConfig>)> {
        // run the two computations async
        let vis_tok = dpy.get_visual_configs(screen)?;
        let fbs_tok = dpy.get_fb_configs(screen)?;
        let vis = dpy.resolve_request(vis_tok)?;
        let fbs = dpy.resolve_request(fbs_tok)?;

        let (vis, fbs) = (
            GlConfig::set_from_properties(
                &vis.property_list,
                vis.num_properties.try_into().expect(U32_NO_FIT),
                vis.num_visuals.try_into().expect(U32_NO_FIT),
                false,
            ),
            GlConfig::set_from_properties(
                &fbs.property_list,
                fbs.num_properties.try_into().expect(U32_NO_FIT),
                fbs.num_fb_configs.try_into().expect(U32_NO_FIT),
                true,
            ),
        );

        Ok((vis, fbs))
    }

    #[cfg(feature = "async")]
    #[inline]
    pub(crate) async fn get_visuals_and_fbconfigs_async<Conn: AsyncConnection + Send>(
        dpy: &mut Display<Conn>,
        screen: usize,
    ) -> breadx::Result<(Vec<GlConfig>, Vec<GlConfig>)> {
        // run the two computations async
        let vis_tok = dpy.get_visual_configs_async(screen).await?;
        let fbs_tok = dpy.get_fb_configs_async(screen).await?;
        let vis = dpy.resolve_request_async(vis_tok).await?;
        let fbs = dpy.resolve_request_async(fbs_tok).await?;

        Ok((
            GlConfig::set_from_properties(
                &vis.property_list,
                vis.num_properties.try_into().expect(U32_NO_FIT),
                vis.num_visuals.try_into().expect(U32_NO_FIT),
                false,
            ),
            GlConfig::set_from_properties(
                &fbs.property_list,
                fbs.num_properties.try_into().expect(U32_NO_FIT),
                fbs.num_fb_configs.try_into().expect(U32_NO_FIT),
                true,
            ),
        ))
    }
}
