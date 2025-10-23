use crate::core::gfx::{BlendMode, ObjectType, RenderList, Texture as RendererTexture};
use crate::core::space::ortho_for_window;
use cgmath::Matrix4;
use glow::{HasContext, PixelUnpackData, UniformLocation};
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextAttributesBuilder, PossiblyCurrentContext},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use image::RgbaImage;
use log::{info, warn};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{collections::HashMap, error::Error, ffi::CStr, mem, num::NonZeroU32, sync::Arc};
use winit::window::Window;

// A handle to an OpenGL texture on the GPU.
#[derive(Debug, Clone, Copy)]
pub struct Texture(pub glow::Texture);

pub struct State {
    pub gl: glow::Context,
    gl_surface: Surface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    program: glow::Program,
    mvp_location: UniformLocation,
    color_location: UniformLocation,
    texture_location: UniformLocation,
    projection: Matrix4<f32>,
    window_size: (u32, u32),
    // A single, shared set of buffers for a unit quad.
    shared_vao: glow::VertexArray,
    _shared_vbo: glow::Buffer,
    _shared_ibo: glow::Buffer,
    index_count: i32,
    uv_scale_location: UniformLocation,
    uv_offset_location: UniformLocation,
    edge_fade_location: UniformLocation,
    instanced_location: UniformLocation,
}

pub fn init(window: Arc<Window>, vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing OpenGL backend...");

    let (gl_surface, gl_context, gl) = create_opengl_context(&window, vsync_enabled)?;
    let (
        program,
        mvp_location,
        color_location,
        texture_location,
        uv_scale_location,
        uv_offset_location,
        edge_fade_location,
        instanced_location,
    ) = create_graphics_program(&gl)?;

    // Create shared static unit quad + index buffer.
    let (shared_vao, _shared_vbo, _shared_ibo, index_count) = unsafe {
        const UNIT_QUAD_VERTICES: [[f32; 4]; 4] = [
            [-0.5, -0.5, 0.0, 1.0],
            [ 0.5, -0.5, 1.0, 1.0],
            [ 0.5,  0.5, 1.0, 0.0],
            [-0.5,  0.5, 0.0, 0.0],
        ];
        const QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let ibo = gl.create_buffer()?;

        gl.bind_vertex_array(Some(vao));

        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck::cast_slice(&UNIT_QUAD_VERTICES),
            glow::STATIC_DRAW,
        );

        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
        gl.buffer_data_u8_slice(
            glow::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(&QUAD_INDICES),
            glow::STATIC_DRAW,
        );

        // Per-vertex attributes: a_pos (location 0), a_tex_coord (location 1)
        let stride = (4 * mem::size_of::<f32>()) as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, (2 * mem::size_of::<f32>()) as i32);
        
        gl.bind_vertex_array(None);

        (vao, vbo, ibo, QUAD_INDICES.len() as i32)
    };

    let initial_size = window.inner_size();
    let projection = ortho_for_window(initial_size.width, initial_size.height);

    unsafe {
        gl.viewport(0, 0, initial_size.width as i32, initial_size.height as i32);
        gl.use_program(Some(program));
        gl.active_texture(glow::TEXTURE0);
        gl.uniform_1_i32(Some(&texture_location), 0);
        gl.uniform_1_i32(Some(&instanced_location), 0);

        // Set default values for uniforms
        gl.uniform_2_f32(Some(&uv_scale_location), 1.0, 1.0);
        gl.uniform_2_f32(Some(&uv_offset_location), 0.0, 0.0);
        gl.uniform_4_f32(Some(&edge_fade_location), 0.0, 0.0, 0.0, 0.0);
        gl.use_program(None);
    }

    let state = State {
        gl,
        gl_surface,
        gl_context,
        program,
        mvp_location,
        color_location,
        texture_location,
        projection,
        window_size: (initial_size.width, initial_size.height),
        shared_vao,
        _shared_vbo,
        _shared_ibo,
        index_count,
        uv_scale_location,
        uv_offset_location,
        edge_fade_location,
        instanced_location,
    };

    info!("OpenGL backend initialized successfully.");
    Ok(state)
}

pub fn create_texture(gl: &glow::Context, image: &RgbaImage) -> Result<Texture, String> {
    unsafe {
        let t = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(t));

        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);

        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_BASE_LEVEL, 0);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAX_LEVEL, 0);

        let internal = glow::RGBA8;
        let w = image.width() as i32;
        let h = image.height() as i32;
        let raw = image.as_raw();

        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            PixelUnpackData::Slice(Some(raw)),
        );

        gl.bind_texture(glow::TEXTURE_2D, None);
        Ok(Texture(t))
    }
}

pub fn draw(
    state: &mut State,
    render_list: &RenderList,
    textures: &HashMap<String, RendererTexture>,
) -> Result<u32, Box<dyn Error>> {
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(0);
    }

    #[inline(always)]
    fn apply_blend(gl: &glow::Context, want: BlendMode, last: &mut Option<BlendMode>) {
        if *last == Some(want) { return; }
        unsafe {
            gl.enable(glow::BLEND);
            match want {
                BlendMode::Alpha => {
                    gl.blend_equation(glow::FUNC_ADD);
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                }
                BlendMode::Add => {
                    gl.blend_equation(glow::FUNC_ADD);
                    gl.blend_func(glow::SRC_ALPHA, glow::ONE);
                }
                BlendMode::Multiply => {
                    gl.blend_equation(glow::FUNC_ADD);
                    gl.blend_func(glow::DST_COLOR, glow::ZERO);
                }
                BlendMode::Subtract => {
                    gl.blend_equation(glow::FUNC_REVERSE_SUBTRACT);
                    gl.blend_func(glow::ONE, glow::ONE);
                }
            }
        }
        *last = Some(want);
    }

    let mut vertices: u32 = 0;

    unsafe {
        let gl = &state.gl;

        let c = render_list.clear_color;
        gl.clear_color(c[0], c[1], c[2], c[3]);
        gl.clear(glow::COLOR_BUFFER_BIT);

        gl.use_program(Some(state.program));
        gl.bind_vertex_array(Some(state.shared_vao));

        gl.uniform_1_i32(Some(&state.instanced_location), 0);

        gl.enable(glow::BLEND);
        gl.blend_equation(glow::FUNC_ADD);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

        gl.active_texture(glow::TEXTURE0);
        gl.uniform_1_i32(Some(&state.texture_location), 0);

        let mut last_bound_tex: Option<glow::Texture> = None;
        let mut last_blend = Some(BlendMode::Alpha);
        let mut last_uv_scale: Option<[f32; 2]> = None;
        let mut last_uv_offset: Option<[f32; 2]> = None;
        let mut last_color: Option<[f32; 4]> = None;
        let mut last_edge_fade: Option<[f32; 4]> = None;

        for obj in &render_list.objects {
            apply_blend(gl, obj.blend, &mut last_blend);

            let mvp_array: [[f32; 4]; 4] = (state.projection * obj.transform).into();
            gl.uniform_matrix_4_f32_slice(Some(&state.mvp_location), false, bytemuck::cast_slice(&mvp_array));

            match &obj.object_type {
                ObjectType::Sprite { texture_id, tint, uv_scale, uv_offset, edge_fade } => {
                    if let Some(RendererTexture::OpenGL(gl_tex)) = textures.get(texture_id) {
                        if last_bound_tex != Some(gl_tex.0) {
                            gl.bind_texture(glow::TEXTURE_2D, Some(gl_tex.0));
                            last_bound_tex = Some(gl_tex.0);
                        }
                        if last_uv_scale != Some(*uv_scale) {
                            gl.uniform_2_f32(Some(&state.uv_scale_location), uv_scale[0], uv_scale[1]);
                            last_uv_scale = Some(*uv_scale);
                        }
                        if last_uv_offset != Some(*uv_offset) {
                            gl.uniform_2_f32(Some(&state.uv_offset_location), uv_offset[0], uv_offset[1]);
                            last_uv_offset = Some(*uv_offset);
                        }
                        if last_color != Some(*tint) {
                            gl.uniform_4_f32_slice(Some(&state.color_location), tint);
                            last_color = Some(*tint);
                        }
                        if last_edge_fade != Some(*edge_fade) {
                            gl.uniform_4_f32_slice(Some(&state.edge_fade_location), edge_fade);
                            last_edge_fade = Some(*edge_fade);
                        }
                        gl.draw_elements(glow::TRIANGLES, state.index_count, glow::UNSIGNED_SHORT, 0);
                        vertices += 4;
                    }
                }
            }
        }
        gl.bind_vertex_array(None);
    }

    state.gl_surface.swap_buffers(&state.gl_context)?;
    Ok(vertices)
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    if width == 0 || height == 0 {
        warn!("Ignoring resize to zero dimensions.");
        return;
    }
    let w = NonZeroU32::new(width).unwrap();
    let h = NonZeroU32::new(height).unwrap();

    state.gl_surface.resize(&state.gl_context, w, h);
    unsafe {
        state.gl.viewport(0, 0, width as i32, height as i32);
    }
    state.projection = ortho_for_window(width, height);
    state.window_size = (width, height);
}

pub fn cleanup(state: &mut State) {
    info!("Cleaning up OpenGL resources...");
    unsafe {
        state.gl.delete_program(state.program);
        state.gl.delete_vertex_array(state.shared_vao);
        state.gl.delete_buffer(state._shared_vbo);
        state.gl.delete_buffer(state._shared_ibo);
    }
    info!("OpenGL resources cleaned up.");
}

fn create_opengl_context(
    window: &Window,
    vsync_enabled: bool,
) -> Result<(Surface<WindowSurface>, PossiblyCurrentContext, glow::Context), Box<dyn Error>> {
    let display_handle = window.display_handle()?.as_raw();

    #[cfg(target_os = "windows")]
    let (display, vsync_logic) = {
        info!("Using WGL for OpenGL context.");
        let preference = DisplayApiPreference::Wgl(None);
        let display = unsafe { Display::new(display_handle, preference)? };

        let vsync_logic = move |display: &Display| {
            info!("Attempting to set VSync via wglSwapIntervalEXT...");
            type SwapIntervalFn = extern "system" fn(i32) -> i32;
            let proc_name = CStr::from_bytes_with_nul(b"wglSwapIntervalEXT\0").unwrap();
            let proc = display.get_proc_address(proc_name);
            if !proc.is_null() {
                let f: SwapIntervalFn = unsafe { std::mem::transmute(proc) };
                let interval = if vsync_enabled { 1 } else { 0 };
                if f(interval) != 0 {
                    info!("Successfully set VSync to: {}", if vsync_enabled { "on" } else { "off" });
                } else {
                    warn!("wglSwapIntervalEXT call failed. VSync state may not be as requested.");
                }
            } else {
                warn!("wglSwapIntervalEXT function not found. Cannot control VSync.");
            }
        };
        (display, vsync_logic)
    };
    
    #[cfg(not(target_os = "windows"))]
    let (display, vsync_logic) = {
        info!("Using EGL for OpenGL context.");
        let preference = DisplayApiPreference::Egl;
        let display = unsafe { Display::new(display_handle, preference)? };
        
        let vsync_logic = move |display: &Display, surface: &Surface<WindowSurface>, context: &PossiblyCurrentContext| {
            use glutin::surface::SwapInterval;
            let interval = if vsync_enabled {
                SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap())
            } else {
                SwapInterval::DontWait
            };

            if let Err(e) = surface.set_swap_interval(&context, interval) {
                warn!("Failed to set swap interval (VSync): {:?}", e);
            } else {
                info!("Successfully set VSync to: {}", if vsync_enabled { "on" } else { "off" });
            }
        };
        (display, vsync_logic)
    };

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        .with_stencil_size(8)
        .build();

    let config = unsafe { display.find_configs(template)?.next() }
        .ok_or("Failed to find a suitable GL config")?;

    let (width, height): (u32, u32) = window.inner_size().into();
    let raw_window_handle = window.window_handle()?.as_raw();
    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    );
    let surface = unsafe { display.create_window_surface(&config, &surface_attributes)? };

    let context_attributes =
        ContextAttributesBuilder::new().build(Some(raw_window_handle));
    let context = unsafe { display.create_context(&config, &context_attributes)? }
        .make_current(&surface)?;

    #[cfg(target_os = "windows")]
    vsync_logic(&display);
    #[cfg(not(target_os = "windows"))]
    vsync_logic(&display, &surface, &context);

    unsafe {
        let gl = glow::Context::from_loader_function_cstr(|s: &CStr| display.get_proc_address(s));
        Ok((surface, context, gl))
    }
}

fn create_graphics_program(
    gl: &glow::Context,
) -> Result<(glow::Program, UniformLocation, UniformLocation, UniformLocation, UniformLocation, UniformLocation, UniformLocation, UniformLocation), String> {
    unsafe {
        let program = gl.create_program()?;
        let compile = |ty, src: &str| -> Result<glow::Shader, String> {
            let sh = gl.create_shader(ty)?;
            gl.shader_source(sh, src);
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                let log = gl.get_shader_info_log(sh);
                gl.delete_shader(sh);
                return Err(log);
            }
            Ok(sh)
        };

        let vert = compile(glow::VERTEX_SHADER, include_str!("../shaders/opengl_shader.vert"))?;
        let frag = compile(glow::FRAGMENT_SHADER, include_str!("../shaders/opengl_shader.frag"))?;

        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.detach_shader(program, vert); gl.detach_shader(program, frag);
            gl.delete_shader(vert); gl.delete_shader(frag);
            gl.delete_program(program);
            return Err(log);
        }
        gl.detach_shader(program, vert); gl.detach_shader(program, frag);
        gl.delete_shader(vert); gl.delete_shader(frag);

        let get = |name: &str| gl.get_uniform_location(program, name).ok_or_else(|| name.to_string());
        let mvp_location = get("u_model_view_proj")?;
        let color_location = get("u_color")?;
        let texture_location = get("u_texture")?;
        let uv_scale_location = get("u_uv_scale")?;
        let uv_offset_location = get("u_uv_offset")?;
        let edge_fade_location = get("u_edge_fade")?;
        let instanced_location = get("u_instanced")?;

        Ok((program, mvp_location, color_location, texture_location, uv_scale_location, uv_offset_location, edge_fade_location, instanced_location))
    }
}

mod bytemuck {
    #[inline(always)]
    pub fn cast_slice<T, U>(slice: &[T]) -> &[U] {
        let (prefix, mid, suffix) = unsafe { slice.align_to::<U>() };
        debug_assert!(prefix.is_empty() && suffix.is_empty(), "cast_slice: misaligned cast");
        mid
    }
}
