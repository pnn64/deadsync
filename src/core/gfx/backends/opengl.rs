use crate::core::gfx::{
    BlendMode, MeshMode, ObjectType, RenderList, SamplerDesc, SamplerFilter, SamplerWrap,
    Texture as RendererTexture,
};
use crate::core::space::ortho_for_window;
use cgmath::Matrix4;
use glow::{HasContext, PixelPackData, PixelUnpackData, UniformLocation};
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextAttributesBuilder, PossiblyCurrentContext},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, WindowSurface},
};
use image::RgbaImage;
use log::{debug, info, warn};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    collections::HashMap, error::Error, ffi::CStr, hash::Hasher, mem, num::NonZeroU32, sync::Arc,
};
use twox_hash::XxHash64;
use winit::window::Window;

// A handle to an OpenGL texture on the GPU.
#[derive(Debug, Clone, Copy)]
pub struct Texture(pub glow::Texture);

#[repr(C)]
#[derive(Clone, Copy)]
struct TexturedMeshVertexRaw {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    tex_matrix_scale: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TexturedMeshInstanceRaw {
    model_col0: [f32; 4],
    model_col1: [f32; 4],
    model_col2: [f32; 4],
    model_col3: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TMeshGeomKey {
    ptr: usize,
    len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TMeshCacheKey {
    hash: u64,
    len: u32,
}

#[derive(Clone, Copy)]
enum FrameTMeshGeom {
    Dynamic {
        vertex_start: u32,
        vertex_count: u32,
    },
    Cached {
        cache_id: u64,
        vertex_count: u32,
        buffer: glow::Buffer,
    },
}

struct TMeshCacheEntry {
    id: u64,
    vertex_count: u32,
    bytes: u64,
    last_used_frame: u64,
    buffer: glow::Buffer,
}

struct TMeshSeenEntry {
    hits: u8,
    last_seen_frame: u64,
}

#[derive(Clone, Copy, Default)]
struct TMeshFrameDebug {
    cache_hits: u64,
    cache_misses: u64,
    cache_promotions: u64,
    cache_evictions: u64,
    dynamic_upload_vertices: u64,
}

#[derive(Default)]
struct TMeshDebugAccum {
    frames: u32,
    cache_hits: u64,
    cache_misses: u64,
    cache_promotions: u64,
    cache_evictions: u64,
    dynamic_upload_vertices: u64,
}

#[derive(Clone, Copy)]
struct TexturedMeshRun {
    vertex_start: u32,
    vertex_count: u32,
    dynamic_geom: bool,
    geom_key: u64,
    cached_vertex_buffer: Option<glow::Buffer>,
    instance_start: u32,
    instance_count: u32,
    mode: MeshMode,
    blend: BlendMode,
    texture: glow::Texture,
    camera: u8,
}

#[derive(Clone, Copy)]
enum DrawOp {
    Sprite(usize),
    Mesh(usize),
    TexturedMesh(TexturedMeshRun),
}

pub struct State {
    pub gl: glow::Context,
    gl_surface: Surface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    program: glow::Program,
    mesh_program: glow::Program,
    tmesh_program: glow::Program,
    mvp_location: UniformLocation,
    mesh_mvp_location: UniformLocation,
    tmesh_mvp_location: UniformLocation,
    tmesh_texture_location: UniformLocation,
    color_location: UniformLocation,
    texture_location: UniformLocation,
    projection: Matrix4<f32>,
    window_size: (u32, u32),
    // A single, shared set of buffers for a unit quad.
    shared_vao: glow::VertexArray,
    _shared_vbo: glow::Buffer,
    _shared_ibo: glow::Buffer,
    index_count: i32,
    mesh_vao: glow::VertexArray,
    mesh_vbo: glow::Buffer,
    tmesh_vao: glow::VertexArray,
    tmesh_vbo: glow::Buffer,
    tmesh_instance_vbo: glow::Buffer,
    scratch_tmesh_vertices: Vec<TexturedMeshVertexRaw>,
    scratch_tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    scratch_ops: Vec<DrawOp>,
    tmesh_cache_entries: HashMap<TMeshCacheKey, TMeshCacheEntry>,
    tmesh_cache_seen: HashMap<TMeshCacheKey, TMeshSeenEntry>,
    tmesh_cache_frame: u64,
    tmesh_cache_total_bytes: u64,
    next_tmesh_cache_id: u64,
    tmesh_debug_enabled: bool,
    tmesh_debug_accum: TMeshDebugAccum,
    uv_scale_location: UniformLocation,
    uv_offset_location: UniformLocation,
    local_offset_location: UniformLocation,
    local_offset_rot_location: UniformLocation,
    edge_fade_location: UniformLocation,
    instanced_location: UniformLocation,
}

pub fn init(
    window: Arc<Window>,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing OpenGL backend...");
    if gfx_debug_enabled {
        debug!("OpenGL debug context requested.");
    }

    let (gl_surface, gl_context, gl) =
        create_opengl_context(&window, vsync_enabled, gfx_debug_enabled)?;
    log_opengl_driver_info(&gl);
    let (
        program,
        mvp_location,
        color_location,
        texture_location,
        uv_scale_location,
        uv_offset_location,
        local_offset_location,
        local_offset_rot_location,
        edge_fade_location,
        instanced_location,
    ) = create_graphics_program(&gl)?;
    let (mesh_program, mesh_mvp_location) = create_mesh_program(&gl)?;
    let (tmesh_program, tmesh_mvp_location, tmesh_texture_location) = create_tmesh_program(&gl)?;

    // Create shared static unit quad + index buffer.
    let (shared_vao, _shared_vbo, _shared_ibo, index_count) = unsafe {
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

        gl.bind_vertex_array(None);

        (vao, vbo, ibo, QUAD_INDICES.len() as i32)
    };

    let (mesh_vao, mesh_vbo) = unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        // a_pos (location 0), a_color (location 1)
        let stride = std::mem::size_of::<crate::core::gfx::MeshVertex>() as i32;
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
    let (tmesh_vao, tmesh_vbo, tmesh_instance_vbo) = unsafe {
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let instance_vbo = gl.create_buffer()?;

        gl.bind_vertex_array(Some(vao));
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_size(glow::ARRAY_BUFFER, 0, glow::DYNAMIC_DRAW);

        // a_pos (location 0), a_uv (location 1), a_color (location 2), a_tex_matrix_scale (location 3)
        let stride = std::mem::size_of::<TexturedMeshVertexRaw>() as i32;
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(
            1,
            2,
            glow::FLOAT,
            false,
            stride,
            (2 * std::mem::size_of::<f32>()) as i32,
        );
        gl.enable_vertex_attrib_array(2);
        gl.vertex_attrib_pointer_f32(
            2,
            4,
            glow::FLOAT,
            false,
            stride,
            (4 * std::mem::size_of::<f32>()) as i32,
        );
        gl.enable_vertex_attrib_array(3);
        gl.vertex_attrib_pointer_f32(
            3,
            2,
            glow::FLOAT,
            false,
            stride,
            (8 * std::mem::size_of::<f32>()) as i32,
        );

        // i_model_col0..i_model_col3 (locations 4..7), i_uv_scale/i_uv_offset/i_uv_tex_shift (8..10)
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
        gl.vertex_attrib_pointer_f32(8, 2, glow::FLOAT, false, inst_stride, 4 * col_size);
        gl.vertex_attrib_divisor(8, 1);
        gl.enable_vertex_attrib_array(9);
        gl.vertex_attrib_pointer_f32(
            9,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            4 * col_size + uv_size,
        );
        gl.vertex_attrib_divisor(9, 1);
        gl.enable_vertex_attrib_array(10);
        gl.vertex_attrib_pointer_f32(
            10,
            2,
            glow::FLOAT,
            false,
            inst_stride,
            4 * col_size + 2 * uv_size,
        );
        gl.vertex_attrib_divisor(10, 1);

        gl.bind_vertex_array(None);
        (vao, vbo, instance_vbo)
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
        gl.uniform_2_f32(Some(&local_offset_location), 0.0, 0.0);
        gl.uniform_2_f32(Some(&local_offset_rot_location), 0.0, 1.0);
        gl.uniform_4_f32(Some(&edge_fade_location), 0.0, 0.0, 0.0, 0.0);
        gl.use_program(None);
    }

    let state = State {
        gl,
        gl_surface,
        gl_context,
        program,
        mesh_program,
        tmesh_program,
        mvp_location,
        mesh_mvp_location,
        tmesh_mvp_location,
        tmesh_texture_location,
        color_location,
        texture_location,
        projection,
        window_size: (initial_size.width, initial_size.height),
        shared_vao,
        _shared_vbo,
        _shared_ibo,
        index_count,
        mesh_vao,
        mesh_vbo,
        tmesh_vao,
        tmesh_vbo,
        tmesh_instance_vbo,
        scratch_tmesh_vertices: Vec::with_capacity(1024),
        scratch_tmesh_instances: Vec::with_capacity(256),
        scratch_ops: Vec::with_capacity(64),
        tmesh_cache_entries: HashMap::new(),
        tmesh_cache_seen: HashMap::new(),
        tmesh_cache_frame: 0,
        tmesh_cache_total_bytes: 0,
        next_tmesh_cache_id: 1,
        tmesh_debug_enabled: gfx_debug_enabled,
        tmesh_debug_accum: TMeshDebugAccum::default(),
        uv_scale_location,
        uv_offset_location,
        local_offset_location,
        local_offset_rot_location,
        edge_fade_location,
        instanced_location,
    };

    info!("OpenGL backend initialized successfully.");
    Ok(state)
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

#[inline(always)]
pub const fn request_screenshot(_state: &mut State) {}

pub fn draw(
    state: &mut State,
    render_list: &RenderList<'_>,
    textures: &HashMap<String, RendererTexture>,
) -> Result<u32, Box<dyn Error>> {
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(0);
    }

    #[inline(always)]
    fn lookup_texture_case_insensitive<'a>(
        textures: &'a HashMap<String, RendererTexture>,
        key: &str,
    ) -> Option<&'a RendererTexture> {
        if let Some(tex) = textures.get(key) {
            return Some(tex);
        }
        textures
            .iter()
            .find_map(|(candidate, tex)| candidate.eq_ignore_ascii_case(key).then_some(tex))
    }

    #[inline(always)]
    fn apply_blend(gl: &glow::Context, want: BlendMode, last: &mut Option<BlendMode>) {
        if *last == Some(want) {
            return;
        }
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

    state.tmesh_cache_frame = state.tmesh_cache_frame.wrapping_add(1);
    prune_tmesh_seen_entries(&mut state.tmesh_cache_seen, state.tmesh_cache_frame);
    let mut tmesh_debug_frame = TMeshFrameDebug::default();
    tmesh_debug_frame.cache_evictions =
        tmesh_debug_frame
            .cache_evictions
            .saturating_add(evict_tmesh_cache_entries(
                &state.gl,
                &mut state.tmesh_cache_entries,
                &mut state.tmesh_cache_total_bytes,
                state.tmesh_cache_frame,
            ) as u64);

    {
        let objects_len = render_list.objects.len();
        let tmesh_vertices = &mut state.scratch_tmesh_vertices;
        tmesh_vertices.clear();
        let want_tmesh = objects_len.saturating_mul(4);
        if tmesh_vertices.capacity() < want_tmesh {
            tmesh_vertices.reserve(want_tmesh - tmesh_vertices.capacity());
        }

        let tmesh_instances = &mut state.scratch_tmesh_instances;
        tmesh_instances.clear();
        if tmesh_instances.capacity() < objects_len {
            tmesh_instances.reserve(objects_len - tmesh_instances.capacity());
        }

        let ops = &mut state.scratch_ops;
        ops.clear();
        if ops.capacity() < objects_len {
            ops.reserve(objects_len - ops.capacity());
        }

        let cache_frame = state.tmesh_cache_frame;
        let tmesh_cache_entries = &mut state.tmesh_cache_entries;
        let tmesh_cache_seen = &mut state.tmesh_cache_seen;
        let next_tmesh_cache_id = &mut state.next_tmesh_cache_id;
        let tmesh_cache_total_bytes = &mut state.tmesh_cache_total_bytes;
        let mut tmesh_geom: HashMap<TMeshGeomKey, FrameTMeshGeom> =
            HashMap::with_capacity(objects_len);

        for (idx, obj) in render_list.objects.iter().enumerate() {
            match &obj.object_type {
                ObjectType::Sprite { .. } => ops.push(DrawOp::Sprite(idx)),
                ObjectType::Mesh { vertices, .. } => {
                    if !vertices.is_empty() {
                        ops.push(DrawOp::Mesh(idx));
                    }
                }
                ObjectType::TexturedMesh {
                    texture_id,
                    vertices,
                    mode,
                    uv_scale,
                    uv_offset,
                    uv_tex_shift,
                } => {
                    if *mode != MeshMode::Triangles || vertices.is_empty() {
                        continue;
                    }

                    let Some(RendererTexture::OpenGL(gl_tex)) =
                        lookup_texture_case_insensitive(textures, texture_id.as_ref())
                    else {
                        continue;
                    };

                    let geom_key = TMeshGeomKey {
                        ptr: vertices.as_ptr() as usize,
                        len: vertices.len(),
                    };
                    let geom = if let Some(&geom) = tmesh_geom.get(&geom_key) {
                        geom
                    } else {
                        let geom = if let Some(cached) = try_get_or_promote_cached_tmesh_geom(
                            &state.gl,
                            tmesh_cache_entries,
                            tmesh_cache_seen,
                            next_tmesh_cache_id,
                            tmesh_cache_total_bytes,
                            &mut tmesh_debug_frame,
                            cache_frame,
                            vertices,
                        ) {
                            cached
                        } else {
                            let start = tmesh_vertices.len() as u32;
                            tmesh_vertices.reserve(vertices.len());
                            for v in vertices.iter() {
                                tmesh_vertices.push(TexturedMeshVertexRaw {
                                    pos: v.pos,
                                    uv: v.uv,
                                    tex_matrix_scale: v.tex_matrix_scale,
                                    color: v.color,
                                });
                            }
                            tmesh_debug_frame.dynamic_upload_vertices = tmesh_debug_frame
                                .dynamic_upload_vertices
                                .saturating_add(vertices.len() as u64);
                            FrameTMeshGeom::Dynamic {
                                vertex_start: start,
                                vertex_count: vertices.len() as u32,
                            }
                        };
                        tmesh_geom.insert(geom_key, geom);
                        geom
                    };
                    let (
                        vertex_start,
                        vertex_count,
                        dynamic_geom,
                        geom_run_key,
                        cached_vertex_buffer,
                    ) = match geom {
                        FrameTMeshGeom::Dynamic {
                            vertex_start,
                            vertex_count,
                        } => (
                            vertex_start,
                            vertex_count,
                            true,
                            (1u64 << 63) | u64::from(vertex_start),
                            None,
                        ),
                        FrameTMeshGeom::Cached {
                            cache_id,
                            vertex_count,
                            buffer,
                        } => (0, vertex_count, false, cache_id, Some(buffer)),
                    };

                    let instance_start = tmesh_instances.len() as u32;
                    let model: [[f32; 4]; 4] = obj.transform.into();
                    tmesh_instances.push(TexturedMeshInstanceRaw {
                        model_col0: model[0],
                        model_col1: model[1],
                        model_col2: model[2],
                        model_col3: model[3],
                        uv_scale: *uv_scale,
                        uv_offset: *uv_offset,
                        uv_tex_shift: *uv_tex_shift,
                    });

                    if let Some(DrawOp::TexturedMesh(last)) = ops.last_mut()
                        && last.texture == gl_tex.0
                        && last.blend == obj.blend
                        && last.camera == obj.camera
                        && last.mode == *mode
                        && last.dynamic_geom == dynamic_geom
                        && last.geom_key == geom_run_key
                        && last.instance_start + last.instance_count == instance_start
                    {
                        last.instance_count += 1;
                        continue;
                    }

                    ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                        vertex_start,
                        vertex_count,
                        dynamic_geom,
                        geom_key: geom_run_key,
                        cached_vertex_buffer,
                        instance_start,
                        instance_count: 1,
                        mode: *mode,
                        blend: obj.blend,
                        texture: gl_tex.0,
                        camera: obj.camera,
                    }));
                }
            }
        }
    }

    let mut vertices: u32 = 0;

    unsafe {
        let gl = &state.gl;

        let c = render_list.clear_color;
        gl.clear_color(c[0], c[1], c[2], c[3]);
        gl.clear(glow::COLOR_BUFFER_BIT);

        gl.enable(glow::BLEND);
        gl.blend_equation(glow::FUNC_ADD);
        gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

        gl.active_texture(glow::TEXTURE0);

        let mut last_bound_tex: Option<glow::Texture> = None;
        let mut last_blend = Some(BlendMode::Alpha);
        let mut last_uv_scale: Option<[f32; 2]> = None;
        let mut last_uv_offset: Option<[f32; 2]> = None;
        let mut last_local_offset: Option<[f32; 2]> = None;
        let mut last_local_offset_rot: Option<[f32; 2]> = None;
        let mut last_color: Option<[f32; 4]> = None;
        let mut last_edge_fade: Option<[f32; 4]> = None;
        let mut last_prog: Option<u8> = None; // 0=sprite, 1=mesh, 2=textured mesh
        let mut last_tmesh_instance_start: Option<u32> = None;
        let mut last_tmesh_geom_key: Option<u64> = None;

        if !state.scratch_tmesh_vertices.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.scratch_tmesh_vertices.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }
        if !state.scratch_tmesh_instances.is_empty() {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.tmesh_instance_vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(state.scratch_tmesh_instances.as_slice()),
                glow::DYNAMIC_DRAW,
            );
        }

        for op in state.scratch_ops.iter().copied() {
            match op {
                DrawOp::Sprite(idx) => {
                    let obj = &render_list.objects[idx];
                    let ObjectType::Sprite {
                        texture_id,
                        tint,
                        uv_scale,
                        uv_offset,
                        local_offset,
                        local_offset_rot_sin_cos,
                        edge_fade,
                    } = &obj.object_type
                    else {
                        continue;
                    };

                    apply_blend(gl, obj.blend, &mut last_blend);

                    let cam = render_list
                        .cameras
                        .get(obj.camera as usize)
                        .copied()
                        .unwrap_or(state.projection);

                    if last_prog != Some(0) {
                        gl.use_program(Some(state.program));
                        gl.bind_vertex_array(Some(state.shared_vao));
                        gl.uniform_1_i32(Some(&state.texture_location), 0);
                        gl.uniform_1_i32(Some(&state.instanced_location), 0);
                        last_prog = Some(0);
                        last_tmesh_geom_key = None;
                    }

                    let mvp_array: [[f32; 4]; 4] = (cam * obj.transform).into();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.mvp_location),
                        false,
                        bytemuck::cast_slice(&mvp_array),
                    );

                    if let Some(RendererTexture::OpenGL(gl_tex)) =
                        lookup_texture_case_insensitive(textures, texture_id.as_ref())
                    {
                        if last_bound_tex != Some(gl_tex.0) {
                            gl.bind_texture(glow::TEXTURE_2D, Some(gl_tex.0));
                            last_bound_tex = Some(gl_tex.0);
                        }
                        if last_uv_scale != Some(*uv_scale) {
                            gl.uniform_2_f32(
                                Some(&state.uv_scale_location),
                                uv_scale[0],
                                uv_scale[1],
                            );
                            last_uv_scale = Some(*uv_scale);
                        }
                        if last_uv_offset != Some(*uv_offset) {
                            gl.uniform_2_f32(
                                Some(&state.uv_offset_location),
                                uv_offset[0],
                                uv_offset[1],
                            );
                            last_uv_offset = Some(*uv_offset);
                        }
                        if last_local_offset != Some(*local_offset) {
                            gl.uniform_2_f32(
                                Some(&state.local_offset_location),
                                local_offset[0],
                                local_offset[1],
                            );
                            last_local_offset = Some(*local_offset);
                        }
                        if last_local_offset_rot != Some(*local_offset_rot_sin_cos) {
                            gl.uniform_2_f32(
                                Some(&state.local_offset_rot_location),
                                local_offset_rot_sin_cos[0],
                                local_offset_rot_sin_cos[1],
                            );
                            last_local_offset_rot = Some(*local_offset_rot_sin_cos);
                        }
                        if last_color != Some(*tint) {
                            gl.uniform_4_f32_slice(Some(&state.color_location), tint);
                            last_color = Some(*tint);
                        }
                        if last_edge_fade != Some(*edge_fade) {
                            gl.uniform_4_f32_slice(Some(&state.edge_fade_location), edge_fade);
                            last_edge_fade = Some(*edge_fade);
                        }
                        gl.draw_elements(
                            glow::TRIANGLES,
                            state.index_count,
                            glow::UNSIGNED_SHORT,
                            0,
                        );
                        vertices += 4;
                    }
                }
                DrawOp::Mesh(idx) => {
                    let obj = &render_list.objects[idx];
                    let ObjectType::Mesh { vertices: vs, mode } = &obj.object_type else {
                        continue;
                    };
                    if vs.is_empty() {
                        continue;
                    }

                    apply_blend(gl, obj.blend, &mut last_blend);

                    let cam = render_list
                        .cameras
                        .get(obj.camera as usize)
                        .copied()
                        .unwrap_or(state.projection);

                    if last_prog != Some(1) {
                        gl.use_program(Some(state.mesh_program));
                        gl.bind_vertex_array(Some(state.mesh_vao));
                        last_prog = Some(1);
                        last_tmesh_geom_key = None;
                    }

                    let mvp_array: [[f32; 4]; 4] = (cam * obj.transform).into();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.mesh_mvp_location),
                        false,
                        bytemuck::cast_slice(&mvp_array),
                    );

                    gl.bind_buffer(glow::ARRAY_BUFFER, Some(state.mesh_vbo));
                    gl.buffer_data_u8_slice(
                        glow::ARRAY_BUFFER,
                        bytemuck::cast_slice(vs.as_ref()),
                        glow::DYNAMIC_DRAW,
                    );

                    let prim = match mode {
                        MeshMode::Triangles => glow::TRIANGLES,
                    };
                    gl.draw_arrays(prim, 0, vs.len() as i32);
                    vertices = vertices.saturating_add(vs.len() as u32);
                }
                DrawOp::TexturedMesh(run) => {
                    apply_blend(gl, run.blend, &mut last_blend);

                    if last_prog != Some(2) {
                        gl.use_program(Some(state.tmesh_program));
                        gl.bind_vertex_array(Some(state.tmesh_vao));
                        gl.uniform_1_i32(Some(&state.tmesh_texture_location), 0);
                        last_prog = Some(2);
                        last_tmesh_instance_start = None;
                        last_tmesh_geom_key = None;
                    }

                    if last_tmesh_geom_key != Some(run.geom_key) {
                        let stride = std::mem::size_of::<TexturedMeshVertexRaw>() as i32;
                        let vertex_buffer = if run.dynamic_geom {
                            Some(state.tmesh_vbo)
                        } else {
                            run.cached_vertex_buffer
                        };
                        let Some(buffer) = vertex_buffer else {
                            continue;
                        };
                        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
                        gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
                        gl.vertex_attrib_pointer_f32(
                            1,
                            2,
                            glow::FLOAT,
                            false,
                            stride,
                            (2 * std::mem::size_of::<f32>()) as i32,
                        );
                        gl.vertex_attrib_pointer_f32(
                            2,
                            4,
                            glow::FLOAT,
                            false,
                            stride,
                            (4 * std::mem::size_of::<f32>()) as i32,
                        );
                        gl.vertex_attrib_pointer_f32(
                            3,
                            2,
                            glow::FLOAT,
                            false,
                            stride,
                            (8 * std::mem::size_of::<f32>()) as i32,
                        );
                        last_tmesh_geom_key = Some(run.geom_key);
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
                            2,
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
                            base + 4 * col_size + uv_size,
                        );
                        gl.vertex_attrib_pointer_f32(
                            10,
                            2,
                            glow::FLOAT,
                            false,
                            inst_stride,
                            base + 4 * col_size + 2 * uv_size,
                        );
                        last_tmesh_instance_start = Some(run.instance_start);
                    }

                    let cam = render_list
                        .cameras
                        .get(run.camera as usize)
                        .copied()
                        .unwrap_or(state.projection);
                    let mvp_array: [[f32; 4]; 4] = cam.into();
                    gl.uniform_matrix_4_f32_slice(
                        Some(&state.tmesh_mvp_location),
                        false,
                        bytemuck::cast_slice(&mvp_array),
                    );

                    if last_bound_tex != Some(run.texture) {
                        gl.bind_texture(glow::TEXTURE_2D, Some(run.texture));
                        last_bound_tex = Some(run.texture);
                    }

                    let prim = match run.mode {
                        MeshMode::Triangles => glow::TRIANGLES,
                    };
                    let draw_start = if run.dynamic_geom {
                        run.vertex_start as i32
                    } else {
                        0
                    };
                    gl.draw_arrays_instanced(
                        prim,
                        draw_start,
                        run.vertex_count as i32,
                        run.instance_count as i32,
                    );
                    let tri_count = run.vertex_count / 3;
                    vertices =
                        vertices.saturating_add(tri_count.saturating_mul(run.instance_count));
                }
            }
        }
        gl.bind_vertex_array(None);
        gl.use_program(None);
    }

    state.gl_surface.swap_buffers(&state.gl_context)?;
    push_tmesh_debug_sample(state, tmesh_debug_frame);
    Ok(vertices)
}

pub fn capture_frame(state: &mut State) -> Result<RgbaImage, Box<dyn Error>> {
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Err(
            std::io::Error::other("Cannot capture screenshot at zero-sized viewport").into(),
        );
    }

    let byte_len = width as usize * height as usize * 4;
    let mut pixels = vec![0u8; byte_len];
    unsafe {
        state.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        state.gl.read_buffer(glow::FRONT);
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
    RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| std::io::Error::other("Failed to build screenshot image").into())
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

#[inline(always)]
fn tmesh_cache_key(vertices: &[crate::core::gfx::TexturedMeshVertex]) -> TMeshCacheKey {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(bytemuck::cast_slice(vertices));
    TMeshCacheKey {
        hash: hasher.finish(),
        len: vertices.len() as u32,
    }
}

#[inline(always)]
fn build_tmesh_vertex_raw(
    vertices: &[crate::core::gfx::TexturedMeshVertex],
) -> Vec<TexturedMeshVertexRaw> {
    let mut out = Vec::with_capacity(vertices.len());
    for v in vertices {
        out.push(TexturedMeshVertexRaw {
            pos: v.pos,
            uv: v.uv,
            tex_matrix_scale: v.tex_matrix_scale,
            color: v.color,
        });
    }
    out
}

fn try_get_or_promote_cached_tmesh_geom(
    gl: &glow::Context,
    cache_entries: &mut HashMap<TMeshCacheKey, TMeshCacheEntry>,
    cache_seen: &mut HashMap<TMeshCacheKey, TMeshSeenEntry>,
    next_cache_id: &mut u64,
    cache_total_bytes: &mut u64,
    debug_frame: &mut TMeshFrameDebug,
    frame: u64,
    vertices: &[crate::core::gfx::TexturedMeshVertex],
) -> Option<FrameTMeshGeom> {
    if vertices.len() < TMESH_CACHE_MIN_VERTS || vertices.is_empty() {
        return None;
    }

    let cache_key = tmesh_cache_key(vertices);
    if let Some(entry) = cache_entries.get_mut(&cache_key) {
        entry.last_used_frame = frame;
        debug_frame.cache_hits = debug_frame.cache_hits.saturating_add(1);
        return Some(FrameTMeshGeom::Cached {
            cache_id: entry.id,
            vertex_count: entry.vertex_count,
            buffer: entry.buffer,
        });
    }
    debug_frame.cache_misses = debug_frame.cache_misses.saturating_add(1);

    let promote = {
        let seen = cache_seen.entry(cache_key).or_insert(TMeshSeenEntry {
            hits: 0,
            last_seen_frame: frame,
        });
        if frame.saturating_sub(seen.last_seen_frame) > TMESH_CACHE_SEEN_TTL_FRAMES {
            seen.hits = 0;
        }
        seen.last_seen_frame = frame;
        seen.hits = seen.hits.saturating_add(1);
        seen.hits >= TMESH_CACHE_PROMOTE_HITS
    };
    if !promote {
        return None;
    }
    debug_frame.cache_promotions = debug_frame.cache_promotions.saturating_add(1);

    let raw = build_tmesh_vertex_raw(vertices);
    let bytes = (raw.len() * std::mem::size_of::<TexturedMeshVertexRaw>()) as u64;
    let buffer = match unsafe { gl.create_buffer() } {
        Ok(buffer) => buffer,
        Err(err) => {
            warn!("OpenGL textured mesh cache allocation failed: {err}");
            return None;
        }
    };
    unsafe {
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
        gl.buffer_data_u8_slice(
            glow::ARRAY_BUFFER,
            bytemuck::cast_slice(raw.as_slice()),
            glow::STATIC_DRAW,
        );
    }

    let cache_id = *next_cache_id;
    *next_cache_id = (*next_cache_id).wrapping_add(1).max(1);
    *cache_total_bytes = cache_total_bytes.saturating_add(bytes);
    cache_entries.insert(
        cache_key,
        TMeshCacheEntry {
            id: cache_id,
            vertex_count: raw.len() as u32,
            bytes,
            last_used_frame: frame,
            buffer,
        },
    );
    debug_frame.cache_evictions =
        debug_frame
            .cache_evictions
            .saturating_add(
                evict_tmesh_cache_entries(gl, cache_entries, cache_total_bytes, frame) as u64,
            );
    Some(FrameTMeshGeom::Cached {
        cache_id,
        vertex_count: raw.len() as u32,
        buffer,
    })
}

fn evict_tmesh_cache_entries(
    gl: &glow::Context,
    cache_entries: &mut HashMap<TMeshCacheKey, TMeshCacheEntry>,
    cache_total_bytes: &mut u64,
    frame: u64,
) -> u32 {
    let mut evicted: u32 = 0;
    while *cache_total_bytes > TMESH_CACHE_MAX_BYTES {
        let Some(oldest_key) = cache_entries
            .iter()
            .filter_map(|(key, entry)| (entry.last_used_frame != frame).then_some((*key, entry)))
            .min_by_key(|(_, entry)| entry.last_used_frame)
            .map(|(key, _)| key)
        else {
            break;
        };
        if let Some(entry) = cache_entries.remove(&oldest_key) {
            *cache_total_bytes = cache_total_bytes.saturating_sub(entry.bytes);
            unsafe {
                gl.delete_buffer(entry.buffer);
            }
            evicted = evicted.saturating_add(1);
        }
    }
    evicted
}

fn prune_tmesh_seen_entries(cache_seen: &mut HashMap<TMeshCacheKey, TMeshSeenEntry>, frame: u64) {
    cache_seen.retain(|_, seen| {
        frame.saturating_sub(seen.last_seen_frame) <= TMESH_CACHE_SEEN_TTL_FRAMES
    });
}

fn push_tmesh_debug_sample(state: &mut State, frame: TMeshFrameDebug) {
    if !state.tmesh_debug_enabled {
        return;
    }
    let accum = &mut state.tmesh_debug_accum;
    accum.frames = accum.frames.saturating_add(1);
    accum.cache_hits = accum.cache_hits.saturating_add(frame.cache_hits);
    accum.cache_misses = accum.cache_misses.saturating_add(frame.cache_misses);
    accum.cache_promotions = accum
        .cache_promotions
        .saturating_add(frame.cache_promotions);
    accum.cache_evictions = accum.cache_evictions.saturating_add(frame.cache_evictions);
    accum.dynamic_upload_vertices = accum
        .dynamic_upload_vertices
        .saturating_add(frame.dynamic_upload_vertices);

    if accum.frames < TMESH_DEBUG_LOG_EVERY_FRAMES {
        return;
    }
    let frames = u64::from(accum.frames).max(1);
    let dyn_avg = accum.dynamic_upload_vertices / frames;
    debug!(
        "OpenGL tmesh-cache: hit={} miss={} promote={} evict={} dyn_upload_vtx/frame={} cache_entries={} cache_mb={:.2}",
        accum.cache_hits,
        accum.cache_misses,
        accum.cache_promotions,
        accum.cache_evictions,
        dyn_avg,
        state.tmesh_cache_entries.len(),
        (state.tmesh_cache_total_bytes as f64) / (1024.0 * 1024.0)
    );
    *accum = TMeshDebugAccum::default();
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
        for (_, entry) in state.tmesh_cache_entries.drain() {
            state.gl.delete_buffer(entry.buffer);
        }
        state.tmesh_cache_seen.clear();
        state.tmesh_cache_total_bytes = 0;
        state.gl.delete_program(state.program);
        state.gl.delete_program(state.mesh_program);
        state.gl.delete_program(state.tmesh_program);
        state.gl.delete_vertex_array(state.shared_vao);
        state.gl.delete_buffer(state._shared_vbo);
        state.gl.delete_buffer(state._shared_ibo);
        state.gl.delete_vertex_array(state.mesh_vao);
        state.gl.delete_buffer(state.mesh_vbo);
        state.gl.delete_vertex_array(state.tmesh_vao);
        state.gl.delete_buffer(state.tmesh_vbo);
        state.gl.delete_buffer(state.tmesh_instance_vbo);
    }
    info!("OpenGL resources cleaned up.");
}

const TMESH_CACHE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const TMESH_CACHE_MIN_VERTS: usize = 32;
const TMESH_CACHE_PROMOTE_HITS: u8 = 2;
const TMESH_CACHE_SEEN_TTL_FRAMES: u64 = 1800;
const TMESH_DEBUG_LOG_EVERY_FRAMES: u32 = 300;

fn create_opengl_context(
    window: &Window,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
) -> Result<
    (
        Surface<WindowSurface>,
        PossiblyCurrentContext,
        glow::Context,
    ),
    Box<dyn Error>,
> {
    let display_handle = window.display_handle()?.as_raw();

    #[cfg(target_os = "windows")]
    let (display, vsync_logic) = {
        debug!("Using WGL for OpenGL context.");
        let preference = DisplayApiPreference::Wgl(None);
        let display = unsafe { Display::new(display_handle, preference)? };

        let vsync_logic = move |display: &Display| {
            debug!("Attempting to set VSync via wglSwapIntervalEXT...");
            type SwapIntervalFn = extern "system" fn(i32) -> i32;
            let proc_name = c"wglSwapIntervalEXT";
            let proc = display.get_proc_address(proc_name);
            if !proc.is_null() {
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
            } else {
                warn!("wglSwapIntervalEXT function not found. Cannot control VSync.");
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

            if let Err(e) = surface.set_swap_interval(&context, interval) {
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

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(0)
        .with_stencil_size(8)
        .with_transparency(false)
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

    let context_attributes = ContextAttributesBuilder::new()
        .with_debug(gfx_debug_enabled)
        .build(Some(raw_window_handle));
    let context =
        unsafe { display.create_context(&config, &context_attributes)? }.make_current(&surface)?;

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
) -> Result<
    (
        glow::Program,
        UniformLocation,
        UniformLocation,
        UniformLocation,
        UniformLocation,
        UniformLocation,
        UniformLocation,
        UniformLocation,
        UniformLocation,
        UniformLocation,
    ),
    String,
> {
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

        let vert = compile(
            glow::VERTEX_SHADER,
            include_str!("../shaders/opengl_shader.vert"),
        )?;
        let frag = compile(
            glow::FRAGMENT_SHADER,
            include_str!("../shaders/opengl_shader.frag"),
        )?;

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
        let color_location = get("u_color")?;
        let texture_location = get("u_texture")?;
        let uv_scale_location = get("u_uv_scale")?;
        let uv_offset_location = get("u_uv_offset")?;
        let local_offset_location = get("u_local_offset")?;
        let local_offset_rot_location = get("u_local_offset_rot_sin_cos")?;
        let edge_fade_location = get("u_edge_fade")?;
        let instanced_location = get("u_instanced")?;

        Ok((
            program,
            mvp_location,
            color_location,
            texture_location,
            uv_scale_location,
            uv_offset_location,
            local_offset_location,
            local_offset_rot_location,
            edge_fade_location,
            instanced_location,
        ))
    }
}

fn create_mesh_program(gl: &glow::Context) -> Result<(glow::Program, UniformLocation), String> {
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

        let vert = compile(
            glow::VERTEX_SHADER,
            include_str!("../shaders/opengl_mesh.vert"),
        )?;
        let frag = compile(
            glow::FRAGMENT_SHADER,
            include_str!("../shaders/opengl_mesh.frag"),
        )?;

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
) -> Result<(glow::Program, UniformLocation, UniformLocation), String> {
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

        let vert = compile(
            glow::VERTEX_SHADER,
            include_str!("../shaders/opengl_tmesh.vert"),
        )?;
        let frag = compile(
            glow::FRAGMENT_SHADER,
            include_str!("../shaders/opengl_tmesh.frag"),
        )?;

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

mod bytemuck {
    #[inline(always)]
    pub fn cast_slice<T, U>(slice: &[T]) -> &[U] {
        let (prefix, mid, suffix) = unsafe { slice.align_to::<U>() };
        debug_assert!(
            prefix.is_empty() && suffix.is_empty(),
            "cast_slice: misaligned cast"
        );
        mid
    }
}
