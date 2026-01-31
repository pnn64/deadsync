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
use std::{borrow::Cow, collections::HashMap, error::Error, mem, sync::Arc};
use wgpu::util::DeviceExt;
use winit::window::Window;

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
    edge_fade: [f32; 4],
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

// A handle to a wgpu texture and its bind group.
pub struct Texture {
    id: u64,
    _texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: Arc<wgpu::BindGroup>,
}

struct SpriteRun {
    start: u32,
    count: u32,
    blend: BlendMode,
    bind_group: Arc<wgpu::BindGroup>,
    key: u64,
    camera: u8,
}

enum Op {
    Sprite(SpriteRun),
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
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    scratch_instances: Vec<InstanceRaw>,
    scratch_mesh_vertices: Vec<crate::core::gfx::MeshVertex>,
    scratch_ops: Vec<Op>,
    mesh_vertex_buffer: wgpu::Buffer,
    mesh_vertex_capacity: usize,
    window_size: (u32, u32),
    vsync_enabled: bool,
    next_texture_id: u64,
}

pub fn init(window: Arc<Window>, vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing Vulkan (wgpu) backend...");

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::default(),
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
    .map_err(|e| format!("No suitable Vulkan adapter found: {e}"))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("deadsync vk device"),
        required_features: wgpu::Features::IMMEDIATES,
        required_limits: wgpu::Limits {
            max_immediate_size: mem::size_of::<[[f32; 4]; 4]>() as u32,
            ..wgpu::Limits::default()
        },
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

    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vk texture layout"),
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

    let (shader, pipeline_layout, pipelines) = build_pipeline_set(&device, &bind_layout, format);
    let (mesh_shader, mesh_pipeline_layout, mesh_pipelines) =
        build_mesh_pipeline_set(&device, format);

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
        label: Some("vk quad vertices"),
        contents: cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("vk quad indices"),
        contents: cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let instance_capacity = 64usize;
    let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vk instance buffer"),
        size: (instance_capacity * mem::size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mesh_vertex_capacity = 1024usize;
    let mesh_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vk mesh vertex buffer"),
        size: (mesh_vertex_capacity * mem::size_of::<crate::core::gfx::MeshVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    info!("Vulkan (wgpu) backend initialized.");

    Ok(State {
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
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        instance_buffer,
        instance_capacity,
        scratch_instances: Vec::with_capacity(instance_capacity),
        scratch_mesh_vertices: Vec::with_capacity(mesh_vertex_capacity),
        scratch_ops: Vec::with_capacity(64),
        mesh_vertex_buffer,
        mesh_vertex_capacity,
        window_size: (size.width, size.height),
        vsync_enabled,
        next_texture_id: 1,
    })
}

pub fn create_texture(
    state: &mut State,
    image: &RgbaImage,
    sampler: SamplerDesc,
) -> Result<Texture, Box<dyn Error>> {
    let size = wgpu::Extent3d {
        width: image.width(),
        height: image.height(),
        depth_or_array_layers: 1,
    };

    let texture = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("vk texture"),
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
    let sampler = get_sampler(state, sampler);
    let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vk texture bind group"),
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

    let id = state.next_texture_id;
    state.next_texture_id = state.next_texture_id.wrapping_add(1);

    Ok(Texture {
        id,
        _texture: texture,
        _view: view,
        bind_group: Arc::new(bind_group),
    })
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

    {
        let objects_len = render_list.objects.len();
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

        let ops = &mut state.scratch_ops;
        ops.clear();
        if ops.capacity() < objects_len {
            ops.reserve(objects_len - ops.capacity());
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

        for obj in &render_list.objects {
            match &obj.object_type {
                ObjectType::Sprite {
                    texture_id,
                    tint,
                    uv_scale,
                    uv_offset,
                    edge_fade,
                } => {
                    let tex = match textures.get(texture_id.as_ref()) {
                        Some(RendererTexture::VulkanWgpu(t)) => t,
                        _ => continue,
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
            label: Some("vk encoder"),
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("vk render pass"),
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
        let mut last_kind: Option<bool> = None;
        let mut last_blend: Option<BlendMode> = None;
        let mut last_bind: Option<u64> = None;
        let mut last_camera: Option<u8> = None;
        for op in &state.scratch_ops {
            match op {
                Op::Sprite(run) => {
                    if last_kind != Some(false) {
                        pass.set_vertex_buffer(0, state.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, state.instance_buffer.slice(..));
                        pass.set_index_buffer(
                            state.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        last_kind = Some(false);
                        last_blend = None;
                        last_bind = None;
                        last_camera = None;
                    }
                    if last_blend != Some(run.blend) {
                        pass.set_pipeline(state.pipelines.get(run.blend));
                        last_blend = Some(run.blend);
                        last_bind = None;
                    }
                    if last_camera != Some(run.camera) {
                        let vp = render_list
                            .cameras
                            .get(run.camera as usize)
                            .copied()
                            .unwrap_or(state.projection);
                        let vp_array: [[f32; 4]; 4] = vp.into();
                        pass.set_immediates(0, cast_slice(&vp_array));
                        last_camera = Some(run.camera);
                    }
                    if last_bind != Some(run.key) {
                        pass.set_bind_group(0, Some(run.bind_group.as_ref()), &[]);
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
                    if last_kind != Some(true) {
                        pass.set_vertex_buffer(0, state.mesh_vertex_buffer.slice(..));
                        last_kind = Some(true);
                        last_blend = None;
                        last_bind = None;
                        last_camera = None;
                    }
                    if last_blend != Some(*blend) {
                        pass.set_pipeline(state.mesh_pipelines.get(*blend));
                        last_blend = Some(*blend);
                    }
                    if last_camera != Some(*camera) {
                        let vp = render_list
                            .cameras
                            .get(*camera as usize)
                            .copied()
                            .unwrap_or(state.projection);
                        let vp_array: [[f32; 4]; 4] = vp.into();
                        pass.set_immediates(0, cast_slice(&vp_array));
                        last_camera = Some(*camera);
                    }
                    match mode {
                        MeshMode::Triangles => pass.draw(*start..(*start + *count), 0..1),
                    }
                }
            }
        }
        drop(pass);
    }

    state.queue.submit(Some(encoder.finish()));
    frame.present();

    Ok((instance_len as u32) * 4)
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

pub fn cleanup(_state: &mut State) {
    info!("Vulkan (wgpu) backend cleanup complete.");
}

fn ensure_instance_capacity(state: &mut State, needed: usize) {
    if needed <= state.instance_capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(64);
    state.instance_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vk instance buffer"),
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
        label: Some("vk mesh vertex buffer"),
        size: (new_cap * mem::size_of::<crate::core::gfx::MeshVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    state.mesh_vertex_capacity = new_cap;
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
    if format_changed {
        let (shader, pipeline_layout, pipelines) =
            build_pipeline_set(&state.device, &state.bind_layout, state.config.format);
        let (mesh_shader, mesh_pipeline_layout, mesh_pipelines) =
            build_mesh_pipeline_set(&state.device, state.config.format);
        state.shader = shader;
        state.pipeline_layout = pipeline_layout;
        state.pipelines = pipelines;
        state.mesh_shader = mesh_shader;
        state.mesh_pipeline_layout = mesh_pipeline_layout;
        state.mesh_pipelines = mesh_pipelines;
    }
}

fn pick_format(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    // Match GL/Vulkan path: avoid sRGB conversion to keep colors consistent.
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
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (wgpu::ShaderModule, wgpu::PipelineLayout, PipelineSet) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vk shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vk pipeline layout"),
        bind_group_layouts: &[bind_layout],
        immediate_size: mem::size_of::<[[f32; 4]; 4]>() as u32,
    });

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
    format: wgpu::TextureFormat,
) -> (wgpu::ShaderModule, wgpu::PipelineLayout, MeshPipelineSet) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("vk mesh shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(MESH_SHADER)),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("vk mesh pipeline layout"),
        bind_group_layouts: &[],
        immediate_size: mem::size_of::<[[f32; 4]; 4]>() as u32,
    });

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

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    mode: BlendMode,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("vk pipeline"),
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
        label: Some("vk mesh pipeline"),
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

const VERT_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
];

const MESH_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2, // pos
    1 => Float32x4, // color
];

const INSTANCE_ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    2 => Float32x2, // center
    3 => Float32x2, // size
    4 => Float32x2, // sin/cos
    5 => Float32x4, // tint
    6 => Float32x2, // uv scale
    7 => Float32x2, // uv offset
    8 => Float32x4, // edge fade
];

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
        label: Some("vk sampler"),
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

const SHADER: &str = include_str!("../shaders/wgpu_sprite.wgsl");
const MESH_SHADER: &str = include_str!("../shaders/wgpu_mesh.wgsl");
