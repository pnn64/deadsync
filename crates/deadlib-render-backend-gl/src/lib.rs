use deadlib_render_core::{
    BlendMode, CameraUploadCache, DenseSlotMap, DrawOp, DrawStats, RenderFrame, RenderTargetFrame,
    SamplerDesc, SamplerFilter, SamplerWrap, SpriteInstanceRaw, TMeshCacheKey, TextureHandle,
    TexturedMeshBufferCache, TexturedMeshInstanceRaw, TexturedMeshUploads, TexturedMeshVertex,
    Yuv420Upload, draw_storage_stats, is_render_target_texture, render_target_base_handle,
    render_target_uses_nearest, resolve_textured_mesh_geometries, resolve_textured_meshes,
};
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
const LOGICAL_HEIGHT: f32 = 480.0;
const DESIGN_WIDTH_16_9: f32 = 854.0;
const MODERN_DESKTOP_GL: GlVersion = GlVersion { major: 3, minor: 3 };
const BASE_INSTANCE_DESKTOP_GL: GlVersion = GlVersion { major: 4, minor: 2 };

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GlVersion {
    major: u32,
    minor: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GlPath {
    Modern,
    Legacy,
}

impl GlPath {
    const fn label(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Legacy => "legacy GL2",
        }
    }
}

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

    const fn shaders(self, path: GlPath) -> ShaderSet {
        match (self, path) {
            (_, GlPath::Legacy) => ShaderSet {
                sprite_vert: include_str!("shaders/opengl_shader_legacy.vert"),
                sprite_frag: include_str!("shaders/opengl_shader_legacy.frag"),
                mesh_vert: include_str!("shaders/opengl_mesh_legacy.vert"),
                mesh_frag: include_str!("shaders/opengl_mesh_legacy.frag"),
                tmesh_vert: include_str!("shaders/opengl_tmesh_legacy.vert"),
                tmesh_frag: include_str!("shaders/opengl_tmesh_legacy.frag"),
            },
            (Self::Desktop, GlPath::Modern) => ShaderSet {
                sprite_vert: include_str!("shaders/opengl_shader.vert"),
                sprite_frag: include_str!("shaders/opengl_shader.frag"),
                mesh_vert: include_str!("shaders/opengl_mesh.vert"),
                mesh_frag: include_str!("shaders/opengl_mesh.frag"),
                tmesh_vert: include_str!("shaders/opengl_tmesh.vert"),
                tmesh_frag: include_str!("shaders/opengl_tmesh.frag"),
            },
            #[cfg(all(unix, not(target_os = "macos")))]
            (Self::Gles, GlPath::Modern) => ShaderSet {
                sprite_vert: include_str!("shaders/opengl_shader_gles.vert"),
                sprite_frag: include_str!("shaders/opengl_shader_gles.frag"),
                mesh_vert: include_str!("shaders/opengl_mesh_gles.vert"),
                mesh_frag: include_str!("shaders/opengl_mesh_gles.frag"),
                tmesh_vert: include_str!("shaders/opengl_tmesh_gles.vert"),
                tmesh_frag: include_str!("shaders/opengl_tmesh_gles.frag"),
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

const SPRITE_ATTRIBS: [(u32, &str); 12] = [
    (0, "a_pos"),
    (1, "a_tex_coord"),
    (2, "i_center"),
    (3, "i_size"),
    (4, "i_rot_sin_cos"),
    (5, "i_tint"),
    (6, "i_uv_scale"),
    (7, "i_uv_offset"),
    (8, "i_local_offset"),
    (9, "i_local_offset_rot_sin_cos"),
    (10, "i_edge_fade"),
    (11, "i_texture_mask"),
];

const MESH_ATTRIBS: [(u32, &str); 2] = [(0, "a_pos"), (1, "a_color")];

const TMESH_ATTRIBS: [(u32, &str); 13] = [
    (0, "a_pos"),
    (1, "a_uv"),
    (2, "a_color"),
    (3, "a_tex_matrix_scale"),
    (4, "i_model_col0"),
    (5, "i_model_col1"),
    (6, "i_model_col2"),
    (7, "i_model_col3"),
    (8, "i_tint"),
    (9, "i_uv_scale"),
    (10, "i_uv_offset"),
    (11, "i_uv_tex_shift"),
    (12, "i_texture_mask"),
];

// A handle to one RGBA texture or three planar video textures on the GPU.
#[derive(Debug)]
pub struct Texture(TextureImages);

#[derive(Debug)]
enum TextureImages {
    Rgba(glow::Texture),
    Yuv420 {
        planes: [glow::Texture; 3],
        levels: [f32; 4],
        coeffs: [f32; 4],
    },
}

impl Texture {
    #[inline(always)]
    const fn primary(&self) -> glow::Texture {
        match &self.0 {
            TextureImages::Rgba(texture) => *texture,
            TextureImages::Yuv420 { planes, .. } => planes[0],
        }
    }

    #[inline(always)]
    const fn is_yuv420(&self) -> bool {
        matches!(self.0, TextureImages::Yuv420 { .. })
    }
}

pub trait TextureLookup {
    fn opengl_texture(&self, handle: TextureHandle) -> Option<&Texture>;
}

struct CachedTMeshGeom {
    vbo: glow::Buffer,
    vertex_count: u32,
}

/// Render-thread-owned `ActorFrameTexture` storage. Slots are bounded by the
/// largest active graph and reused by position; steady-state gameplay only
/// binds and redraws them. Screen-content changes may replace a slot, while
/// cache pressure never prunes or destroys targets mid-song.
struct OffscreenTarget {
    handle: TextureHandle,
    width: u32,
    height: u32,
    texture: Texture,
    framebuffer: glow::Framebuffer,
    depth: glow::Renderbuffer,
    initialized: bool,
}

#[derive(Clone, Copy)]
struct LegacySpriteUniforms {
    center: UniformLocation,
    size: UniformLocation,
    rot_sin_cos: UniformLocation,
    tint: UniformLocation,
    uv_scale: UniformLocation,
    uv_offset: UniformLocation,
    local_offset: UniformLocation,
    local_offset_rot_sin_cos: UniformLocation,
    edge_fade: UniformLocation,
    texture_mask: UniformLocation,
}

#[derive(Clone, Copy)]
struct LegacyTMeshUniforms {
    model: UniformLocation,
    tint: UniformLocation,
    uv_scale: UniformLocation,
    uv_offset: UniformLocation,
    uv_tex_shift: UniformLocation,
    texture_mask: UniformLocation,
}

pub struct State {
    gl: glow::Context,
    gl_surface: Surface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    path: GlPath,
    base_instance: bool,
    program: glow::Program,
    mesh_program: glow::Program,
    tmesh_program: glow::Program,
    mvp_location: UniformLocation,
    mesh_mvp_location: UniformLocation,
    tmesh_mvp_location: UniformLocation,
    tmesh_texture_location: UniformLocation,
    texture_location: UniformLocation,
    texture_yuv_location: UniformLocation,
    yuv_levels_location: UniformLocation,
    yuv_coeffs_location: UniformLocation,
    legacy_sprite_uniforms: Option<LegacySpriteUniforms>,
    legacy_tmesh_uniforms: Option<LegacyTMeshUniforms>,
    projection: Matrix4,
    window_size: (u32, u32),
    // A single, shared set of buffers for a unit quad.
    shared_vao: Option<glow::VertexArray>,
    shared_vbo: glow::Buffer,
    shared_ibo: glow::Buffer,
    shared_instance_vbo: Option<glow::Buffer>,
    index_count: i32,
    mesh_vao: Option<glow::VertexArray>,
    mesh_vbo: glow::Buffer,
    tmesh_vao: Option<glow::VertexArray>,
    tmesh_vbo: glow::Buffer,
    tmesh_instance_vbo: Option<glow::Buffer>,
    uploads: TexturedMeshUploads,
    offscreen_targets: Vec<OffscreenTarget>,
    cached_tmesh: DenseSlotMap<CachedTMeshGeom>,
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
    let path = select_gl_path(&gl, api);
    info!("OpenGL render path: {}", path.label());
    let base_instance = supports_base_instance(&gl, api, path);
    info!(
        "OpenGL base-instance draws: {}",
        if base_instance { "enabled" } else { "disabled" }
    );
    let shaders = api.shaders(path);
    let (program, mvp_location, texture_location) =
        create_graphics_program(&gl, shaders.sprite_vert, shaders.sprite_frag)?;
    let texture_u_location = uniform_location(&gl, program, "u_texture_u")?;
    let texture_v_location = uniform_location(&gl, program, "u_texture_v")?;
    let texture_yuv_location = uniform_location(&gl, program, "u_yuv420")?;
    let yuv_levels_location = uniform_location(&gl, program, "u_yuv_levels")?;
    let yuv_coeffs_location = uniform_location(&gl, program, "u_yuv_coeffs")?;
    let (mesh_program, mesh_mvp_location) =
        create_mesh_program(&gl, shaders.mesh_vert, shaders.mesh_frag)?;
    let (tmesh_program, tmesh_mvp_location, tmesh_texture_location) =
        create_tmesh_program(&gl, shaders.tmesh_vert, shaders.tmesh_frag)?;
    let legacy_sprite_uniforms = if path == GlPath::Legacy {
        Some(legacy_sprite_uniforms(&gl, program)?)
    } else {
        None
    };
    let legacy_tmesh_uniforms = if path == GlPath::Legacy {
        Some(legacy_tmesh_uniforms(&gl, tmesh_program)?)
    } else {
        None
    };

    // Create shared static unit quad + index buffer.
    // SAFETY: the OpenGL context created above is current on this thread for the
    // duration of initialization, so creating and configuring these GL objects is
    // valid here.
    let (shared_vao, shared_vbo, shared_ibo, shared_instance_vbo, index_count) = unsafe {
        const UNIT_QUAD_VERTICES: [[f32; 4]; 4] = [
            [-0.5, -0.5, 0.0, 1.0],
            [0.5, -0.5, 1.0, 1.0],
            [0.5, 0.5, 1.0, 0.0],
            [-0.5, 0.5, 0.0, 0.0],
        ];
        const QUAD_INDICES: [u16; 6] = [0, 1, 2, 2, 3, 0];

        let vao = if path == GlPath::Modern {
            Some(gl.create_vertex_array()?)
        } else {
            None
        };
        let vbo = gl.create_buffer()?;
        let ibo = gl.create_buffer()?;
        let instance_vbo = if path == GlPath::Modern {
            Some(gl.create_buffer()?)
        } else {
            None
        };

        if let Some(vao) = vao {
            gl.bind_vertex_array(Some(vao));
        }

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

        if let Some(instance_vbo) = instance_vbo {
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
            gl.vertex_attrib_pointer_f32(
                4,
                2,
                glow::FLOAT,
                false,
                inst_stride,
                vec4_size + vec2_size,
            );
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
            gl.enable_vertex_attrib_array(11);
            gl.vertex_attrib_pointer_f32(
                11,
                1,
                glow::FLOAT,
                false,
                inst_stride,
                3 * vec4_size + 6 * vec2_size,
            );
            gl.vertex_attrib_divisor(11, 1);

            gl.bind_vertex_array(None);
        }

        (vao, vbo, ibo, instance_vbo, QUAD_INDICES.len() as i32)
    };

    // SAFETY: the OpenGL context is still current on this thread, so creating and
    // configuring the mesh VAO/VBO pair is valid here.
    let (mesh_vao, mesh_vbo) = unsafe {
        let vao = if path == GlPath::Modern {
            Some(gl.create_vertex_array()?)
        } else {
            None
        };
        let vbo = gl.create_buffer()?;

        if let Some(vao) = vao {
            gl.bind_vertex_array(Some(vao));
        }
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        if path == GlPath::Modern {
            // a_pos (location 0), a_color (location 1)
            let stride = std::mem::size_of::<deadlib_render_core::MeshVertex>() as i32;
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
        }
        (vao, vbo)
    };
    // SAFETY: the OpenGL context is still current on this thread, so creating and
    // configuring the textured-mesh VAO/VBO pair is valid here.
    let (tmesh_vao, tmesh_vbo, tmesh_instance_vbo) = unsafe {
        let vao = if path == GlPath::Modern {
            Some(gl.create_vertex_array()?)
        } else {
            None
        };
        let vbo = gl.create_buffer()?;
        let instance_vbo = if path == GlPath::Modern {
            Some(gl.create_buffer()?)
        } else {
            None
        };

        if let Some(vao) = vao {
            gl.bind_vertex_array(Some(vao));
        }
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        if let Some(instance_vbo) = instance_vbo {
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
            // i_uv_scale/i_uv_offset/i_uv_tex_shift/i_texture_mask (9..12)
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
            gl.enable_vertex_attrib_array(12);
            gl.vertex_attrib_pointer_f32(
                12,
                1,
                glow::FLOAT,
                false,
                inst_stride,
                5 * col_size + 3 * uv_size,
            );
            gl.vertex_attrib_divisor(12, 1);

            gl.bind_vertex_array(None);
        }
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
        // This backend is the sole owner of the context. All texture uploads use
        // tightly packed RGBA or single-channel plane slices, so establish their
        // invariant unpack state once instead of issuing four driver calls for
        // every create/update.
        gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
        gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_ROWS, 0);
        gl.pixel_store_i32(glow::UNPACK_SKIP_PIXELS, 0);
        gl.use_program(Some(program));
        gl.active_texture(glow::TEXTURE0);
        gl.uniform_1_i32(Some(&texture_location), 0);
        gl.uniform_1_i32(Some(&texture_u_location), 1);
        gl.uniform_1_i32(Some(&texture_v_location), 2);
        gl.uniform_1_i32(Some(&texture_yuv_location), 0);
        gl.use_program(None);
    }

    let state = State {
        gl,
        gl_surface,
        gl_context,
        path,
        base_instance,
        program,
        mesh_program,
        tmesh_program,
        mvp_location,
        mesh_mvp_location,
        tmesh_mvp_location,
        tmesh_texture_location,
        texture_location,
        texture_yuv_location,
        yuv_levels_location,
        yuv_coeffs_location,
        legacy_sprite_uniforms,
        legacy_tmesh_uniforms,
        projection,
        window_size: (initial_width, initial_height),
        shared_vao,
        shared_vbo,
        shared_ibo,
        shared_instance_vbo,
        index_count,
        mesh_vao,
        mesh_vbo,
        tmesh_vao,
        tmesh_vbo,
        tmesh_instance_vbo,
        uploads: TexturedMeshUploads::with_capacity(1024, 64),
        offscreen_targets: Vec::with_capacity(4),
        cached_tmesh: DenseSlotMap::with_capacity(256),
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

fn select_gl_path(gl: &glow::Context, api: GlApi) -> GlPath {
    // SAFETY: driver string queries only read state from the current OpenGL
    // context and do not retain Rust pointers.
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    if api == GlApi::Desktop && parse_gl_version(&version).is_some_and(|v| v < MODERN_DESKTOP_GL) {
        return GlPath::Legacy;
    }
    GlPath::Modern
}

fn supports_base_instance(gl: &glow::Context, api: GlApi, path: GlPath) -> bool {
    // SAFETY: the driver string query only reads state from the current OpenGL
    // context and does not retain Rust pointers.
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    base_instance_capability(
        api,
        path,
        parse_gl_version(&version),
        gl.supported_extensions().contains("GL_ARB_base_instance"),
    )
}

fn base_instance_capability(
    api: GlApi,
    path: GlPath,
    version: Option<GlVersion>,
    has_arb_extension: bool,
) -> bool {
    api == GlApi::Desktop
        && matches!(path, GlPath::Modern)
        && (matches!(version, Some(version) if version.major > BASE_INSTANCE_DESKTOP_GL.major
            || (version.major == BASE_INSTANCE_DESKTOP_GL.major
                && version.minor >= BASE_INSTANCE_DESKTOP_GL.minor))
            || has_arb_extension)
}

fn parse_gl_version(version: &str) -> Option<GlVersion> {
    let start = version.find(|c: char| c.is_ascii_digit())?;
    let mut parts = version[start..].splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor_part = parts.next()?;
    let minor_end = minor_part
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(minor_part.len());
    if minor_end == 0 {
        return None;
    }
    Some(GlVersion {
        major,
        minor: minor_part[..minor_end].parse().ok()?,
    })
}

pub fn create_texture(
    state: &State,
    image: &RgbaImage,
    sampler: SamplerDesc,
) -> Result<Texture, String> {
    create_gl_texture(
        &state.gl,
        image.width(),
        image.height(),
        glow::RGBA8,
        glow::RGBA,
        image.as_raw(),
        sampler,
    )
    .map(|texture| Texture(TextureImages::Rgba(texture)))
}

fn create_gl_texture(
    gl: &glow::Context,
    width: u32,
    height: u32,
    internal: u32,
    format: u32,
    raw: &[u8],
    sampler: SamplerDesc,
) -> Result<glow::Texture, String> {
    if width == 0 || height == 0 {
        return Err("OpenGL textures must have non-zero dimensions".to_string());
    }
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

        let w = width as i32;
        let h = height as i32;

        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            internal as i32,
            w,
            h,
            0,
            format,
            glow::UNSIGNED_BYTE,
            PixelUnpackData::Slice(Some(raw)),
        );
        Ok(t)
    }
}

pub fn update_texture(state: &State, texture: &Texture, image: &RgbaImage) -> Result<(), String> {
    let gl = &state.gl;
    let w = i32::try_from(image.width()).map_err(|_| "texture width overflow".to_string())?;
    let h = i32::try_from(image.height()).map_err(|_| "texture height overflow".to_string())?;
    let raw = image.as_raw();
    // SAFETY: `texture.0` is a live OpenGL texture handle owned by this backend,
    // and `raw` exposes initialized RGBA bytes that stay alive for the duration of
    // the update call.
    unsafe {
        let TextureImages::Rgba(raw_texture) = &texture.0 else {
            return Err("cannot upload RGBA into YUV420 texture".to_string());
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(*raw_texture));
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
    }
    Ok(())
}

pub fn create_yuv420_texture(
    state: &State,
    upload: Yuv420Upload<'_>,
    sampler: SamplerDesc,
) -> Result<Texture, String> {
    if !upload.is_valid() {
        return Err("invalid YUV420 planes".to_string());
    }
    let (internal, format) = if state.path == GlPath::Legacy {
        (glow::LUMINANCE, glow::LUMINANCE)
    } else {
        (glow::R8, glow::RED)
    };
    let create = |width, height, pixels| {
        create_gl_texture(&state.gl, width, height, internal, format, pixels, sampler)
    };
    let y_texture = create(upload.width, upload.height, upload.y)?;
    let u_texture = match create(upload.width / 2, upload.height / 2, upload.u) {
        Ok(texture) => texture,
        Err(error) => {
            // SAFETY: the first plane was created by this live context and has
            // not been published to another owner.
            unsafe { state.gl.delete_texture(y_texture) };
            return Err(error);
        }
    };
    let v_texture = match create(upload.width / 2, upload.height / 2, upload.v) {
        Ok(texture) => texture,
        Err(error) => {
            // SAFETY: both planes were created by this live context and have
            // not been published to another owner.
            unsafe {
                state.gl.delete_texture(y_texture);
                state.gl.delete_texture(u_texture);
            }
            return Err(error);
        }
    };
    Ok(Texture(TextureImages::Yuv420 {
        planes: [y_texture, u_texture, v_texture],
        levels: upload.levels,
        coeffs: upload.coeffs,
    }))
}

pub fn update_yuv420_texture(
    state: &State,
    texture: &mut Texture,
    upload: Yuv420Upload<'_>,
) -> Result<(), String> {
    if !upload.is_valid() {
        return Err("invalid YUV420 planes".to_string());
    }
    let TextureImages::Yuv420 {
        planes,
        levels: texture_levels,
        coeffs: texture_coeffs,
    } = &mut texture.0
    else {
        return Err("cannot upload YUV420 into RGBA texture".to_string());
    };
    let format = if state.path == GlPath::Legacy {
        glow::LUMINANCE
    } else {
        glow::RED
    };
    // SAFETY: all three handles are live R8 textures owned by this backend, and
    // each source slice remains valid for the duration of its upload call.
    unsafe {
        for (texture, plane, plane_width, plane_height) in [
            (planes[0], upload.y, upload.width, upload.height),
            (planes[1], upload.u, upload.width / 2, upload.height / 2),
            (planes[2], upload.v, upload.width / 2, upload.height / 2),
        ] {
            state.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            state.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                plane_width as i32,
                plane_height as i32,
                format,
                glow::UNSIGNED_BYTE,
                PixelUnpackData::Slice(Some(plane)),
            );
        }
    }
    *texture_levels = upload.levels;
    *texture_coeffs = upload.coeffs;
    Ok(())
}

#[inline(always)]
#[must_use]
pub const fn texture_is_yuv420(texture: &Texture) -> bool {
    texture.is_yuv420()
}

pub fn delete_texture(state: &State, texture: &Texture) {
    // SAFETY: `texture.0` was created by this OpenGL backend, and the caller
    // only asks the live owning backend state to delete it.
    unsafe {
        match &texture.0 {
            TextureImages::Rgba(texture) => state.gl.delete_texture(*texture),
            TextureImages::Yuv420 { planes, .. } => {
                for texture in planes {
                    state.gl.delete_texture(*texture);
                }
            }
        }
    }
}

fn create_offscreen_target(
    gl: &glow::Context,
    handle: TextureHandle,
    width: u32,
    height: u32,
) -> Result<OffscreenTarget, String> {
    let width = width.max(1);
    let height = height.max(1);
    // SAFETY: all resources are created and attached on the current context;
    // no Rust pointers survive these calls.
    unsafe {
        let raw = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(raw));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            PixelUnpackData::Slice(None),
        );
        let framebuffer = gl.create_framebuffer()?;
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(raw),
            0,
        );
        let depth = gl.create_renderbuffer()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            glow::DEPTH_COMPONENT24,
            width as i32,
            height as i32,
        );
        gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_ATTACHMENT,
            glow::RENDERBUFFER,
            Some(depth),
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_renderbuffer(depth);
            gl.delete_framebuffer(framebuffer);
            gl.delete_texture(raw);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            return Err("OpenGL ActorFrameTexture framebuffer is incomplete".to_string());
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        Ok(OffscreenTarget {
            handle,
            width,
            height,
            texture: Texture(TextureImages::Rgba(raw)),
            framebuffer,
            depth,
            initialized: false,
        })
    }
}

fn destroy_offscreen_target(gl: &glow::Context, target: OffscreenTarget) {
    // SAFETY: these resources were created by this context and remain uniquely
    // owned by the target slot.
    unsafe {
        gl.delete_texture(target.texture.primary());
        gl.delete_framebuffer(target.framebuffer);
        gl.delete_renderbuffer(target.depth);
    }
}

fn ensure_offscreen_targets(state: &mut State, frame: &RenderFrame) -> Result<(), String> {
    for (index, pass) in frame.render_targets.iter().enumerate() {
        let matches = state.offscreen_targets.get(index).is_some_and(|target| {
            target.handle == pass.texture_handle
                && target.width == pass.width.max(1)
                && target.height == pass.height.max(1)
        });
        if matches {
            continue;
        }
        let target =
            create_offscreen_target(&state.gl, pass.texture_handle, pass.width, pass.height)?;
        if index < state.offscreen_targets.len() {
            let old = std::mem::replace(&mut state.offscreen_targets[index], target);
            destroy_offscreen_target(&state.gl, old);
        } else {
            state.offscreen_targets.push(target);
        }
    }
    Ok(())
}

fn resolved_texture<'a, T: TextureLookup + ?Sized>(
    state: &'a State,
    textures: &'a T,
    handle: TextureHandle,
) -> Option<&'a Texture> {
    if is_render_target_texture(handle) {
        let handle = render_target_base_handle(handle);
        return state
            .offscreen_targets
            .iter()
            .find(|target| target.handle == handle)
            .map(|target| &target.texture);
    }
    textures.opengl_texture(handle)
}

fn apply_render_target_filter(gl: &glow::Context, handle: TextureHandle) {
    if !is_render_target_texture(handle) {
        return;
    }
    let filter = if render_target_uses_nearest(handle) {
        glow::NEAREST
    } else {
        glow::LINEAR
    };
    // SAFETY: callers invoke this immediately after binding the referenced AFT
    // texture on the live render context.
    unsafe {
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter as i32);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter as i32);
    }
}

fn bind_sprite_texture(state: &State, texture: &Texture) {
    // SAFETY: every handle belongs to the current context and the sprite
    // program is active at both call sites. Texture units are restored to zero
    // so the mesh paths retain their existing binding contract.
    unsafe {
        state.gl.active_texture(glow::TEXTURE0);
        match &texture.0 {
            TextureImages::Rgba(raw) => {
                state.gl.bind_texture(glow::TEXTURE_2D, Some(*raw));
                state.gl.uniform_1_i32(Some(&state.texture_yuv_location), 0);
            }
            TextureImages::Yuv420 {
                planes: [y, u, v],
                levels,
                coeffs,
            } => {
                state.gl.bind_texture(glow::TEXTURE_2D, Some(*y));
                state.gl.active_texture(glow::TEXTURE1);
                state.gl.bind_texture(glow::TEXTURE_2D, Some(*u));
                state.gl.active_texture(glow::TEXTURE2);
                state.gl.bind_texture(glow::TEXTURE_2D, Some(*v));
                state.gl.active_texture(glow::TEXTURE0);
                state.gl.uniform_1_i32(Some(&state.texture_yuv_location), 1);
                state
                    .gl
                    .uniform_4_f32_slice(Some(&state.yuv_levels_location), levels);
                state
                    .gl
                    .uniform_4_f32_slice(Some(&state.yuv_coeffs_location), coeffs);
            }
        }
    }
}

fn ensure_cached_tmesh(
    gl: &glow::Context,
    cached_tmesh: &mut DenseSlotMap<CachedTMeshGeom>,
    cached_tmesh_bytes: &mut usize,
    cache_key: TMeshCacheKey,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
) -> Option<u64> {
    if let Some((buffer_key, entry)) = cached_tmesh.get(cache_key) {
        return (entry.vertex_count == vertices.len() as u32).then_some(buffer_key);
    }

    let bytes = std::mem::size_of_val(vertices);
    if bytes > OPENGL_TMESH_CACHE_MAX_BYTES
        || cached_tmesh_bytes.saturating_add(bytes) > OPENGL_TMESH_CACHE_MAX_BYTES
    {
        return None;
    }

    // SAFETY: the OpenGL context is current on this thread while draw prep runs,
    // and the uploaded slice remains alive for the duration of the call.
    let vbo = unsafe {
        let Ok(vbo) = gl.create_buffer() else {
            return None;
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

    let buffer_key = cached_tmesh.insert(
        cache_key,
        CachedTMeshGeom {
            vbo,
            vertex_count: vertices.len() as u32,
        },
    );
    *cached_tmesh_bytes = cached_tmesh_bytes.saturating_add(bytes);
    Some(buffer_key)
}

#[inline(always)]
pub const fn request_screenshot(state: &mut State) {
    state.screenshot_requested = true;
}

fn draw_modern_offscreen_pass(
    state: &State,
    frame: &deadlib_render_core::RenderTargetFrame,
    textures: &impl TextureLookup,
) -> u32 {
    // SAFETY: the backend owns a current context for the whole draw call; every
    // GL handle below belongs to it and all uploaded slices remain borrowed
    // until their immediate driver call returns.
    unsafe {
        let gl = &state.gl;
        let shared_vao = state
            .shared_vao
            .expect("modern OpenGL creates a sprite VAO");
        let shared_instance_vbo = state
            .shared_instance_vbo
            .expect("modern OpenGL creates a sprite instance VBO");
        let mesh_vao = state.mesh_vao.expect("modern OpenGL creates a mesh VAO");
        let tmesh_vao = state
            .tmesh_vao
            .expect("modern OpenGL creates a textured mesh VAO");
        let tmesh_instance_vbo = state
            .tmesh_instance_vbo
            .expect("modern OpenGL creates a textured mesh instance VBO");

        if !frame.sprite_instances.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(shared_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&frame.sprite_instances),
                glow::DYNAMIC_DRAW,
            );
        }
        if !frame.mesh_vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&frame.mesh_vertices),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.uploads.vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&state.uploads.vertices),
                glow::DYNAMIC_DRAW,
            );
        }
        if !frame.tmesh_instances.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(tmesh_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&frame.tmesh_instances),
                glow::DYNAMIC_DRAW,
            );
        }

        let apply_blend = |want: BlendMode, last: &mut Option<BlendMode>| {
            if *last == Some(want) {
                return;
            }
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
            *last = Some(want);
        };
        let apply_depth = |want: bool, last: &mut Option<bool>| {
            if *last == Some(want) {
                return;
            }
            if want {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.depth_mask(true);
            } else {
                gl.depth_mask(false);
                gl.disable(glow::DEPTH_TEST);
            }
            *last = Some(want);
        };
        let flipped_camera = |camera: u8| {
            Matrix4::from_scale(glam::Vec3::new(1.0, -1.0, 1.0))
                * frame
                    .cameras
                    .get(camera as usize)
                    .copied()
                    .unwrap_or(state.projection)
        };

        let mut vertices = 0u32;
        let mut last_bound_tex = None;
        let mut last_blend = None;
        let mut last_prog = None;
        let mut last_cameras = [CameraUploadCache::default(); 3];
        let mut last_sprite_instance_start = None;
        let mut last_tmesh_instance_start = None;
        let mut tmesh_buffer_cache = TexturedMeshBufferCache::default();
        let mut last_depth = None;
        gl.active_texture(glow::TEXTURE0);

        for op in frame.ops.iter().copied() {
            match op {
                DrawOp::Sprite(run) => {
                    apply_blend(run.blend, &mut last_blend);
                    apply_depth(false, &mut last_depth);
                    if last_prog != Some(0) {
                        gl.use_program(Some(state.program));
                        gl.bind_vertex_array(Some(shared_vao));
                        gl.uniform_1_i32(Some(&state.texture_location), 0);
                        last_prog = Some(0);
                        last_sprite_instance_start = None;
                        tmesh_buffer_cache.reset();
                    }
                    if !state.base_instance
                        && last_sprite_instance_start != Some(run.instance_start)
                    {
                        let stride = mem::size_of::<SpriteInstanceRaw>() as i32;
                        let v2 = (2 * mem::size_of::<f32>()) as i32;
                        let v4 = (4 * mem::size_of::<f32>()) as i32;
                        let base = run.instance_start as i32 * stride;
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(shared_instance_vbo));
                        for (location, size, offset) in [
                            (2, 4, 0),
                            (3, 2, v4),
                            (4, 2, v4 + v2),
                            (5, 4, v4 + 2 * v2),
                            (6, 2, 2 * v4 + 2 * v2),
                            (7, 2, 2 * v4 + 3 * v2),
                            (8, 2, 2 * v4 + 4 * v2),
                            (9, 2, 2 * v4 + 5 * v2),
                            (10, 4, 2 * v4 + 6 * v2),
                            (11, 1, 3 * v4 + 6 * v2),
                        ] {
                            gl.vertex_attrib_pointer_f32(
                                location,
                                size,
                                glow::FLOAT,
                                false,
                                stride,
                                base + offset,
                            );
                        }
                        last_sprite_instance_start = Some(run.instance_start);
                    }
                    if last_cameras[0].update_required(run.camera) {
                        let matrix = flipped_camera(run.camera).to_cols_array_2d();
                        gl.uniform_matrix_4_f32_slice(
                            Some(&state.mvp_location),
                            false,
                            bytemuck::cast_slice(&matrix),
                        );
                    }
                    let Some(texture) = resolved_texture(state, textures, run.texture_handle)
                    else {
                        continue;
                    };
                    if last_bound_tex != Some(texture.primary()) {
                        bind_sprite_texture(state, texture);
                        last_bound_tex = Some(texture.primary());
                    }
                    apply_render_target_filter(gl, run.texture_handle);
                    if state.base_instance {
                        gl.draw_elements_instanced_base_vertex_base_instance(
                            glow::TRIANGLES,
                            state.index_count,
                            glow::UNSIGNED_SHORT,
                            0,
                            run.instance_count as i32,
                            0,
                            run.instance_start,
                        );
                    } else {
                        gl.draw_elements_instanced(
                            glow::TRIANGLES,
                            state.index_count,
                            glow::UNSIGNED_SHORT,
                            0,
                            run.instance_count as i32,
                        );
                    }
                    vertices = vertices.saturating_add(4 * run.instance_count);
                }
                DrawOp::Mesh(run) => {
                    if run.vertex_count == 0 {
                        continue;
                    }
                    apply_blend(run.blend, &mut last_blend);
                    apply_depth(false, &mut last_depth);
                    if last_prog != Some(1) {
                        gl.use_program(Some(state.mesh_program));
                        gl.bind_vertex_array(Some(mesh_vao));
                        last_prog = Some(1);
                        tmesh_buffer_cache.reset();
                    }
                    if last_cameras[1].update_required(run.camera) {
                        let matrix = flipped_camera(run.camera).to_cols_array_2d();
                        gl.uniform_matrix_4_f32_slice(
                            Some(&state.mesh_mvp_location),
                            false,
                            bytemuck::cast_slice(&matrix),
                        );
                    }
                    gl.draw_arrays(
                        glow::TRIANGLES,
                        run.vertex_start as i32,
                        run.vertex_count as i32,
                    );
                    vertices = vertices.saturating_add(run.vertex_count);
                }
                DrawOp::TexturedMesh(run) => {
                    let Some(source) = state.uploads.source(run.geometry) else {
                        continue;
                    };
                    apply_blend(run.blend, &mut last_blend);
                    apply_depth(run.depth_test, &mut last_depth);
                    if last_prog != Some(2) {
                        gl.use_program(Some(state.tmesh_program));
                        gl.bind_vertex_array(Some(tmesh_vao));
                        gl.uniform_1_i32(Some(&state.tmesh_texture_location), 0);
                        last_prog = Some(2);
                        last_tmesh_instance_start = None;
                        tmesh_buffer_cache.reset();
                    }
                    if tmesh_buffer_cache.update_required(source) {
                        let stride = mem::size_of::<TexturedMeshVertex>() as i32;
                        let vbo = source.buffer_key().and_then(|key| {
                            state.cached_tmesh.get_slot(key).map(|entry| entry.vbo)
                        });
                        let Some(vbo) = vbo
                            .or_else(|| source.buffer_key().is_none().then_some(state.tmesh_vbo))
                        else {
                            tmesh_buffer_cache.reset();
                            continue;
                        };
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
                        for (location, size, offset) in [
                            (0, 3, 0),
                            (1, 2, 3 * mem::size_of::<f32>() as i32),
                            (2, 4, 5 * mem::size_of::<f32>() as i32),
                            (3, 2, 9 * mem::size_of::<f32>() as i32),
                        ] {
                            gl.vertex_attrib_pointer_f32(
                                location,
                                size,
                                glow::FLOAT,
                                false,
                                stride,
                                offset,
                            );
                        }
                    }
                    if !state.base_instance && last_tmesh_instance_start != Some(run.instance_start)
                    {
                        let stride = mem::size_of::<TexturedMeshInstanceRaw>() as i32;
                        let col = (4 * mem::size_of::<f32>()) as i32;
                        let uv = (2 * mem::size_of::<f32>()) as i32;
                        let base = run.instance_start as i32 * stride;
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(tmesh_instance_vbo));
                        for (location, size, offset) in [
                            (4, 4, 0),
                            (5, 4, col),
                            (6, 4, 2 * col),
                            (7, 4, 3 * col),
                            (8, 4, 4 * col),
                            (9, 2, 5 * col),
                            (10, 2, 5 * col + uv),
                            (11, 2, 5 * col + 2 * uv),
                            (12, 1, 5 * col + 3 * uv),
                        ] {
                            gl.vertex_attrib_pointer_f32(
                                location,
                                size,
                                glow::FLOAT,
                                false,
                                stride,
                                base + offset,
                            );
                        }
                        last_tmesh_instance_start = Some(run.instance_start);
                    }
                    if last_cameras[2].update_required(run.camera) {
                        let matrix = flipped_camera(run.camera).to_cols_array_2d();
                        gl.uniform_matrix_4_f32_slice(
                            Some(&state.tmesh_mvp_location),
                            false,
                            bytemuck::cast_slice(&matrix),
                        );
                    }
                    let Some(texture) =
                        resolved_texture(state, textures, run.texture_handle).map(Texture::primary)
                    else {
                        continue;
                    };
                    if last_bound_tex != Some(texture) {
                        gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                        last_bound_tex = Some(texture);
                    }
                    apply_render_target_filter(gl, run.texture_handle);
                    let start = source.vertex_start() as i32;
                    let count = source.vertex_count() as i32;
                    if state.base_instance {
                        gl.draw_arrays_instanced_base_instance(
                            glow::TRIANGLES,
                            start,
                            count,
                            run.instance_count as i32,
                            run.instance_start,
                        );
                    } else {
                        gl.draw_arrays_instanced(
                            glow::TRIANGLES,
                            start,
                            count,
                            run.instance_count as i32,
                        );
                    }
                    vertices = vertices.saturating_add(
                        (source.vertex_count() / 3).saturating_mul(run.instance_count),
                    );
                }
            }
        }
        apply_depth(false, &mut last_depth);
        gl.bind_vertex_array(None);
        gl.use_program(None);
        vertices
    }
}

fn draw_legacy_offscreen_pass(
    state: &State,
    frame: &RenderTargetFrame,
    textures: &impl TextureLookup,
) -> u32 {
    let gl = &state.gl;
    let sprite_uniforms = state
        .legacy_sprite_uniforms
        .expect("legacy OpenGL path creates sprite uniforms");
    let tmesh_uniforms = state
        .legacy_tmesh_uniforms
        .expect("legacy OpenGL path creates textured mesh uniforms");
    let camera = |index: u8| {
        Matrix4::from_scale(glam::Vec3::new(1.0, -1.0, 1.0))
            * frame
                .cameras
                .get(index as usize)
                .copied()
                .unwrap_or(state.projection)
    };
    let apply_blend = |mode: BlendMode| {
        // SAFETY: these calls only mutate state on the current context.
        unsafe {
            gl.enable(glow::BLEND);
            match mode {
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
    };
    let apply_depth = |enabled: bool| {
        // SAFETY: these calls only mutate state on the current context.
        unsafe {
            if enabled {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.depth_mask(true);
            } else {
                gl.depth_mask(false);
                gl.disable(glow::DEPTH_TEST);
            }
        }
    };

    // SAFETY: all buffers, programs, textures, and uniform locations belong to
    // the current context and borrowed slices live through their upload calls.
    unsafe {
        if !frame.mesh_vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&frame.mesh_vertices),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.uploads.vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&state.uploads.vertices),
                glow::DYNAMIC_DRAW,
            );
        }
        gl.active_texture(glow::TEXTURE0);
        let mut vertices = 0u32;
        for op in frame.ops.iter().copied() {
            match op {
                DrawOp::Sprite(run) => {
                    let Some(texture) = resolved_texture(state, textures, run.texture_handle)
                    else {
                        continue;
                    };
                    apply_blend(run.blend);
                    apply_depth(false);
                    gl.use_program(Some(state.program));
                    gl.uniform_1_i32(Some(&state.texture_location), 0);
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.shared_vbo));
                    gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(state.shared_ibo));
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
                    gl.disable_vertex_attrib_array(2);
                    gl.disable_vertex_attrib_array(3);
                    let projection = camera(run.camera).to_cols_array_2d();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.mvp_location),
                        false,
                        bytemuck::cast_slice(&projection),
                    );
                    bind_sprite_texture(state, texture);
                    apply_render_target_filter(gl, run.texture_handle);
                    let end = run.instance_start.saturating_add(run.instance_count);
                    for index in run.instance_start..end {
                        let Some(instance) = frame.sprite_instances.get(index as usize) else {
                            continue;
                        };
                        gl.uniform_4_f32(
                            Some(&sprite_uniforms.center),
                            instance.center[0],
                            instance.center[1],
                            instance.center[2],
                            instance.center[3],
                        );
                        gl.uniform_2_f32(
                            Some(&sprite_uniforms.size),
                            instance.size[0],
                            instance.size[1],
                        );
                        gl.uniform_2_f32(
                            Some(&sprite_uniforms.rot_sin_cos),
                            instance.rot_sin_cos[0],
                            instance.rot_sin_cos[1],
                        );
                        gl.uniform_4_f32(
                            Some(&sprite_uniforms.tint),
                            instance.tint[0],
                            instance.tint[1],
                            instance.tint[2],
                            instance.tint[3],
                        );
                        gl.uniform_2_f32(
                            Some(&sprite_uniforms.uv_scale),
                            instance.uv_scale[0],
                            instance.uv_scale[1],
                        );
                        gl.uniform_2_f32(
                            Some(&sprite_uniforms.uv_offset),
                            instance.uv_offset[0],
                            instance.uv_offset[1],
                        );
                        gl.uniform_2_f32(
                            Some(&sprite_uniforms.local_offset),
                            instance.local_offset[0],
                            instance.local_offset[1],
                        );
                        gl.uniform_2_f32(
                            Some(&sprite_uniforms.local_offset_rot_sin_cos),
                            instance.local_offset_rot_sin_cos[0],
                            instance.local_offset_rot_sin_cos[1],
                        );
                        gl.uniform_4_f32(
                            Some(&sprite_uniforms.edge_fade),
                            instance.edge_fade[0],
                            instance.edge_fade[1],
                            instance.edge_fade[2],
                            instance.edge_fade[3],
                        );
                        gl.uniform_1_f32(
                            Some(&sprite_uniforms.texture_mask),
                            instance.texture_mask,
                        );
                        gl.draw_elements(
                            glow::TRIANGLES,
                            state.index_count,
                            glow::UNSIGNED_SHORT,
                            0,
                        );
                        vertices = vertices.saturating_add(4);
                    }
                }
                DrawOp::Mesh(run) => {
                    if run.vertex_count == 0 {
                        continue;
                    }
                    apply_blend(run.blend);
                    apply_depth(false);
                    gl.use_program(Some(state.mesh_program));
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
                    let stride = mem::size_of::<deadlib_render_core::MeshVertex>() as i32;
                    gl.enable_vertex_attrib_array(0);
                    gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
                    gl.enable_vertex_attrib_array(1);
                    gl.vertex_attrib_pointer_f32(
                        1,
                        4,
                        glow::FLOAT,
                        false,
                        stride,
                        (2 * mem::size_of::<f32>()) as i32,
                    );
                    gl.disable_vertex_attrib_array(2);
                    gl.disable_vertex_attrib_array(3);
                    let projection = camera(run.camera).to_cols_array_2d();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.mesh_mvp_location),
                        false,
                        bytemuck::cast_slice(&projection),
                    );
                    gl.draw_arrays(
                        glow::TRIANGLES,
                        run.vertex_start as i32,
                        run.vertex_count as i32,
                    );
                    vertices = vertices.saturating_add(run.vertex_count);
                }
                DrawOp::TexturedMesh(run) => {
                    let Some(source) = state.uploads.source(run.geometry) else {
                        continue;
                    };
                    let Some(texture) =
                        resolved_texture(state, textures, run.texture_handle).map(Texture::primary)
                    else {
                        continue;
                    };
                    apply_blend(run.blend);
                    apply_depth(run.depth_test);
                    gl.use_program(Some(state.tmesh_program));
                    gl.uniform_1_i32(Some(&state.tmesh_texture_location), 0);
                    gl.enable_vertex_attrib_array(0);
                    gl.enable_vertex_attrib_array(1);
                    gl.enable_vertex_attrib_array(2);
                    gl.enable_vertex_attrib_array(3);
                    let stride = mem::size_of::<TexturedMeshVertex>() as i32;
                    let buffer = if let Some(key) = source.buffer_key() {
                        state.cached_tmesh.get_slot(key).map(|entry| entry.vbo)
                    } else {
                        Some(state.tmesh_vbo)
                    };
                    let Some(buffer) = buffer else {
                        continue;
                    };
                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
                    gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
                    gl.vertex_attrib_pointer_f32(
                        1,
                        2,
                        glow::FLOAT,
                        false,
                        stride,
                        (3 * mem::size_of::<f32>()) as i32,
                    );
                    gl.vertex_attrib_pointer_f32(
                        2,
                        4,
                        glow::FLOAT,
                        false,
                        stride,
                        (5 * mem::size_of::<f32>()) as i32,
                    );
                    gl.vertex_attrib_pointer_f32(
                        3,
                        2,
                        glow::FLOAT,
                        false,
                        stride,
                        (9 * mem::size_of::<f32>()) as i32,
                    );
                    let projection = camera(run.camera).to_cols_array_2d();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.tmesh_mvp_location),
                        false,
                        bytemuck::cast_slice(&projection),
                    );
                    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                    apply_render_target_filter(gl, run.texture_handle);
                    let start = source.vertex_start() as i32;
                    let count = source.vertex_count() as i32;
                    let triangles = source.vertex_count() / 3;
                    let end = run.instance_start.saturating_add(run.instance_count);
                    for index in run.instance_start..end {
                        let Some(instance) = frame.tmesh_instances.get(index as usize) else {
                            continue;
                        };
                        let model = [
                            instance.model_col0[0],
                            instance.model_col0[1],
                            instance.model_col0[2],
                            instance.model_col0[3],
                            instance.model_col1[0],
                            instance.model_col1[1],
                            instance.model_col1[2],
                            instance.model_col1[3],
                            instance.model_col2[0],
                            instance.model_col2[1],
                            instance.model_col2[2],
                            instance.model_col2[3],
                            instance.model_col3[0],
                            instance.model_col3[1],
                            instance.model_col3[2],
                            instance.model_col3[3],
                        ];
                        gl.uniform_matrix_4_f32_slice(Some(&tmesh_uniforms.model), false, &model);
                        gl.uniform_4_f32(
                            Some(&tmesh_uniforms.tint),
                            instance.tint[0],
                            instance.tint[1],
                            instance.tint[2],
                            instance.tint[3],
                        );
                        gl.uniform_2_f32(
                            Some(&tmesh_uniforms.uv_scale),
                            instance.uv_scale[0],
                            instance.uv_scale[1],
                        );
                        gl.uniform_2_f32(
                            Some(&tmesh_uniforms.uv_offset),
                            instance.uv_offset[0],
                            instance.uv_offset[1],
                        );
                        gl.uniform_2_f32(
                            Some(&tmesh_uniforms.uv_tex_shift),
                            instance.uv_tex_shift[0],
                            instance.uv_tex_shift[1],
                        );
                        gl.uniform_1_f32(Some(&tmesh_uniforms.texture_mask), instance.texture_mask);
                        gl.draw_arrays(glow::TRIANGLES, start, count);
                        vertices = vertices.saturating_add(triangles);
                    }
                }
            }
        }
        apply_depth(false);
        gl.use_program(None);
        vertices
    }
}

pub fn draw(
    state: &mut State,
    frame: &RenderFrame,
    textures: &impl TextureLookup,
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

    #[inline(always)]
    fn apply_depth_test(gl: &glow::Context, want: bool, last: &mut Option<bool>) {
        if *last == Some(want) {
            return;
        }
        // SAFETY: depth-state calls only mutate GL state on the current context and
        // do not retain Rust pointers.
        unsafe {
            if want {
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.depth_mask(true);
            } else {
                gl.depth_mask(false);
                gl.disable(glow::DEPTH_TEST);
            }
        }
        *last = Some(want);
    }

    ensure_offscreen_targets(state, frame).map_err(std::io::Error::other)?;
    let offscreen_started = Instant::now();
    let mut offscreen_vertices = 0u32;
    for (index, target_frame) in frame.render_targets.iter().enumerate() {
        {
            let uploads = &mut state.uploads;
            let gl = &state.gl;
            let cached_tmesh = &mut state.cached_tmesh;
            let cached_tmesh_bytes = &mut state.cached_tmesh_bytes;
            resolve_textured_mesh_geometries(
                &target_frame.tmesh_geometries,
                uploads,
                |cache_key, vertices| {
                    ensure_cached_tmesh(gl, cached_tmesh, cached_tmesh_bytes, cache_key, vertices)
                },
            );
        }
        let target = &state.offscreen_targets[index];
        // SAFETY: this framebuffer belongs to the current context and remains
        // alive for the whole pass.
        unsafe {
            state
                .gl
                .bind_framebuffer(glow::FRAMEBUFFER, Some(target.framebuffer));
            state
                .gl
                .viewport(0, 0, target.width as i32, target.height as i32);
            state.gl.color_mask(true, true, true, true);
            state.gl.clear_depth(1.0);
            state.gl.depth_mask(true);
            let mut clear = glow::DEPTH_BUFFER_BIT;
            if !target_frame.preserve || !target.initialized {
                state
                    .gl
                    .clear_color(0.0, 0.0, 0.0, if target_frame.alpha { 0.0 } else { 1.0 });
                clear |= glow::COLOR_BUFFER_BIT;
            }
            state.gl.clear(clear);
            state.gl.color_mask(true, true, true, target_frame.alpha);
            state.gl.depth_mask(false);
            if state.path == GlPath::Modern {
                offscreen_vertices = offscreen_vertices.saturating_add(draw_modern_offscreen_pass(
                    state,
                    target_frame,
                    textures,
                ));
            } else {
                offscreen_vertices = offscreen_vertices.saturating_add(draw_legacy_offscreen_pass(
                    state,
                    target_frame,
                    textures,
                ));
            }
        }
        state.offscreen_targets[index].initialized = true;
    }
    // SAFETY: restore the window framebuffer and viewport before the ordinary
    // main pass executes.
    unsafe {
        state.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        state.gl.viewport(0, 0, width as i32, height as i32);
    }
    let offscreen_us = elapsed_us_since(offscreen_started);

    let backend_prepare_started = Instant::now();
    {
        let uploads = &mut state.uploads;
        let gl = &state.gl;
        let cached_tmesh = &mut state.cached_tmesh;
        let cached_tmesh_bytes = &mut state.cached_tmesh_bytes;
        resolve_textured_meshes(frame, uploads, |cache_key, vertices| {
            ensure_cached_tmesh(gl, cached_tmesh, cached_tmesh_bytes, cache_key, vertices)
        });
    }
    let mut stats = DrawStats {
        backend_prepare_us: elapsed_us_since(backend_prepare_started).saturating_add(offscreen_us),
        storage: draw_storage_stats(frame, Some(&state.uploads)),
        ..DrawStats::default()
    };

    let mut vertices: u32 = 0;

    let backend_record_started = Instant::now();
    // SAFETY: the OpenGL context in `state` is current on this thread for drawing,
    // and all buffer uploads reference slices that remain alive for the duration of
    // each call.
    unsafe {
        let gl = &state.gl;

        let c = frame.clear_color;
        gl.color_mask(true, true, true, true);
        gl.clear_color(c[0], c[1], c[2], 1.0);
        gl.clear_depth(1.0);
        gl.depth_mask(true);
        gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
        gl.depth_mask(false);
        gl.disable(glow::DEPTH_TEST);
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
        let mut last_cameras = [CameraUploadCache::default(); 3];
        let mut last_sprite_instance_start: Option<u32> = None;
        let mut last_tmesh_instance_start: Option<u32> = None;
        let mut tmesh_buffer_cache = TexturedMeshBufferCache::default();
        let mut last_depth_test = Some(false);

        let backend_upload_started = Instant::now();
        if state.path == GlPath::Modern && !frame.sprite_instances.is_empty() {
            let shared_instance_vbo = state
                .shared_instance_vbo
                .expect("modern OpenGL path creates a sprite instance VBO");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(shared_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(frame.sprite_instances.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if !frame.mesh_vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(frame.mesh_vertices.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.uploads.vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.uploads.vertices.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if state.path == GlPath::Modern && !frame.tmesh_instances.is_empty() {
            let tmesh_instance_vbo = state
                .tmesh_instance_vbo
                .expect("modern OpenGL path creates a textured mesh instance VBO");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(tmesh_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(frame.tmesh_instances.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        stats.backend_upload_us = elapsed_us_since(backend_upload_started);

        if state.path == GlPath::Modern {
            let shared_vao = state
                .shared_vao
                .expect("modern OpenGL path creates a sprite VAO");
            let shared_instance_vbo = state
                .shared_instance_vbo
                .expect("modern OpenGL path creates a sprite instance VBO");
            let mesh_vao = state
                .mesh_vao
                .expect("modern OpenGL path creates a mesh VAO");
            let tmesh_vao = state
                .tmesh_vao
                .expect("modern OpenGL path creates a textured mesh VAO");
            let tmesh_instance_vbo = state
                .tmesh_instance_vbo
                .expect("modern OpenGL path creates a textured mesh instance VBO");

            for op in frame.ops.iter().copied() {
                match op {
                    DrawOp::Sprite(run) => {
                        apply_blend(gl, run.blend, &mut last_blend);
                        apply_depth_test(gl, false, &mut last_depth_test);

                        if last_prog != Some(0) {
                            gl.use_program(Some(state.program));
                            gl.bind_vertex_array(Some(shared_vao));
                            gl.uniform_1_i32(Some(&state.texture_location), 0);
                            last_prog = Some(0);
                            last_sprite_instance_start = None;
                            tmesh_buffer_cache.reset();
                        }

                        if !state.base_instance
                            && last_sprite_instance_start != Some(run.instance_start)
                        {
                            let inst_stride = std::mem::size_of::<SpriteInstanceRaw>() as i32;
                            let vec2_size = (2 * std::mem::size_of::<f32>()) as i32;
                            let vec4_size = (4 * std::mem::size_of::<f32>()) as i32;
                            let base = (run.instance_start as i32) * inst_stride;
                            gl.bind_buffer(glow::ARRAY_BUFFER, Some(shared_instance_vbo));
                            gl.vertex_attrib_pointer_f32(
                                2,
                                4,
                                glow::FLOAT,
                                false,
                                inst_stride,
                                base,
                            );
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
                            gl.vertex_attrib_pointer_f32(
                                11,
                                1,
                                glow::FLOAT,
                                false,
                                inst_stride,
                                base + 3 * vec4_size + 6 * vec2_size,
                            );
                            last_sprite_instance_start = Some(run.instance_start);
                        }

                        if last_cameras[0].update_required(run.camera) {
                            let cam = frame
                                .cameras
                                .get(run.camera as usize)
                                .copied()
                                .unwrap_or(state.projection);
                            let mvp_array = cam.to_cols_array_2d();
                            gl.uniform_matrix_4_f32_slice(
                                Some(&state.mvp_location),
                                false,
                                bytemuck::cast_slice(&mvp_array),
                            );
                        }

                        let Some(texture) = resolved_texture(state, textures, run.texture_handle)
                        else {
                            continue;
                        };

                        if last_bound_tex != Some(texture.primary()) {
                            bind_sprite_texture(state, texture);
                            last_bound_tex = Some(texture.primary());
                        }
                        apply_render_target_filter(gl, run.texture_handle);

                        if state.base_instance {
                            gl.draw_elements_instanced_base_vertex_base_instance(
                                glow::TRIANGLES,
                                state.index_count,
                                glow::UNSIGNED_SHORT,
                                0,
                                run.instance_count as i32,
                                0,
                                run.instance_start,
                            );
                        } else {
                            gl.draw_elements_instanced(
                                glow::TRIANGLES,
                                state.index_count,
                                glow::UNSIGNED_SHORT,
                                0,
                                run.instance_count as i32,
                            );
                        }
                        vertices = vertices.saturating_add(4 * run.instance_count);
                    }
                    DrawOp::Mesh(run) => {
                        if run.vertex_count == 0 {
                            continue;
                        }

                        apply_blend(gl, run.blend, &mut last_blend);
                        apply_depth_test(gl, false, &mut last_depth_test);

                        if last_prog != Some(1) {
                            gl.use_program(Some(state.mesh_program));
                            gl.bind_vertex_array(Some(mesh_vao));
                            last_prog = Some(1);
                            tmesh_buffer_cache.reset();
                        }

                        if last_cameras[1].update_required(run.camera) {
                            let cam = frame
                                .cameras
                                .get(run.camera as usize)
                                .copied()
                                .unwrap_or(state.projection);
                            let mvp_array = cam.to_cols_array_2d();
                            gl.uniform_matrix_4_f32_slice(
                                Some(&state.mesh_mvp_location),
                                false,
                                bytemuck::cast_slice(&mvp_array),
                            );
                        }

                        gl.draw_arrays(
                            glow::TRIANGLES,
                            run.vertex_start as i32,
                            run.vertex_count as i32,
                        );
                        vertices = vertices.saturating_add(run.vertex_count);
                    }
                    DrawOp::TexturedMesh(run) => {
                        let Some(source) = state.uploads.source(run.geometry) else {
                            continue;
                        };
                        apply_blend(gl, run.blend, &mut last_blend);
                        apply_depth_test(gl, run.depth_test, &mut last_depth_test);

                        if last_prog != Some(2) {
                            gl.use_program(Some(state.tmesh_program));
                            gl.bind_vertex_array(Some(tmesh_vao));
                            gl.uniform_1_i32(Some(&state.tmesh_texture_location), 0);
                            last_prog = Some(2);
                            last_tmesh_instance_start = None;
                            tmesh_buffer_cache.reset();
                        }

                        if tmesh_buffer_cache.update_required(source) {
                            let stride = std::mem::size_of::<TexturedMeshVertex>() as i32;
                            let vertex_buffer = if let Some(buffer_key) = source.buffer_key() {
                                state
                                    .cached_tmesh
                                    .get_slot(buffer_key)
                                    .map(|entry| entry.vbo)
                            } else {
                                Some(state.tmesh_vbo)
                            };
                            let Some(vertex_buffer) = vertex_buffer else {
                                tmesh_buffer_cache.reset();
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
                        }

                        if !state.base_instance
                            && last_tmesh_instance_start != Some(run.instance_start)
                        {
                            let inst_stride = std::mem::size_of::<TexturedMeshInstanceRaw>() as i32;
                            let col_size = (4 * std::mem::size_of::<f32>()) as i32;
                            let uv_size = (2 * std::mem::size_of::<f32>()) as i32;
                            let base = (run.instance_start as i32) * inst_stride;
                            gl.bind_buffer(glow::ARRAY_BUFFER, Some(tmesh_instance_vbo));
                            gl.vertex_attrib_pointer_f32(
                                4,
                                4,
                                glow::FLOAT,
                                false,
                                inst_stride,
                                base,
                            );
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
                            gl.vertex_attrib_pointer_f32(
                                12,
                                1,
                                glow::FLOAT,
                                false,
                                inst_stride,
                                base + 5 * col_size + 3 * uv_size,
                            );
                            last_tmesh_instance_start = Some(run.instance_start);
                        }

                        if last_cameras[2].update_required(run.camera) {
                            let cam = frame
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
                        }

                        let Some(texture) = resolved_texture(state, textures, run.texture_handle)
                            .map(Texture::primary)
                        else {
                            continue;
                        };

                        if last_bound_tex != Some(texture) {
                            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                            last_bound_tex = Some(texture);
                        }
                        apply_render_target_filter(gl, run.texture_handle);

                        let draw_start = source.vertex_start() as i32;
                        let draw_count = source.vertex_count() as i32;
                        if state.base_instance {
                            gl.draw_arrays_instanced_base_instance(
                                glow::TRIANGLES,
                                draw_start,
                                draw_count,
                                run.instance_count as i32,
                                run.instance_start,
                            );
                        } else {
                            gl.draw_arrays_instanced(
                                glow::TRIANGLES,
                                draw_start,
                                draw_count,
                                run.instance_count as i32,
                            );
                        }
                        let tri_count = source.vertex_count() / 3;
                        vertices =
                            vertices.saturating_add(tri_count.saturating_mul(run.instance_count));
                    }
                }
            }
        } else {
            let sprite_uniforms = state
                .legacy_sprite_uniforms
                .expect("legacy OpenGL path creates sprite uniforms");
            let tmesh_uniforms = state
                .legacy_tmesh_uniforms
                .expect("legacy OpenGL path creates textured mesh uniforms");

            for op in frame.ops.iter().copied() {
                match op {
                    DrawOp::Sprite(run) => {
                        apply_blend(gl, run.blend, &mut last_blend);
                        apply_depth_test(gl, false, &mut last_depth_test);

                        if last_prog != Some(0) {
                            gl.use_program(Some(state.program));
                            gl.uniform_1_i32(Some(&state.texture_location), 0);
                            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.shared_vbo));
                            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(state.shared_ibo));
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
                            gl.disable_vertex_attrib_array(2);
                            gl.disable_vertex_attrib_array(3);
                            last_prog = Some(0);
                            tmesh_buffer_cache.reset();
                        }

                        if last_cameras[0].update_required(run.camera) {
                            let cam = frame
                                .cameras
                                .get(run.camera as usize)
                                .copied()
                                .unwrap_or(state.projection);
                            let mvp_array = cam.to_cols_array_2d();
                            gl.uniform_matrix_4_f32_slice(
                                Some(&state.mvp_location),
                                false,
                                bytemuck::cast_slice(&mvp_array),
                            );
                        }

                        let Some(texture) = resolved_texture(state, textures, run.texture_handle)
                        else {
                            continue;
                        };

                        if last_bound_tex != Some(texture.primary()) {
                            bind_sprite_texture(state, texture);
                            last_bound_tex = Some(texture.primary());
                        }
                        apply_render_target_filter(gl, run.texture_handle);

                        let end = run.instance_start.saturating_add(run.instance_count);
                        for idx in run.instance_start..end {
                            let Some(instance) = frame.sprite_instances.get(idx as usize) else {
                                continue;
                            };
                            gl.uniform_4_f32(
                                Some(&sprite_uniforms.center),
                                instance.center[0],
                                instance.center[1],
                                instance.center[2],
                                instance.center[3],
                            );
                            gl.uniform_2_f32(
                                Some(&sprite_uniforms.size),
                                instance.size[0],
                                instance.size[1],
                            );
                            gl.uniform_2_f32(
                                Some(&sprite_uniforms.rot_sin_cos),
                                instance.rot_sin_cos[0],
                                instance.rot_sin_cos[1],
                            );
                            gl.uniform_4_f32(
                                Some(&sprite_uniforms.tint),
                                instance.tint[0],
                                instance.tint[1],
                                instance.tint[2],
                                instance.tint[3],
                            );
                            gl.uniform_2_f32(
                                Some(&sprite_uniforms.uv_scale),
                                instance.uv_scale[0],
                                instance.uv_scale[1],
                            );
                            gl.uniform_2_f32(
                                Some(&sprite_uniforms.uv_offset),
                                instance.uv_offset[0],
                                instance.uv_offset[1],
                            );
                            gl.uniform_2_f32(
                                Some(&sprite_uniforms.local_offset),
                                instance.local_offset[0],
                                instance.local_offset[1],
                            );
                            gl.uniform_2_f32(
                                Some(&sprite_uniforms.local_offset_rot_sin_cos),
                                instance.local_offset_rot_sin_cos[0],
                                instance.local_offset_rot_sin_cos[1],
                            );
                            gl.uniform_4_f32(
                                Some(&sprite_uniforms.edge_fade),
                                instance.edge_fade[0],
                                instance.edge_fade[1],
                                instance.edge_fade[2],
                                instance.edge_fade[3],
                            );
                            gl.uniform_1_f32(
                                Some(&sprite_uniforms.texture_mask),
                                instance.texture_mask,
                            );
                            gl.draw_elements(
                                glow::TRIANGLES,
                                state.index_count,
                                glow::UNSIGNED_SHORT,
                                0,
                            );
                            vertices = vertices.saturating_add(4);
                        }
                    }
                    DrawOp::Mesh(run) => {
                        if run.vertex_count == 0 {
                            continue;
                        }

                        apply_blend(gl, run.blend, &mut last_blend);
                        apply_depth_test(gl, false, &mut last_depth_test);

                        if last_prog != Some(1) {
                            gl.use_program(Some(state.mesh_program));
                            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
                            let stride =
                                std::mem::size_of::<deadlib_render_core::MeshVertex>() as i32;
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
                            gl.disable_vertex_attrib_array(2);
                            gl.disable_vertex_attrib_array(3);
                            last_prog = Some(1);
                            tmesh_buffer_cache.reset();
                        }

                        if last_cameras[1].update_required(run.camera) {
                            let cam = frame
                                .cameras
                                .get(run.camera as usize)
                                .copied()
                                .unwrap_or(state.projection);
                            let mvp_array = cam.to_cols_array_2d();
                            gl.uniform_matrix_4_f32_slice(
                                Some(&state.mesh_mvp_location),
                                false,
                                bytemuck::cast_slice(&mvp_array),
                            );
                        }

                        gl.draw_arrays(
                            glow::TRIANGLES,
                            run.vertex_start as i32,
                            run.vertex_count as i32,
                        );
                        vertices = vertices.saturating_add(run.vertex_count);
                    }
                    DrawOp::TexturedMesh(run) => {
                        let Some(source) = state.uploads.source(run.geometry) else {
                            continue;
                        };
                        apply_blend(gl, run.blend, &mut last_blend);
                        apply_depth_test(gl, run.depth_test, &mut last_depth_test);

                        if last_prog != Some(2) {
                            gl.use_program(Some(state.tmesh_program));
                            gl.uniform_1_i32(Some(&state.tmesh_texture_location), 0);
                            gl.enable_vertex_attrib_array(0);
                            gl.enable_vertex_attrib_array(1);
                            gl.enable_vertex_attrib_array(2);
                            gl.enable_vertex_attrib_array(3);
                            last_prog = Some(2);
                            tmesh_buffer_cache.reset();
                        }

                        if tmesh_buffer_cache.update_required(source) {
                            let stride = std::mem::size_of::<TexturedMeshVertex>() as i32;
                            let vertex_buffer = if let Some(buffer_key) = source.buffer_key() {
                                state
                                    .cached_tmesh
                                    .get_slot(buffer_key)
                                    .map(|entry| entry.vbo)
                            } else {
                                Some(state.tmesh_vbo)
                            };
                            let Some(vertex_buffer) = vertex_buffer else {
                                tmesh_buffer_cache.reset();
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
                        }

                        if last_cameras[2].update_required(run.camera) {
                            let cam = frame
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
                        }

                        let Some(texture) = resolved_texture(state, textures, run.texture_handle)
                            .map(Texture::primary)
                        else {
                            continue;
                        };

                        if last_bound_tex != Some(texture) {
                            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                            last_bound_tex = Some(texture);
                        }
                        apply_render_target_filter(gl, run.texture_handle);

                        let draw_start = source.vertex_start() as i32;
                        let draw_count = source.vertex_count() as i32;
                        let tri_count = source.vertex_count() / 3;
                        let end = run.instance_start.saturating_add(run.instance_count);
                        for idx in run.instance_start..end {
                            let Some(instance) = frame.tmesh_instances.get(idx as usize) else {
                                continue;
                            };
                            let model = [
                                instance.model_col0[0],
                                instance.model_col0[1],
                                instance.model_col0[2],
                                instance.model_col0[3],
                                instance.model_col1[0],
                                instance.model_col1[1],
                                instance.model_col1[2],
                                instance.model_col1[3],
                                instance.model_col2[0],
                                instance.model_col2[1],
                                instance.model_col2[2],
                                instance.model_col2[3],
                                instance.model_col3[0],
                                instance.model_col3[1],
                                instance.model_col3[2],
                                instance.model_col3[3],
                            ];
                            gl.uniform_matrix_4_f32_slice(
                                Some(&tmesh_uniforms.model),
                                false,
                                &model,
                            );
                            gl.uniform_4_f32(
                                Some(&tmesh_uniforms.tint),
                                instance.tint[0],
                                instance.tint[1],
                                instance.tint[2],
                                instance.tint[3],
                            );
                            gl.uniform_2_f32(
                                Some(&tmesh_uniforms.uv_scale),
                                instance.uv_scale[0],
                                instance.uv_scale[1],
                            );
                            gl.uniform_2_f32(
                                Some(&tmesh_uniforms.uv_offset),
                                instance.uv_offset[0],
                                instance.uv_offset[1],
                            );
                            gl.uniform_2_f32(
                                Some(&tmesh_uniforms.uv_tex_shift),
                                instance.uv_tex_shift[0],
                                instance.uv_tex_shift[1],
                            );
                            gl.uniform_1_f32(
                                Some(&tmesh_uniforms.texture_mask),
                                instance.texture_mask,
                            );
                            gl.draw_arrays(glow::TRIANGLES, draw_start, draw_count);
                            vertices = vertices.saturating_add(tri_count);
                        }
                    }
                }
            }
        }
        apply_depth_test(gl, false, &mut last_depth_test);
        if state.path == GlPath::Modern {
            gl.bind_vertex_array(None);
        }
        gl.use_program(None);
    }
    stats.backend_record_us =
        elapsed_us_since(backend_record_started).saturating_sub(stats.backend_upload_us);

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
    stats.vertices = vertices.saturating_add(offscreen_vertices);
    Ok(stats)
}

pub fn capture_frame(state: &mut State) -> Result<RgbaImage, Box<dyn Error>> {
    state
        .captured_frame
        .take()
        .ok_or_else(|| std::io::Error::other("No captured screenshot frame available").into())
}

#[inline(always)]
pub const fn render_size(state: &State) -> (u32, u32) {
    state.window_size
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

pub const fn set_default_projection(state: &mut State, projection: Matrix4) {
    state.projection = projection;
}

pub fn cleanup(state: &mut State) {
    info!("Cleaning up OpenGL resources...");
    // SAFETY: all GL object handles below were created by this backend and are
    // still owned by `state`, so deleting them once during cleanup is valid.
    unsafe {
        for target in state.offscreen_targets.drain(..) {
            state.gl.delete_texture(target.texture.primary());
            state.gl.delete_framebuffer(target.framebuffer);
            state.gl.delete_renderbuffer(target.depth);
        }
        state.gl.delete_program(state.program);
        state.gl.delete_program(state.mesh_program);
        state.gl.delete_program(state.tmesh_program);
        if let Some(vao) = state.shared_vao {
            state.gl.delete_vertex_array(vao);
        }
        state.gl.delete_buffer(state.shared_vbo);
        state.gl.delete_buffer(state.shared_ibo);
        if let Some(vbo) = state.shared_instance_vbo {
            state.gl.delete_buffer(vbo);
        }
        if let Some(vao) = state.mesh_vao {
            state.gl.delete_vertex_array(vao);
        }
        state.gl.delete_buffer(state.mesh_vbo);
        if let Some(vao) = state.tmesh_vao {
            state.gl.delete_vertex_array(vao);
        }
        for geom in state.cached_tmesh.drain() {
            state.gl.delete_buffer(geom.vbo);
        }
        state.cached_tmesh_bytes = 0;
        state.gl.delete_buffer(state.tmesh_vbo);
        if let Some(vbo) = state.tmesh_instance_vbo {
            state.gl.delete_buffer(vbo);
        }
    }
    info!("OpenGL resources cleaned up.");
}

#[inline(always)]
fn ortho_for_window(width: u32, height: u32) -> Matrix4 {
    let aspect = if height == 0 {
        1.0
    } else {
        width as f32 / height as f32
    };
    let h = LOGICAL_HEIGHT;
    let w = if aspect >= 16.0 / 9.0 {
        DESIGN_WIDTH_16_9
    } else {
        (h * aspect).min(DESIGN_WIDTH_16_9)
    };
    let half_w = 0.5 * w;
    let half_h = 0.5 * h;
    glam::camera::rh::proj::opengl::orthographic(-half_w, half_w, -half_h, half_h, -1.0, 1.0)
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
        for (index, name) in SPRITE_ATTRIBS {
            gl.bind_attrib_location(program, index, name);
        }
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
        for (index, name) in MESH_ATTRIBS {
            gl.bind_attrib_location(program, index, name);
        }
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
        for (index, name) in TMESH_ATTRIBS {
            gl.bind_attrib_location(program, index, name);
        }
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

fn uniform_location(
    gl: &glow::Context,
    program: glow::Program,
    name: &str,
) -> Result<UniformLocation, String> {
    // SAFETY: uniform lookup only reads program metadata from the current GL
    // context and does not retain Rust pointers.
    unsafe {
        gl.get_uniform_location(program, name)
            .ok_or_else(|| name.to_string())
    }
}

fn legacy_sprite_uniforms(
    gl: &glow::Context,
    program: glow::Program,
) -> Result<LegacySpriteUniforms, String> {
    Ok(LegacySpriteUniforms {
        center: uniform_location(gl, program, "u_center")?,
        size: uniform_location(gl, program, "u_size")?,
        rot_sin_cos: uniform_location(gl, program, "u_rot_sin_cos")?,
        tint: uniform_location(gl, program, "u_tint")?,
        uv_scale: uniform_location(gl, program, "u_uv_scale")?,
        uv_offset: uniform_location(gl, program, "u_uv_offset")?,
        local_offset: uniform_location(gl, program, "u_local_offset")?,
        local_offset_rot_sin_cos: uniform_location(gl, program, "u_local_offset_rot_sin_cos")?,
        edge_fade: uniform_location(gl, program, "u_edge_fade")?,
        texture_mask: uniform_location(gl, program, "u_texture_mask")?,
    })
}

fn legacy_tmesh_uniforms(
    gl: &glow::Context,
    program: glow::Program,
) -> Result<LegacyTMeshUniforms, String> {
    Ok(LegacyTMeshUniforms {
        model: uniform_location(gl, program, "u_model")?,
        tint: uniform_location(gl, program, "u_tint")?,
        uv_scale: uniform_location(gl, program, "u_uv_scale")?,
        uv_offset: uniform_location(gl, program, "u_uv_offset")?,
        uv_tex_shift: uniform_location(gl, program, "u_uv_tex_shift")?,
        texture_mask: uniform_location(gl, program, "u_texture_mask")?,
    })
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
        .with_depth_size(24)
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
    use super::{
        GlApi, GlPath, GlVersion, base_instance_capability, parse_gl_version, surface_extent,
    };

    #[test]
    fn surface_extent_clamps_zero_dims() {
        assert_eq!(surface_extent(0, 0).0.get(), 1);
        assert_eq!(surface_extent(0, 0).1.get(), 1);
        assert_eq!(surface_extent(1920, 1080).0.get(), 1920);
        assert_eq!(surface_extent(1920, 1080).1.get(), 1080);
    }

    #[test]
    fn parse_gl_version_reads_driver_prefixes() {
        assert_eq!(
            parse_gl_version("2.1.0 - Build 8.15.10.2555"),
            Some(GlVersion { major: 2, minor: 1 })
        );
        assert_eq!(
            parse_gl_version("4.6.0 NVIDIA 551.86"),
            Some(GlVersion { major: 4, minor: 6 })
        );
        assert_eq!(
            parse_gl_version("OpenGL ES 3.0 Mesa 24.0.0"),
            Some(GlVersion { major: 3, minor: 0 })
        );
        assert_eq!(parse_gl_version("unknown"), None);
    }

    #[test]
    fn base_instance_requires_modern_supported_desktop_gl() {
        let gl_4_1 = Some(GlVersion { major: 4, minor: 1 });
        let gl_4_2 = Some(GlVersion { major: 4, minor: 2 });

        assert!(base_instance_capability(
            GlApi::Desktop,
            GlPath::Modern,
            gl_4_2,
            false
        ));
        assert!(base_instance_capability(
            GlApi::Desktop,
            GlPath::Modern,
            gl_4_1,
            true
        ));
        assert!(!base_instance_capability(
            GlApi::Desktop,
            GlPath::Modern,
            gl_4_1,
            false
        ));
        assert!(!base_instance_capability(
            GlApi::Desktop,
            GlPath::Legacy,
            gl_4_2,
            true
        ));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn base_instance_keeps_gles_on_pointer_fallback() {
        assert!(!base_instance_capability(
            GlApi::Gles,
            GlPath::Modern,
            Some(GlVersion { major: 4, minor: 2 }),
            true
        ));
    }
}
