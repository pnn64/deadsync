use crate::core::gfx::{
    BlendMode, MeshMode, ObjectType, RenderList, SamplerDesc, SamplerFilter, SamplerWrap,
    Texture as RendererTexture,
};
use crate::core::space::ortho_for_window;
use cgmath::{Matrix4, Vector4};
use image::RgbaImage;
use log::{info, warn};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::{
    borrow::Cow,
    collections::HashMap,
    error::Error,
    hash::Hasher,
    mem,
    sync::{Arc, mpsc},
};
use twox_hash::XxHash64;
use wgpu::util::DeviceExt;
use winit::window::Window;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Api {
    Vulkan,
    OpenGL,
    #[cfg(target_os = "windows")]
    DirectX,
}

impl Api {
    #[inline(always)]
    const fn name(self) -> &'static str {
        match self {
            Self::Vulkan => "Vulkan",
            Self::OpenGL => "OpenGL",
            #[cfg(target_os = "windows")]
            Self::DirectX => "DirectX",
        }
    }

    #[inline(always)]
    const fn backends(self) -> wgpu::Backends {
        match self {
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::OpenGL => wgpu::Backends::GL,
            #[cfg(target_os = "windows")]
            Self::DirectX => wgpu::Backends::DX12,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InstanceRaw {
    center: [f32; 2],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    edge_fade: [f32; 4],
}

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

struct PipelineSet {
    alpha: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
    multiply: wgpu::RenderPipeline,
    subtract: wgpu::RenderPipeline,
}

impl PipelineSet {
    #[inline(always)]
    const fn get(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Alpha => &self.alpha,
            BlendMode::Add => &self.add,
            BlendMode::Multiply => &self.multiply,
            BlendMode::Subtract => &self.subtract,
        }
    }
}

struct MeshPipelineSet {
    alpha: wgpu::RenderPipeline,
    add: wgpu::RenderPipeline,
    multiply: wgpu::RenderPipeline,
    subtract: wgpu::RenderPipeline,
}

impl MeshPipelineSet {
    #[inline(always)]
    const fn get(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
        match mode {
            BlendMode::Alpha => &self.alpha,
            BlendMode::Add => &self.add,
            BlendMode::Multiply => &self.multiply,
            BlendMode::Subtract => &self.subtract,
        }
    }
}

enum ProjState {
    Immediates,
    Uniform {
        stride: u64,
        capacity: usize,
        buffer: wgpu::Buffer,
        group: wgpu::BindGroup,
        layout: wgpu::BindGroupLayout,
    },
}

// A handle to a wgpu texture and its bind group.
pub struct Texture {
    id: u64,
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: Arc<wgpu::BindGroup>,
    bind_group_repeat: Arc<wgpu::BindGroup>,
}

struct SpriteRun {
    start: u32,
    count: u32,
    blend: BlendMode,
    bind_group: Arc<wgpu::BindGroup>,
    key: u64,
    camera: u8,
}

struct TexturedMeshRun {
    vertex_start: u32,
    vertex_count: u32,
    dynamic_geom: bool,
    geom_key: u64,
    cached_vertex_buffer: Option<wgpu::Buffer>,
    instance_start: u32,
    instance_count: u32,
    mode: MeshMode,
    blend: BlendMode,
    bind_group: Arc<wgpu::BindGroup>,
    key: u64,
    camera: u8,
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

#[derive(Clone)]
enum FrameTMeshGeom {
    Dynamic {
        vertex_start: u32,
        vertex_count: u32,
    },
    Cached {
        cache_id: u64,
        vertex_count: u32,
        buffer: wgpu::Buffer,
    },
}

struct TMeshCacheEntry {
    id: u64,
    vertex_count: u32,
    bytes: u64,
    last_used_frame: u64,
    buffer: wgpu::Buffer,
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

enum Op {
    Sprite(SpriteRun),
    TexturedMesh(TexturedMeshRun),
    Mesh {
        start: u32,
        count: u32,
        mode: MeshMode,
        blend: BlendMode,
        camera: u8,
    },
}

struct OwnedWindowHandle(pub Arc<Window>);

impl HasWindowHandle for OwnedWindowHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}
impl HasDisplayHandle for OwnedWindowHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

pub struct State {
    api: Api,
    proj: ProjState,
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    projection: Matrix4<f32>,
    bind_layout: wgpu::BindGroupLayout,
    samplers: HashMap<SamplerDesc, wgpu::Sampler>,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: PipelineSet,
    mesh_shader: wgpu::ShaderModule,
    mesh_pipeline_layout: wgpu::PipelineLayout,
    mesh_pipelines: MeshPipelineSet,
    tmesh_shader: wgpu::ShaderModule,
    tmesh_pipeline_layout: wgpu::PipelineLayout,
    tmesh_pipelines: PipelineSet,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    scratch_instances: Vec<InstanceRaw>,
    scratch_mesh_vertices: Vec<crate::core::gfx::MeshVertex>,
    scratch_tmesh_vertices: Vec<TexturedMeshVertexRaw>,
    scratch_tmesh_instances: Vec<TexturedMeshInstanceRaw>,
    scratch_ops: Vec<Op>,
    mesh_vertex_buffer: wgpu::Buffer,
    mesh_vertex_capacity: usize,
    tmesh_vertex_buffer: wgpu::Buffer,
    tmesh_vertex_capacity: usize,
    tmesh_instance_buffer: wgpu::Buffer,
    tmesh_instance_capacity: usize,
    tmesh_cache_entries: HashMap<TMeshCacheKey, TMeshCacheEntry>,
    tmesh_cache_seen: HashMap<TMeshCacheKey, TMeshSeenEntry>,
    tmesh_cache_frame: u64,
    tmesh_cache_total_bytes: u64,
    next_tmesh_cache_id: u64,
    tmesh_debug_enabled: bool,
    tmesh_debug_accum: TMeshDebugAccum,
    window_size: (u32, u32),
    vsync_enabled: bool,
    next_texture_id: u64,
    screenshot_requested: bool,
    captured_frame: Option<RgbaImage>,
}

pub fn init_vulkan(
    window: Arc<Window>,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(Api::Vulkan, window, vsync_enabled, gfx_debug_enabled)
}

pub fn init_opengl(
    window: Arc<Window>,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(Api::OpenGL, window, vsync_enabled, gfx_debug_enabled)
}

#[cfg(target_os = "windows")]
pub fn init_dx12(
    window: Arc<Window>,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(Api::DirectX, window, vsync_enabled, gfx_debug_enabled)
}

fn init(
    api: Api,
    window: Arc<Window>,
    vsync_enabled: bool,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing {} (wgpu) backend...", api.name());
    if gfx_debug_enabled {
        info!("{} (wgpu) validation/debug is enabled.", api.name());
    }
    let instance_flags = if gfx_debug_enabled {
        wgpu::InstanceFlags::debugging()
    } else {
        wgpu::InstanceFlags::empty()
    };

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: api.backends(),
        flags: instance_flags,
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
    });

    let surface_target = OwnedWindowHandle(window.clone());
    let surface = instance
        .create_surface(surface_target)
        .map_err(|e| format!("Failed to create wgpu surface: {e}"))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .map_err(|e| format!("No suitable {} adapter found: {e}", api.name()))?;

    let want_immediates = matches!(api, Api::Vulkan);
    let use_immediates = want_immediates && adapter.features().contains(wgpu::Features::IMMEDIATES);
    if want_immediates && !use_immediates {
        warn!(
            "{} adapter does not support wgpu immediates; falling back to uniform projection.",
            api.name()
        );
    }

    let required_features = if use_immediates {
        wgpu::Features::IMMEDIATES
    } else {
        wgpu::Features::empty()
    };
    let required_limits = if use_immediates {
        wgpu::Limits {
            max_immediate_size: PROJ_BYTES as u32,
            ..wgpu::Limits::default()
        }
    } else {
        wgpu::Limits::default()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("deadsync wgpu device"),
        required_features,
        required_limits,
        memory_hints: wgpu::MemoryHints::Performance,
        trace: Default::default(),
        experimental_features: Default::default(),
    }))?;

    let size = window.inner_size();
    let caps = surface.get_capabilities(&adapter);
    let format = pick_format(&caps);
    let present_mode = pick_present_mode(&caps.present_modes, vsync_enabled);
    let alpha_mode = caps
        .alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Opaque);

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 0,
    };
    surface.configure(&device, &config);

    let projection = ortho_for_window(size.width, size.height);
    let proj = if use_immediates {
        ProjState::Immediates
    } else {
        init_uniform_proj(&device, &queue, projection)
    };

    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("wgpu texture layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });

    let (shader, pipeline_layout, pipelines) =
        build_pipeline_set(&device, &proj, &bind_layout, format);
    let (mesh_shader, mesh_pipeline_layout, mesh_pipelines) =
        build_mesh_pipeline_set(&device, &proj, format);
    let (tmesh_shader, tmesh_pipeline_layout, tmesh_pipelines) =
        build_textured_mesh_pipeline_set(&device, &proj, &bind_layout, format);

    let vertex_data = [
        Vertex {
            pos: [-0.5, -0.5],
            uv: [0.0, 1.0],
        },
        Vertex {
            pos: [0.5, -0.5],
            uv: [1.0, 1.0],
        },
        Vertex {
            pos: [0.5, 0.5],
            uv: [1.0, 0.0],
        },
        Vertex {
            pos: [-0.5, 0.5],
            uv: [0.0, 0.0],
        },
    ];
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("wgpu quad vertices"),
        contents: cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("wgpu quad indices"),
        contents: cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let instance_capacity = 64usize;
    let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu instance buffer"),
        size: (instance_capacity * mem::size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mesh_vertex_capacity = 1024usize;
    let mesh_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu mesh vertex buffer"),
        size: (mesh_vertex_capacity * mem::size_of::<crate::core::gfx::MeshVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let tmesh_vertex_capacity = 1024usize;
    let tmesh_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu textured-mesh vertex buffer"),
        size: (tmesh_vertex_capacity * mem::size_of::<TexturedMeshVertexRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let tmesh_instance_capacity = 256usize;
    let tmesh_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu textured-mesh instance buffer"),
        size: (tmesh_instance_capacity * mem::size_of::<TexturedMeshInstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    info!("{} (wgpu) backend initialized.", api.name());

    Ok(State {
        api,
        proj,
        _instance: instance,
        surface,
        adapter,
        device,
        queue,
        config,
        projection,
        bind_layout,
        samplers: HashMap::new(),
        shader,
        pipeline_layout,
        pipelines,
        mesh_shader,
        mesh_pipeline_layout,
        mesh_pipelines,
        tmesh_shader,
        tmesh_pipeline_layout,
        tmesh_pipelines,
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        instance_buffer,
        instance_capacity,
        scratch_instances: Vec::with_capacity(instance_capacity),
        scratch_mesh_vertices: Vec::with_capacity(mesh_vertex_capacity),
        scratch_tmesh_vertices: Vec::with_capacity(tmesh_vertex_capacity),
        scratch_tmesh_instances: Vec::with_capacity(tmesh_instance_capacity),
        scratch_ops: Vec::with_capacity(64),
        mesh_vertex_buffer,
        mesh_vertex_capacity,
        tmesh_vertex_buffer,
        tmesh_vertex_capacity,
        tmesh_instance_buffer,
        tmesh_instance_capacity,
        tmesh_cache_entries: HashMap::new(),
        tmesh_cache_seen: HashMap::new(),
        tmesh_cache_frame: 0,
        tmesh_cache_total_bytes: 0,
        next_tmesh_cache_id: 1,
        tmesh_debug_enabled: gfx_debug_enabled,
        tmesh_debug_accum: TMeshDebugAccum::default(),
        window_size: (size.width, size.height),
        vsync_enabled,
        next_texture_id: 1,
        screenshot_requested: false,
        captured_frame: None,
    })
}

fn init_uniform_proj(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    projection: Matrix4<f32>,
) -> ProjState {
    let align = device.limits().min_uniform_buffer_offset_alignment as u64;
    let stride = if align > 0 {
        ((PROJ_BYTES + align - 1) / align) * align
    } else {
        PROJ_BYTES
    };
    let capacity = 4usize;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu projection"),
        size: (capacity as u64) * stride,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let proj_array: [[f32; 4]; 4] = projection.into();
    queue.write_buffer(&buffer, 0, cast_slice(&proj_array));

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("wgpu proj layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: wgpu::BufferSize::new(PROJ_BYTES),
            },
            count: None,
        }],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wgpu proj group"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });

    ProjState::Uniform {
        stride,
        capacity,
        buffer,
        group,
        layout,
    }
}

pub fn create_texture(
    state: &mut State,
    image: &RgbaImage,
    sampler_desc: SamplerDesc,
) -> Result<Texture, Box<dyn Error>> {
    let size = wgpu::Extent3d {
        width: image.width(),
        height: image.height(),
        depth_or_array_layers: 1,
    };

    let texture = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wgpu texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    state.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        image.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * size.width),
            rows_per_image: Some(size.height),
        },
        size,
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = get_sampler(state, sampler_desc);
    let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wgpu texture bind group"),
        layout: &state.bind_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    });
    let sampler_repeat = get_sampler(
        state,
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..sampler_desc
        },
    );
    let bind_group_repeat = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wgpu texture bind group repeat"),
        layout: &state.bind_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(&sampler_repeat),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    });

    let id = state.next_texture_id;
    state.next_texture_id = state.next_texture_id.wrapping_add(1);

    Ok(Texture {
        id,
        _texture: texture,
        _view: view,
        bind_group: Arc::new(bind_group),
        bind_group_repeat: Arc::new(bind_group_repeat),
    })
}

#[inline(always)]
fn decompose_2d(m: [[f32; 4]; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let center = [m[3][0], m[3][1]];
    let c0 = [m[0][0], m[0][1]];
    let c1 = [m[1][0], m[1][1]];
    let sx = c0[0].hypot(c0[1]).max(1e-12);
    let sy = c1[0].hypot(c1[1]).max(1e-12);
    let cos_t = c0[0] / sx;
    let sin_t = c0[1] / sx;
    (center, [sx, sy], [sin_t, cos_t])
}

#[inline(always)]
fn pick_tex<'a>(api: Api, tex: &'a RendererTexture) -> Option<&'a Texture> {
    match (api, tex) {
        (Api::Vulkan, RendererTexture::VulkanWgpu(t)) => Some(t),
        (Api::OpenGL, RendererTexture::OpenGLWgpu(t)) => Some(t),
        #[cfg(target_os = "windows")]
        (Api::DirectX, RendererTexture::DirectX(t)) => Some(t),
        _ => None,
    }
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
pub fn request_screenshot(state: &mut State) {
    state.screenshot_requested = true;
}

pub fn capture_frame(state: &mut State) -> Result<RgbaImage, Box<dyn Error>> {
    state
        .captured_frame
        .take()
        .ok_or_else(|| std::io::Error::other("No captured screenshot frame available").into())
}

pub fn draw(
    state: &mut State,
    render_list: &RenderList<'_>,
    textures: &HashMap<String, RendererTexture>,
) -> Result<u32, Box<dyn Error>> {
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(0);
    }

    let api = state.api;
    let objects_len = render_list.objects.len();
    state.tmesh_cache_frame = state.tmesh_cache_frame.wrapping_add(1);
    prune_tmesh_seen_entries(&mut state.tmesh_cache_seen, state.tmesh_cache_frame);
    let mut tmesh_debug_frame = TMeshFrameDebug::default();

    {
        let cache_frame = state.tmesh_cache_frame;
        let device = &state.device;
        let tmesh_cache_entries = &mut state.tmesh_cache_entries;
        let tmesh_cache_seen = &mut state.tmesh_cache_seen;
        let next_tmesh_cache_id = &mut state.next_tmesh_cache_id;
        let tmesh_cache_total_bytes = &mut state.tmesh_cache_total_bytes;

        let instances = &mut state.scratch_instances;
        instances.clear();
        if instances.capacity() < objects_len {
            instances.reserve(objects_len - instances.capacity());
        }

        let mesh_vertices = &mut state.scratch_mesh_vertices;
        mesh_vertices.clear();
        let want_mesh = objects_len.saturating_mul(4);
        if mesh_vertices.capacity() < want_mesh {
            mesh_vertices.reserve(want_mesh - mesh_vertices.capacity());
        }

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
        let mut tmesh_geom: HashMap<TMeshGeomKey, FrameTMeshGeom> = HashMap::new();
        tmesh_geom.reserve(objects_len);

        let ops = &mut state.scratch_ops;
        ops.clear();
        if ops.capacity() < objects_len {
            ops.reserve(objects_len - ops.capacity());
        }

        for obj in &render_list.objects {
            match &obj.object_type {
                ObjectType::Sprite {
                    texture_id,
                    tint,
                    uv_scale,
                    uv_offset,
                    local_offset,
                    local_offset_rot_sin_cos,
                    edge_fade,
                } => {
                    let tex = lookup_texture_case_insensitive(textures, texture_id.as_ref())
                        .and_then(|t| pick_tex(api, t));
                    let Some(tex) = tex else {
                        continue;
                    };

                    let model: [[f32; 4]; 4] = obj.transform.into();
                    let (center, size, sincos) = decompose_2d(model);
                    let start = instances.len() as u32;
                    instances.push(InstanceRaw {
                        center,
                        size,
                        rot_sin_cos: sincos,
                        tint: *tint,
                        uv_scale: *uv_scale,
                        uv_offset: *uv_offset,
                        local_offset: *local_offset,
                        local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                        edge_fade: *edge_fade,
                    });

                    if let Some(Op::Sprite(last)) = ops.last_mut()
                        && last.key == tex.id
                        && last.blend == obj.blend
                        && last.camera == obj.camera
                    {
                        last.count += 1;
                        continue;
                    }

                    ops.push(Op::Sprite(SpriteRun {
                        start,
                        count: 1,
                        blend: obj.blend,
                        bind_group: tex.bind_group.clone(),
                        key: tex.id,
                        camera: obj.camera,
                    }));
                }
                ObjectType::Mesh { vertices, mode } => {
                    if vertices.is_empty() {
                        continue;
                    }
                    let start = mesh_vertices.len() as u32;
                    mesh_vertices.reserve(vertices.len());
                    for v in vertices.iter() {
                        let p = obj.transform * Vector4::new(v.pos[0], v.pos[1], 0.0, 1.0);
                        mesh_vertices.push(crate::core::gfx::MeshVertex {
                            pos: [p.x, p.y],
                            color: v.color,
                        });
                    }

                    ops.push(Op::Mesh {
                        start,
                        count: vertices.len() as u32,
                        mode: *mode,
                        blend: obj.blend,
                        camera: obj.camera,
                    });
                }
                ObjectType::TexturedMesh {
                    texture_id,
                    vertices,
                    mode,
                    uv_scale,
                    uv_offset,
                    uv_tex_shift,
                } => {
                    if vertices.is_empty() {
                        continue;
                    }
                    let tex = lookup_texture_case_insensitive(textures, texture_id.as_ref())
                        .and_then(|t| pick_tex(api, t));
                    let Some(tex) = tex else {
                        continue;
                    };
                    let geom_key = TMeshGeomKey {
                        ptr: vertices.as_ptr() as usize,
                        len: vertices.len(),
                    };
                    let resolved_geom = if let Some(geom) = tmesh_geom.get(&geom_key) {
                        geom.clone()
                    } else {
                        let geom = if let Some(cached) = try_get_or_promote_cached_tmesh_geom(
                            device,
                            tmesh_cache_entries,
                            tmesh_cache_seen,
                            next_tmesh_cache_id,
                            tmesh_cache_total_bytes,
                            &mut tmesh_debug_frame,
                            cache_frame,
                            vertices.as_ref(),
                        ) {
                            cached
                        } else {
                            let start = tmesh_vertices.len() as u32;
                            let count = vertices.len() as u32;
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
                                .saturating_add(count as u64);
                            FrameTMeshGeom::Dynamic {
                                vertex_start: start,
                                vertex_count: count,
                            }
                        };
                        tmesh_geom.insert(geom_key, geom.clone());
                        geom
                    };
                    let (
                        vertex_start,
                        vertex_count,
                        dynamic_geom,
                        geom_run_key,
                        cached_vertex_buffer,
                    ) = match resolved_geom {
                        FrameTMeshGeom::Dynamic {
                            vertex_start,
                            vertex_count,
                        } => {
                            let key = (((vertex_start as u64) << 32) | (vertex_count as u64))
                                .wrapping_shl(1);
                            (vertex_start, vertex_count, true, key, None)
                        }
                        FrameTMeshGeom::Cached {
                            cache_id,
                            vertex_count,
                            buffer,
                        } => {
                            let key = cache_id.wrapping_shl(1) | 1;
                            (0, vertex_count, false, key, Some(buffer))
                        }
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
                    let key = tex.id.wrapping_shl(1) | 1;
                    if let Some(Op::TexturedMesh(last)) = ops.last_mut()
                        && last.key == key
                        && last.blend == obj.blend
                        && last.camera == obj.camera
                        && last.mode == *mode
                        && last.dynamic_geom == dynamic_geom
                        && last.geom_key == geom_run_key
                        && last.vertex_count == vertex_count
                        && last.instance_start + last.instance_count == instance_start
                    {
                        last.instance_count += 1;
                        continue;
                    }

                    ops.push(Op::TexturedMesh(TexturedMeshRun {
                        vertex_start,
                        vertex_count,
                        dynamic_geom,
                        geom_key: geom_run_key,
                        cached_vertex_buffer,
                        instance_start,
                        instance_count: 1,
                        mode: *mode,
                        blend: obj.blend,
                        bind_group: tex.bind_group_repeat.clone(),
                        key,
                        camera: obj.camera,
                    }));
                }
            }
        }
    }

    let instance_len = state.scratch_instances.len();
    ensure_instance_capacity(state, instance_len);
    if instance_len > 0 {
        state.queue.write_buffer(
            &state.instance_buffer,
            0,
            cast_slice(state.scratch_instances.as_slice()),
        );
    }
    let mesh_len = state.scratch_mesh_vertices.len();
    ensure_mesh_vertex_capacity(state, mesh_len);
    if mesh_len > 0 {
        state.queue.write_buffer(
            &state.mesh_vertex_buffer,
            0,
            cast_slice(state.scratch_mesh_vertices.as_slice()),
        );
    }
    let tmesh_len = state.scratch_tmesh_vertices.len();
    ensure_tmesh_vertex_capacity(state, tmesh_len);
    if tmesh_len > 0 {
        state.queue.write_buffer(
            &state.tmesh_vertex_buffer,
            0,
            cast_slice(state.scratch_tmesh_vertices.as_slice()),
        );
    }
    let tmesh_instance_len = state.scratch_tmesh_instances.len();
    ensure_tmesh_instance_capacity(state, tmesh_instance_len);
    if tmesh_instance_len > 0 {
        state.queue.write_buffer(
            &state.tmesh_instance_buffer,
            0,
            cast_slice(state.scratch_tmesh_instances.as_slice()),
        );
    }
    upload_projections(state, &render_list.cameras);

    let frame = match state.surface.get_current_texture() {
        Ok(f) => f,
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
            reconfigure_surface(state);
            return Ok(0);
        }
        Err(wgpu::SurfaceError::OutOfMemory) => return Err("Surface out of memory".into()),
        Err(wgpu::SurfaceError::Timeout) => return Ok(0),
        Err(wgpu::SurfaceError::Other) => return Ok(0),
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wgpu encoder"),
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wgpu render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(render_list.clear_color[0]),
                        g: f64::from(render_list.clear_color[1]),
                        b: f64::from(render_list.clear_color[2]),
                        a: f64::from(render_list.clear_color[3]),
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        let camera_count = render_list.cameras.len();
        let texture_group = match state.proj {
            ProjState::Immediates => 0,
            ProjState::Uniform { .. } => 1,
        };

        let mut last_kind: Option<u8> = None; // 0=sprite, 1=mesh, 2=textured mesh
        let mut last_blend: Option<BlendMode> = None;
        let mut last_bind: Option<u64> = None;
        let mut last_camera: Option<u8> = None;
        let mut last_tmesh_geom_key: Option<u64> = None;
        for op in &state.scratch_ops {
            match op {
                Op::Sprite(run) => {
                    if last_kind != Some(0) {
                        pass.set_vertex_buffer(0, state.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, state.instance_buffer.slice(..));
                        pass.set_index_buffer(
                            state.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        last_kind = Some(0);
                        last_blend = None;
                        last_bind = None;
                        last_camera = None;
                        last_tmesh_geom_key = None;
                    }
                    if last_blend != Some(run.blend) {
                        pass.set_pipeline(state.pipelines.get(run.blend));
                        last_blend = Some(run.blend);
                        last_bind = None;
                    }
                    if last_camera != Some(run.camera) {
                        set_camera(
                            &mut pass,
                            &state.proj,
                            run.camera,
                            camera_count,
                            &render_list.cameras,
                            state.projection,
                        );
                        last_camera = Some(run.camera);
                    }
                    if last_bind != Some(run.key) {
                        pass.set_bind_group(texture_group, Some(run.bind_group.as_ref()), &[]);
                        last_bind = Some(run.key);
                    }
                    pass.draw_indexed(0..state.index_count, 0, run.start..(run.start + run.count));
                }
                Op::Mesh {
                    start,
                    count,
                    mode,
                    blend,
                    camera,
                } => {
                    if *count == 0 {
                        continue;
                    }
                    if last_kind != Some(1) {
                        pass.set_vertex_buffer(0, state.mesh_vertex_buffer.slice(..));
                        last_kind = Some(1);
                        last_blend = None;
                        last_bind = None;
                        last_camera = None;
                        last_tmesh_geom_key = None;
                    }
                    if last_blend != Some(*blend) {
                        pass.set_pipeline(state.mesh_pipelines.get(*blend));
                        last_blend = Some(*blend);
                    }
                    if last_camera != Some(*camera) {
                        set_camera(
                            &mut pass,
                            &state.proj,
                            *camera,
                            camera_count,
                            &render_list.cameras,
                            state.projection,
                        );
                        last_camera = Some(*camera);
                    }
                    match mode {
                        MeshMode::Triangles => pass.draw(*start..(*start + *count), 0..1),
                    }
                }
                Op::TexturedMesh(run) => {
                    if run.vertex_count == 0 || run.instance_count == 0 {
                        continue;
                    }
                    if last_kind != Some(2) {
                        pass.set_vertex_buffer(1, state.tmesh_instance_buffer.slice(..));
                        last_kind = Some(2);
                        last_blend = None;
                        last_bind = None;
                        last_camera = None;
                        last_tmesh_geom_key = None;
                    }
                    if last_blend != Some(run.blend) {
                        pass.set_pipeline(state.tmesh_pipelines.get(run.blend));
                        last_blend = Some(run.blend);
                        last_bind = None;
                    }
                    if last_camera != Some(run.camera) {
                        set_camera(
                            &mut pass,
                            &state.proj,
                            run.camera,
                            camera_count,
                            &render_list.cameras,
                            state.projection,
                        );
                        last_camera = Some(run.camera);
                    }
                    if last_bind != Some(run.key) {
                        pass.set_bind_group(texture_group, Some(run.bind_group.as_ref()), &[]);
                        last_bind = Some(run.key);
                    }
                    if last_tmesh_geom_key != Some(run.geom_key) {
                        if run.dynamic_geom {
                            pass.set_vertex_buffer(0, state.tmesh_vertex_buffer.slice(..));
                        } else if let Some(buffer) = run.cached_vertex_buffer.as_ref() {
                            pass.set_vertex_buffer(0, buffer.slice(..));
                        } else {
                            continue;
                        }
                        last_tmesh_geom_key = Some(run.geom_key);
                    }
                    let draw_start = if run.dynamic_geom {
                        run.vertex_start
                    } else {
                        0
                    };
                    let draw_end = draw_start + run.vertex_count;
                    match run.mode {
                        MeshMode::Triangles => pass.draw(
                            draw_start..draw_end,
                            run.instance_start..(run.instance_start + run.instance_count),
                        ),
                    }
                }
            }
        }
        drop(pass);
    }

    let screenshot_readback = if state.screenshot_requested {
        state.screenshot_requested = false;
        let width = state.config.width.max(1);
        let height = state.config.height.max(1);
        let bytes_per_row = 4 * width;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = bytes_per_row.div_ceil(align) * align;
        let readback_size = padded_bytes_per_row as u64 * height as u64;
        let readback_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wgpu screenshot readback"),
            size: readback_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Some((
            readback_buffer,
            width as usize,
            height as usize,
            padded_bytes_per_row as usize,
            state.config.format,
        ))
    } else {
        None
    };

    state.queue.submit(Some(encoder.finish()));
    frame.present();
    if let Some((readback_buffer, width, height, padded_row_bytes, format)) = screenshot_readback {
        let slice = readback_buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let _ = state.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        if rx.recv().is_ok_and(|res| res.is_ok()) {
            let data = slice.get_mapped_range();
            let row_bytes = width * 4;
            let mut rgba = vec![0u8; row_bytes * height];
            let swap_rb = matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            for y in 0..height {
                let src = y * padded_row_bytes;
                let dst = (height - 1 - y) * row_bytes;
                if swap_rb {
                    let mut x = 0usize;
                    while x < width {
                        let s = src + x * 4;
                        let d = dst + x * 4;
                        rgba[d] = data[s + 2];
                        rgba[d + 1] = data[s + 1];
                        rgba[d + 2] = data[s];
                        rgba[d + 3] = data[s + 3];
                        x += 1;
                    }
                } else {
                    rgba[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
                }
            }
            drop(data);
            readback_buffer.unmap();
            if let Some(img) = RgbaImage::from_raw(width as u32, height as u32, rgba) {
                state.captured_frame = Some(img);
            }
        } else {
            readback_buffer.unmap();
            state.captured_frame = None;
            warn!("wgpu screenshot readback failed: map_async returned error");
        }
    }
    push_tmesh_debug_sample(state, tmesh_debug_frame);

    let mut tmesh_vpf = 0u32;
    for op in &state.scratch_ops {
        if let Op::TexturedMesh(run) = op {
            let tri_count = run.vertex_count / 3;
            tmesh_vpf = tmesh_vpf.saturating_add(tri_count.saturating_mul(run.instance_count));
        }
    }
    Ok((instance_len as u32) * 4 + mesh_len as u32 + tmesh_vpf)
}

#[inline(always)]
fn tmesh_cache_key(vertices: &[crate::core::gfx::TexturedMeshVertex]) -> TMeshCacheKey {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(cast_slice(vertices));
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
    device: &wgpu::Device,
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
            buffer: entry.buffer.clone(),
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

    let raw = build_tmesh_vertex_raw(vertices);
    let vertex_count = raw.len() as u32;
    let bytes = (raw.len() * mem::size_of::<TexturedMeshVertexRaw>()) as u64;
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("wgpu textured-mesh cached vertices"),
        contents: cast_slice(raw.as_slice()),
        usage: wgpu::BufferUsages::VERTEX,
    });
    debug_frame.cache_promotions = debug_frame.cache_promotions.saturating_add(1);
    let cache_id = *next_cache_id;
    *next_cache_id = (*next_cache_id).wrapping_add(1).max(1);
    *cache_total_bytes = cache_total_bytes.saturating_add(bytes);
    cache_entries.insert(
        cache_key,
        TMeshCacheEntry {
            id: cache_id,
            vertex_count,
            bytes,
            last_used_frame: frame,
            buffer: buffer.clone(),
        },
    );
    debug_frame.cache_evictions = debug_frame
        .cache_evictions
        .saturating_add(evict_tmesh_cache_entries(cache_entries, cache_total_bytes) as u64);
    Some(FrameTMeshGeom::Cached {
        cache_id,
        vertex_count,
        buffer,
    })
}

fn evict_tmesh_cache_entries(
    cache_entries: &mut HashMap<TMeshCacheKey, TMeshCacheEntry>,
    cache_total_bytes: &mut u64,
) -> u32 {
    let mut evicted: u32 = 0;
    while *cache_total_bytes > TMESH_CACHE_MAX_BYTES {
        let Some(oldest_key) = cache_entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used_frame)
            .map(|(key, _)| *key)
        else {
            break;
        };
        if let Some(entry) = cache_entries.remove(&oldest_key) {
            *cache_total_bytes = cache_total_bytes.saturating_sub(entry.bytes);
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
    info!(
        "{} (wgpu) tmesh-cache: hit={} miss={} promote={} evict={} dyn_upload_vtx/frame={} cache_entries={} cache_mb={:.2}",
        state.api.name(),
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

fn upload_projections(state: &mut State, cameras: &[Matrix4<f32>]) {
    let ProjState::Uniform { .. } = state.proj else {
        return;
    };
    let needed = cameras.len().saturating_add(1).max(1);
    ensure_projection_capacity(state, needed);

    let ProjState::Uniform { buffer, stride, .. } = &state.proj else {
        return;
    };
    for (i, &vp) in cameras.iter().enumerate() {
        let arr: [[f32; 4]; 4] = vp.into();
        let offset = (i as u64) * *stride;
        state.queue.write_buffer(buffer, offset, cast_slice(&arr));
    }
    let fallback_offset = (cameras.len() as u64) * *stride;
    let fallback: [[f32; 4]; 4] = state.projection.into();
    state
        .queue
        .write_buffer(buffer, fallback_offset, cast_slice(&fallback));
}

fn set_camera(
    pass: &mut wgpu::RenderPass<'_>,
    proj: &ProjState,
    camera: u8,
    camera_count: usize,
    cameras: &[Matrix4<f32>],
    fallback: Matrix4<f32>,
) {
    match proj {
        ProjState::Immediates => {
            let vp = cameras.get(camera as usize).copied().unwrap_or(fallback);
            let vp_array: [[f32; 4]; 4] = vp.into();
            pass.set_immediates(0, cast_slice(&vp_array));
        }
        ProjState::Uniform { group, stride, .. } => {
            let idx = if (camera as usize) < camera_count {
                camera as usize
            } else {
                camera_count
            };
            let offset = ((idx as u64) * *stride) as u32;
            pass.set_bind_group(0, group, &[offset]);
        }
    }
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    if width == 0 || height == 0 {
        warn!("Ignoring resize to zero dimensions for wgpu backend.");
        return;
    }
    state.window_size = (width, height);
    state.projection = ortho_for_window(width, height);
    reconfigure_surface(state);
}

pub fn cleanup(state: &mut State) {
    info!("{} (wgpu) backend cleanup complete.", state.api.name());
}

fn ensure_instance_capacity(state: &mut State, needed: usize) {
    if needed <= state.instance_capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(64);
    state.instance_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu instance buffer"),
        size: (new_cap * mem::size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    state.instance_capacity = new_cap;
}

fn ensure_mesh_vertex_capacity(state: &mut State, needed: usize) {
    if needed <= state.mesh_vertex_capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(1024);
    state.mesh_vertex_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu mesh vertex buffer"),
        size: (new_cap * mem::size_of::<crate::core::gfx::MeshVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    state.mesh_vertex_capacity = new_cap;
}

fn ensure_tmesh_vertex_capacity(state: &mut State, needed: usize) {
    if needed <= state.tmesh_vertex_capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(1024);
    state.tmesh_vertex_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu textured-mesh vertex buffer"),
        size: (new_cap * mem::size_of::<TexturedMeshVertexRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    state.tmesh_vertex_capacity = new_cap;
}

fn ensure_tmesh_instance_capacity(state: &mut State, needed: usize) {
    if needed <= state.tmesh_instance_capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(256);
    state.tmesh_instance_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu textured-mesh instance buffer"),
        size: (new_cap * mem::size_of::<TexturedMeshInstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    state.tmesh_instance_capacity = new_cap;
}

fn ensure_projection_capacity(state: &mut State, needed: usize) {
    let ProjState::Uniform {
        stride,
        capacity,
        buffer,
        group,
        layout,
    } = &mut state.proj
    else {
        return;
    };
    if needed <= *capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(4);
    *buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu projection"),
        size: (new_cap as u64) * *stride,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    *group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("wgpu proj group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    *capacity = new_cap;
}

fn reconfigure_surface(state: &mut State) {
    if state.window_size.0 == 0 || state.window_size.1 == 0 {
        return;
    }
    let caps = state.surface.get_capabilities(&state.adapter);
    let new_format = pick_format(&caps);
    let format_changed = new_format != state.config.format;
    state.config.format = new_format;
    state.config.present_mode = pick_present_mode(&caps.present_modes, state.vsync_enabled);
    state.config.alpha_mode = caps
        .alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Opaque);
    state.config.width = state.window_size.0;
    state.config.height = state.window_size.1;
    state.surface.configure(&state.device, &state.config);

    if matches!(state.proj, ProjState::Uniform { .. }) {
        let fallback: [[f32; 4]; 4] = state.projection.into();
        if let ProjState::Uniform { buffer, .. } = &state.proj {
            state.queue.write_buffer(buffer, 0, cast_slice(&fallback));
        }
    }

    if format_changed {
        let (shader, pipeline_layout, pipelines) = build_pipeline_set(
            &state.device,
            &state.proj,
            &state.bind_layout,
            state.config.format,
        );
        let (mesh_shader, mesh_pipeline_layout, mesh_pipelines) =
            build_mesh_pipeline_set(&state.device, &state.proj, state.config.format);
        let (tmesh_shader, tmesh_pipeline_layout, tmesh_pipelines) =
            build_textured_mesh_pipeline_set(
                &state.device,
                &state.proj,
                &state.bind_layout,
                state.config.format,
            );
        state.shader = shader;
        state.pipeline_layout = pipeline_layout;
        state.pipelines = pipelines;
        state.mesh_shader = mesh_shader;
        state.mesh_pipeline_layout = mesh_pipeline_layout;
        state.mesh_pipelines = mesh_pipelines;
        state.tmesh_shader = tmesh_shader;
        state.tmesh_pipeline_layout = tmesh_pipeline_layout;
        state.tmesh_pipelines = tmesh_pipelines;
    }
}

fn pick_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    // Avoid sRGB conversion to keep colors consistent across backends.
    caps.formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .unwrap_or_else(|| caps.formats[0])
}

fn pick_present_mode(modes: &[wgpu::PresentMode], vsync: bool) -> wgpu::PresentMode {
    let preferred = if vsync {
        [
            wgpu::PresentMode::AutoVsync,
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::FifoRelaxed,
        ]
    } else {
        [
            wgpu::PresentMode::AutoNoVsync,
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
        ]
    };

    preferred
        .iter()
        .copied()
        .find(|p| modes.contains(p))
        .unwrap_or_else(|| modes[0])
}

fn blend_state(mode: BlendMode) -> Option<wgpu::BlendState> {
    let comp = |src, dst, op| wgpu::BlendComponent {
        src_factor: src,
        dst_factor: dst,
        operation: op,
    };
    match mode {
        BlendMode::Alpha => Some(wgpu::BlendState {
            color: comp(
                wgpu::BlendFactor::SrcAlpha,
                wgpu::BlendFactor::OneMinusSrcAlpha,
                wgpu::BlendOperation::Add,
            ),
            alpha: comp(
                wgpu::BlendFactor::SrcAlpha,
                wgpu::BlendFactor::OneMinusSrcAlpha,
                wgpu::BlendOperation::Add,
            ),
        }),
        BlendMode::Add => Some(wgpu::BlendState {
            color: comp(
                wgpu::BlendFactor::SrcAlpha,
                wgpu::BlendFactor::One,
                wgpu::BlendOperation::Add,
            ),
            alpha: comp(
                wgpu::BlendFactor::SrcAlpha,
                wgpu::BlendFactor::One,
                wgpu::BlendOperation::Add,
            ),
        }),
        BlendMode::Multiply => Some(wgpu::BlendState {
            color: comp(
                wgpu::BlendFactor::Dst,
                wgpu::BlendFactor::Zero,
                wgpu::BlendOperation::Add,
            ),
            alpha: comp(
                wgpu::BlendFactor::DstAlpha,
                wgpu::BlendFactor::Zero,
                wgpu::BlendOperation::Add,
            ),
        }),
        BlendMode::Subtract => Some(wgpu::BlendState {
            color: comp(
                wgpu::BlendFactor::One,
                wgpu::BlendFactor::One,
                wgpu::BlendOperation::ReverseSubtract,
            ),
            alpha: comp(
                wgpu::BlendFactor::One,
                wgpu::BlendFactor::One,
                wgpu::BlendOperation::ReverseSubtract,
            ),
        }),
    }
}

fn build_pipeline_set(
    device: &wgpu::Device,
    proj: &ProjState,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (wgpu::ShaderModule, wgpu::PipelineLayout, PipelineSet) {
    let shader_src = match proj {
        ProjState::Immediates => SHADER_IMM,
        ProjState::Uniform { .. } => SHADER_UBO,
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wgpu shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_src)),
    });

    let pipeline_layout = match proj {
        ProjState::Immediates => {
            let layouts = [bind_layout];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: PROJ_BYTES as u32,
            })
        }
        ProjState::Uniform { layout, .. } => {
            let layouts = [layout, bind_layout];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            })
        }
    };

    let pipelines = PipelineSet {
        alpha: build_pipeline(device, &pipeline_layout, format, BlendMode::Alpha, &shader),
        add: build_pipeline(device, &pipeline_layout, format, BlendMode::Add, &shader),
        multiply: build_pipeline(
            device,
            &pipeline_layout,
            format,
            BlendMode::Multiply,
            &shader,
        ),
        subtract: build_pipeline(
            device,
            &pipeline_layout,
            format,
            BlendMode::Subtract,
            &shader,
        ),
    };

    (shader, pipeline_layout, pipelines)
}

fn build_mesh_pipeline_set(
    device: &wgpu::Device,
    proj: &ProjState,
    format: wgpu::TextureFormat,
) -> (wgpu::ShaderModule, wgpu::PipelineLayout, MeshPipelineSet) {
    let shader_src = match proj {
        ProjState::Immediates => MESH_SHADER_IMM,
        ProjState::Uniform { .. } => MESH_SHADER_UBO,
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wgpu mesh shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_src)),
    });

    let pipeline_layout = match proj {
        ProjState::Immediates => device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wgpu mesh pipeline layout"),
            bind_group_layouts: &[],
            immediate_size: PROJ_BYTES as u32,
        }),
        ProjState::Uniform { layout, .. } => {
            let layouts = [layout];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu mesh pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            })
        }
    };

    let pipelines = MeshPipelineSet {
        alpha: build_mesh_pipeline(device, &pipeline_layout, format, BlendMode::Alpha, &shader),
        add: build_mesh_pipeline(device, &pipeline_layout, format, BlendMode::Add, &shader),
        multiply: build_mesh_pipeline(
            device,
            &pipeline_layout,
            format,
            BlendMode::Multiply,
            &shader,
        ),
        subtract: build_mesh_pipeline(
            device,
            &pipeline_layout,
            format,
            BlendMode::Subtract,
            &shader,
        ),
    };

    (shader, pipeline_layout, pipelines)
}

fn build_textured_mesh_pipeline_set(
    device: &wgpu::Device,
    proj: &ProjState,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (wgpu::ShaderModule, wgpu::PipelineLayout, PipelineSet) {
    let shader_src = match proj {
        ProjState::Immediates => TMESH_SHADER_IMM,
        ProjState::Uniform { .. } => TMESH_SHADER_UBO,
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wgpu textured-mesh shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_src)),
    });

    let pipeline_layout = match proj {
        ProjState::Immediates => {
            let layouts = [bind_layout];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu textured-mesh pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: PROJ_BYTES as u32,
            })
        }
        ProjState::Uniform { layout, .. } => {
            let layouts = [layout, bind_layout];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu textured-mesh pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            })
        }
    };

    let pipelines = PipelineSet {
        alpha: build_tmesh_pipeline(device, &pipeline_layout, format, BlendMode::Alpha, &shader),
        add: build_tmesh_pipeline(device, &pipeline_layout, format, BlendMode::Add, &shader),
        multiply: build_tmesh_pipeline(
            device,
            &pipeline_layout,
            format,
            BlendMode::Multiply,
            &shader,
        ),
        subtract: build_tmesh_pipeline(
            device,
            &pipeline_layout,
            format,
            BlendMode::Subtract,
            &shader,
        ),
    };

    (shader, pipeline_layout, pipelines)
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    mode: BlendMode,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgpu pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_layout(), instance_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend_state(mode),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn build_mesh_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    mode: BlendMode,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgpu mesh pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[mesh_vertex_layout()],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend_state(mode),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn build_tmesh_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    mode: BlendMode,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgpu textured-mesh pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[
                textured_mesh_vertex_layout(),
                textured_mesh_instance_layout(),
            ],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend_state(mode),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

const fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERT_ATTRS,
    }
}

const fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<InstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &INSTANCE_ATTRS,
    }
}

const fn mesh_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<crate::core::gfx::MeshVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &MESH_ATTRS,
    }
}

const fn textured_mesh_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<TexturedMeshVertexRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &TMESH_ATTRS,
    }
}

const fn textured_mesh_instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<TexturedMeshInstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &TMESH_INSTANCE_ATTRS,
    }
}

const VERT_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
];

const MESH_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2, // pos
    1 => Float32x4, // color
];

const TMESH_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x2, // pos
    1 => Float32x2, // uv
    2 => Float32x4, // color
    3 => Float32x2, // tex-matrix scale
];

const TMESH_INSTANCE_ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    4 => Float32x4, // model column 0
    5 => Float32x4, // model column 1
    6 => Float32x4, // model column 2
    7 => Float32x4, // model column 3
    8 => Float32x2, // uv scale
    9 => Float32x2, // uv offset
    10 => Float32x2, // uv texture-matrix shift
];

const INSTANCE_ATTRS: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    2 => Float32x2, // center
    3 => Float32x2, // size
    4 => Float32x2, // sin/cos
    5 => Float32x4, // tint
    6 => Float32x2, // uv scale
    7 => Float32x2, // uv offset
    8 => Float32x2, // local offset
    9 => Float32x2, // local offset sin/cos
    10 => Float32x4, // edge fade
];

const PROJ_BYTES: u64 = mem::size_of::<[[f32; 4]; 4]>() as u64;
const TMESH_CACHE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const TMESH_CACHE_MIN_VERTS: usize = 32;
const TMESH_CACHE_PROMOTE_HITS: u8 = 2;
const TMESH_CACHE_SEEN_TTL_FRAMES: u64 = 1800;
const TMESH_DEBUG_LOG_EVERY_FRAMES: u32 = 300;

#[inline(always)]
const fn cast_slice<T>(data: &[T]) -> &[u8] {
    let len = std::mem::size_of_val(data);
    unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), len) }
}

#[inline(always)]
const fn wgpu_filter_mode(filter: SamplerFilter) -> wgpu::FilterMode {
    match filter {
        SamplerFilter::Linear => wgpu::FilterMode::Linear,
        SamplerFilter::Nearest => wgpu::FilterMode::Nearest,
    }
}

#[inline(always)]
const fn wgpu_address_mode(wrap: SamplerWrap) -> wgpu::AddressMode {
    match wrap {
        SamplerWrap::Clamp => wgpu::AddressMode::ClampToEdge,
        SamplerWrap::Repeat => wgpu::AddressMode::Repeat,
    }
}

#[inline(always)]
fn sampler_descriptor(desc: SamplerDesc) -> wgpu::SamplerDescriptor<'static> {
    let filter = wgpu_filter_mode(desc.filter);
    let address = wgpu_address_mode(desc.wrap);
    let mip_filter = if desc.mipmaps {
        match desc.filter {
            SamplerFilter::Linear => wgpu::MipmapFilterMode::Linear,
            SamplerFilter::Nearest => wgpu::MipmapFilterMode::Nearest,
        }
    } else {
        wgpu::MipmapFilterMode::Nearest
    };
    wgpu::SamplerDescriptor {
        label: Some("wgpu sampler"),
        address_mode_u: address,
        address_mode_v: address,
        address_mode_w: address,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: mip_filter,
        ..Default::default()
    }
}

fn get_sampler(state: &mut State, desc: SamplerDesc) -> wgpu::Sampler {
    if let Some(existing) = state.samplers.get(&desc) {
        return existing.clone();
    }
    let sampler = state.device.create_sampler(&sampler_descriptor(desc));
    state.samplers.insert(desc, sampler.clone());
    sampler
}

const SHADER_IMM: &str = include_str!("../shaders/wgpu_sprite.wgsl");
const MESH_SHADER_IMM: &str = include_str!("../shaders/wgpu_mesh.wgsl");
const TMESH_SHADER_IMM: &str = include_str!("../shaders/wgpu_tmesh.wgsl");
const SHADER_UBO: &str = include_str!("../shaders/wgpu_sprite_ubo.wgsl");
const MESH_SHADER_UBO: &str = include_str!("../shaders/wgpu_mesh_ubo.wgsl");
const TMESH_SHADER_UBO: &str = include_str!("../shaders/wgpu_tmesh_ubo.wgsl");
