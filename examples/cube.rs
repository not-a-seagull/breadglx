// MIT/Apache2 License

use breadglx::{GlConfigRule, GlContext, GlContextRule, GlDisplay, GlScreen, GlVisualType};
use breadx::{
    ColormapAlloc, DisplayConnection, Event, EventMask, Pixmap, Result, Window, WindowClass,
    WindowParameters,
};
use gl::types::*;
use log::LevelFilter;
use nalgebra::{Matrix4, Point3, Vector3};
use std::{
    convert::TryInto,
    ffi::CString,
    mem::{self, MaybeUninit},
    os::raw::c_int,
    ptr,
};

fn main() -> Result {
    env_logger::Builder::new()
        .filter(Some("breadx"), LevelFilter::Warn)
        .filter(Some("breadglx"), LevelFilter::Info)
        .init();

    // set up our GLX setup. This is mostly identical to examples/basic.rs, except we don't
    // use 1.0 on failure
    let conn = DisplayConnection::create(None, None)?;
    let root = conn.default_screen().root;
    let root_index = conn.default_screen_index();
    let mut conn = GlDisplay::new(conn)?;
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
        .max_by_key(|fbconfig| fbconfig.samples)
        .expect("Could not find valid framebuffer config");
    let mut dpy = conn.display();
    let vis = dpy
        .visual_id_to_visual(fbconfig.visual_id as _)
        .expect("Could not match visual to fbconfig");
    let depth = dpy.depth_of_visual(vis.visual_id).unwrap();
    let vis = vis.visual_id;

    let cmap = dpy.create_colormap(root, vis, ColormapAlloc::None)?;
    let wp = WindowParameters {
        colormap: Some(cmap),
        background_pixmap: Some(Pixmap::const_from_xid(0)),
        border_pixel: Some(0),
        event_mask: Some(
            EventMask::STRUCTURE_NOTIFY | EventMask::EXPOSURE | EventMask::BUTTON_PRESS,
        ),
        ..Default::default()
    };
    let mut width = 640;
    let mut height = 400;
    let win = dpy.create_window(
        root,
        WindowClass::InputOutput,
        Some(depth),
        Some(vis),
        0,
        0,
        width,
        height,
        0,
        wp,
    )?;
    win.set_title(&mut dpy, "Cube Demonstration")?;
    win.map(&mut dpy)?;

    let wdw = dpy.intern_atom_immediate("WM_DELETE_WINDOW".to_string(), false)?;
    win.set_wm_protocols(&mut dpy, &[wdw])?;
    mem::drop(dpy);

    const CONTEXT_RULES: &[GlContextRule] = &[
        GlContextRule::MajorVersion(3),
        GlContextRule::MinorVersion(3),
    ];

    let context = screen.create_context(&conn, &fbconfig, CONTEXT_RULES, None)?;
    context.bind(&conn, win)?;

    let dpy_clone = conn.clone();
    gl::load_with(move |s| match dpy_clone.get_proc_address(s) {
        Ok(pa) => pa,
        Err(e) => {
            eprintln!("Unable to get proc address for {}: {:?}", s, e);
            std::ptr::null()
        }
    });

    unsafe {
        gl::Enable(gl::DEPTH_TEST);
        gl::DepthFunc(gl::LESS);
    }

    // now that we have GL, set up our vertex arrays
    let mut vertex_array_id = MaybeUninit::<GLuint>::uninit();
    let vertex_array_id = unsafe {
        gl::GenVertexArrays(1, vertex_array_id.as_mut_ptr());
        let vertex_array_id = vertex_array_id.assume_init();
        gl::BindVertexArray(vertex_array_id);
        vertex_array_id
    };

    let mut vertex_buffer = MaybeUninit::<GLuint>::uninit();
    let vertex_buffer = unsafe {
        gl::GenBuffers(1, vertex_buffer.as_mut_ptr());
        let vertex_buffer = vertex_buffer.assume_init();
        gl::BindBuffer(gl::ARRAY_BUFFER, vertex_buffer);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (CUBE_VERTEX_DATA.len() * mem::size_of::<f32>()) as _,
            CUBE_VERTEX_DATA.as_ptr() as *const _,
            gl::STATIC_DRAW,
        );
        vertex_buffer
    };

    // compile the shaders
    let program = load_shaders()?;

    // set up our MVP (Model, View, Projection) matrix.
    let projection =
        Matrix4::<GLfloat>::new_perspective(width as f32 / height as f32, radians(45.0), 0.1, 100.0);
    let mut camera_posn = Point3::new(10.0, 6.0, 6.0);
    let mut view = Matrix4::<GLfloat>::face_towards(
        &Point3::new(0.0, 0.0, 0.0),
        &camera_posn,
        &Vector3::new(0.0, 1.0, 0.0),
    );
    let rotation = Matrix4::<GLfloat>::new_rotation(Vector3::new(0.0, 0.0, radians(5.0)));
    let model = Matrix4::<GLfloat>::identity();

    // make sure OpenGL knows about this matrix
    let matrix_id = unsafe { gl::GetUniformLocation(program, b"MVP\0".as_ptr() as *const _) };

    let mut back_color = [0.0f32; 3];
    let mut cube_color = [1.0f32; 3];

    loop {
        let event = conn.display().wait_for_event()?;
        let mut do_render = false;

        match event {
            Event::ConfigureNotify(cne) => {
                width = cne.width;
                height = cne.height;
                // TODO: recalculate MVP
                unsafe { gl::Viewport(0, 0, width as _, height as _) };
                do_render = true;
            }
            Event::Expose(e) => {
                do_render = true;
            }
            Event::ButtonPress(_) => {
                do_render = true;
//                back_color = [fastrand::f32(), fastrand::f32(), fastrand::f32()];
//                cube_color = [fastrand::f32(), fastrand::f32(), fastrand::f32()];
                view *= rotation.clone();
            }
            Event::ClientMessage(cme) => {
                if cme.data.longs()[0] == wdw.xid {
                    break;
                }
            }
            _ => (),
        }

        if do_render {
            let mvp = projection * view * model;

            render(
                back_color,
                cube_color,
                &mvp,
                matrix_id,
                vertex_buffer,
                program,
            );
            screen.swap_buffers(&conn, win)?;
        }
    }

    GlContext::<DisplayConnection>::unbind()?;

    Ok(())
}

fn render(
    back_color: [f32; 3],
    cube_color: [f32; 3],
    matrix: &Matrix4<f32>,
    matrix_id: GLint,
    buffer: GLuint,
    program: GLuint,
) {
    // Use the program.
    unsafe { gl::UseProgram(program) };

    // Clear the screen.
    unsafe {
        gl::ClearColor(back_color[0], back_color[1], back_color[2], 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
    }

    // Set our program's cube color.
    unsafe {
        let color_id = gl::GetUniformLocation(program, b"in_color\0".as_ptr() as *const _);
        gl::Uniform3fv(color_id, 1, cube_color.as_ptr());
        gl::UniformMatrix4fv(matrix_id, 1, gl::TRUE, matrix.as_ptr());
    }

    // Draw our triangles
    unsafe {
        gl::EnableVertexAttribArray(0);
        gl::BindBuffer(gl::ARRAY_BUFFER, buffer);
        gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, 0, ptr::null_mut());
        let cvdlen: GLsizei = CUBE_VERTEX_DATA.len().try_into().unwrap();
        gl::DrawArrays(gl::TRIANGLES, 0, cvdlen / 3);
        gl::DisableVertexAttribArray(0);
    }
}

// loader shaders and return the program
fn load_shaders() -> Result<GLuint> {
    unsafe fn compile_shader(id: GLuint, source: &[u8]) -> Result {
        let mut res = MaybeUninit::<GLint>::uninit();
        let mut info_log_length = MaybeUninit::<c_int>::uninit();

        gl::ShaderSource(id, 1, &source.as_ptr() as *const _ as *const _, ptr::null());
        gl::CompileShader(id);

        // check to see if compilation succeeded
        gl::GetShaderiv(id, gl::COMPILE_STATUS, res.as_mut_ptr());
        gl::GetShaderiv(id, gl::INFO_LOG_LENGTH, info_log_length.as_mut_ptr());

        let info_log_length = info_log_length.assume_init();
        if info_log_length > 0 {
            let mut info_log: Vec<u8> = Vec::with_capacity(info_log_length as usize + 1);
            let mut true_length = 0;
            gl::GetShaderInfoLog(
                id,
                info_log_length,
                &mut true_length,
                info_log.as_mut_ptr() as *mut _,
            );
            info_log.set_len(true_length as usize + 1);
            info_log.pop();
            Err(breadx::BreadError::Msg(format!(
                "Shader Error: {}",
                CString::new(info_log).unwrap().into_string().unwrap()
            )))
        } else {
            Ok(())
        }
    }

    let mut res = MaybeUninit::<GLint>::uninit();
    let mut info_log_length = MaybeUninit::<c_int>::uninit();

    unsafe {
        let vertex_id = gl::CreateShader(gl::VERTEX_SHADER);
        let fragment_id = gl::CreateShader(gl::FRAGMENT_SHADER);

        compile_shader(vertex_id, VERTEX_SHADER)?;
        compile_shader(fragment_id, FRAGMENT_SHADER)?;

        // link the program
        let program = gl::CreateProgram();
        gl::AttachShader(program, vertex_id);
        gl::AttachShader(program, fragment_id);
        gl::LinkProgram(program);

        // make sure linking went well
        gl::GetProgramiv(program, gl::LINK_STATUS, res.as_mut_ptr());
        gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, info_log_length.as_mut_ptr());
        let info_log_length = info_log_length.assume_init();
        if info_log_length > 0 {
            let mut info_log = Vec::<u8>::with_capacity(info_log_length as usize + 1);
            let mut true_length = 0;
            gl::GetProgramInfoLog(
                program,
                info_log_length,
                &mut true_length,
                info_log.as_mut_ptr() as *mut _,
            );
            info_log.set_len(true_length as usize + 1);
            info_log.pop();
            return Err(breadx::BreadError::Msg(format!(
                "Program error: {}",
                CString::new(info_log).unwrap().into_string().unwrap()
            )));
        }

        gl::DetachShader(program, vertex_id);
        gl::DetachShader(program, fragment_id);

        gl::DeleteShader(vertex_id);
        gl::DeleteShader(fragment_id);

        Ok(program)
    }
}

#[inline]
fn radians(deg: f32) -> f32 {
    deg * (std::f32::consts::PI / 180.0)
}

const CUBE_VERTEX_DATA: &[GLfloat] = &[
    -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0,
    -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, -1.0,
    -1.0, -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0,
    -1.0, -1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0,
    -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
    -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0,
    1.0, 1.0, 1.0, -1.0, 1.0,
];

const CUBE_COLOR_DATA: &[GLfloat] = &[
    0.583,  0.771,  0.014,
    0.609,  0.115,  0.436,
    0.327,  0.483,  0.844,
    0.822,  0.569,  0.201,
    0.435,  0.602,  0.223,
    0.310,  0.747,  0.185,
    0.597,  0.770,  0.761,
    0.559,  0.436,  0.730,
    0.359,  0.583,  0.152,
    0.483,  0.596,  0.789,
    0.559,  0.861,  0.639,
    0.195,  0.548,  0.859,
    0.014,  0.184,  0.576,
    0.771,  0.328,  0.970,
    0.406,  0.615,  0.116,
    0.676,  0.977,  0.133,
    0.971,  0.572,  0.833,
    0.140,  0.616,  0.489,
    0.997,  0.513,  0.064,
    0.945,  0.719,  0.592,
    0.543,  0.021,  0.978,
    0.279,  0.317,  0.505,
    0.167,  0.620,  0.077,
    0.347,  0.857,  0.137,
    0.055,  0.953,  0.042,
    0.714,  0.505,  0.345,
    0.783,  0.290,  0.734,
    0.722,  0.645,  0.174,
    0.302,  0.455,  0.848,
    0.225,  0.587,  0.040,
    0.517,  0.713,  0.338,
    0.053,  0.959,  0.120,
    0.393,  0.621,  0.362,
    0.673,  0.211,  0.457,
    0.820,  0.883,  0.371,
    0.982,  0.099,  0.879
];

//const CUBE_VERTEX_DATA: &[f32] = &[-1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0];

const VERTEX_SHADER: &[u8] = b"
#version 330 core

layout(location = 0) in vec3 vertexPosition;
uniform mat4 MVP;

void main() { 
  gl_Position = MVP * vec4(vertexPosition, 1);
}
\0";

const FRAGMENT_SHADER: &[u8] = b"
#version 330 core

out vec3 color;
uniform vec3 in_color;

void main() {
    color = in_color;
}
\0";
