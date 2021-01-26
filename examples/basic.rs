// MIT/Apache2 License

use breadglx::{GlConfigRule, GlContextRule, GlDisplay, GlVisualType, GlContext};
use breadx::{
    auto::xproto::ExposeEvent, ColormapAlloc, DisplayConnection, Event, EventMask, Pixmap, Result,
    VisualClass, WindowClass, WindowParameters,
};
use log::LevelFilter;
use std::{env, io::Write, mem, thread, time::Duration};

// This code is a rough translate of the following:
// https://www.khronos.org/opengl/wiki/Tutorial:_OpenGL_3.0_Context_Creation_(GLX)

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter(Some("breadx"), LevelFilter::Warn)
        .filter(Some("breadglx"), LevelFilter::Trace)
        .init();

    // establish a connection, wrap it in a GlDisplay, and use that to produce a GlScreen
    let conn = DisplayConnection::create(None, None)?;
    let mut conn = GlDisplay::new(conn)?;
    let root = conn.display().default_screen().root;
    let root_index = conn.display().default_screen_index();
    let mut screen = conn.create_screen(root_index)?;

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
        GlConfigRule::XRenderable(1),
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
        event_mask: Some(
            EventMask::STRUCTURE_NOTIFY | EventMask::EXPOSURE | EventMask::BUTTON_PRESS,
        ),
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
    let wdw = dpy.intern_atom_immediate("WM_DELETE_WINDOW".to_string(), false)?;
    win.set_wm_protocols(&mut *dpy, &[wdw])?;

    // drop the mutex lock
    mem::drop(dpy);

    // now that we have a window, establish a GlContext
    const CONTEXT_RULES: &[GlContextRule] = &[
        GlContextRule::MajorVersion(3),
        GlContextRule::MinorVersion(0),
    ];

    let context = match screen.create_context(&conn, &fbconfig, CONTEXT_RULES, None) {
        Ok(context) => context,
        Err(e) => {
            // if we failed to initialize the GlContext, fall back to an older version of OpenGL
            const FALLBACK_CONTEXT_RULES: &[GlContextRule] = &[
                GlContextRule::MajorVersion(1),
                GlContextRule::MinorVersion(0),
            ];

            screen.create_context(&conn, &fbconfig, FALLBACK_CONTEXT_RULES, None)?
        }
    };

    // bind the current context
    context.bind(&conn, win)?;

    // use the get proc address function as a way of getting GL functions for whatever interface
    // we end up using.
    // here we use the `gl` crate, but if you use something like glow it shouldn't be dissimilar
    let dpy_clone = conn.clone();
    gl::load_with(move |s| match dpy_clone.get_proc_address(s) {
        Ok(pa) => pa,
        Err(e) => {
            eprintln!("Unable to get proc address for {}: {:?}", s, e);
            std::ptr::null()
        }
    });

    let mut color = [1.0f32; 3];
    loop {
        // Note: make sure not to hold onto the mutex for too long, since Gl functions
        //       need it
        let event = conn.display().wait_for_event()?;
        let mut render = false;

        match event {
            Event::Expose(e) => {
                render = true;
            }
            Event::ButtonPress(_) => {
                render = true;
                color = [fastrand::f32(), fastrand::f32(), fastrand::f32()];
            }
            Event::ClientMessage(cme) => {
                if cme.data.longs()[0] == wdw.xid {
                    break;
                }
            }
            _ => (),
        }

        if render {
            // Render the OpenGL to the screen
            // SAFETY: We are most likely calling GL primitives here
            unsafe {
                gl::ClearColor(color[0], color[1], color[2], 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }

            screen.swap_buffers(&conn, win)?;
        }
    }

    // As we exit, unbind the context to let everything drop.
    GlContext::<DisplayConnection>::unbind()?;

    Ok(())
}
