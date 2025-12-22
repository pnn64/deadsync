use crate::core::gfx::{
    BlendMode, ObjectType, RenderList, SamplerDesc, SamplerFilter, SamplerWrap,
    Texture as RendererTexture,
};
use crate::core::space::ortho_for_window;
use cgmath::Matrix4;
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
    fn get(&self, mode: BlendMode) -> &wgpu::RenderPipeline {
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
    projection_buffer: wgpu::Buffer,
    projection_group: wgpu::BindGroup,
    proj_layout: wgpu::BindGroupLayout,
    bind_layout: wgpu::BindGroupLayout,
    samplers: HashMap<SamplerDesc, wgpu::Sampler>,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: PipelineSet,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    window_size: (u32, u32),
    vsync_enabled: bool,
    next_texture_id: u64,
}

pub fn init(window: Arc<Window>, vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing DirectX (wgpu) backend...");

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12,
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
    .map_err(|e| format!("No suitable DirectX adapter found: {e}"))?;

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("deadsync dx12 device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
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
    let proj_array: [[f32; 4]; 4] = projection.into();
    let projection_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("dx12 projection"),
        contents: cast_slice(&proj_array),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let proj_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dx12 proj layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new((mem::size_of::<[[f32; 4]; 4]>()) as u64),
            },
            count: None,
        }],
    });
    let projection_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("dx12 proj group"),
        layout: &proj_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: projection_buffer.as_entire_binding(),
        }],
    });

    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dx12 texture layout"),
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
        build_pipeline_set(&device, &proj_layout, &bind_layout, format);

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
        label: Some("dx12 quad vertices"),
        contents: cast_slice(&vertex_data),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("dx12 quad indices"),
        contents: cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let instance_capacity = 64usize;
    let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dx12 instance buffer"),
        size: (instance_capacity * mem::size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    info!("DirectX (wgpu) backend initialized.");

    Ok(State {
        _instance: instance,
        surface,
        adapter,
        device,
        queue,
        config,
        projection,
        projection_buffer,
        projection_group,
        proj_layout,
        bind_layout,
        samplers: HashMap::new(),
        shader,
        pipeline_layout,
        pipelines,
        vertex_buffer,
        index_buffer,
        index_count: indices.len() as u32,
        instance_buffer,
        instance_capacity,
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
        label: Some("dx12 texture"),
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
        label: Some("dx12 texture bind group"),
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

pub fn draw<'a>(
    state: &mut State,
    render_list: &RenderList<'a>,
    textures: &HashMap<String, RendererTexture>,
) -> Result<u32, Box<dyn Error>> {
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(0);
    }

    struct Run {
        start: u32,
        count: u32,
        blend: BlendMode,
        bind_group: Arc<wgpu::BindGroup>,
        key: u64,
    }

    let mut instances: Vec<InstanceRaw> = Vec::with_capacity(render_list.objects.len());
    let mut runs: Vec<Run> = Vec::new();

    #[inline(always)]
    fn decompose_2d(m: [[f32; 4]; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
        let center = [m[3][0], m[3][1]];
        let c0 = [m[0][0], m[0][1]];
        let c1 = [m[1][0], m[1][1]];
        let sx = (c0[0] * c0[0] + c0[1] * c0[1]).sqrt().max(1e-12);
        let sy = (c1[0] * c1[0] + c1[1] * c1[1]).sqrt().max(1e-12);
        let cos_t = c0[0] / sx;
        let sin_t = c0[1] / sx;
        (center, [sx, sy], [sin_t, cos_t])
    }

    for obj in &render_list.objects {
        let (texture_id, tint, uv_scale, uv_offset, edge_fade) = match &obj.object_type {
            ObjectType::Sprite {
                texture_id,
                tint,
                uv_scale,
                uv_offset,
                edge_fade,
            } => (texture_id, tint, uv_scale, uv_offset, edge_fade),
        };

        let tex = match textures.get(texture_id.as_ref()) {
            Some(RendererTexture::DirectX(t)) => t,
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

        if let Some(last) = runs.last_mut() {
            if last.key == tex.id && last.blend == obj.blend {
                last.count += 1;
                continue;
            }
        }
        runs.push(Run {
            start,
            count: 1,
            blend: obj.blend,
            bind_group: tex.bind_group.clone(),
            key: tex.id,
        });
    }

    ensure_instance_capacity(state, instances.len());
    if !instances.is_empty() {
        state
            .queue
            .write_buffer(&state.instance_buffer, 0, cast_slice(&instances));
    }

    write_projection(&state.queue, &state.projection_buffer, state.projection);

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
            label: Some("dx12 encoder"),
        });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dx12 render pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: render_list.clear_color[0] as f64,
                        g: render_list.clear_color[1] as f64,
                        b: render_list.clear_color[2] as f64,
                        a: render_list.clear_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        pass.set_vertex_buffer(0, state.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, state.instance_buffer.slice(..));
        pass.set_index_buffer(state.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_bind_group(0, &state.projection_group, &[]);

        for run in runs {
            pass.set_pipeline(state.pipelines.get(run.blend));
            pass.set_bind_group(1, Some(run.bind_group.as_ref()), &[]);
            pass.draw_indexed(0..state.index_count, 0, run.start..(run.start + run.count));
        }
        drop(pass);
    }

    state.queue.submit(Some(encoder.finish()));
    frame.present();

    Ok((instances.len() as u32) * 4)
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
    info!("DirectX (wgpu) backend cleanup complete.");
}

fn ensure_instance_capacity(state: &mut State, needed: usize) {
    if needed <= state.instance_capacity {
        return;
    }
    let new_cap = needed.next_power_of_two().max(64);
    state.instance_buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dx12 instance buffer"),
        size: (new_cap * mem::size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    state.instance_capacity = new_cap;
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
    write_projection(&state.queue, &state.projection_buffer, state.projection);
    if format_changed {
        let (shader, pipeline_layout, pipelines) = build_pipeline_set(
            &state.device,
            &state.proj_layout,
            &state.bind_layout,
            state.config.format,
        );
        state.shader = shader;
        state.pipeline_layout = pipeline_layout;
        state.pipelines = pipelines;
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
    proj_layout: &wgpu::BindGroupLayout,
    bind_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> (wgpu::ShaderModule, wgpu::PipelineLayout, PipelineSet) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("dx12 shader module"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("dx12 pipeline layout"),
        bind_group_layouts: &[proj_layout, bind_layout],
        push_constant_ranges: &[],
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

fn build_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    mode: BlendMode,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dx12 pipeline"),
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
        multiview: None,
        cache: None,
    })
}

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<Vertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &VERT_ATTRS,
    }
}

fn instance_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: mem::size_of::<InstanceRaw>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &INSTANCE_ATTRS,
    }
}

const VERT_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
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
fn cast_slice<T>(data: &[T]) -> &[u8] {
    let len = data.len() * mem::size_of::<T>();
    unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, len) }
}

#[inline(always)]
fn wgpu_filter_mode(filter: SamplerFilter) -> wgpu::FilterMode {
    match filter {
        SamplerFilter::Linear => wgpu::FilterMode::Linear,
        SamplerFilter::Nearest => wgpu::FilterMode::Nearest,
    }
}

#[inline(always)]
fn wgpu_address_mode(wrap: SamplerWrap) -> wgpu::AddressMode {
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
        filter
    } else {
        wgpu::FilterMode::Nearest
    };
    wgpu::SamplerDescriptor {
        label: Some("dx12 sampler"),
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

#[inline(always)]
fn write_projection(queue: &wgpu::Queue, buffer: &wgpu::Buffer, proj: Matrix4<f32>) {
    let arr: [[f32; 4]; 4] = proj.into();
    queue.write_buffer(buffer, 0, cast_slice(&arr));
}

const SHADER: &str = r#"
struct Proj {
    proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u_proj: Proj;
@group(1) @binding(0) var u_sampler: sampler;
@group(1) @binding(1) var u_tex: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) center: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) rot: vec2<f32>,
    @location(5) tint: vec4<f32>,
    @location(6) uv_scale: vec2<f32>,
    @location(7) uv_offset: vec2<f32>,
    @location(8) edge_fade: vec4<f32>,
};

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) edge_fade: vec4<f32>,
};

@vertex
fn vs_main(input: VertexIn) -> VertexOut {
    let local = vec2<f32>(input.pos.x * input.size.x, input.pos.y * input.size.y);
    let s = input.rot.x;
    let c = input.rot.y;
    let rotated = vec2<f32>(c * local.x - s * local.y, s * local.x + c * local.y);
    let world = input.center + rotated;

    var out: VertexOut;
    out.pos = u_proj.proj * vec4<f32>(world, 0.0, 1.0);
    out.uv = input.uv * input.uv_scale + input.uv_offset;
    out.tint = input.tint;
    out.edge_fade = input.edge_fade;
    return out;
}

fn edge_factor(t: f32, feather_l: f32, feather_r: f32) -> f32 {
    var l = 1.0;
    var r = 1.0;
    if feather_l > 0.0 {
        l = clamp((t - 0.0) / feather_l, 0.0, 1.0);
    }
    if feather_r > 0.0 {
        r = clamp((1.0 - t) / feather_r, 0.0, 1.0);
    }
    return min(l, r);
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
    let texel = textureSample(u_tex, u_sampler, input.uv);
    let fade_x = edge_factor(input.uv.x, input.edge_fade.x, input.edge_fade.y);
    let fade_y = edge_factor(input.uv.y, input.edge_fade.z, input.edge_fade.w);
    let fade = min(fade_x, fade_y);
    var color = texel * input.tint;
    color.a = color.a * fade;
    return color;
}
"#;
