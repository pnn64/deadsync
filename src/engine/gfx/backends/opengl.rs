use crate::engine::gfx::{
    BlendMode, DrawStats, FastU64Map, MeshMode, RenderList, SamplerDesc, SamplerFilter,
    SamplerWrap, TMeshCacheKey, Texture as RendererTexture, TextureHandleMap, TexturedMeshVertex,
    draw_prep::{
        self, DrawOp, DrawScratch, SpriteInstanceRaw, TexturedMeshInstanceRaw, TexturedMeshSource,
    },
};
use crate::engine::space::ortho_for_window;
use glam::Mat4 as Matrix4;
use glow::{HasContext, PixelPackData, PixelUnpackData, UniformLocation};
use glutin::{
    config::{Api as ConfigApi, Config, ConfigTemplateBuilder},
    context::{ContextAttributes, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use image::RgbaImage;
use log::{debug, info, warn};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
use std::{error::Error, ffi::CStr, mem, num::NonZeroU32, sync::Arc, time::Instant};
use winit::window::Window;

#[cfg(all(unix, not(target_os = "macos")))]
use glutin::context::{ContextApi, GlProfile, Version};

#[cfg(target_os = "macos")]
fn logical_px_for_physical(px: u32, scale: f64) -> u32 {
    if px == 0 {
        return 0;
    }
    ((f64::from(px) / scale.max(0.001)).round().max(1.0)) as u32
}

#[cfg(target_os = "macos")]
fn opengl_render_size(window: &Window, high_dpi_enabled: bool) -> (u32, u32) {
    let size = window.inner_size();
    if high_dpi_enabled {
        return (size.width, size.height);
    }
    let scale = window.scale_factor();
    (
        logical_px_for_physical(size.width, scale),
        logical_px_for_physical(size.height, scale),
    )
}

#[cfg(not(target_os = "macos"))]
fn opengl_render_size(window: &Window, _high_dpi_enabled: bool) -> (u32, u32) {
    let size = window.inner_size();
    (size.width, size.height)
}

#[cfg(target_os = "macos")]
fn set_macos_opengl_high_dpi_surface(window: &Window, enabled: bool) {
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;

    let Ok(handle) = window.window_handle() else {
        warn!("Unable to get macOS window handle for OpenGL high-DPI setup.");
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: the raw AppKit view handle comes from the live winit window and is
    // retained only for this call so the Objective-C object stays valid.
    let Some(view) = (unsafe { Retained::retain(appkit.ns_view.as_ptr().cast::<NSView>()) }) else {
        warn!("Unable to get macOS NSView for OpenGL high-DPI setup.");
        return;
    };
    #[allow(deprecated)]
    view.setWantsBestResolutionOpenGLSurface(enabled);
}

const OPENGL_PRESENT_SPIKE_US: u32 = 3_000;
const OPENGL_GPU_WAIT_SPIKE_US: u32 = 1_000;
const OPENGL_TMESH_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlApi {
    Desktop,
    #[cfg(all(unix, not(target_os = "macos")))]
    Gles,
}

impl GlApi {
    const fn label(self) -> &'static str {
        match self {
            Self::Desktop => "desktop OpenGL",
            #[cfg(all(unix, not(target_os = "macos")))]
            Self::Gles => "OpenGL ES",
        }
    }

    const fn shaders(self) -> ShaderSet {
        match self {
            Self::Desktop => ShaderSet {
                sprite_vert: include_str!("../shaders/opengl_shader.vert"),
                sprite_frag: include_str!("../shaders/opengl_shader.frag"),
                mesh_vert: include_str!("../shaders/opengl_mesh.vert"),
                mesh_frag: include_str!("../shaders/opengl_mesh.frag"),
                tmesh_vert: include_str!("../shaders/opengl_tmesh.vert"),
                tmesh_frag: include_str!("../shaders/opengl_tmesh.frag"),
            },
            #[cfg(all(unix, not(target_os = "macos")))]
            Self::Gles => ShaderSet {
                sprite_vert: include_str!("../shaders/opengl_shader_gles.vert"),
                sprite_frag: include_str!("../shaders/opengl_shader_gles.frag"),
                mesh_vert: include_str!("../shaders/opengl_mesh_gles.vert"),
                mesh_frag: include_str!("../shaders/opengl_mesh_gles.frag"),
                tmesh_vert: include_str!("../shaders/opengl_tmesh_gles.vert"),
                tmesh_frag: include_str!("../shaders/opengl_tmesh_gles.frag"),
            },
        }
    }
}

#[derive(Clone, Copy)]
struct ShaderSet {
    sprite_vert: &'static str,
    sprite_frag: &'static str,
    mesh_vert: &'static str,
    mesh_frag: &'static str,
    tmesh_vert: &'static str,
    tmesh_frag: &'static str,
}

// A handle to an OpenGL texture on the GPU.
#[derive(Debug, Clone, Copy)]
pub struct Texture(pub glow::Texture);

struct CachedTMeshGeom {
    vbo: glow::Buffer,
    vertex_count: u32,
}

pub struct State {
    pub gl: glow::Context,
    gl_surface: Surface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    api: GlApi,
    program: glow::Program,
    mesh_program: glow::Program,
    tmesh_program: glow::Program,
    mvp_location: UniformLocation,
    mesh_mvp_location: UniformLocation,
    tmesh_mvp_location: UniformLocation,
    tmesh_texture_location: UniformLocation,
    texture_location: UniformLocation,
    projection: Matrix4,
    window_size: (u32, u32),
    // A single, shared set of buffers for a unit quad.
    shared_vao: glow::VertexArray,
    _shared_vbo: glow::Buffer,
    _shared_ibo: glow::Buffer,
    shared_instance_vbo: glow::Buffer,
    index_count: i32,
    mesh_vao: glow::VertexArray,
    mesh_vbo: glow::Buffer,
    tmesh_vao: glow::VertexArray,
    tmesh_vbo: glow::Buffer,
    tmesh_instance_vbo: glow::Buffer,
    prep: DrawScratch,
    cached_tmesh: FastU64Map<CachedTMeshGeom>,
    cached_tmesh_bytes: usize,
    vsync_enabled: bool,
    screenshot_requested: bool,
    captured_frame: Option<RgbaImage>,
}

pub fn init(
    window: Arc<Window>,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
    high_dpi_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing OpenGL backend...");
    if gfx_debug_enabled {
        debug!("OpenGL debug context requested.");
    }

    let (gl_surface, gl_context, gl, api) =
        create_opengl_context(&window, vsync_enabled, gfx_debug_enabled, high_dpi_enabled)?;
    #[cfg(target_os = "macos")]
    set_macos_opengl_high_dpi_surface(&window, high_dpi_enabled);
    info!("OpenGL context API: {}", api.label());
    log_opengl_driver_info(&gl);
    let shaders = api.shaders();
    let (program, mvp_location, texture_location) =
        create_graphics_program(&gl, shaders.sprite_vert, shaders.sprite_frag)?;
    let (mesh_program, mesh_mvp_location) =
        create_mesh_program(&gl, shaders.mesh_vert, shaders.mesh_frag)?;
    let (tmesh_program, tmesh_mvp_location, tmesh_texture_location) =
        create_tmesh_program(&gl, shaders.tmesh_vert, shaders.tmesh_frag)?;

    // Create shared static unit quad + index buffer.
    // SAFETY: the OpenGL context created above is current on this thread for the
    // duration of initialization, so creating and configuring these GL objects is
    // valid here.
    let (shared_vao, _shared_vbo, _shared_ibo, shared_instance_vbo, index_count) = unsafe {
        const UNIT_QUAD_VERTICES: [[f32; 4]; 4] = [
            [-0.5, -0.5, 0.0, 1.0],
            [0.5, -0.5, 1.0, 1.0],
            [0.5, 0.5, 1.0, 0.0],
            [-0.5, 0.5, 0.0, 0.0],
        ];
        const QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let ibo = gl.create_buffer()?;
        let instance_vbo = gl.create_buffer()?;

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
        gl.vertex_attrib_pointer_f32(
            1,
            2,
            glow::FLOAT,
            false,
            stride,
            (2 * mem::size_of::<f32>()) as i32,
        );

        gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        let inst_stride = mem::size_of::<SpriteInstanceRaw>() as i32;
        let vec2_size = (2 * mem::size_of::<f32>()) as i32;
        let vec4_size = (4 * mem::size_of::<f32>()) as i32;
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, inst_stride, 0);
        gl.vertex_attrib_divisor(2, 1);
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(3, 2, glow::FLOAT, false, inst_stride, vec4_size);
        gl.vertex_attrib_divisor(3, 1);
        gl.enable_vertex_attrib_array(4);
        gl.vertex_attrib_pointer_f32(4, 2, glow::FLOAT, false, inst_stride, vec4_size + vec2_size);
        gl.vertex_attrib_divisor(4, 1);
        gl.enable_vertex_attrib_array(5);
        gl.vertex_attrib_pointer_f32(
            5,
            4,
            glow::FLOAT,
            false,
            inst_stride,
            vec4_size + 2 * vec2_size,
        );
        gl.vertex_attrib_divisor(5, 1);
        gl.enable_vertex_attrib_array(6);
        gl.vertex_attrib_pointer_f32(
            6,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            2 * vec4_size + 2 * vec2_size,
        );
        gl.vertex_attrib_divisor(6, 1);
        gl.enable_vertex_attrib_array(7);
        gl.vertex_attrib_pointer_f32(
            7,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            2 * vec4_size + 3 * vec2_size,
        );
        gl.vertex_attrib_divisor(7, 1);
        gl.enable_vertex_attrib_array(8);
        gl.vertex_attrib_pointer_f32(
            8,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            2 * vec4_size + 4 * vec2_size,
        );
        gl.vertex_attrib_divisor(8, 1);
        gl.enable_vertex_attrib_array(9);
        gl.vertex_attrib_pointer_f32(
            9,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            2 * vec4_size + 5 * vec2_size,
        );
        gl.vertex_attrib_divisor(9, 1);
        gl.enable_vertex_attrib_array(10);
        gl.vertex_attrib_pointer_f32(
            10,
            4,
            glow::FLOAT,
            false,
            inst_stride,
            2 * vec4_size + 6 * vec2_size,
        );
        gl.vertex_attrib_divisor(10, 1);

        gl.bind_vertex_array(None);

        (vao, vbo, ibo, instance_vbo, QUAD_INDICES.len() as i32)
    };

    // SAFETY: the OpenGL context is still current on this thread, so creating and
    // configuring the mesh VAO/VBO pair is valid here.
    let (mesh_vao, mesh_vbo) = unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        // a_pos (location 0), a_color (location 1)
        let stride = std::mem::size_of::<crate::engine::gfx::MeshVertex>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(
            1,
            4,
            glow::FLOAT,
            false,
            stride,
            (2 * std::mem::size_of::<f32>()) as i32,
        );

        gl.bind_vertex_array(None);
        (vao, vbo)
    };
    // SAFETY: the OpenGL context is still current on this thread, so creating and
    // configuring the textured-mesh VAO/VBO pair is valid here.
    let (tmesh_vao, tmesh_vbo, tmesh_instance_vbo) = unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let instance_vbo = gl.create_buffer()?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        // a_pos (location 0), a_uv (location 1), a_color (location 2), a_tex_matrix_scale (location 3)
        let stride = std::mem::size_of::<TexturedMeshVertex>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(
            1,
            2,
            glow::FLOAT,
            false,
            stride,
            (3 * std::mem::size_of::<f32>()) as i32,
        );
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(
            2,
            4,
            glow::FLOAT,
            false,
            stride,
            (5 * std::mem::size_of::<f32>()) as i32,
        );
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(
            3,
            2,
            glow::FLOAT,
            false,
            stride,
            (9 * std::mem::size_of::<f32>()) as i32,
        );

        // i_model_col0..i_model_col3 (locations 4..7), i_tint (8),
        // i_uv_scale/i_uv_offset/i_uv_tex_shift (9..11)
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(instance_vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        let inst_stride = std::mem::size_of::<TexturedMeshInstanceRaw>() as i32;
        let col_size = (4 * std::mem::size_of::<f32>()) as i32;
        let uv_size = (2 * std::mem::size_of::<f32>()) as i32;
        gl.enable_vertex_attrib_array(4);
        gl.vertex_attrib_pointer_f32(4, 4, glow::FLOAT, false, inst_stride, 0);
        gl.vertex_attrib_divisor(4, 1);
        gl.enable_vertex_attrib_array(5);
        gl.vertex_attrib_pointer_f32(5, 4, glow::FLOAT, false, inst_stride, col_size);
        gl.vertex_attrib_divisor(5, 1);
        gl.enable_vertex_attrib_array(6);
        gl.vertex_attrib_pointer_f32(6, 4, glow::FLOAT, false, inst_stride, 2 * col_size);
        gl.vertex_attrib_divisor(6, 1);
        gl.enable_vertex_attrib_array(7);
        gl.vertex_attrib_pointer_f32(7, 4, glow::FLOAT, false, inst_stride, 3 * col_size);
        gl.vertex_attrib_divisor(7, 1);
        gl.enable_vertex_attrib_array(8);
        gl.vertex_attrib_pointer_f32(8, 4, glow::FLOAT, false, inst_stride, 4 * col_size);
        gl.vertex_attrib_divisor(8, 1);
        gl.enable_vertex_attrib_array(9);
        gl.vertex_attrib_pointer_f32(9, 2, glow::FLOAT, false, inst_stride, 5 * col_size);
        gl.vertex_attrib_divisor(9, 1);
        gl.enable_vertex_attrib_array(10);
        gl.vertex_attrib_pointer_f32(
            10,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            5 * col_size + uv_size,
        );
        gl.vertex_attrib_divisor(10, 1);
        gl.enable_vertex_attrib_array(11);
        gl.vertex_attrib_pointer_f32(
            11,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            5 * col_size + 2 * uv_size,
        );
        gl.vertex_attrib_divisor(11, 1);

        gl.bind_vertex_array(None);
        (vao, vbo, instance_vbo)
    };

    let (initial_width, initial_height) = opengl_render_size(&window, high_dpi_enabled);
    let projection = ortho_for_window(initial_width, initial_height);
    let (surface_width, surface_height) = surface_extent(initial_width, initial_height);
    gl_surface.resize(&gl_context, surface_width, surface_height);
    info!(
        "OpenGL render size: {}x{} high_dpi={} window_physical={}x{} scale={:.2}",
        initial_width,
        initial_height,
        high_dpi_enabled,
        window.inner_size().width,
        window.inner_size().height,
        window.scale_factor()
    );

    // SAFETY: the OpenGL context is current and all program/texture handles used
    // below were created successfully in this function.
    unsafe {
        gl.viewport(0, 0, initial_width as i32, initial_height as i32);
        gl.use_program(Some(program));
        gl.active_texture(glow::TEXTURE0);
        gl.uniform_1_i32(Some(&texture_location), 0);
        gl.use_program(None);
    }

    let state = State {
        gl,
        gl_surface,
        gl_context,
        api,
        program,
        mesh_program,
        tmesh_program,
        mvp_location,
        mesh_mvp_location,
        tmesh_mvp_location,
        tmesh_texture_location,
        texture_location,
        projection,
        window_size: (initial_width, initial_height),
        shared_vao,
        _shared_vbo,
        _shared_ibo,
        shared_instance_vbo,
        index_count,
        mesh_vao,
        mesh_vbo,
        tmesh_vao,
        tmesh_vbo,
        tmesh_instance_vbo,
        prep: DrawScratch::with_capacity(256, 1024, 1024, 256, 64),
        cached_tmesh: FastU64Map::default(),
        cached_tmesh_bytes: 0,
        vsync_enabled,
        screenshot_requested: false,
        captured_frame: None,
    };

    info!("OpenGL backend initialized successfully.");
    Ok(state)
}

pub fn set_vsync_enabled(state: &mut State, enabled: bool) {
    if state.vsync_enabled == enabled {
        return;
    }
    state.vsync_enabled = enabled;
    let interval = if enabled {
        SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap())
    } else {
        SwapInterval::DontWait
    };
    if let Err(e) = state
        .gl_surface
        .set_swap_interval(&state.gl_context, interval)
    {
        warn!("Failed to update OpenGL swap interval (VSync): {:?}", e);
    } else {
        debug!(
            "Updated OpenGL VSync to {}",
            if enabled { "on" } else { "off" }
        );
    }
}

fn log_opengl_driver_info(gl: &glow::Context) {
    #[inline(always)]
    fn norm(value: String) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            "<unknown>".to_string()
        } else {
            trimmed.to_string()
        }
    }
    // SAFETY: driver string queries only read state from the current OpenGL
    // context and do not retain Rust pointers.
    unsafe {
        let vendor = norm(gl.get_parameter_string(glow::VENDOR));
        let renderer = norm(gl.get_parameter_string(glow::RENDERER));
        let version = norm(gl.get_parameter_string(glow::VERSION));
        let glsl = norm(gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION));
        info!(
            "OpenGL driver: {} [{}], {}, GLSL {}",
            renderer, vendor, version, glsl
        );
    }
}

pub fn create_texture(
    gl: &glow::Context,
    image: &RgbaImage,
    sampler: SamplerDesc,
) -> Result<Texture, String> {
    let wrap_mode = match sampler.wrap {
        SamplerWrap::Clamp => glow::CLAMP_TO_EDGE,
        SamplerWrap::Repeat => glow::REPEAT,
    };
    let filter_mode = match sampler.filter {
        SamplerFilter::Linear => glow::LINEAR,
        SamplerFilter::Nearest => glow::NEAREST,
    };
    // SAFETY: the caller provides a live OpenGL context, and `image.as_raw()`
    // exposes initialized RGBA bytes that stay alive for the duration of the GL
    // upload call.
    unsafe {
        let t = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(t));

        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);

        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap_mode as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap_mode as i32);
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            filter_mode as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            filter_mode as i32,
        );
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

pub fn update_texture(
    gl: &glow::Context,
    texture: &Texture,
    image: &RgbaImage,
) -> Result<(), String> {
    let w = i32::try_from(image.width()).map_err(|_| "texture width overflow".to_string())?;
    let h = i32::try_from(image.height()).map_err(|_| "texture height overflow".to_string())?;
    let raw = image.as_raw();
    // SAFETY: `texture.0` is a live OpenGL texture handle owned by this backend,
    // and `raw` exposes initialized RGBA bytes that stay alive for the duration of
    // the update call.
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(texture.0));
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            0,
            0,
            w,
            h,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            PixelUnpackData::Slice(Some(raw)),
        );
        gl.bind_texture(glow::TEXTURE_2D, None);
    }
    Ok(())
}

fn ensure_cached_tmesh(
    gl: &glow::Context,
    cached_tmesh: &mut FastU64Map<CachedTMeshGeom>,
    cached_tmesh_bytes: &mut usize,
    cache_key: TMeshCacheKey,
    vertices: &[crate::engine::gfx::TexturedMeshVertex],
) -> bool {
    if let Some(entry) = cached_tmesh.get(&cache_key) {
        return entry.vertex_count == vertices.len() as u32;
    }

    let bytes = vertices.len() * std::mem::size_of::<TexturedMeshVertex>();
    if bytes > OPENGL_TMESH_CACHE_MAX_BYTES
        || cached_tmesh_bytes.saturating_add(bytes) > OPENGL_TMESH_CACHE_MAX_BYTES
    {
        return false;
    }

    // SAFETY: the OpenGL context is current on this thread while draw prep runs,
    // and the uploaded slice remains alive for the duration of the call.
    let vbo = unsafe {
        let Ok(vbo) = gl.create_buffer() else {
            return false;
        };
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck::cast_slice(vertices),
            glow::STATIC_DRAW,
        );
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        vbo
    };

    cached_tmesh.insert(
        cache_key,
        CachedTMeshGeom {
            vbo,
            vertex_count: vertices.len() as u32,
        },
    );
    *cached_tmesh_bytes = cached_tmesh_bytes.saturating_add(bytes);
    true
}

#[inline(always)]
pub fn request_screenshot(state: &mut State) {
    state.screenshot_requested = true;
}

pub fn draw(
    state: &mut State,
    render_list: &RenderList,
    textures: &TextureHandleMap<RendererTexture>,
    apply_present_back_pressure: bool,
) -> Result<DrawStats, Box<dyn Error>> {
    #[inline(always)]
    fn elapsed_us_since(started: Instant) -> u32 {
        let elapsed = started.elapsed().as_micros();
        if elapsed > u128::from(u32::MAX) {
            u32::MAX
        } else {
            elapsed as u32
        }
    }

    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(DrawStats::default());
    }

    #[inline(always)]
    fn apply_blend(gl: &glow::Context, want: BlendMode, last: &mut Option<BlendMode>) {
        if *last == Some(want) {
            return;
        }
        // SAFETY: blend-state calls only mutate GL state on the current context and
        // do not retain Rust pointers.
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

    let backend_prepare_started = Instant::now();
    {
        let prep = &mut state.prep;
        let gl = &state.gl;
        let cached_tmesh = &mut state.cached_tmesh;
        let cached_tmesh_bytes = &mut state.cached_tmesh_bytes;
        let _prep_stats = draw_prep::prepare(render_list, prep, |cache_key, vertices| {
            ensure_cached_tmesh(gl, cached_tmesh, cached_tmesh_bytes, cache_key, vertices)
        });
    }
    let mut stats = DrawStats::default();
    stats.backend_prepare_us = elapsed_us_since(backend_prepare_started);

    let mut vertices: u32 = 0;

    let backend_record_started = Instant::now();
    // SAFETY: the OpenGL context in `state` is current on this thread for drawing,
    // and all buffer uploads reference slices that remain alive for the duration of
    // each call.
    unsafe {
        let gl = &state.gl;

        let c = render_list.clear_color;
        gl.color_mask(true, true, true, true);
        gl.clear_color(c[0], c[1], c[2], 1.0);
        gl.clear(glow::COLOR_BUFFER_BIT);
        // Keep the presented window surface opaque even when EGL hands us an
        // alpha-bearing default framebuffer. Otherwise Linux compositors can
        // treat the game as translucent and the whole scene looks ghosted.
        gl.color_mask(true, true, true, false);

        gl.enable(glow::BLEND);
        gl.blend_equation(glow::FUNC_ADD);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

        gl.active_texture(glow::TEXTURE0);

        let mut last_bound_tex: Option<glow::Texture> = None;
        let mut last_blend = Some(BlendMode::Alpha);
        let mut last_prog: Option<u8> = None; // 0=sprite, 1=mesh, 2=textured mesh
        let mut last_sprite_instance_start: Option<u32> = None;
        let mut last_tmesh_instance_start: Option<u32> = None;
        let mut last_tmesh_source: Option<TexturedMeshSource> = None;

        if !state.prep.sprite_instances.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.shared_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.prep.sprite_instances.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.prep.mesh_vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.prep.mesh_vertices.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.prep.tmesh_vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.prep.tmesh_vertices.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.prep.tmesh_instances.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.prep.tmesh_instances.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }

        for op in state.prep.ops.iter().copied() {
            match op {
                DrawOp::Sprite(run) => {
                    apply_blend(gl, run.blend, &mut last_blend);

                    let cam = render_list
                        .cameras
                        .get(run.camera as usize)
                        .copied()
                        .unwrap_or(state.projection);

                    if last_prog != Some(0) {
                        gl.use_program(Some(state.program));
                        gl.bind_vertex_array(Some(state.shared_vao));
                        gl.uniform_1_i32(Some(&state.texture_location), 0);
                        last_prog = Some(0);
                        last_sprite_instance_start = None;
                        last_tmesh_source = None;
                    }

                    if last_sprite_instance_start != Some(run.instance_start) {
                        let inst_stride = std::mem::size_of::<SpriteInstanceRaw>() as i32;
                        let vec2_size = (2 * std::mem::size_of::<f32>()) as i32;
                        let vec4_size = (4 * std::mem::size_of::<f32>()) as i32;
                        let base = (run.instance_start as i32) * inst_stride;
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.shared_instance_vbo));
                        gl.vertex_attrib_pointer_f32(2, 4, glow::FLOAT, false, inst_stride, base);
                        gl.vertex_attrib_pointer_f32(
                            3,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + vec4_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            4,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + vec4_size + vec2_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            5,
                            4,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + vec4_size + 2 * vec2_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            6,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 2 * vec4_size + 2 * vec2_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            7,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 2 * vec4_size + 3 * vec2_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            8,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 2 * vec4_size + 4 * vec2_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            9,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 2 * vec4_size + 5 * vec2_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            10,
                            4,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 2 * vec4_size + 6 * vec2_size,
                        );
                        last_sprite_instance_start = Some(run.instance_start);
                    }

                    let mvp_array = cam.to_cols_array_2d();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.mvp_location),
                        false,
                        bytemuck::cast_slice(&mvp_array),
                    );

                    let Some(texture) = textures.get(&run.texture_handle).and_then(|texture| {
                        if let RendererTexture::OpenGL(texture) = texture {
                            Some(texture.0)
                        } else {
                            None
                        }
                    }) else {
                        continue;
                    };

                    if last_bound_tex != Some(texture) {
                        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                        last_bound_tex = Some(texture);
                    }

                    gl.draw_elements_instanced(
                        glow::TRIANGLES,
                        state.index_count,
                        glow::UNSIGNED_SHORT,
                        0,
                        run.instance_count as i32,
                    );
                    vertices = vertices.saturating_add(4 * run.instance_count);
                }
                DrawOp::Mesh(run) => {
                    if run.vertex_count == 0 {
                        continue;
                    }

                    apply_blend(gl, run.blend, &mut last_blend);

                    let cam = render_list
                        .cameras
                        .get(run.camera as usize)
                        .copied()
                        .unwrap_or(state.projection);

                    if last_prog != Some(1) {
                        gl.use_program(Some(state.mesh_program));
                        gl.bind_vertex_array(Some(state.mesh_vao));
                        last_prog = Some(1);
                        last_tmesh_source = None;
                    }

                    let mvp_array = cam.to_cols_array_2d();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.mesh_mvp_location),
                        false,
                        bytemuck::cast_slice(&mvp_array),
                    );

                    let prim = match run.mode {
                        MeshMode::Triangles => glow::TRIANGLES,
                    };
                    gl.draw_arrays(prim, run.vertex_start as i32, run.vertex_count as i32);
                    vertices = vertices.saturating_add(run.vertex_count);
                }
                DrawOp::TexturedMesh(run) => {
                    apply_blend(gl, run.blend, &mut last_blend);

                    if last_prog != Some(2) {
                        gl.use_program(Some(state.tmesh_program));
                        gl.bind_vertex_array(Some(state.tmesh_vao));
                        gl.uniform_1_i32(Some(&state.tmesh_texture_location), 0);
                        last_prog = Some(2);
                        last_tmesh_instance_start = None;
                        last_tmesh_source = None;
                    }

                    if last_tmesh_source != Some(run.source) {
                        let stride = std::mem::size_of::<TexturedMeshVertex>() as i32;
                        let Some(vertex_buffer) = (match run.source {
                            TexturedMeshSource::Transient { .. } => Some(state.tmesh_vbo),
                            TexturedMeshSource::Cached { cache_key, .. } => {
                                state.cached_tmesh.get(&cache_key).map(|entry| entry.vbo)
                            }
                        }) else {
                            continue;
                        };
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertex_buffer));
                        gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
                        gl.vertex_attrib_pointer_f32(
                            1,
                            2,
                            glow::FLOAT,
                            false,
                            stride,
                            (3 * std::mem::size_of::<f32>()) as i32,
                        );
                        gl.vertex_attrib_pointer_f32(
                            2,
                            4,
                            glow::FLOAT,
                            false,
                            stride,
                            (5 * std::mem::size_of::<f32>()) as i32,
                        );
                        gl.vertex_attrib_pointer_f32(
                            3,
                            2,
                            glow::FLOAT,
                            false,
                            stride,
                            (9 * std::mem::size_of::<f32>()) as i32,
                        );
                        last_tmesh_source = Some(run.source);
                    }

                    if last_tmesh_instance_start != Some(run.instance_start) {
                        let inst_stride = std::mem::size_of::<TexturedMeshInstanceRaw>() as i32;
                        let col_size = (4 * std::mem::size_of::<f32>()) as i32;
                        let uv_size = (2 * std::mem::size_of::<f32>()) as i32;
                        let base = (run.instance_start as i32) * inst_stride;
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_instance_vbo));
                        gl.vertex_attrib_pointer_f32(4, 4, glow::FLOAT, false, inst_stride, base);
                        gl.vertex_attrib_pointer_f32(
                            5,
                            4,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + col_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            6,
                            4,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 2 * col_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            7,
                            4,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 3 * col_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            8,
                            4,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 4 * col_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            9,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 5 * col_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            10,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 5 * col_size + uv_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            11,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 5 * col_size + 2 * uv_size,
                        );
                        last_tmesh_instance_start = Some(run.instance_start);
                    }

                    let cam = render_list
                        .cameras
                        .get(run.camera as usize)
                        .copied()
                        .unwrap_or(state.projection);
                    let mvp_array = cam.to_cols_array_2d();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.tmesh_mvp_location),
                        false,
                        bytemuck::cast_slice(&mvp_array),
                    );

                    let Some(texture) = textures.get(&run.texture_handle).and_then(|texture| {
                        if let RendererTexture::OpenGL(texture) = texture {
                            Some(texture.0)
                        } else {
                            None
                        }
                    }) else {
                        continue;
                    };

                    if last_bound_tex != Some(texture) {
                        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                        last_bound_tex = Some(texture);
                    }

                    let prim = match run.mode {
                        MeshMode::Triangles => glow::TRIANGLES,
                    };
                    let draw_start = run.source.vertex_start() as i32;
                    let draw_count = run.source.vertex_count() as i32;
                    gl.draw_arrays_instanced(
                        prim,
                        draw_start,
                        draw_count,
                        run.instance_count as i32,
                    );
                    let tri_count = run.source.vertex_count() / 3;
                    vertices =
                        vertices.saturating_add(tri_count.saturating_mul(run.instance_count));
                }
            }
        }
        gl.bind_vertex_array(None);
        gl.use_program(None);
    }
    stats.backend_record_us = elapsed_us_since(backend_record_started);

    if state.screenshot_requested {
        state.screenshot_requested = false;
        state.captured_frame = None;
        let (width, height) = state.window_size;
        if width > 0 && height > 0 {
            let byte_len = width as usize * height as usize * 4;
            let mut pixels = vec![0u8; byte_len];
            // SAFETY: reads RGBA bytes from the back buffer before swap.
            unsafe {
                state.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
                state.gl.read_buffer(glow::BACK);
                state.gl.read_pixels(
                    0,
                    0,
                    width as i32,
                    height as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    PixelPackData::Slice(Some(pixels.as_mut_slice())),
                );
            }
            flip_rows_rgba_in_place(width as usize, height as usize, &mut pixels);
            state.captured_frame = RgbaImage::from_raw(width, height, pixels);
        }
    }

    let present_started = Instant::now();
    state.gl_surface.swap_buffers(&state.gl_context)?;
    stats.present_us = elapsed_us_since(present_started);
    if apply_present_back_pressure {
        // Mirror ITGmania's uncapped GL path: block here so the CPU does not
        // run frames far ahead of the GPU when swap interval is disabled.
        let wait_started = Instant::now();
        // SAFETY: `finish` only blocks on the current GL context and does not
        // retain Rust pointers.
        unsafe {
            state.gl.finish();
        }
        stats.gpu_wait_us = elapsed_us_since(wait_started);
    }
    if stats.present_us >= OPENGL_PRESENT_SPIKE_US || stats.gpu_wait_us >= OPENGL_GPU_WAIT_SPIKE_US
    {
        let (width, height) = state.window_size;
        debug!(
            "OpenGL present spike: swap_ms={:.3} gpu_wait_ms={:.3} back_pressure={} vertices={} viewport={}x{}",
            stats.present_us as f32 / 1000.0,
            stats.gpu_wait_us as f32 / 1000.0,
            apply_present_back_pressure,
            vertices,
            width,
            height
        );
    }
    stats.vertices = vertices;
    Ok(stats)
}

pub fn capture_frame(state: &mut State) -> Result<RgbaImage, Box<dyn Error>> {
    state
        .captured_frame
        .take()
        .ok_or_else(|| std::io::Error::other("No captured screenshot frame available").into())
}

#[inline(always)]
fn flip_rows_rgba_in_place(width: usize, height: usize, pixels: &mut [u8]) {
    let row_bytes = width.saturating_mul(4);
    if row_bytes == 0 || height <= 1 {
        return;
    }
    let half = height / 2;
    for y in 0..half {
        let top = y * row_bytes;
        let bottom = (height - 1 - y) * row_bytes;
        for i in 0..row_bytes {
            pixels.swap(top + i, bottom + i);
        }
    }
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    state.window_size = (width, height);
    if width == 0 || height == 0 {
        return;
    }
    let (w, h) = surface_extent(width, height);

    state.gl_surface.resize(&state.gl_context, w, h);
    // SAFETY: the OpenGL context remains current for this surface, so updating the
    // viewport to match the resized window is valid here.
    unsafe {
        state.gl.viewport(0, 0, width as i32, height as i32);
    }
    state.projection = ortho_for_window(width, height);
}

pub fn cleanup(state: &mut State) {
    info!("Cleaning up OpenGL resources...");
    // SAFETY: all GL object handles below were created by this backend and are
    // still owned by `state`, so deleting them once during cleanup is valid.
    unsafe {
        state.gl.delete_program(state.program);
        state.gl.delete_program(state.mesh_program);
        state.gl.delete_program(state.tmesh_program);
        state.gl.delete_vertex_array(state.shared_vao);
        state.gl.delete_buffer(state._shared_vbo);
        state.gl.delete_buffer(state._shared_ibo);
        state.gl.delete_buffer(state.shared_instance_vbo);
        state.gl.delete_vertex_array(state.mesh_vao);
        state.gl.delete_buffer(state.mesh_vbo);
        state.gl.delete_vertex_array(state.tmesh_vao);
        for geom in state.cached_tmesh.drain().map(|(_, geom)| geom) {
            state.gl.delete_buffer(geom.vbo);
        }
        state.cached_tmesh_bytes = 0;
        state.gl.delete_buffer(state.tmesh_vbo);
        state.gl.delete_buffer(state.tmesh_instance_vbo);
    }
    info!("OpenGL resources cleaned up.");
}

fn create_opengl_context(
    window: &Window,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
    high_dpi_enabled: bool,
) -> Result<
    (
        Surface<WindowSurface>,
        PossiblyCurrentContext,
        glow::Context,
        GlApi,
    ),
    Box<dyn Error>,
> {
    let display_handle = window.display_handle()?.as_raw();

    #[cfg(target_os = "windows")]
    let (display, vsync_logic) = {
        debug!("Using WGL for OpenGL context.");
        let preference = DisplayApiPreference::Wgl(None);
        // SAFETY: `display_handle` comes from the live winit window and remains
        // valid for the duration of context creation.
        let display = unsafe { Display::new(display_handle, preference)? };

        let vsync_logic = move |display: &Display| {
            debug!("Attempting to set VSync via wglSwapIntervalEXT...");
            type SwapIntervalFn = extern "system" fn(i32) -> i32;
            let proc_name = c"wglSwapIntervalEXT";
            let proc = display.get_proc_address(proc_name);
            if proc.is_null() {
                warn!("wglSwapIntervalEXT function not found. Cannot control VSync.");
            } else {
                // SAFETY: `proc` was looked up specifically as `wglSwapIntervalEXT`
                // and is only called when non-null, so this cast matches the WGL
                // extension signature.
                let f: SwapIntervalFn = unsafe { std::mem::transmute(proc) };
                let interval = i32::from(vsync_enabled);
                if f(interval) != 0 {
                    debug!(
                        "Successfully set VSync to: {}",
                        if vsync_enabled { "on" } else { "off" }
                    );
                } else {
                    warn!("wglSwapIntervalEXT call failed. VSync state may not be as requested.");
                }
            }
        };
        (display, vsync_logic)
    };

    #[cfg(not(target_os = "windows"))]
    let (display, vsync_logic) = {
        // Select the appropriate DisplayApiPreference based on the OS
        #[cfg(target_os = "macos")]
        let preference = {
            debug!("Using CGL for OpenGL context.");
            DisplayApiPreference::Cgl
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let preference = {
            debug!("Using EGL for OpenGL context.");
            DisplayApiPreference::Egl
        };

        // The rest of the logic is common for macOS and Linux/BSD
        // SAFETY: `display_handle` comes from the live winit window and remains
        // valid for the duration of context creation.
        let display = unsafe { Display::new(display_handle, preference)? };

        let vsync_logic = move |_display: &Display,
                                surface: &Surface<WindowSurface>,
                                context: &PossiblyCurrentContext| {
            use glutin::surface::SwapInterval;
            let interval = if vsync_enabled {
                SwapInterval::Wait(std::num::NonZeroU32::new(1).unwrap())
            } else {
                SwapInterval::DontWait
            };

            if let Err(e) = surface.set_swap_interval(context, interval) {
                warn!("Failed to set swap interval (VSync): {:?}", e);
            } else {
                debug!(
                    "Successfully set VSync to: {}",
                    if vsync_enabled { "on" } else { "off" }
                );
            }
        };
        (display, vsync_logic)
    };

    let (width, height) = opengl_render_size(window, high_dpi_enabled);
    let raw_window_handle = window.window_handle()?.as_raw();
    #[cfg(target_os = "windows")]
    let (surface, context, api) = {
        let config = find_config(
            &display,
            raw_window_handle,
            ConfigApi::OPENGL,
            "desktop OpenGL",
        )?;
        let context_attributes = ContextAttributesBuilder::new()
            .with_debug(gfx_debug_enabled)
            .build(Some(raw_window_handle));
        let (surface, context) = create_window_surface_context(
            &display,
            &config,
            raw_window_handle,
            width,
            height,
            context_attributes,
        )?;
        (surface, context, GlApi::Desktop)
    };

    #[cfg(target_os = "macos")]
    let (surface, context, api) = {
        let config = find_config(
            &display,
            raw_window_handle,
            ConfigApi::OPENGL,
            "desktop OpenGL",
        )?;
        let context_attributes = ContextAttributesBuilder::new()
            .with_debug(gfx_debug_enabled)
            .build(Some(raw_window_handle));
        let (surface, context) = create_window_surface_context(
            &display,
            &config,
            raw_window_handle,
            width,
            height,
            context_attributes,
        )?;
        (surface, context, GlApi::Desktop)
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let (surface, context, api) = {
        let try_desktop =
            || -> Result<(Surface<WindowSurface>, PossiblyCurrentContext), Box<dyn Error>> {
                let config = find_config(
                    &display,
                    raw_window_handle,
                    ConfigApi::OPENGL,
                    "desktop OpenGL",
                )?;
                let context_attributes = ContextAttributesBuilder::new()
                    .with_debug(gfx_debug_enabled)
                    .with_profile(GlProfile::Core)
                    .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
                    .build(Some(raw_window_handle));
                create_window_surface_context(
                    &display,
                    &config,
                    raw_window_handle,
                    width,
                    height,
                    context_attributes,
                )
            };

        match try_desktop() {
            Ok((surface, context)) => (surface, context, GlApi::Desktop),
            Err(desktop_err) => {
                warn!(
                    "Desktop OpenGL context creation failed over EGL: {desktop_err}. Retrying with OpenGL ES 3.0."
                );
                let config = find_config(
                    &display,
                    raw_window_handle,
                    ConfigApi::GLES3,
                    "OpenGL ES 3.x",
                )?;
                let context_attributes = ContextAttributesBuilder::new()
                    .with_debug(gfx_debug_enabled)
                    .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
                    .build(Some(raw_window_handle));
                let (surface, context) = create_window_surface_context(
                    &display,
                    &config,
                    raw_window_handle,
                    width,
                    height,
                    context_attributes,
                )
                .map_err(|gles_err| {
                    std::io::Error::other(format!(
                        "Failed to create an OpenGL ES 3.0 context after desktop OpenGL fallback. desktop={desktop_err}; gles={gles_err}"
                    ))
                })?;
                (surface, context, GlApi::Gles)
            }
        }
    };

    #[cfg(target_os = "windows")]
    vsync_logic(&display);
    #[cfg(not(target_os = "windows"))]
    vsync_logic(&display, &surface, &context);

    // SAFETY: `display.get_proc_address` is valid while the display/context are
    // alive, and `glow` only stores the function pointers returned by this loader.
    unsafe {
        let gl = glow::Context::from_loader_function_cstr(|s: &CStr| display.get_proc_address(s));
        Ok((surface, context, gl, api))
    }
}

fn create_graphics_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<(glow::Program, UniformLocation, UniformLocation), String> {
    // SAFETY: shader/program creation and linkage only touch the current OpenGL
    // context, and all temporary shader/program handles are cleaned up on every
    // exit path below.
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

        let vert = compile(glow::VERTEX_SHADER, vert_src)?;
        let frag = compile(glow::FRAGMENT_SHADER, frag_src)?;

        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.detach_shader(program, vert);
            gl.detach_shader(program, frag);
            gl.delete_shader(vert);
            gl.delete_shader(frag);
            gl.delete_program(program);
            return Err(log);
        }
        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        let get = |name: &str| {
            gl.get_uniform_location(program, name)
                .ok_or_else(|| name.to_string())
        };
        let mvp_location = get("u_model_view_proj")?;
        let texture_location = get("u_texture")?;

        Ok((program, mvp_location, texture_location))
    }
}

fn create_mesh_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<(glow::Program, UniformLocation), String> {
    // SAFETY: shader/program creation and linkage only touch the current OpenGL
    // context, and all temporary shader/program handles are cleaned up on every
    // exit path below.
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

        let vert = compile(glow::VERTEX_SHADER, vert_src)?;
        let frag = compile(glow::FRAGMENT_SHADER, frag_src)?;

        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.detach_shader(program, vert);
            gl.detach_shader(program, frag);
            gl.delete_shader(vert);
            gl.delete_shader(frag);
            gl.delete_program(program);
            return Err(log);
        }
        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        let mvp_location = gl
            .get_uniform_location(program, "u_model_view_proj")
            .ok_or_else(|| "u_model_view_proj".to_string())?;

        Ok((program, mvp_location))
    }
}

fn create_tmesh_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> Result<(glow::Program, UniformLocation, UniformLocation), String> {
    // SAFETY: shader/program creation and linkage only touch the current OpenGL
    // context, and all temporary shader/program handles are cleaned up on every
    // exit path below.
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

        let vert = compile(glow::VERTEX_SHADER, vert_src)?;
        let frag = compile(glow::FRAGMENT_SHADER, frag_src)?;

        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.detach_shader(program, vert);
            gl.detach_shader(program, frag);
            gl.delete_shader(vert);
            gl.delete_shader(frag);
            gl.delete_program(program);
            return Err(log);
        }
        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        let mvp_location = gl
            .get_uniform_location(program, "u_model_view_proj")
            .ok_or_else(|| "u_model_view_proj".to_string())?;
        let texture_location = gl
            .get_uniform_location(program, "u_texture")
            .ok_or_else(|| "u_texture".to_string())?;

        Ok((program, mvp_location, texture_location))
    }
}

fn find_config(
    display: &Display,
    raw_window_handle: RawWindowHandle,
    api: ConfigApi,
    label: &str,
) -> Result<Config, Box<dyn Error>> {
    let template = ConfigTemplateBuilder::new()
        .with_api(api)
        .with_alpha_size(0)
        .with_depth_size(0)
        .with_stencil_size(0)
        .with_transparency(false)
        .compatible_with_native_window(raw_window_handle)
        .build();
    // SAFETY: `display` is live and `template` contains the raw window handle
    // borrowed from the live winit window for the duration of this call.
    unsafe { display.find_configs(template)?.next() }
        .ok_or_else(|| std::io::Error::other(format!("Failed to find a suitable {label} config")))
        .map_err(Into::into)
}

fn create_window_surface_context(
    display: &Display,
    config: &Config,
    raw_window_handle: RawWindowHandle,
    width: u32,
    height: u32,
    context_attributes: ContextAttributes,
) -> Result<(Surface<WindowSurface>, PossiblyCurrentContext), Box<dyn Error>> {
    let (width, height) = surface_extent(width, height);
    let surface_attributes =
        SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window_handle, width, height);
    // SAFETY: `display`, `config`, and `surface_attributes` all refer to the live
    // window and chosen GL config for this thread, so creating the window surface
    // is valid here.
    let surface = unsafe { display.create_window_surface(config, &surface_attributes)? };
    // SAFETY: `display`, `config`, and `context_attributes` refer to the same live
    // window/config pair as the surface above, so creating and making the GL
    // context current on that surface is valid here.
    let context =
        unsafe { display.create_context(config, &context_attributes)? }.make_current(&surface)?;
    Ok((surface, context))
}

#[inline(always)]
fn surface_extent(width: u32, height: u32) -> (NonZeroU32, NonZeroU32) {
    (
        NonZeroU32::new(width.max(1)).expect("surface width is clamped to at least 1"),
        NonZeroU32::new(height.max(1)).expect("surface height is clamped to at least 1"),
    )
}

#[cfg(test)]
mod tests {
    use super::surface_extent;

    #[test]
    fn surface_extent_clamps_zero_dims() {
        assert_eq!(surface_extent(0, 0).0.get(), 1);
        assert_eq!(surface_extent(0, 0).1.get(), 1);
        assert_eq!(surface_extent(1920, 1080).0.get(), 1920);
        assert_eq!(surface_extent(1920, 1080).1.get(), 1080);
    }
}
