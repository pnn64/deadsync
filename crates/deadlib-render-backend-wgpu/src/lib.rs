use deadlib_render_core::{
    BlendMode, ClockDomainTrace, DenseSlotMap, DrawOp, DrawStats, PresentModePolicy,
    PresentModeTrace, PresentStats, RenderFrame, SamplerCache, SamplerDesc, SamplerFilter,
    SamplerWrap, TMeshCacheKey, TextureHandle, TexturedMeshBufferCache, TexturedMeshUploads,
    TexturedMeshVertex, Yuv420Upload, draw_storage_stats, is_render_target_texture,
    resolve_textured_mesh_geometries, resolve_textured_meshes,
};
use glam::Mat4 as Matrix4;
use image::RgbaImage;
use log::{debug, info, warn};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::{
    borrow::Cow,
    error::Error,
    mem,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};
use wgpu::util::DeviceExt;
use winit::window::Window;

const WGPU_IMAGE_WAIT_THRESHOLD_US: u32 = 1_000;
const WGPU_BACK_PRESSURE_THRESHOLD_US: u32 = 1_000;
const WGPU_TMESH_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;
const LOGICAL_HEIGHT: f32 = 480.0;
const DESIGN_WIDTH_16_9: f32 = 854.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Api {
    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    Vulkan,
    #[cfg(target_os = "macos")]
    Metal,
    OpenGL,
    #[cfg(target_os = "windows")]
    DirectX,
}

impl Api {
    #[inline(always)]
    const fn name(self) -> &'static str {
        match self {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            Self::Vulkan => "Vulkan",
            #[cfg(target_os = "macos")]
            Self::Metal => "Metal",
            Self::OpenGL => "OpenGL",
            #[cfg(target_os = "windows")]
            Self::DirectX => "DirectX",
        }
    }

    #[inline(always)]
    const fn backends(self) -> wgpu::Backends {
        match self {
            #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
            Self::Vulkan => wgpu::Backends::VULKAN,
            #[cfg(target_os = "macos")]
            Self::Metal => wgpu::Backends::METAL,
            Self::OpenGL => wgpu::Backends::GL,
            #[cfg(target_os = "windows")]
            Self::DirectX => wgpu::Backends::DX12,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    edge_fade: [f32; 4],
    texture_mask: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TexturedMeshInstanceRaw {
    model_col0: [f32; 4],
    model_col1: [f32; 4],
    model_col2: [f32; 4],
    model_col3: [f32; 4],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: f32,
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

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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
    images: TextureImages,
    bind_group: Arc<wgpu::BindGroup>,
    bind_group_repeat: Arc<wgpu::BindGroup>,
}

impl Texture {
    fn rgba_view(&self) -> Option<&wgpu::TextureView> {
        match &self.images {
            TextureImages::Rgba { _view, .. } => Some(_view),
            TextureImages::Yuv420 { .. } => None,
        }
    }
}

enum TextureImages {
    Rgba {
        texture: wgpu::Texture,
        _view: wgpu::TextureView,
    },
    Yuv420 {
        planes: Box<[YuvPlane; 3]>,
        conversion: wgpu::Buffer,
        params: [[f32; 4]; 2],
    },
}

struct YuvPlane {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
}

impl TextureImages {
    #[inline(always)]
    const fn is_yuv420(&self) -> bool {
        matches!(self, Self::Yuv420 { .. })
    }
}

pub trait TextureLookup {
    fn wgpu_texture(&self, handle: TextureHandle) -> Option<&Texture>;
}

struct CachedTMeshGeom {
    buffer: Arc<wgpu::Buffer>,
    vertex_count: u32,
}

/// Render-thread-owned, session-retained AFT storage. Slots are bounded by the
/// largest render-target graph seen in one frame and are reused by list
/// position. Screen-entry changes may replace a slot; gameplay hits do no
/// allocation, pruning, scanning beyond the small active graph, or I/O.
struct OffscreenTarget {
    handle: TextureHandle,
    width: u32,
    height: u32,
    texture: Texture,
    _depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    initialized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstanceBinding {
    Sprite,
    TexturedMesh,
}

/// Frame-local record of bindings that survive compatible pipeline changes.
///
/// The render thread owns this fixed scalar state for one render pass. It starts
/// cold, emits a bind on an exact-key miss, and is dropped after recording; it
/// has no heap capacity, eviction, pruning, or deferred destruction. Immediate
/// projection storage is explicitly invalidated when a pipeline layout changes,
/// while uniform groups remain compatible. Each query is constant time. The
/// fixed fields and backend command benchmark provide its instrumentation.
#[derive(Debug, Default)]
struct DrawBindingCache {
    camera: Option<u8>,
    texture: Option<(u64, bool)>,
    instance: Option<InstanceBinding>,
    index_bound: bool,
}

impl DrawBindingCache {
    #[inline(always)]
    fn camera_required(&mut self, camera: u8) -> bool {
        update_binding(&mut self.camera, camera)
    }

    #[inline(always)]
    fn reset_camera(&mut self) {
        self.camera = None;
    }

    #[inline(always)]
    fn texture_required(&mut self, texture: u64, repeat: bool) -> bool {
        update_binding(&mut self.texture, (texture, repeat))
    }

    #[inline(always)]
    fn instance_required(&mut self, instance: InstanceBinding) -> bool {
        update_binding(&mut self.instance, instance)
    }

    #[inline(always)]
    fn index_required(&mut self) -> bool {
        !mem::replace(&mut self.index_bound, true)
    }
}

#[inline(always)]
fn update_binding<T: Copy + PartialEq>(current: &mut Option<T>, next: T) -> bool {
    if *current == Some(next) {
        false
    } else {
        *current = Some(next);
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PresentCompletion {
    present_id: u32,
    host_ns: u64,
    interval_ns: u64,
    refresh_ns: u64,
}

/// Fixed-size atomic latest-completion cell shared by wgpu callbacks and the render thread.
///
/// Writers serialize through the odd sequence value, publish one coherent snapshot,
/// and release it by advancing to the next even value. The render thread retries if
/// publication overlaps its read. The cell is renderer-lifetime fixed storage with
/// no queue growth, allocation, pruning, or missed-completion backlog.
struct PresentCompletionCell {
    version: AtomicU32,
    present_id: AtomicU32,
    host_ns: AtomicU64,
    interval_ns: AtomicU64,
    refresh_ns: AtomicU64,
}

impl PresentCompletionCell {
    fn new() -> Self {
        Self {
            version: AtomicU32::new(0),
            present_id: AtomicU32::new(0),
            host_ns: AtomicU64::new(0),
            interval_ns: AtomicU64::new(0),
            refresh_ns: AtomicU64::new(0),
        }
    }

    fn publish(&self, present_id: u32, host_ns: u64) {
        let version = loop {
            let version = self.version.load(Ordering::Relaxed);
            if version & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            if self
                .version
                .compare_exchange_weak(
                    version,
                    version.wrapping_add(1),
                    Ordering::Acquire,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break version;
            }
        };

        let previous_host = self.host_ns.load(Ordering::Relaxed);
        let previous_refresh = self.refresh_ns.load(Ordering::Relaxed);
        let interval_ns = if host_ns == 0 || previous_host == 0 {
            0
        } else {
            host_ns.saturating_sub(previous_host)
        };
        let refresh_ns = smooth_present_interval(previous_refresh, interval_ns);
        self.present_id.store(present_id, Ordering::Relaxed);
        if host_ns != 0 {
            self.host_ns.store(host_ns, Ordering::Relaxed);
        }
        self.interval_ns.store(interval_ns, Ordering::Relaxed);
        self.refresh_ns.store(refresh_ns, Ordering::Relaxed);
        self.version
            .store(version.wrapping_add(2), Ordering::Release);
    }

    fn load(&self) -> PresentCompletion {
        loop {
            let before = self.version.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let completion = PresentCompletion {
                present_id: self.present_id.load(Ordering::Relaxed),
                host_ns: self.host_ns.load(Ordering::Relaxed),
                interval_ns: self.interval_ns.load(Ordering::Relaxed),
                refresh_ns: self.refresh_ns.load(Ordering::Relaxed),
            };
            if self.version.load(Ordering::Acquire) == before {
                return completion;
            }
        }
    }
}

struct OwnedWindowHandle(pub Arc<Window>);

impl std::fmt::Debug for OwnedWindowHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OwnedWindowHandle(..)")
    }
}

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
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    projection: Matrix4,
    // Render-thread-only uniform bytes double as the exact bitwise camera
    // cache. Capacity warms at initialization/growth and lives for the session.
    projection_upload: Vec<u8>,
    bind_layout: wgpu::BindGroupLayout,
    rgba_conversion: wgpu::Buffer,
    samplers: SamplerCache<wgpu::Sampler>,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: PipelineSet,
    alpha_pipelines: PipelineSet,
    yuv_shader: wgpu::ShaderModule,
    yuv_pipeline_layout: wgpu::PipelineLayout,
    yuv_pipelines: PipelineSet,
    alpha_yuv_pipelines: PipelineSet,
    mesh_shader: wgpu::ShaderModule,
    mesh_pipeline_layout: wgpu::PipelineLayout,
    mesh_pipelines: MeshPipelineSet,
    alpha_mesh_pipelines: MeshPipelineSet,
    tmesh_shader: wgpu::ShaderModule,
    tmesh_pipeline_layout: wgpu::PipelineLayout,
    tmesh_pipelines: PipelineSet,
    tmesh_depth_pipelines: PipelineSet,
    alpha_tmesh_pipelines: PipelineSet,
    alpha_tmesh_depth_pipelines: PipelineSet,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    uploads: TexturedMeshUploads,
    offscreen_targets: Vec<OffscreenTarget>,
    cached_tmesh: DenseSlotMap<CachedTMeshGeom>,
    cached_tmesh_bytes: usize,
    mesh_vertex_buffer: wgpu::Buffer,
    mesh_vertex_capacity: usize,
    tmesh_vertex_buffer: wgpu::Buffer,
    tmesh_vertex_capacity: usize,
    tmesh_instance_buffer: wgpu::Buffer,
    tmesh_instance_capacity: usize,
    window_size: (u32, u32),
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    next_texture_id: u64,
    next_present_id: u32,
    present_done: Arc<PresentCompletionCell>,
    last_completed_present_id: u32,
    last_host_present_ns: u64,
    last_present_interval_ns: u64,
    screenshot_requested: bool,
    captured_frame: Option<RgbaImage>,
}

#[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
pub fn init_vulkan(
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(
        Api::Vulkan,
        window,
        vsync_enabled,
        present_mode_policy,
        gfx_debug_enabled,
    )
}

#[cfg(target_os = "macos")]
pub fn init_metal(
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(
        Api::Metal,
        window,
        vsync_enabled,
        present_mode_policy,
        gfx_debug_enabled,
    )
}

pub fn init_opengl(
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(
        Api::OpenGL,
        window,
        vsync_enabled,
        present_mode_policy,
        gfx_debug_enabled,
    )
}

#[cfg(target_os = "windows")]
pub fn init_dx12(
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    init(
        Api::DirectX,
        window,
        vsync_enabled,
        present_mode_policy,
        gfx_debug_enabled,
    )
}

fn init(
    api: Api,
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing {} (wgpu) backend...", api.name());
    if gfx_debug_enabled {
        debug!("{} (wgpu) validation/debug is enabled.", api.name());
    }
    let instance_flags = if gfx_debug_enabled {
        wgpu::InstanceFlags::debugging()
    } else {
        wgpu::InstanceFlags::empty()
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: api.backends(),
        flags: instance_flags,
        memory_budget_thresholds: Default::default(),
        backend_options: Default::default(),
        display: Some(Box::new(OwnedWindowHandle(window.clone()))),
    });

    let surface_target = OwnedWindowHandle(window.clone());
    let surface = instance
        .create_surface(surface_target)
        .map_err(|e| format!("Failed to create wgpu surface: {e}"))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .map_err(|e| format!("No suitable {} adapter found: {e}", api.name()))?;
    log_wgpu_adapter_info(api, &adapter);

    #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
    let want_immediates = matches!(api, Api::Vulkan);
    #[cfg(any(target_pointer_width = "32", target_vendor = "win7"))]
    let want_immediates = false;
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
    let present_mode = pick_present_mode(&caps.present_modes, vsync_enabled, present_mode_policy);
    let alpha_mode = pick_alpha_mode(&caps);

    let config = wgpu::SurfaceConfiguration {
        usage: pick_surface_usage(&caps),
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 0,
    };
    surface.configure(&device, &config);
    let (depth_texture, depth_view) =
        create_depth_target(&device, config.width.max(1), config.height.max(1));

    let projection = ortho_for_window(size.width, size.height);
    let proj = if use_immediates {
        ProjState::Immediates
    } else {
        init_uniform_proj(&device, &queue, projection)
    };
    let projection_upload_capacity = match &proj {
        ProjState::Immediates => 0,
        ProjState::Uniform {
            stride, capacity, ..
        } => (*stride as usize) * *capacity,
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
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(32),
                },
                count: None,
            },
        ],
    });
    let rgba_conversion = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("wgpu unused RGBA conversion"),
        contents: &[0; 32],
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let (shader, pipeline_layout, pipelines, alpha_pipelines) =
        build_pipeline_set(&device, &proj, &bind_layout, format, false);
    let (yuv_shader, yuv_pipeline_layout, yuv_pipelines, alpha_yuv_pipelines) =
        build_pipeline_set(&device, &proj, &bind_layout, format, true);
    let (mesh_shader, mesh_pipeline_layout, mesh_pipelines, alpha_mesh_pipelines) =
        build_mesh_pipeline_set(&device, &proj, format);
    let (
        tmesh_shader,
        tmesh_pipeline_layout,
        tmesh_pipelines,
        tmesh_depth_pipelines,
        alpha_tmesh_pipelines,
        alpha_tmesh_depth_pipelines,
    ) = build_textured_mesh_pipeline_set(&device, &proj, &bind_layout, format);

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
        size: (mesh_vertex_capacity * mem::size_of::<deadlib_render_core::MeshVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let tmesh_vertex_capacity = 1024usize;
    let tmesh_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu textured-mesh vertex buffer"),
        size: (tmesh_vertex_capacity * mem::size_of::<TexturedMeshVertex>()) as u64,
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
    let present_done = Arc::new(PresentCompletionCell::new());

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
        projection_upload: Vec::with_capacity(projection_upload_capacity),
        bind_layout,
        rgba_conversion,
        samplers: SamplerCache::default(),
        shader,
        pipeline_layout,
        pipelines,
        alpha_pipelines,
        yuv_shader,
        yuv_pipeline_layout,
        yuv_pipelines,
        alpha_yuv_pipelines,
        mesh_shader,
        mesh_pipeline_layout,
        mesh_pipelines,
        alpha_mesh_pipelines,
        tmesh_shader,
        tmesh_pipeline_layout,
        tmesh_pipelines,
        tmesh_depth_pipelines,
        alpha_tmesh_pipelines,
        alpha_tmesh_depth_pipelines,
        depth_texture,
        depth_view,
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        instance_buffer,
        instance_capacity,
        uploads: TexturedMeshUploads::with_capacity(tmesh_vertex_capacity, 64),
        offscreen_targets: Vec::with_capacity(4),
        cached_tmesh: DenseSlotMap::with_capacity(256),
        cached_tmesh_bytes: 0,
        mesh_vertex_buffer,
        mesh_vertex_capacity,
        tmesh_vertex_buffer,
        tmesh_vertex_capacity,
        tmesh_instance_buffer,
        tmesh_instance_capacity,
        window_size: (size.width, size.height),
        vsync_enabled,
        present_mode_policy,
        next_texture_id: 1,
        next_present_id: 1,
        present_done,
        last_completed_present_id: 0,
        last_host_present_ns: 0,
        last_present_interval_ns: 0,
        screenshot_requested: false,
        captured_frame: None,
    })
}

#[cfg(target_os = "windows")]
#[inline(always)]
fn current_host_nanos() -> u64 {
    deadlib_platform::windows_rt::current_host_nanos()
}

#[cfg(not(target_os = "windows"))]
#[inline(always)]
const fn current_host_nanos() -> u64 {
    0
}

#[cfg(target_os = "windows")]
#[inline(always)]
const fn host_clock_trace() -> ClockDomainTrace {
    ClockDomainTrace::Qpc
}

#[cfg(not(target_os = "windows"))]
#[inline(always)]
const fn host_clock_trace() -> ClockDomainTrace {
    ClockDomainTrace::Unknown
}

#[inline(always)]
const fn wgpu_present_mode_trace(mode: wgpu::PresentMode) -> PresentModeTrace {
    match mode {
        wgpu::PresentMode::Fifo | wgpu::PresentMode::AutoVsync => PresentModeTrace::Fifo,
        wgpu::PresentMode::FifoRelaxed => PresentModeTrace::FifoRelaxed,
        wgpu::PresentMode::Mailbox => PresentModeTrace::Mailbox,
        wgpu::PresentMode::Immediate | wgpu::PresentMode::AutoNoVsync => {
            PresentModeTrace::Immediate
        }
    }
}

#[inline(always)]
fn next_present_id(state: &mut State) -> u32 {
    let id = state.next_present_id.max(1);
    state.next_present_id = state.next_present_id.wrapping_add(1);
    if state.next_present_id == 0 {
        state.next_present_id = 1;
    }
    id
}

#[inline(always)]
fn drain_present_completions(state: &mut State) -> PresentCompletion {
    let mut latest = PresentCompletion {
        present_id: state.last_completed_present_id,
        host_ns: state.last_host_present_ns,
        interval_ns: 0,
        refresh_ns: state.last_present_interval_ns,
    };
    let done = state.present_done.load();
    if done.present_id == 0 || done.present_id == state.last_completed_present_id {
        return latest;
    }
    state.last_completed_present_id = done.present_id;
    state.last_host_present_ns = done.host_ns;
    state.last_present_interval_ns = done.refresh_ns;
    latest = done;
    latest
}

#[inline(always)]
const fn smooth_present_interval(previous: u64, interval: u64) -> u64 {
    if interval == 0 {
        previous
    } else if previous == 0 {
        interval
    } else {
        previous.saturating_mul(3).saturating_add(interval) / 4
    }
}

#[inline(always)]
const fn wgpu_vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x13B5 => "ARM",
        0x5143 => "Qualcomm",
        0x1010 => "ImgTec",
        0x106B => "Apple",
        0x1414 => "Microsoft",
        _ => "Unknown",
    }
}

fn log_wgpu_adapter_info(api: Api, adapter: &wgpu::Adapter) {
    let info_data = adapter.get_info();
    let vendor_name = wgpu_vendor_name(info_data.vendor);
    let name = {
        let trimmed = info_data.name.trim();
        if trimmed.is_empty() {
            "<unknown>".to_string()
        } else {
            trimmed.to_string()
        }
    };
    let driver = {
        let a = info_data.driver.trim();
        let b = info_data.driver_info.trim();
        if a.is_empty() && b.is_empty() {
            "unknown".to_string()
        } else if b.is_empty() {
            a.to_string()
        } else if a.is_empty() {
            b.to_string()
        } else {
            format!("{a}, {b}")
        }
    };
    info!(
        "{} adapter: {} [{}], driver {}, backend {:?} (vendor=0x{:04x}, device=0x{:04x}, type={:?})",
        api.name(),
        name,
        vendor_name,
        driver,
        info_data.backend,
        info_data.vendor,
        info_data.device,
        info_data.device_type
    );
}

fn init_uniform_proj(device: &wgpu::Device, queue: &wgpu::Queue, projection: Matrix4) -> ProjState {
    let align = device.limits().min_uniform_buffer_offset_alignment as u64;
    let stride = if align > 0 {
        PROJ_BYTES.div_ceil(align) * align
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
    let proj_array = projection.to_cols_array_2d();
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
            resource: proj_binding(&buffer),
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

#[inline(always)]
fn proj_binding(buffer: &wgpu::Buffer) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset: 0,
        size: wgpu::BufferSize::new(PROJ_BYTES),
    })
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
    let (bind_group, bind_group_repeat) =
        create_texture_groups(state, sampler_desc, [&view, &view, &view], None);
    let id = next_texture_id(state);

    Ok(Texture {
        id,
        images: TextureImages::Rgba {
            texture,
            _view: view,
        },
        bind_group,
        bind_group_repeat,
    })
}

fn create_texture_groups(
    state: &mut State,
    sampler_desc: SamplerDesc,
    views: [&wgpu::TextureView; 3],
    conversion: Option<&wgpu::Buffer>,
) -> (Arc<wgpu::BindGroup>, Arc<wgpu::BindGroup>) {
    let sampler = get_sampler(state, sampler_desc);
    let sampler_repeat = get_sampler(
        state,
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..sampler_desc
        },
    );
    let conversion = conversion.unwrap_or(&state.rgba_conversion);
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
                resource: wgpu::BindingResource::TextureView(views[0]),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(views[1]),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(views[2]),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: conversion.as_entire_binding(),
            },
        ],
    });
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
                resource: wgpu::BindingResource::TextureView(views[0]),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(views[1]),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(views[2]),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: conversion.as_entire_binding(),
            },
        ],
    });
    (Arc::new(bind_group), Arc::new(bind_group_repeat))
}

#[inline(always)]
fn next_texture_id(state: &mut State) -> u64 {
    let id = state.next_texture_id;
    state.next_texture_id = state.next_texture_id.wrapping_add(1);
    id
}

fn create_plane_texture(
    state: &State,
    width: u32,
    height: u32,
    label: &'static str,
    pixels: &[u8],
) -> Result<(wgpu::Texture, wgpu::TextureView), Box<dyn Error>> {
    if width == 0 || height == 0 || pixels.len() != width as usize * height as usize {
        return Err(std::io::Error::other("invalid YUV420 plane").into());
    }
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write_plane(&state.queue, &texture, width, height, pixels);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Ok((texture, view))
}

#[inline(always)]
fn write_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

pub fn create_yuv420_texture(
    state: &mut State,
    upload: Yuv420Upload<'_>,
    sampler_desc: SamplerDesc,
) -> Result<Texture, Box<dyn Error>> {
    if !upload.is_valid() {
        return Err(std::io::Error::other("invalid YUV420 planes").into());
    }
    let (y_texture, y_view) =
        create_plane_texture(state, upload.width, upload.height, "wgpu video Y", upload.y)?;
    let (u_texture, u_view) = create_plane_texture(
        state,
        upload.width / 2,
        upload.height / 2,
        "wgpu video U",
        upload.u,
    )?;
    let (v_texture, v_view) = create_plane_texture(
        state,
        upload.width / 2,
        upload.height / 2,
        "wgpu video V",
        upload.v,
    )?;
    let params = [upload.levels, upload.coeffs];
    let conversion = state
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wgpu video conversion"),
            contents: cast_slice(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let (bind_group, bind_group_repeat) = create_texture_groups(
        state,
        sampler_desc,
        [&y_view, &u_view, &v_view],
        Some(&conversion),
    );
    Ok(Texture {
        id: next_texture_id(state),
        images: TextureImages::Yuv420 {
            planes: Box::new([
                YuvPlane {
                    texture: y_texture,
                    _view: y_view,
                },
                YuvPlane {
                    texture: u_texture,
                    _view: u_view,
                },
                YuvPlane {
                    texture: v_texture,
                    _view: v_view,
                },
            ]),
            conversion,
            params,
        },
        bind_group,
        bind_group_repeat,
    })
}

fn create_depth_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wgpu depth texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_offscreen_target(
    state: &mut State,
    handle: TextureHandle,
    width: u32,
    height: u32,
) -> OffscreenTarget {
    let width = width.max(1);
    let height = height.max(1);
    let raw = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wgpu actor-frame texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: state.config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = raw.create_view(&wgpu::TextureViewDescriptor::default());
    let (bind_group, bind_group_repeat) = create_texture_groups(
        state,
        SamplerDesc {
            filter: SamplerFilter::Linear,
            wrap: SamplerWrap::Clamp,
            mipmaps: false,
        },
        [&view, &view, &view],
        None,
    );
    let texture = Texture {
        id: next_texture_id(state),
        images: TextureImages::Rgba {
            texture: raw,
            _view: view,
        },
        bind_group,
        bind_group_repeat,
    };
    let (depth_texture, depth_view) = create_depth_target(&state.device, width, height);
    OffscreenTarget {
        handle,
        width,
        height,
        texture,
        _depth_texture: depth_texture,
        depth_view,
        initialized: false,
    }
}

fn ensure_offscreen_targets(state: &mut State, frame: &RenderFrame) {
    for (index, pass) in frame.render_targets.iter().enumerate() {
        let matches = state.offscreen_targets.get(index).is_some_and(|target| {
            target.handle == pass.texture_handle
                && target.width == pass.width.max(1)
                && target.height == pass.height.max(1)
        });
        if matches {
            continue;
        }
        let target = create_offscreen_target(state, pass.texture_handle, pass.width, pass.height);
        if index < state.offscreen_targets.len() {
            state.offscreen_targets[index] = target;
        } else {
            state.offscreen_targets.push(target);
        }
    }
}

fn resolved_texture<'a, T: TextureLookup + ?Sized>(
    state: &'a State,
    textures: &'a T,
    handle: TextureHandle,
) -> Option<&'a Texture> {
    if is_render_target_texture(handle) {
        return state
            .offscreen_targets
            .iter()
            .find(|target| target.handle == handle)
            .map(|target| &target.texture);
    }
    textures.wgpu_texture(handle)
}

struct PassDrawData<'a> {
    cameras: &'a [Matrix4],
    ops: &'a [DrawOp],
    sprite_buffer_offset: u64,
    mesh_buffer_offset: u64,
    camera_buffer_start: usize,
}

fn record_draw_ops<'pass, T: TextureLookup + ?Sized>(
    pass: &mut wgpu::RenderPass<'pass>,
    state: &'pass State,
    data: PassDrawData<'pass>,
    uploads: &'pass TexturedMeshUploads,
    textures: &'pass T,
    write_alpha: bool,
) -> u32 {
    let camera_count = data.cameras.len();
    let texture_group = match state.proj {
        ProjState::Immediates => 0,
        ProjState::Uniform { .. } => 1,
    };
    let mut vertices_drawn = 0u32;
    let mut last_kind = None;
    let mut last_blend = None;
    let mut last_sprite_yuv = None;
    let mut bindings = DrawBindingCache::default();
    let mut tmesh_buffer_cache = TexturedMeshBufferCache::default();
    let mut last_tmesh_depth_test = None;
    let pipelines = if write_alpha {
        &state.alpha_pipelines
    } else {
        &state.pipelines
    };
    let yuv_pipelines = if write_alpha {
        &state.alpha_yuv_pipelines
    } else {
        &state.yuv_pipelines
    };
    let mesh_pipelines = if write_alpha {
        &state.alpha_mesh_pipelines
    } else {
        &state.mesh_pipelines
    };
    let tmesh_pipelines = if write_alpha {
        &state.alpha_tmesh_pipelines
    } else {
        &state.tmesh_pipelines
    };
    let tmesh_depth_pipelines = if write_alpha {
        &state.alpha_tmesh_depth_pipelines
    } else {
        &state.tmesh_depth_pipelines
    };
    for op in data.ops {
        match op {
            DrawOp::Sprite(run) => {
                let Some(tex) = resolved_texture(state, textures, run.texture_handle) else {
                    continue;
                };
                if last_kind != Some(0) {
                    pass.set_vertex_buffer(0, state.vertex_buffer.slice(..));
                    if bindings.instance_required(InstanceBinding::Sprite) {
                        pass.set_vertex_buffer(
                            1,
                            state.instance_buffer.slice(data.sprite_buffer_offset..),
                        );
                    }
                    if bindings.index_required() {
                        pass.set_index_buffer(
                            state.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                    }
                    last_kind = Some(0);
                    last_blend = None;
                    if matches!(state.proj, ProjState::Immediates) {
                        bindings.reset_camera();
                    }
                    tmesh_buffer_cache.reset();
                    last_tmesh_depth_test = None;
                }
                let yuv = tex.images.is_yuv420();
                if last_blend != Some(run.blend) || last_sprite_yuv != Some(yuv) {
                    pass.set_pipeline(if yuv {
                        yuv_pipelines.get(run.blend)
                    } else {
                        pipelines.get(run.blend)
                    });
                    if last_sprite_yuv.is_some_and(|last| last != yuv)
                        && matches!(state.proj, ProjState::Immediates)
                    {
                        bindings.reset_camera();
                    }
                    last_blend = Some(run.blend);
                    last_sprite_yuv = Some(yuv);
                }
                if bindings.camera_required(run.camera) {
                    set_camera(
                        pass,
                        &state.proj,
                        run.camera,
                        camera_count,
                        data.cameras,
                        state.projection,
                        data.camera_buffer_start,
                    );
                }
                if bindings.texture_required(tex.id, false) {
                    pass.set_bind_group(texture_group, Some(tex.bind_group.as_ref()), &[]);
                }
                pass.draw_indexed(
                    0..state.index_count,
                    0,
                    run.instance_start..(run.instance_start + run.instance_count),
                );
                vertices_drawn = vertices_drawn.saturating_add(4 * run.instance_count);
            }
            DrawOp::Mesh(run) => {
                if run.vertex_count == 0 {
                    continue;
                }
                if last_kind != Some(1) {
                    pass.set_vertex_buffer(
                        0,
                        state.mesh_vertex_buffer.slice(data.mesh_buffer_offset..),
                    );
                    last_kind = Some(1);
                    last_blend = None;
                    last_sprite_yuv = None;
                    if matches!(state.proj, ProjState::Immediates) {
                        bindings.reset_camera();
                    }
                    tmesh_buffer_cache.reset();
                    last_tmesh_depth_test = None;
                }
                if last_blend != Some(run.blend) {
                    pass.set_pipeline(mesh_pipelines.get(run.blend));
                    last_blend = Some(run.blend);
                }
                if bindings.camera_required(run.camera) {
                    set_camera(
                        pass,
                        &state.proj,
                        run.camera,
                        camera_count,
                        data.cameras,
                        state.projection,
                        data.camera_buffer_start,
                    );
                }
                pass.draw(
                    run.vertex_start..(run.vertex_start + run.vertex_count),
                    0..1,
                );
                vertices_drawn = vertices_drawn.saturating_add(run.vertex_count);
            }
            DrawOp::TexturedMesh(run) => {
                let Some(source) = uploads.source(run.geometry) else {
                    continue;
                };
                if source.vertex_count() == 0 || run.instance_count == 0 {
                    continue;
                }
                let Some(tex) = resolved_texture(state, textures, run.texture_handle) else {
                    continue;
                };
                if last_kind != Some(2) {
                    if bindings.instance_required(InstanceBinding::TexturedMesh) {
                        pass.set_vertex_buffer(1, state.tmesh_instance_buffer.slice(..));
                    }
                    last_kind = Some(2);
                    last_blend = None;
                    last_sprite_yuv = None;
                    if matches!(state.proj, ProjState::Immediates) {
                        bindings.reset_camera();
                    }
                    tmesh_buffer_cache.reset();
                    last_tmesh_depth_test = None;
                }
                if last_blend != Some(run.blend) || last_tmesh_depth_test != Some(run.depth_test) {
                    pass.set_pipeline(if run.depth_test {
                        tmesh_depth_pipelines.get(run.blend)
                    } else {
                        tmesh_pipelines.get(run.blend)
                    });
                    last_blend = Some(run.blend);
                    last_tmesh_depth_test = Some(run.depth_test);
                }
                if bindings.camera_required(run.camera) {
                    set_camera(
                        pass,
                        &state.proj,
                        run.camera,
                        camera_count,
                        data.cameras,
                        state.projection,
                        data.camera_buffer_start,
                    );
                }
                if bindings.texture_required(tex.id, true) {
                    pass.set_bind_group(texture_group, Some(tex.bind_group_repeat.as_ref()), &[]);
                }
                if tmesh_buffer_cache.update_required(source) {
                    if let Some(buffer_key) = source.buffer_key() {
                        let Some(entry) = state.cached_tmesh.get_slot(buffer_key) else {
                            tmesh_buffer_cache.reset();
                            continue;
                        };
                        pass.set_vertex_buffer(0, entry.buffer.slice(..));
                    } else {
                        pass.set_vertex_buffer(0, state.tmesh_vertex_buffer.slice(..));
                    }
                }
                let draw_start = source.vertex_start();
                pass.draw(
                    draw_start..draw_start + source.vertex_count(),
                    run.instance_start..(run.instance_start + run.instance_count),
                );
                let tri_count = source.vertex_count() / 3;
                vertices_drawn =
                    vertices_drawn.saturating_add(tri_count.saturating_mul(run.instance_count));
            }
        }
    }
    vertices_drawn
}

pub fn update_texture(
    state: &mut State,
    texture: &mut Texture,
    image: &RgbaImage,
) -> Result<(), Box<dyn Error>> {
    let size = wgpu::Extent3d {
        width: image.width(),
        height: image.height(),
        depth_or_array_layers: 1,
    };
    state.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: match &texture.images {
                TextureImages::Rgba { texture, .. } => texture,
                TextureImages::Yuv420 { .. } => {
                    return Err(
                        std::io::Error::other("cannot upload RGBA into YUV420 texture").into(),
                    );
                }
            },
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
    Ok(())
}

pub fn update_yuv420_texture(
    state: &mut State,
    texture: &mut Texture,
    upload: Yuv420Upload<'_>,
) -> Result<(), Box<dyn Error>> {
    let TextureImages::Yuv420 {
        planes,
        conversion,
        params,
        ..
    } = &mut texture.images
    else {
        return Err(std::io::Error::other("cannot upload YUV420 into RGBA texture").into());
    };
    if !upload.is_valid() {
        return Err(std::io::Error::other("invalid YUV420 planes").into());
    }
    write_plane(
        &state.queue,
        &planes[0].texture,
        upload.width,
        upload.height,
        upload.y,
    );
    write_plane(
        &state.queue,
        &planes[1].texture,
        upload.width / 2,
        upload.height / 2,
        upload.u,
    );
    write_plane(
        &state.queue,
        &planes[2].texture,
        upload.width / 2,
        upload.height / 2,
        upload.v,
    );
    let next_params = [upload.levels, upload.coeffs];
    if *params != next_params {
        state
            .queue
            .write_buffer(conversion, 0, cast_slice(&next_params));
        *params = next_params;
    }
    Ok(())
}

#[inline(always)]
pub const fn texture_is_yuv420(texture: &Texture) -> bool {
    texture.images.is_yuv420()
}

fn ensure_cached_tmesh(
    device: &wgpu::Device,
    cached_tmesh: &mut DenseSlotMap<CachedTMeshGeom>,
    cached_tmesh_bytes: &mut usize,
    cache_key: TMeshCacheKey,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
) -> Option<u64> {
    if let Some((buffer_key, entry)) = cached_tmesh.get(cache_key) {
        return (entry.vertex_count == vertices.len() as u32).then_some(buffer_key);
    }

    let bytes = std::mem::size_of_val(vertices);
    if bytes > WGPU_TMESH_CACHE_MAX_BYTES
        || cached_tmesh_bytes.saturating_add(bytes) > WGPU_TMESH_CACHE_MAX_BYTES
    {
        return None;
    }

    let buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("wgpu cached textured-mesh vertex buffer"),
            contents: cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    );
    let buffer_key = cached_tmesh.insert(
        cache_key,
        CachedTMeshGeom {
            buffer,
            vertex_count: vertices.len() as u32,
        },
    );
    *cached_tmesh_bytes = cached_tmesh_bytes.saturating_add(bytes);
    Some(buffer_key)
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

    let mut stats = DrawStats::default();
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(stats);
    }

    ensure_offscreen_targets(state, frame);
    stats.vertices = draw_offscreen_targets(state, frame, textures, &mut stats);

    let backend_prepare_started = Instant::now();
    {
        let uploads = &mut state.uploads;
        let device = &state.device;
        let cached_tmesh = &mut state.cached_tmesh;
        let cached_tmesh_bytes = &mut state.cached_tmesh_bytes;
        resolve_textured_meshes(frame, uploads, |cache_key, vertices| {
            ensure_cached_tmesh(
                device,
                cached_tmesh,
                cached_tmesh_bytes,
                cache_key,
                vertices,
            )
        });
        stats.storage = draw_storage_stats(frame, Some(uploads));
    }
    stats.backend_prepare_us = stats
        .backend_prepare_us
        .saturating_add(elapsed_us_since(backend_prepare_started));

    let backend_upload_started = Instant::now();
    let instance_len = frame.sprite_instances.len();
    ensure_instance_capacity(state, instance_len);
    if instance_len > 0 {
        state.queue.write_buffer(
            &state.instance_buffer,
            0,
            cast_slice(frame.sprite_instances.as_slice()),
        );
    }
    let mesh_len = frame.mesh_vertices.len();
    ensure_mesh_vertex_capacity(state, mesh_len);
    if mesh_len > 0 {
        state.queue.write_buffer(
            &state.mesh_vertex_buffer,
            0,
            cast_slice(frame.mesh_vertices.as_slice()),
        );
    }
    let tmesh_len = state.uploads.vertices.len();
    ensure_tmesh_vertex_capacity(state, tmesh_len);
    if tmesh_len > 0 {
        state.queue.write_buffer(
            &state.tmesh_vertex_buffer,
            0,
            cast_slice(state.uploads.vertices.as_slice()),
        );
    }
    let tmesh_instance_len = frame.tmesh_instances.len();
    ensure_tmesh_instance_capacity(state, tmesh_instance_len);
    if tmesh_instance_len > 0 {
        state.queue.write_buffer(
            &state.tmesh_instance_buffer,
            0,
            cast_slice(frame.tmesh_instances.as_slice()),
        );
    }
    upload_projections(state, &frame.cameras);
    stats.backend_upload_us = stats
        .backend_upload_us
        .saturating_add(elapsed_us_since(backend_upload_started));

    let acquire_started = Instant::now();
    let (surface_frame, suboptimal) = match state.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
        wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
            stats.acquire_us = elapsed_us_since(acquire_started);
            reconfigure_surface(state);
            return Ok(stats);
        }
        wgpu::CurrentSurfaceTexture::Timeout
        | wgpu::CurrentSurfaceTexture::Occluded
        | wgpu::CurrentSurfaceTexture::Validation => {
            stats.acquire_us = elapsed_us_since(acquire_started);
            return Ok(stats);
        }
    };
    stats.acquire_us = elapsed_us_since(acquire_started);
    let waited_for_image = stats.acquire_us >= WGPU_IMAGE_WAIT_THRESHOLD_US;
    let backend_setup_started = Instant::now();
    let view = surface_frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wgpu encoder"),
        });
    stats.backend_setup_us = stats
        .backend_setup_us
        .saturating_add(elapsed_us_since(backend_setup_started));

    let backend_record_started = Instant::now();
    let mut vertices_drawn = 0u32;
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wgpu render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: f64::from(frame.clear_color[0]),
                        g: f64::from(frame.clear_color[1]),
                        b: f64::from(frame.clear_color[2]),
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &state.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        let camera_count = frame.cameras.len();
        let texture_group = match state.proj {
            ProjState::Immediates => 0,
            ProjState::Uniform { .. } => 1,
        };

        let mut last_kind: Option<u8> = None; // 0=sprite, 1=mesh, 2=textured mesh
        let mut last_blend: Option<BlendMode> = None;
        let mut last_sprite_yuv = None;
        let mut bindings = DrawBindingCache::default();
        let mut tmesh_buffer_cache = TexturedMeshBufferCache::default();
        let mut last_tmesh_depth_test: Option<bool> = None;
        for op in &frame.ops {
            match op {
                DrawOp::Sprite(run) => {
                    let Some(tex) = resolved_texture(state, textures, run.texture_handle) else {
                        continue;
                    };
                    if last_kind != Some(0) {
                        pass.set_vertex_buffer(0, state.vertex_buffer.slice(..));
                        if bindings.instance_required(InstanceBinding::Sprite) {
                            pass.set_vertex_buffer(1, state.instance_buffer.slice(..));
                        }
                        if bindings.index_required() {
                            pass.set_index_buffer(
                                state.index_buffer.slice(..),
                                wgpu::IndexFormat::Uint16,
                            );
                        }
                        last_kind = Some(0);
                        last_blend = None;
                        if matches!(state.proj, ProjState::Immediates) {
                            // wgpu clears immediate storage when the pipeline
                            // layout changes; uniform bind groups remain valid.
                            bindings.reset_camera();
                        }
                        tmesh_buffer_cache.reset();
                        last_tmesh_depth_test = None;
                    }
                    let yuv = tex.images.is_yuv420();
                    if last_blend != Some(run.blend) || last_sprite_yuv != Some(yuv) {
                        pass.set_pipeline(if yuv {
                            state.yuv_pipelines.get(run.blend)
                        } else {
                            state.pipelines.get(run.blend)
                        });
                        if last_sprite_yuv.is_some_and(|last| last != yuv)
                            && matches!(state.proj, ProjState::Immediates)
                        {
                            bindings.reset_camera();
                        }
                        last_blend = Some(run.blend);
                        last_sprite_yuv = Some(yuv);
                    }
                    if bindings.camera_required(run.camera) {
                        set_camera(
                            &mut pass,
                            &state.proj,
                            run.camera,
                            camera_count,
                            &frame.cameras,
                            state.projection,
                            0,
                        );
                    }
                    if bindings.texture_required(tex.id, false) {
                        pass.set_bind_group(texture_group, Some(tex.bind_group.as_ref()), &[]);
                    }
                    pass.draw_indexed(
                        0..state.index_count,
                        0,
                        run.instance_start..(run.instance_start + run.instance_count),
                    );
                    vertices_drawn = vertices_drawn.saturating_add(4 * run.instance_count);
                }
                DrawOp::Mesh(run) => {
                    if run.vertex_count == 0 {
                        continue;
                    }
                    if last_kind != Some(1) {
                        pass.set_vertex_buffer(0, state.mesh_vertex_buffer.slice(..));
                        last_kind = Some(1);
                        last_blend = None;
                        last_sprite_yuv = None;
                        if matches!(state.proj, ProjState::Immediates) {
                            bindings.reset_camera();
                        }
                        tmesh_buffer_cache.reset();
                        last_tmesh_depth_test = None;
                    }
                    if last_blend != Some(run.blend) {
                        pass.set_pipeline(state.mesh_pipelines.get(run.blend));
                        last_blend = Some(run.blend);
                    }
                    if bindings.camera_required(run.camera) {
                        set_camera(
                            &mut pass,
                            &state.proj,
                            run.camera,
                            camera_count,
                            &frame.cameras,
                            state.projection,
                            0,
                        );
                    }
                    pass.draw(
                        run.vertex_start..(run.vertex_start + run.vertex_count),
                        0..1,
                    );
                    vertices_drawn = vertices_drawn.saturating_add(run.vertex_count);
                }
                DrawOp::TexturedMesh(run) => {
                    let Some(source) = state.uploads.source(run.geometry) else {
                        continue;
                    };
                    if source.vertex_count() == 0 || run.instance_count == 0 {
                        continue;
                    }
                    let Some(tex) = resolved_texture(state, textures, run.texture_handle) else {
                        continue;
                    };
                    if last_kind != Some(2) {
                        if bindings.instance_required(InstanceBinding::TexturedMesh) {
                            pass.set_vertex_buffer(1, state.tmesh_instance_buffer.slice(..));
                        }
                        last_kind = Some(2);
                        last_blend = None;
                        last_sprite_yuv = None;
                        if matches!(state.proj, ProjState::Immediates) {
                            bindings.reset_camera();
                        }
                        tmesh_buffer_cache.reset();
                        last_tmesh_depth_test = None;
                    }
                    if last_blend != Some(run.blend)
                        || last_tmesh_depth_test != Some(run.depth_test)
                    {
                        pass.set_pipeline(if run.depth_test {
                            state.tmesh_depth_pipelines.get(run.blend)
                        } else {
                            state.tmesh_pipelines.get(run.blend)
                        });
                        last_blend = Some(run.blend);
                        last_tmesh_depth_test = Some(run.depth_test);
                    }
                    if bindings.camera_required(run.camera) {
                        set_camera(
                            &mut pass,
                            &state.proj,
                            run.camera,
                            camera_count,
                            &frame.cameras,
                            state.projection,
                            0,
                        );
                    }
                    if bindings.texture_required(tex.id, true) {
                        pass.set_bind_group(
                            texture_group,
                            Some(tex.bind_group_repeat.as_ref()),
                            &[],
                        );
                    }
                    if tmesh_buffer_cache.update_required(source) {
                        if let Some(buffer_key) = source.buffer_key() {
                            let Some(entry) = state.cached_tmesh.get_slot(buffer_key) else {
                                tmesh_buffer_cache.reset();
                                continue;
                            };
                            pass.set_vertex_buffer(0, entry.buffer.slice(..));
                        } else {
                            pass.set_vertex_buffer(0, state.tmesh_vertex_buffer.slice(..));
                        }
                    }
                    let draw_start = source.vertex_start();
                    let draw_end = draw_start + source.vertex_count();
                    pass.draw(
                        draw_start..draw_end,
                        run.instance_start..(run.instance_start + run.instance_count),
                    );
                    let tri_count = source.vertex_count() / 3;
                    vertices_drawn =
                        vertices_drawn.saturating_add(tri_count.saturating_mul(run.instance_count));
                }
            }
        }
        drop(pass);
    }

    let screenshot_readback = if state.screenshot_requested {
        state.screenshot_requested = false;
        if state.config.usage.contains(wgpu::TextureUsages::COPY_SRC) {
            let format = state.config.format;
            debug!(
                "wgpu screenshot: surface format={:?} size={}x{}",
                format, state.config.width, state.config.height
            );
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
                    texture: &surface_frame.texture,
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
            state.captured_frame = None;
            warn!(
                "{} (wgpu) surface does not support COPY_SRC; screenshot unavailable.",
                state.api.name()
            );
            None
        }
    } else {
        None
    };
    stats.backend_record_us = stats
        .backend_record_us
        .saturating_add(elapsed_us_since(backend_record_started));

    let submitted_present_id = next_present_id(state);
    let submit_started = Instant::now();
    let submission_index = state.queue.submit(Some(encoder.finish()));
    stats.submit_us = stats
        .submit_us
        .saturating_add(elapsed_us_since(submit_started));
    let present_done = Arc::clone(&state.present_done);
    state.queue.on_submitted_work_done(move || {
        present_done.publish(submitted_present_id, current_host_nanos());
    });
    let present_started = Instant::now();
    state.queue.present(surface_frame);
    stats.present_us = elapsed_us_since(present_started);
    let mut back_pressure_waited = false;
    if apply_present_back_pressure && screenshot_readback.is_none() {
        // Uncapped wgpu submission can otherwise keep the CPU hot by queuing
        // work continuously; wait for this frame to retire before proceeding.
        let wait_started = Instant::now();
        let _ = state.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission_index),
            timeout: None,
        });
        let wait_us = elapsed_us_since(wait_started);
        stats.gpu_wait_us = stats.gpu_wait_us.saturating_add(wait_us);
        back_pressure_waited = wait_us >= WGPU_BACK_PRESSURE_THRESHOLD_US;
    }
    let mut queue_idle_waited = false;
    if let Some((readback_buffer, width, height, padded_row_bytes, format)) = screenshot_readback {
        let slice = readback_buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let wait_started = Instant::now();
        let _ = state.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let wait_us = elapsed_us_since(wait_started);
        stats.gpu_wait_us = stats.gpu_wait_us.saturating_add(wait_us);
        queue_idle_waited = wait_us != 0;
        if rx.recv().is_ok_and(|res| res.is_ok()) {
            match slice.get_mapped_range() {
                Ok(data) => {
                    let row_bytes = width * 4;
                    let mut rgba = vec![0u8; row_bytes * height];
                    let swap_rb = matches!(
                        format,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                    );
                    for y in 0..height {
                        let src = y * padded_row_bytes;
                        // Surface readback rows are already top-to-bottom for this path.
                        let dst = y * row_bytes;
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
                }
                Err(err) => {
                    readback_buffer.unmap();
                    state.captured_frame = None;
                    warn!("wgpu screenshot readback failed: {err}");
                }
            }
        } else {
            readback_buffer.unmap();
            state.captured_frame = None;
            warn!("wgpu screenshot readback failed: map_async returned error");
        }
    }
    let completion = drain_present_completions(state);
    let in_flight_images = if completion.present_id == 0 {
        1
    } else if submitted_present_id >= completion.present_id {
        submitted_present_id
            .saturating_sub(completion.present_id)
            .min(u32::from(u8::MAX)) as u8
    } else {
        0
    };
    stats.present_stats = PresentStats {
        mode: wgpu_present_mode_trace(state.config.present_mode),
        display_clock: ClockDomainTrace::Unknown,
        host_clock: if completion.host_ns != 0 {
            host_clock_trace()
        } else {
            ClockDomainTrace::Unknown
        },
        in_flight_images,
        waited_for_image,
        applied_back_pressure: back_pressure_waited,
        queue_idle_waited,
        suboptimal,
        submitted_present_id,
        completed_present_id: completion.present_id,
        refresh_ns: state.last_present_interval_ns,
        actual_interval_ns: completion.interval_ns,
        present_margin_ns: 0,
        host_present_ns: completion.host_ns,
        calibration_error_ns: 0,
    };
    if suboptimal {
        reconfigure_surface(state);
    }

    stats.vertices = stats.vertices.saturating_add(vertices_drawn);
    Ok(stats)
}

fn upload_pass_data(
    state: &mut State,
    cameras: &[Matrix4],
    sprite_instances: &[deadlib_render_core::SpriteInstanceRaw],
    mesh_vertices: &[deadlib_render_core::MeshVertex],
    tmesh_instances: &[deadlib_render_core::TexturedMeshInstanceRaw],
) {
    ensure_instance_capacity(state, sprite_instances.len());
    if !sprite_instances.is_empty() {
        state
            .queue
            .write_buffer(&state.instance_buffer, 0, cast_slice(sprite_instances));
    }
    ensure_mesh_vertex_capacity(state, mesh_vertices.len());
    if !mesh_vertices.is_empty() {
        state
            .queue
            .write_buffer(&state.mesh_vertex_buffer, 0, cast_slice(mesh_vertices));
    }
    let tmesh_len = state.uploads.vertices.len();
    ensure_tmesh_vertex_capacity(state, tmesh_len);
    if tmesh_len > 0 {
        state.queue.write_buffer(
            &state.tmesh_vertex_buffer,
            0,
            cast_slice(state.uploads.vertices.as_slice()),
        );
    }
    ensure_tmesh_instance_capacity(state, tmesh_instances.len());
    if !tmesh_instances.is_empty() {
        state
            .queue
            .write_buffer(&state.tmesh_instance_buffer, 0, cast_slice(tmesh_instances));
    }
    upload_projections(state, cameras);
}

fn draw_offscreen_targets(
    state: &mut State,
    frame: &RenderFrame,
    textures: &impl TextureLookup,
    stats: &mut DrawStats,
) -> u32 {
    if frame.render_targets.len() > 1
        && frame
            .render_targets
            .iter()
            .all(|target| target.tmesh_geometries.is_empty() && target.tmesh_instances.is_empty())
    {
        return draw_offscreen_targets_batched(state, frame, textures, stats);
    }
    draw_offscreen_targets_serial(state, frame, textures, stats)
}

/// Record a sprite/mesh-only AFT graph in one command buffer. Disjoint slices
/// of the retained vertex and projection buffers let dependent targets execute
/// in order without a queue submission between every target. The buffers are
/// session-owned and grow only when a larger graph first appears; steady
/// gameplay performs bounded writes over the current frame and one submit.
fn draw_offscreen_targets_batched(
    state: &mut State,
    frame: &RenderFrame,
    textures: &impl TextureLookup,
    stats: &mut DrawStats,
) -> u32 {
    let total_instances = frame
        .render_targets
        .iter()
        .map(|target| target.sprite_instances.len())
        .sum();
    let total_mesh_vertices = frame
        .render_targets
        .iter()
        .map(|target| target.mesh_vertices.len())
        .sum();

    let upload_started = Instant::now();
    ensure_instance_capacity(state, total_instances);
    ensure_mesh_vertex_capacity(state, total_mesh_vertices);
    upload_offscreen_projections(state, frame);
    let mut instance_start = 0usize;
    let mut mesh_start = 0usize;
    for target in &frame.render_targets {
        if !target.sprite_instances.is_empty() {
            state.queue.write_buffer(
                &state.instance_buffer,
                (instance_start * mem::size_of::<InstanceRaw>()) as u64,
                cast_slice(target.sprite_instances.as_slice()),
            );
        }
        if !target.mesh_vertices.is_empty() {
            state.queue.write_buffer(
                &state.mesh_vertex_buffer,
                (mesh_start * mem::size_of::<deadlib_render_core::MeshVertex>()) as u64,
                cast_slice(target.mesh_vertices.as_slice()),
            );
        }
        instance_start += target.sprite_instances.len();
        mesh_start += target.mesh_vertices.len();
    }
    stats.backend_upload_us = stats
        .backend_upload_us
        .saturating_add(elapsed_us(upload_started.elapsed()));

    let setup_started = Instant::now();
    let mut encoder = state
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("wgpu batched actor-frame texture encoder"),
        });
    stats.backend_setup_us = stats
        .backend_setup_us
        .saturating_add(elapsed_us(setup_started.elapsed()));

    let record_started = Instant::now();
    let mut vertices = 0u32;
    let mut instance_start = 0usize;
    let mut mesh_start = 0usize;
    let mut camera_start = 0usize;
    for (index, target_frame) in frame.render_targets.iter().enumerate() {
        let target = &state.offscreen_targets[index];
        let color_view = target
            .texture
            .rgba_view()
            .expect("offscreen targets are RGBA textures");
        let color_load = if target_frame.preserve && target.initialized {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: if target_frame.alpha { 0.0 } else { 1.0 },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wgpu batched actor-frame texture pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        vertices = vertices.saturating_add(record_draw_ops(
            &mut pass,
            state,
            PassDrawData {
                cameras: &target_frame.cameras,
                ops: &target_frame.ops,
                sprite_buffer_offset: (instance_start * mem::size_of::<InstanceRaw>()) as u64,
                mesh_buffer_offset: (mesh_start * mem::size_of::<deadlib_render_core::MeshVertex>())
                    as u64,
                camera_buffer_start: camera_start,
            },
            &state.uploads,
            textures,
            target_frame.alpha,
        ));
        instance_start += target_frame.sprite_instances.len();
        mesh_start += target_frame.mesh_vertices.len();
        camera_start += target_frame.cameras.len() + 1;
    }
    stats.backend_record_us = stats
        .backend_record_us
        .saturating_add(elapsed_us(record_started.elapsed()));

    let submit_started = Instant::now();
    state.queue.submit(Some(encoder.finish()));
    stats.submit_us = stats
        .submit_us
        .saturating_add(elapsed_us(submit_started.elapsed()));
    for target in state
        .offscreen_targets
        .iter_mut()
        .take(frame.render_targets.len())
    {
        target.initialized = true;
    }
    vertices
}

fn draw_offscreen_targets_serial(
    state: &mut State,
    frame: &RenderFrame,
    textures: &impl TextureLookup,
    stats: &mut DrawStats,
) -> u32 {
    let mut vertices = 0u32;
    for (index, target_frame) in frame.render_targets.iter().enumerate() {
        let prepare_started = Instant::now();
        {
            let uploads = &mut state.uploads;
            let device = &state.device;
            let cached_tmesh = &mut state.cached_tmesh;
            let cached_tmesh_bytes = &mut state.cached_tmesh_bytes;
            resolve_textured_mesh_geometries(
                &target_frame.tmesh_geometries,
                uploads,
                |cache_key, geometry| {
                    ensure_cached_tmesh(
                        device,
                        cached_tmesh,
                        cached_tmesh_bytes,
                        cache_key,
                        geometry,
                    )
                },
            );
        }
        stats.backend_prepare_us = stats
            .backend_prepare_us
            .saturating_add(elapsed_us(prepare_started.elapsed()));

        let upload_started = Instant::now();
        upload_pass_data(
            state,
            &target_frame.cameras,
            &target_frame.sprite_instances,
            &target_frame.mesh_vertices,
            &target_frame.tmesh_instances,
        );
        stats.backend_upload_us = stats
            .backend_upload_us
            .saturating_add(elapsed_us(upload_started.elapsed()));

        let setup_started = Instant::now();
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wgpu actor-frame texture encoder"),
            });
        stats.backend_setup_us = stats
            .backend_setup_us
            .saturating_add(elapsed_us(setup_started.elapsed()));

        let record_started = Instant::now();
        {
            let target = &state.offscreen_targets[index];
            let color_view = target
                .texture
                .rgba_view()
                .expect("offscreen targets are RGBA textures");
            let color_load = if target_frame.preserve && target.initialized {
                wgpu::LoadOp::Load
            } else {
                wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: if target_frame.alpha { 0.0 } else { 1.0 },
                })
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("wgpu actor-frame texture pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            vertices = vertices.saturating_add(record_draw_ops(
                &mut pass,
                state,
                PassDrawData {
                    cameras: &target_frame.cameras,
                    ops: &target_frame.ops,
                    sprite_buffer_offset: 0,
                    mesh_buffer_offset: 0,
                    camera_buffer_start: 0,
                },
                &state.uploads,
                textures,
                target_frame.alpha,
            ));
        }
        stats.backend_record_us = stats
            .backend_record_us
            .saturating_add(elapsed_us(record_started.elapsed()));
        let submit_started = Instant::now();
        state.queue.submit(Some(encoder.finish()));
        stats.submit_us = stats
            .submit_us
            .saturating_add(elapsed_us(submit_started.elapsed()));
        state.offscreen_targets[index].initialized = true;
    }
    vertices
}

#[inline(always)]
fn elapsed_us(elapsed: std::time::Duration) -> u32 {
    elapsed.as_micros().min(u128::from(u32::MAX)) as u32
}

fn upload_offscreen_projections(state: &mut State, frame: &RenderFrame) {
    let ProjState::Uniform { stride, .. } = &state.proj else {
        return;
    };
    let stride = *stride as usize;
    let matrix_count = frame
        .render_targets
        .iter()
        .map(|target| target.cameras.len() + 1)
        .sum::<usize>()
        .max(1);
    ensure_projection_capacity(state, matrix_count);
    stage_offscreen_projection_upload(
        &mut state.projection_upload,
        frame
            .render_targets
            .iter()
            .map(|target| target.cameras.as_slice()),
        state.projection,
        stride,
    );

    let ProjState::Uniform { buffer, .. } = &state.proj else {
        return;
    };
    state
        .queue
        .write_buffer(buffer, 0, &state.projection_upload);
}

fn stage_offscreen_projection_upload<'a>(
    upload: &mut Vec<u8>,
    camera_sets: impl IntoIterator<Item = &'a [Matrix4]>,
    fallback: Matrix4,
    stride: usize,
) {
    debug_assert!(stride >= PROJ_BYTES as usize);
    upload.clear();
    for cameras in camera_sets {
        for matrix in cameras.iter().chain(std::iter::once(&fallback)) {
            let offset = upload.len();
            upload.resize(offset + stride, 0);
            let columns = matrix.to_cols_array();
            let bytes = cast_slice(std::slice::from_ref(&columns));
            upload[offset..offset + bytes.len()].copy_from_slice(bytes);
        }
    }
}

#[inline(always)]
fn upload_projections(state: &mut State, cameras: &[Matrix4]) {
    let ProjState::Uniform { .. } = state.proj else {
        return;
    };
    let needed = cameras.len().saturating_add(1).max(1);
    let buffer_recreated = ensure_projection_capacity(state, needed);

    let ProjState::Uniform { stride, .. } = &state.proj else {
        return;
    };
    let stride = *stride as usize;
    debug_assert!(stride >= PROJ_BYTES as usize);
    let changed = stage_projection_upload(
        &mut state.projection_upload,
        cameras,
        state.projection,
        stride,
    );
    if !changed && !buffer_recreated {
        return;
    }

    let ProjState::Uniform { buffer, .. } = &state.proj else {
        return;
    };
    state
        .queue
        .write_buffer(buffer, 0, &state.projection_upload);
}

fn stage_projection_upload(
    upload: &mut Vec<u8>,
    cameras: &[Matrix4],
    fallback: Matrix4,
    stride: usize,
) -> bool {
    let needed = cameras.len().saturating_add(1).max(1);
    let mut changed = upload.len() != needed * stride;
    upload.resize(needed * stride, 0);
    for (index, matrix) in cameras.iter().chain(std::iter::once(&fallback)).enumerate() {
        let columns = matrix.to_cols_array();
        let bytes = cast_slice(std::slice::from_ref(&columns));
        let offset = index * stride;
        let slot = &mut upload[offset..offset + bytes.len()];
        if slot != bytes {
            slot.copy_from_slice(bytes);
            changed = true;
        }
    }
    changed
}

fn set_camera(
    pass: &mut wgpu::RenderPass<'_>,
    proj: &ProjState,
    camera: u8,
    camera_count: usize,
    cameras: &[Matrix4],
    fallback: Matrix4,
    buffer_start: usize,
) {
    match proj {
        ProjState::Immediates => {
            let vp = cameras.get(camera as usize).copied().unwrap_or(fallback);
            let vp_array = vp.to_cols_array_2d();
            pass.set_immediates(0, cast_slice(&vp_array));
        }
        ProjState::Uniform { group, stride, .. } => {
            let idx = if (camera as usize) < camera_count {
                camera as usize
            } else {
                camera_count
            };
            let offset = (((buffer_start + idx) as u64) * *stride) as u32;
            pass.set_bind_group(0, group, &[offset]);
        }
    }
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    state.window_size = (width, height);
    if width == 0 || height == 0 {
        return;
    }
    state.projection = ortho_for_window(width, height);
    reconfigure_surface(state);
}

pub fn set_default_projection(state: &mut State, projection: Matrix4) {
    state.projection = projection;
}

pub fn cleanup(state: &mut State) {
    info!("{} (wgpu) backend cleanup complete.", state.api.name());
}

pub fn wait_for_idle(state: &mut State) {
    let _ = state.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
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
        size: (new_cap * mem::size_of::<deadlib_render_core::MeshVertex>()) as u64,
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
        size: (new_cap * mem::size_of::<TexturedMeshVertex>()) as u64,
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

fn ensure_projection_capacity(state: &mut State, needed: usize) -> bool {
    let ProjState::Uniform {
        stride,
        capacity,
        buffer,
        group,
        layout,
    } = &mut state.proj
    else {
        return false;
    };
    if needed <= *capacity {
        return false;
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
            resource: proj_binding(buffer),
        }],
    });
    *capacity = new_cap;
    true
}

fn reconfigure_surface(state: &mut State) {
    if state.window_size.0 == 0 || state.window_size.1 == 0 {
        return;
    }
    let caps = state.surface.get_capabilities(&state.adapter);
    let new_format = pick_format(&caps);
    let format_changed = new_format != state.config.format;
    state.config.format = new_format;
    state.config.present_mode = pick_present_mode(
        &caps.present_modes,
        state.vsync_enabled,
        state.present_mode_policy,
    );
    state.config.alpha_mode = pick_alpha_mode(&caps);
    state.config.usage = pick_surface_usage(&caps);
    state.config.width = state.window_size.0;
    state.config.height = state.window_size.1;
    state.surface.configure(&state.device, &state.config);
    (state.depth_texture, state.depth_view) = create_depth_target(
        &state.device,
        state.config.width.max(1),
        state.config.height.max(1),
    );

    if matches!(state.proj, ProjState::Uniform { .. }) {
        let fallback = state.projection.to_cols_array_2d();
        if let ProjState::Uniform { buffer, .. } = &state.proj {
            state.queue.write_buffer(buffer, 0, cast_slice(&fallback));
        }
    }

    if format_changed {
        let (shader, pipeline_layout, pipelines, alpha_pipelines) = build_pipeline_set(
            &state.device,
            &state.proj,
            &state.bind_layout,
            state.config.format,
            false,
        );
        let (yuv_shader, yuv_pipeline_layout, yuv_pipelines, alpha_yuv_pipelines) =
            build_pipeline_set(
                &state.device,
                &state.proj,
                &state.bind_layout,
                state.config.format,
                true,
            );
        let (mesh_shader, mesh_pipeline_layout, mesh_pipelines, alpha_mesh_pipelines) =
            build_mesh_pipeline_set(&state.device, &state.proj, state.config.format);
        let (
            tmesh_shader,
            tmesh_pipeline_layout,
            tmesh_pipelines,
            tmesh_depth_pipelines,
            alpha_tmesh_pipelines,
            alpha_tmesh_depth_pipelines,
        ) = build_textured_mesh_pipeline_set(
            &state.device,
            &state.proj,
            &state.bind_layout,
            state.config.format,
        );
        state.shader = shader;
        state.pipeline_layout = pipeline_layout;
        state.pipelines = pipelines;
        state.alpha_pipelines = alpha_pipelines;
        state.yuv_shader = yuv_shader;
        state.yuv_pipeline_layout = yuv_pipeline_layout;
        state.yuv_pipelines = yuv_pipelines;
        state.alpha_yuv_pipelines = alpha_yuv_pipelines;
        state.mesh_shader = mesh_shader;
        state.mesh_pipeline_layout = mesh_pipeline_layout;
        state.mesh_pipelines = mesh_pipelines;
        state.alpha_mesh_pipelines = alpha_mesh_pipelines;
        state.tmesh_shader = tmesh_shader;
        state.tmesh_pipeline_layout = tmesh_pipeline_layout;
        state.tmesh_pipelines = tmesh_pipelines;
        state.tmesh_depth_pipelines = tmesh_depth_pipelines;
        state.alpha_tmesh_pipelines = alpha_tmesh_pipelines;
        state.alpha_tmesh_depth_pipelines = alpha_tmesh_depth_pipelines;
    }
}

fn pick_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    // Prefer 8-bit non-sRGB formats for consistent colors and correct screenshot
    // readback. The screenshot path assumes 4 bytes/pixel RGBA or BGRA; formats
    // like Rgb10a2 or Rgba16Float would produce garbled captures.
    const PREFERRED: &[wgpu::TextureFormat] = &[
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ];
    for &pref in PREFERRED {
        if caps.formats.contains(&pref) {
            return pref;
        }
    }
    // Fall back to the first non-sRGB, then the first format overall.
    caps.formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .unwrap_or_else(|| caps.formats[0])
}

#[inline(always)]
fn pick_surface_usage(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureUsages {
    let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
    if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
        usage |= wgpu::TextureUsages::COPY_SRC;
    }
    usage
}

#[inline(always)]
fn pick_alpha_mode(caps: &wgpu::SurfaceCapabilities) -> wgpu::CompositeAlphaMode {
    caps.alpha_modes
        .iter()
        .copied()
        .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
        .unwrap_or_else(|| {
            caps.alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Opaque)
        })
}

#[inline(always)]
const fn surface_write_mask() -> wgpu::ColorWrites {
    wgpu::ColorWrites::RED
        .union(wgpu::ColorWrites::GREEN)
        .union(wgpu::ColorWrites::BLUE)
}

fn pick_present_mode(
    modes: &[wgpu::PresentMode],
    vsync: bool,
    present_mode_policy: PresentModePolicy,
) -> wgpu::PresentMode {
    let preferred = if vsync {
        [
            wgpu::PresentMode::AutoVsync,
            wgpu::PresentMode::Fifo,
            wgpu::PresentMode::FifoRelaxed,
        ]
    } else if present_mode_policy == PresentModePolicy::Immediate {
        [
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::AutoNoVsync,
            wgpu::PresentMode::Mailbox,
        ]
    } else {
        [
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::AutoNoVsync,
            wgpu::PresentMode::Immediate,
        ]
    };

    preferred
        .iter()
        .copied()
        .find(|p| modes.contains(p))
        .unwrap_or_else(|| modes[0])
}

pub fn set_present_config(
    state: &mut State,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
) {
    if state.vsync_enabled == vsync_enabled && state.present_mode_policy == present_mode_policy {
        return;
    }
    state.vsync_enabled = vsync_enabled;
    state.present_mode_policy = present_mode_policy;
    reconfigure_surface(state);
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
    yuv420: bool,
) -> (
    wgpu::ShaderModule,
    wgpu::PipelineLayout,
    PipelineSet,
    PipelineSet,
) {
    let shader_src = match (proj, yuv420) {
        (ProjState::Immediates, false) => SHADER_IMM,
        (ProjState::Uniform { .. }, false) => SHADER_UBO,
        (ProjState::Immediates, true) => YUV_SHADER_IMM,
        (ProjState::Uniform { .. }, true) => YUV_SHADER_UBO,
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("wgpu shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_src)),
    });

    let pipeline_layout = match proj {
        ProjState::Immediates => {
            let layouts = [Some(bind_layout)];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: PROJ_BYTES as u32,
            })
        }
        ProjState::Uniform { layout, .. } => {
            let layouts = [Some(layout), Some(bind_layout)];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            })
        }
    };

    let pipelines = build_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        surface_write_mask(),
    );
    let alpha_pipelines = build_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        wgpu::ColorWrites::ALL,
    );

    (shader, pipeline_layout, pipelines, alpha_pipelines)
}

fn build_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    write_mask: wgpu::ColorWrites,
) -> PipelineSet {
    PipelineSet {
        alpha: build_pipeline(device, layout, format, BlendMode::Alpha, shader, write_mask),
        add: build_pipeline(device, layout, format, BlendMode::Add, shader, write_mask),
        multiply: build_pipeline(
            device,
            layout,
            format,
            BlendMode::Multiply,
            shader,
            write_mask,
        ),
        subtract: build_pipeline(
            device,
            layout,
            format,
            BlendMode::Subtract,
            shader,
            write_mask,
        ),
    }
}

fn build_mesh_pipeline_set(
    device: &wgpu::Device,
    proj: &ProjState,
    format: wgpu::TextureFormat,
) -> (
    wgpu::ShaderModule,
    wgpu::PipelineLayout,
    MeshPipelineSet,
    MeshPipelineSet,
) {
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
            let layouts = [Some(layout)];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu mesh pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            })
        }
    };

    let pipelines = build_mesh_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        surface_write_mask(),
    );
    let alpha_pipelines = build_mesh_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        wgpu::ColorWrites::ALL,
    );

    (shader, pipeline_layout, pipelines, alpha_pipelines)
}

fn build_mesh_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    write_mask: wgpu::ColorWrites,
) -> MeshPipelineSet {
    MeshPipelineSet {
        alpha: build_mesh_pipeline(device, layout, format, BlendMode::Alpha, shader, write_mask),
        add: build_mesh_pipeline(device, layout, format, BlendMode::Add, shader, write_mask),
        multiply: build_mesh_pipeline(
            device,
            layout,
            format,
            BlendMode::Multiply,
            shader,
            write_mask,
        ),
        subtract: build_mesh_pipeline(
            device,
            layout,
            format,
            BlendMode::Subtract,
            shader,
            write_mask,
        ),
    }
}

fn build_textured_mesh_pipeline_set(
    device: &wgpu::Device,
    proj: &ProjState,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (
    wgpu::ShaderModule,
    wgpu::PipelineLayout,
    PipelineSet,
    PipelineSet,
    PipelineSet,
    PipelineSet,
) {
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
            let layouts = [Some(bind_layout)];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu textured-mesh pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: PROJ_BYTES as u32,
            })
        }
        ProjState::Uniform { layout, .. } => {
            let layouts = [Some(layout), Some(bind_layout)];
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("wgpu textured-mesh pipeline layout"),
                bind_group_layouts: &layouts,
                immediate_size: 0,
            })
        }
    };

    let pipelines = build_tmesh_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        false,
        surface_write_mask(),
    );
    let depth_pipelines = build_tmesh_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        true,
        surface_write_mask(),
    );
    let alpha_pipelines = build_tmesh_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        false,
        wgpu::ColorWrites::ALL,
    );
    let alpha_depth_pipelines = build_tmesh_pipelines(
        device,
        &pipeline_layout,
        format,
        &shader,
        true,
        wgpu::ColorWrites::ALL,
    );

    (
        shader,
        pipeline_layout,
        pipelines,
        depth_pipelines,
        alpha_pipelines,
        alpha_depth_pipelines,
    )
}

fn build_tmesh_pipelines(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    use_depth: bool,
    write_mask: wgpu::ColorWrites,
) -> PipelineSet {
    PipelineSet {
        alpha: build_tmesh_pipeline(
            device,
            layout,
            format,
            BlendMode::Alpha,
            shader,
            use_depth,
            write_mask,
        ),
        add: build_tmesh_pipeline(
            device,
            layout,
            format,
            BlendMode::Add,
            shader,
            use_depth,
            write_mask,
        ),
        multiply: build_tmesh_pipeline(
            device,
            layout,
            format,
            BlendMode::Multiply,
            shader,
            use_depth,
            write_mask,
        ),
        subtract: build_tmesh_pipeline(
            device,
            layout,
            format,
            BlendMode::Subtract,
            shader,
            use_depth,
            write_mask,
        ),
    }
}

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    mode: BlendMode,
    shader: &wgpu::ShaderModule,
    write_mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgpu pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(vertex_layout()), Some(instance_layout())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend_state(mode),
                write_mask,
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
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
    write_mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgpu mesh pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[Some(mesh_vertex_layout())],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend_state(mode),
                write_mask,
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
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
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
    use_depth: bool,
    write_mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wgpu textured-mesh pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[
                Some(textured_mesh_vertex_layout()),
                Some(textured_mesh_instance_layout()),
            ],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend_state(mode),
                write_mask,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: use_depth.then_some(wgpu::Face::Back),
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(use_depth),
            depth_compare: Some(if use_depth {
                wgpu::CompareFunction::LessEqual
            } else {
                wgpu::CompareFunction::Always
            }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
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
        array_stride: mem::size_of::<deadlib_render_core::MeshVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &MESH_ATTRS,
    }
}

const fn textured_mesh_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<TexturedMeshVertex>() as u64,
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
    0 => Float32x3, // pos
    1 => Float32x2, // uv
    2 => Float32x4, // color
    3 => Float32x2, // tex-matrix scale
];

const TMESH_INSTANCE_ATTRS: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    4 => Float32x4, // model column 0
    5 => Float32x4, // model column 1
    6 => Float32x4, // model column 2
    7 => Float32x4, // model column 3
    8 => Float32x4, // tint
    9 => Float32x2, // uv scale
    10 => Float32x2, // uv offset
    11 => Float32x2, // uv texture-matrix shift
    12 => Float32, // texture alpha-mask mode
];

const INSTANCE_ATTRS: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
    2 => Float32x4, // center xyz + pad
    3 => Float32x2, // size
    4 => Float32x2, // sin/cos
    5 => Float32x4, // tint
    6 => Float32x2, // uv scale
    7 => Float32x2, // uv offset
    8 => Float32x2, // local offset
    9 => Float32x2, // local offset sin/cos
    10 => Float32x4, // edge fade
    11 => Float32, // texture alpha-mask mode
];

const PROJ_BYTES: u64 = mem::size_of::<[[f32; 4]; 4]>() as u64;

#[inline(always)]
fn cast_slice<T: bytemuck::Pod>(data: &[T]) -> &[u8] {
    bytemuck::cast_slice(data)
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
    if let Some(existing) = state.samplers.get(desc) {
        return existing.clone();
    }
    let sampler = state.device.create_sampler(&sampler_descriptor(desc));
    state.samplers.insert(desc, sampler.clone());
    sampler
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

const SHADER_IMM: &str = include_str!("shaders/wgpu_sprite.wgsl");
const YUV_SHADER_IMM: &str = include_str!("shaders/wgpu_sprite_yuv.wgsl");
const MESH_SHADER_IMM: &str = include_str!("shaders/wgpu_mesh.wgsl");
const TMESH_SHADER_IMM: &str = include_str!("shaders/wgpu_tmesh.wgsl");
const SHADER_UBO: &str = include_str!("shaders/wgpu_sprite_ubo.wgsl");
const YUV_SHADER_UBO: &str = include_str!("shaders/wgpu_sprite_yuv_ubo.wgsl");
const MESH_SHADER_UBO: &str = include_str!("shaders/wgpu_mesh_ubo.wgsl");
const TMESH_SHADER_UBO: &str = include_str!("shaders/wgpu_tmesh_ubo.wgsl");

#[cfg(test)]
mod tests {
    use super::{
        DrawBindingCache, InstanceBinding, Matrix4, PresentCompletion, PresentCompletionCell,
        YUV_SHADER_IMM, YUV_SHADER_UBO, stage_offscreen_projection_upload, stage_projection_upload,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    const STRIDE: usize = 256;

    #[test]
    fn planar_video_shaders_validate() {
        for source in [YUV_SHADER_IMM, YUV_SHADER_UBO] {
            let module = naga::front::wgsl::parse_str(source).expect("YUV shader parses");
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .expect("YUV shader validates");
        }
    }

    #[test]
    fn draw_binding_cache_keeps_exact_camera_and_texture_bindings() {
        let mut cache = DrawBindingCache::default();

        assert!(cache.camera_required(2));
        assert!(!cache.camera_required(2));
        cache.reset_camera();
        assert!(cache.camera_required(2));

        assert!(cache.texture_required(7, false));
        assert!(!cache.texture_required(7, false));
        assert!(cache.texture_required(7, true));
        assert!(cache.texture_required(8, true));
        assert!(!cache.texture_required(8, true));
    }

    #[test]
    fn draw_binding_cache_retains_index_and_tracks_instance_slot_kind() {
        let mut cache = DrawBindingCache::default();

        assert!(cache.index_required());
        assert!(!cache.index_required());
        assert!(cache.instance_required(InstanceBinding::Sprite));
        assert!(!cache.instance_required(InstanceBinding::Sprite));
        assert!(cache.instance_required(InstanceBinding::TexturedMesh));
        assert!(!cache.instance_required(InstanceBinding::TexturedMesh));
        assert!(cache.instance_required(InstanceBinding::Sprite));
    }

    #[test]
    fn projection_cache_compares_float_bits() {
        let mut upload = Vec::new();
        let positive_zero = Matrix4::from_cols_array(&[0.0; 16]);
        let mut negative_zero_bits = [0.0; 16];
        negative_zero_bits[12] = -0.0;
        let negative_zero = Matrix4::from_cols_array(&negative_zero_bits);

        assert!(stage_projection_upload(
            &mut upload,
            &[],
            positive_zero,
            STRIDE,
        ));
        assert!(stage_projection_upload(
            &mut upload,
            &[],
            negative_zero,
            STRIDE,
        ));
        assert!(!stage_projection_upload(
            &mut upload,
            &[],
            negative_zero,
            STRIDE,
        ));
    }

    #[test]
    fn projection_cache_stages_cameras_then_fallback_with_zero_padding() {
        let mut upload = Vec::new();
        let cameras = [Matrix4::from_scale([2.0, 3.0, 4.0].into())];
        let fallback = Matrix4::from_translation([5.0, 6.0, 7.0].into());

        assert!(stage_projection_upload(
            &mut upload,
            &cameras,
            fallback,
            STRIDE,
        ));
        assert_eq!(upload.len(), STRIDE * 2);
        assert_eq!(
            &upload[..64],
            bytemuck::cast_slice(&cameras[0].to_cols_array())
        );
        assert_eq!(
            &upload[STRIDE..STRIDE + 64],
            bytemuck::cast_slice(&fallback.to_cols_array())
        );
        assert!(upload[64..STRIDE].iter().all(|byte| *byte == 0));
        assert!(upload[STRIDE + 64..].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn offscreen_projection_upload_keeps_each_pass_fallback_adjacent() {
        let mut upload = Vec::new();
        let first = [Matrix4::from_translation([1.0, 2.0, 3.0].into())];
        let second = [
            Matrix4::from_scale([2.0, 3.0, 4.0].into()),
            Matrix4::from_rotation_z(0.5),
        ];
        let fallback = Matrix4::from_translation([5.0, 6.0, 7.0].into());

        stage_offscreen_projection_upload(
            &mut upload,
            [first.as_slice(), second.as_slice()],
            fallback,
            STRIDE,
        );

        let expected = [first[0], fallback, second[0], second[1], fallback];
        assert_eq!(upload.len(), expected.len() * STRIDE);
        for (index, matrix) in expected.iter().enumerate() {
            let offset = index * STRIDE;
            assert_eq!(
                &upload[offset..offset + 64],
                bytemuck::cast_slice(&matrix.to_cols_array())
            );
            assert!(
                upload[offset + 64..offset + STRIDE]
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
    }

    #[test]
    fn completion_cell_publishes_coherent_intervals_and_smoothing() {
        let cell = PresentCompletionCell::new();
        assert_eq!(cell.load().present_id, 0);

        cell.publish(1, 100);
        assert_eq!(
            cell.load(),
            PresentCompletion {
                present_id: 1,
                host_ns: 100,
                interval_ns: 0,
                refresh_ns: 0,
            }
        );

        cell.publish(2, 116);
        assert_eq!(
            cell.load(),
            PresentCompletion {
                present_id: 2,
                host_ns: 116,
                interval_ns: 16,
                refresh_ns: 16,
            }
        );

        cell.publish(3, 136);
        assert_eq!(
            cell.load(),
            PresentCompletion {
                present_id: 3,
                host_ns: 136,
                interval_ns: 20,
                refresh_ns: 17,
            }
        );

        cell.publish(4, 0);
        assert_eq!(
            cell.load(),
            PresentCompletion {
                present_id: 4,
                host_ns: 136,
                interval_ns: 0,
                refresh_ns: 17,
            }
        );
    }

    #[test]
    fn completion_cell_never_exposes_a_torn_snapshot() {
        let cell = Arc::new(PresentCompletionCell::new());
        let done = Arc::new(AtomicBool::new(false));
        let writer_cell = Arc::clone(&cell);
        let writer_done = Arc::clone(&done);
        let writer = std::thread::spawn(move || {
            for present_id in 1..=10_000 {
                writer_cell.publish(present_id, u64::from(present_id) * 10);
            }
            writer_done.store(true, Ordering::Release);
        });

        while !done.load(Ordering::Acquire) {
            let snapshot = cell.load();
            if snapshot.present_id == 0 {
                continue;
            }
            assert_eq!(snapshot.host_ns, u64::from(snapshot.present_id) * 10);
            if snapshot.present_id == 1 {
                assert_eq!(snapshot.interval_ns, 0);
                assert_eq!(snapshot.refresh_ns, 0);
            } else {
                assert_eq!(snapshot.interval_ns, 10);
                assert_eq!(snapshot.refresh_ns, 10);
            }
        }
        writer.join().expect("completion writer");
        assert_eq!(cell.load().present_id, 10_000);
    }
}
