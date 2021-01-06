// MIT/Apache2 License

use breadglx::{GlConfigRule, GlContextRule, GlDisplay, GlVisualType};
use breadx::{
    ColormapAlloc, DisplayConnection, EventMask, Pixmap, Result, VisualClass, WindowClass,
    WindowParameters,
};
use log::LevelFilter;
use std::{io::Write, env, mem};

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter(Some("breadx"), LevelFilter::Warn)
        .filter(Some("breadglx"), LevelFilter::Info)
        .init();

    // establish a connection, wrap it in a GlDisplay, and use that to produce a GlScreen
    let conn = DisplayConnection::create(None, None)?;
    let mut conn = GlDisplay::new(conn)?;
    let root = conn.display().default_screen().root;
    let root_index = conn.display().default_screen_index();
    let mut screen = conn.create_screen(root_index)?;

    let extinfo = conn
        .display()
        .query_extension_immediate("GLX".to_string())?;
    println!("GLX: {:?}", &extinfo);

    // find the ideal framebuffer config for our use
    const FBCONFIG_RULES: &[GlConfigRule] = &[
        GlConfigRule::DrawableType(breadglx::WINDOW_BIT),
        GlConfigRule::RenderType(breadglx::RGBA_BIT),
        GlConfigRule::VisualType(GlVisualType::TrueColor),
        GlConfigRule::RedBits(8),
        GlConfigRule::GreenBits(8),
        GlConfigRule::BlueBits(8),
        GlConfigRule::AlphaBits(8),
        GlConfigRule::DepthBits(24),
        GlConfigRule::StencilBits(8),
        GlConfigRule::DoubleBufferMode(1),
        //        GlConfigRule::XRenderable(1),
    ];

    let fbconfig = screen
        .choose_fbconfigs(FBCONFIG_RULES)
        .into_iter()
        .max_by_key(|fbconfig| fbconfig.samples)
        .expect("Could not find valid framebuffer config");

    // get the visual information associated with that fbconfig
    let mut dpy = conn.display();
    let vis = dpy
        .visual_id_to_visual(fbconfig.visual_id as _)
        .expect("Could not match visual to fbconfig");
    let depth = dpy.depth_of_visual(vis.visual_id).unwrap();
    let vis = vis.visual_id;

    // create a colormap identified with the chosen visual style
    let cmap = dpy.create_colormap(root, vis, ColormapAlloc::None)?;

    // create a window
    let wp = WindowParameters {
        colormap: Some(cmap),
        background_pixmap: Some(Pixmap::const_from_xid(0)),
        border_pixel: Some(0),
        event_mask: Some(EventMask::STRUCTURE_NOTIFY),
        ..Default::default()
    };
    let win = dpy.create_window(
        root,
        WindowClass::InputOutput,
        Some(depth),
        Some(vis),
        0,
        0,
        640,
        400,
        0,
        wp,
    )?;

    // set up the window's properties
    win.set_title(&mut *dpy, "BreadGLX Demonstration")?;
    win.map(&mut *dpy)?;

    // drop the mutex lock
    mem::drop(dpy);

    // now that we have a window, establish a GlContext
    const CONTEXT_RULES: &[GlContextRule] = &[
        GlContextRule::MajorVersion(3),
        GlContextRule::MinorVersion(0),
    ];

    let context = match screen.create_context(&mut conn, &fbconfig, CONTEXT_RULES, None) {
        Ok(context) => context,
        Err(e) => {
            // if we failed to initialize the GlContext, fall back to an older version of OpenGL
            const FALLBACK_CONTEXT_RULES: &[GlContextRule] = &[
                GlContextRule::MajorVersion(1),
                GlContextRule::MinorVersion(0),
            ];

            screen.create_context(&mut conn, &fbconfig, FALLBACK_CONTEXT_RULES, None)?
        }
    };

    //context.bind(&mut conn, win)?;

    Ok(())
}
