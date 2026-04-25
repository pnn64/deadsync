use crate::engine::gfx::{
    BlendMode, ClockDomainTrace, DrawStats, FastU64Map, MeshVertex, PresentModePolicy,
    PresentModeTrace, PresentStats, RenderList, SamplerDesc, SamplerFilter, SamplerWrap,
    TMeshCacheKey, Texture as RendererTexture, TextureHandleMap,
    draw_prep::{
        self, DrawOp, DrawScratch, SpriteInstanceRaw as InstanceData,
        TexturedMeshInstanceRaw as TexturedMeshInstanceGpu, TexturedMeshSource,
        TexturedMeshVertexRaw as TexturedMeshVertexGpu,
    },
};
use crate::engine::space::ortho_for_window;
use ash::{
    Device, Entry, Instance,
    google::display_timing,
    khr::{calibrated_timestamps, surface, swapchain},
    vk,
};
use glam::Mat4 as Matrix4;
use image::RgbaImage;
use log::{debug, error, info, warn};
use std::{collections::HashMap, error::Error, ffi, mem, sync::Arc, time::Instant};
#[cfg(windows)]
use windows::Win32::System::Performance;
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::Window,
};

// --- Constants ---
const MAX_FRAMES_IN_FLIGHT: usize = 3;
const DESCRIPTOR_POOL_SET_CAPACITY: u32 = 1024;
const VULKAN_IMAGE_WAIT_THRESHOLD_US: u32 = 1_000;
const VULKAN_BACK_PRESSURE_THRESHOLD_US: u32 = 1_000;
const VULKAN_PRESENT_DISPLAY_TIMING_TELEMETRY: bool = false;
const VULKAN_TMESH_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;
#[cfg(windows)]
static QPC_FREQ_HZ: std::sync::LazyLock<Option<u64>> = std::sync::LazyLock::new(qpc_freq_hz);

// --- Structs ---
// Vulkan consumes the shared draw-prep raw layouts directly so the dynamic
// upload path can memcpy them into the mapped ring without repacking.

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ProjPush {
    proj: [[f32; 4]; 4],
}

struct PipelinePair {
    layout: vk::PipelineLayout,
    pipe: vk::Pipeline,
}

// A handle to a Vulkan texture on the GPU.
pub struct Texture {
    device: Arc<Device>,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    pub descriptor_set: vk::DescriptorSet,
    pub descriptor_set_repeat: vk::DescriptorSet,
    pool: vk::DescriptorPool,
}

impl Drop for Texture {
    fn drop(&mut self) {
        // SAFETY: `Texture` owns these Vulkan objects, the `Device` outlives `self` via `Arc`,
        // and both descriptor sets were allocated from `self.pool` and are not freed elsewhere.
        unsafe {
            let _ = self.device.free_descriptor_sets(
                self.pool,
                &[self.descriptor_set, self.descriptor_set_repeat],
            );
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

struct BufferResource {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}

struct SubmittedTextureUpload {
    frame: usize,
    cmd: vk::CommandBuffer,
    staging: Vec<BufferResource>,
}

struct RetiredTexture {
    retire_after_present_id: u32,
    _texture: Texture,
}

struct CachedTMeshGeom {
    buffer: BufferResource,
    vertex_count: u32,
}

struct SwapchainResources {
    swapchain_loader: swapchain::Device,
    swapchain: vk::SwapchainKHR,
    _images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    extent: vk::Extent2D,
    format: vk::SurfaceFormatKHR,
    present_mode: vk::PresentModeKHR,
    supports_transfer_src: bool,
}

#[derive(Clone, Copy, Default)]
struct DeviceExts {
    display_timing: bool,
    calibrated_timestamps: bool,
}

#[derive(Clone, Copy, Default)]
struct CompletedPresentTiming {
    present_id: u32,
    actual_present_time_ns: u64,
    interval_ns: u64,
    present_margin_ns: u64,
    host_present_time_ns: u64,
}

struct PresentTelemetryState {
    display_timing: Option<display_timing::Device>,
    calibrated_timestamps: Option<calibrated_timestamps::Device>,
    display_clock: Option<vk::TimeDomainKHR>,
    host_clock: Option<vk::TimeDomainKHR>,
    refresh_ns: u64,
    host_minus_display_ns: i64,
    calibration_error_ns: u64,
    next_present_id: u32,
    last_seen_present_id: u32,
    cpu_last_completed_present_id: u32,
    cpu_last_completed_host_ns: u64,
    last_completed: Option<CompletedPresentTiming>,
    cpu_image_present_ids: Vec<u32>,
    scratch_timings: Vec<vk::PastPresentationTimingGOOGLE>,
}

impl Default for PresentTelemetryState {
    fn default() -> Self {
        Self {
            display_timing: None,
            calibrated_timestamps: None,
            display_clock: None,
            host_clock: None,
            refresh_ns: 0,
            host_minus_display_ns: 0,
            calibration_error_ns: 0,
            next_present_id: 1,
            last_seen_present_id: 0,
            cpu_last_completed_present_id: 0,
            cpu_last_completed_host_ns: 0,
            last_completed: None,
            cpu_image_present_ids: Vec::new(),
            scratch_timings: Vec::new(),
        }
    }
}

// The main Vulkan state struct, now simplified.
pub struct State {
    _entry: Entry,
    instance: Instance,
    debug_messenger: Option<vk::DebugUtilsMessengerEXT>,
    debug_loader: Option<ash::ext::debug_utils::Instance>,
    surface: vk::SurfaceKHR,
    surface_loader: surface::Instance,
    pub pdevice: vk::PhysicalDevice,
    pub device: Option<Arc<Device>>,
    pub queue: vk::Queue,
    pub command_pool: vk::CommandPool,
    swapchain_resources: SwapchainResources,
    render_pass: vk::RenderPass,
    sprite_pipeline_layout: vk::PipelineLayout,
    sprite_pipeline: vk::Pipeline,
    mesh_pipeline_layout: vk::PipelineLayout,
    mesh_pipeline: vk::Pipeline,
    textured_mesh_pipeline_layout: vk::PipelineLayout,
    textured_mesh_pipeline: vk::Pipeline,
    vertex_buffer: Option<BufferResource>,
    index_buffer: Option<BufferResource>,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pools: Vec<vk::DescriptorPool>,
    sampler_cache: HashMap<SamplerDesc, vk::Sampler>,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
    images_in_flight: Vec<vk::Fence>,
    current_frame: usize,
    swapchain_valid: bool,
    window_size: PhysicalSize<u32>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    projection: Matrix4,
    instance_ring: Option<BufferResource>, // one big VB for all frames
    instance_ring_ptr: *mut InstanceData,  // persistently mapped pointer
    instance_capacity_instances: usize,    // total instances across ring
    per_frame_stride_instances: usize,     // instances reserved per frame
    mesh_ring: Option<BufferResource>,     // one big VB for all frames
    mesh_ring_ptr: *mut MeshVertex,        // persistently mapped pointer
    mesh_capacity_vertices: usize,         // total vertices across ring
    per_frame_stride_vertices: usize,      // vertices reserved per frame
    tmesh_ring: Option<BufferResource>,    // one big VB for all frames (textured mesh)
    tmesh_ring_ptr: *mut TexturedMeshVertexGpu, // persistently mapped pointer
    tmesh_capacity_vertices: usize,        // total textured mesh vertices across ring
    per_frame_stride_tmesh_vertices: usize, // textured mesh vertices reserved per frame
    tmesh_instance_ring: Option<BufferResource>, // one big instanced VB for textured meshes
    tmesh_instance_ring_ptr: *mut TexturedMeshInstanceGpu, // persistently mapped pointer
    tmesh_capacity_instances: usize,       // total textured mesh instances across ring
    per_frame_stride_tmesh_instances: usize, // textured mesh instances reserved per frame
    prep: DrawScratch,
    cached_tmesh: FastU64Map<CachedTMeshGeom>,
    cached_tmesh_bytes: usize,
    pending_tex_upload_cmd: Option<vk::CommandBuffer>, // batched texture upload cmd
    pending_tex_staging: Vec<BufferResource>, // keep staging alive until upload batch flush
    submitted_tex_uploads: Vec<SubmittedTextureUpload>, // retired when the tagged frame slot completes
    retired_textures: Vec<RetiredTexture>,
    last_submitted_present_id: u32,
    present_telemetry: PresentTelemetryState,
    screenshot_requested: bool,
    captured_frame: Option<RgbaImage>,
}

// --- Main Procedural Functions ---
pub fn init(
    window: &Window,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing Vulkan backend...");
    let entry = Entry::linked();
    let (instance, debug_utils_enabled) = create_instance(&entry, window, gfx_debug_enabled)?;
    let (debug_loader, debug_messenger) =
        setup_debug_messenger(&entry, &instance, debug_utils_enabled)?;
    let surface = create_surface(&entry, &instance, window)?;
    let surface_loader = surface::Instance::new(&entry, &instance);
    let pdevice = select_physical_device(&instance, &surface_loader, surface)?;
    log_selected_device(&instance, pdevice);
    let (device, queue, queue_family_index, device_exts) =
        create_logical_device(&instance, pdevice, &surface_loader, surface)?;
    let device = Some(Arc::new(device));
    let command_pool = create_command_pool(device.as_ref().unwrap(), queue_family_index)?;

    let initial_size = window.inner_size();
    let mut swapchain_resources = create_swapchain(
        &instance,
        device.as_ref().unwrap(),
        pdevice,
        surface,
        &surface_loader,
        initial_size,
        None,
        vsync_enabled,
        present_mode_policy,
    )?;
    let render_pass =
        create_render_pass(device.as_ref().unwrap(), swapchain_resources.format.format)?;
    recreate_framebuffers(
        device.as_ref().unwrap(),
        &mut swapchain_resources,
        render_pass,
    )?;

    let descriptor_set_layout = create_descriptor_set_layout(device.as_ref().unwrap())?;
    let descriptor_pools = vec![create_descriptor_pool(device.as_ref().unwrap())?];

    let PipelinePair {
        layout: sprite_pipeline_layout,
        pipe: sprite_pipeline,
    } = create_sprite_pipeline(
        device.as_ref().unwrap(),
        render_pass,
        descriptor_set_layout,
        BlendMode::Alpha,
    )?;

    let PipelinePair {
        layout: mesh_pipeline_layout,
        pipe: mesh_pipeline,
    } = create_mesh_pipeline(device.as_ref().unwrap(), render_pass, BlendMode::Alpha)?;
    let PipelinePair {
        layout: textured_mesh_pipeline_layout,
        pipe: textured_mesh_pipeline,
    } = create_textured_mesh_pipeline(
        device.as_ref().unwrap(),
        render_pass,
        descriptor_set_layout,
        BlendMode::Alpha,
    )?;

    let command_buffers =
        create_command_buffers(device.as_ref().unwrap(), command_pool, MAX_FRAMES_IN_FLIGHT)?;
    let (image_available_semaphores, render_finished_semaphores, in_flight_fences) =
        create_sync_objects(device.as_ref().unwrap())?;
    let images_in_flight = vec![vk::Fence::null(); swapchain_resources._images.len()];

    let projection = ortho_for_window(initial_size.width, initial_size.height);
    let present_telemetry = init_present_telemetry(
        &entry,
        &instance,
        device.as_ref().unwrap(),
        pdevice,
        swapchain_resources.swapchain,
        device_exts,
    );

    let mut state = State {
        _entry: entry,
        instance,
        debug_messenger,
        debug_loader,
        surface,
        surface_loader,
        pdevice,
        device: device.clone(),
        queue,
        command_pool,
        swapchain_resources,
        render_pass,
        sprite_pipeline_layout,
        sprite_pipeline,
        mesh_pipeline_layout,
        mesh_pipeline,
        textured_mesh_pipeline_layout,
        textured_mesh_pipeline,
        vertex_buffer: None,
        index_buffer: None,
        descriptor_set_layout,
        descriptor_pools,
        sampler_cache: HashMap::new(),
        command_buffers,
        image_available_semaphores,
        render_finished_semaphores,
        in_flight_fences,
        images_in_flight,
        current_frame: 0,
        swapchain_valid: true,
        window_size: initial_size,
        vsync_enabled,
        present_mode_policy,
        projection,
        instance_ring: None,
        instance_ring_ptr: std::ptr::null_mut(),
        instance_capacity_instances: 0,
        per_frame_stride_instances: 0,
        mesh_ring: None,
        mesh_ring_ptr: std::ptr::null_mut(),
        mesh_capacity_vertices: 0,
        per_frame_stride_vertices: 0,
        tmesh_ring: None,
        tmesh_ring_ptr: std::ptr::null_mut(),
        tmesh_capacity_vertices: 0,
        per_frame_stride_tmesh_vertices: 0,
        tmesh_instance_ring: None,
        tmesh_instance_ring_ptr: std::ptr::null_mut(),
        tmesh_capacity_instances: 0,
        per_frame_stride_tmesh_instances: 0,
        prep: DrawScratch::with_capacity(256, 1024, 1024, 256, 64),
        cached_tmesh: FastU64Map::default(),
        cached_tmesh_bytes: 0,
        pending_tex_upload_cmd: None,
        pending_tex_staging: Vec::new(),
        submitted_tex_uploads: Vec::new(),
        retired_textures: Vec::new(),
        last_submitted_present_id: 0,
        present_telemetry,
        screenshot_requested: false,
        captured_frame: None,
    };

    // Static unit quad buffers
    let vertices: [[f32; 4]; 4] = [
        [-0.5, -0.5, 0.0, 1.0],
        [0.5, -0.5, 1.0, 1.0],
        [0.5, 0.5, 1.0, 0.0],
        [-0.5, 0.5, 0.0, 0.0],
    ];
    let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];

    let device_arc = device.as_ref().unwrap();
    state.vertex_buffer = Some(create_buffer(
        &state.instance,
        device_arc,
        state.pdevice,
        state.command_pool,
        state.queue,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
        Some(&vertices),
    )?);
    state.index_buffer = Some(create_buffer(
        &state.instance,
        device_arc,
        state.pdevice,
        state.command_pool,
        state.queue,
        vk::BufferUsageFlags::INDEX_BUFFER,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
        Some(&indices),
    )?);

    info!("Vulkan backend initialized successfully.");
    Ok(state)
}

fn create_sampler(device: &Device, desc: SamplerDesc) -> Result<vk::Sampler, vk::Result> {
    let filter = match desc.filter {
        SamplerFilter::Linear => vk::Filter::LINEAR,
        SamplerFilter::Nearest => vk::Filter::NEAREST,
    };
    let address = match desc.wrap {
        SamplerWrap::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        SamplerWrap::Repeat => vk::SamplerAddressMode::REPEAT,
    };
    let mipmap_mode = if desc.mipmaps {
        match desc.filter {
            SamplerFilter::Linear => vk::SamplerMipmapMode::LINEAR,
            SamplerFilter::Nearest => vk::SamplerMipmapMode::NEAREST,
        }
    } else {
        vk::SamplerMipmapMode::NEAREST
    };
    let sampler_info = vk::SamplerCreateInfo::default()
        .mag_filter(filter)
        .min_filter(filter)
        .mipmap_mode(mipmap_mode)
        .address_mode_u(address)
        .address_mode_v(address)
        .address_mode_w(address)
        .anisotropy_enable(false)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS);
    // SAFETY: `sampler_info` references only stack data for this call, and `device` is a live
    // logical device owned by the renderer.
    unsafe { device.create_sampler(&sampler_info, None) }
}

fn get_sampler(state: &mut State, desc: SamplerDesc) -> Result<vk::Sampler, vk::Result> {
    if let Some(&sampler) = state.sampler_cache.get(&desc) {
        return Ok(sampler);
    }
    let device = state.device.as_ref().unwrap();
    let sampler = create_sampler(device, desc)?;
    state.sampler_cache.insert(desc, sampler);
    Ok(sampler)
}

fn create_descriptor_set_layout(device: &Device) -> Result<vk::DescriptorSetLayout, vk::Result> {
    let sampler_layout_binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT);

    let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
        .bindings(std::slice::from_ref(&sampler_layout_binding));

    // SAFETY: The create info references stack data valid for the duration of the call, and the
    // descriptor set layout is created on a live logical device.
    unsafe { device.create_descriptor_set_layout(&layout_info, None) }
}

fn create_descriptor_pool(device: &Device) -> Result<vk::DescriptorPool, vk::Result> {
    let pool_size = vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(DESCRIPTOR_POOL_SET_CAPACITY);

    let pool_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(std::slice::from_ref(&pool_size))
        .max_sets(DESCRIPTOR_POOL_SET_CAPACITY)
        .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

    // SAFETY: The create info contains only plain values and `device` is a valid logical device.
    unsafe { device.create_descriptor_pool(&pool_info, None) }
}

fn grow_descriptor_pool(state: &mut State) -> Result<vk::DescriptorPool, vk::Result> {
    let pool = create_descriptor_pool(state.device.as_ref().unwrap())?;
    state.descriptor_pools.push(pool);
    debug!(
        "Allocated Vulkan descriptor pool #{} ({} sets)",
        state.descriptor_pools.len(),
        DESCRIPTOR_POOL_SET_CAPACITY
    );
    Ok(pool)
}

fn create_sprite_pipeline(
    device: &Device,
    render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
    mode: BlendMode,
) -> Result<PipelinePair, Box<dyn Error>> {
    // Shaders (recompiled SPIR-V with compact instance layout)
    let vert_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_shader.vert.spv"));
    let frag_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_shader.frag.spv"));
    let vert_module = create_shader_module(device, vert_shader_code)?;
    let frag_module = create_shader_module(device, frag_shader_code)?;
    let main_name = c"main";

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(main_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(main_name),
    ];

    // Vertex inputs: binding 0 (unit quad), binding 1 (compact per-instance)
    let (binding_descriptions, attribute_descriptions) =
        vertex_input_descriptions_textured_instanced();
    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment = color_blend_for(mode);
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment));

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(std::mem::size_of::<ProjPush>() as u32);

    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(std::slice::from_ref(&set_layout))
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));

    // SAFETY: The descriptor set layout and push-constant range are valid for this pipeline, and
    // the create info borrows only stack data for the duration of the call.
    let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None)? };

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_info)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0);

    // SAFETY: All referenced shader modules, pipeline layout, and render pass are live and owned
    // by the caller; Vulkan copies the provided create info before returning.
    let pipe = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            .map_err(|e| e.1)?[0]
    };

    // SAFETY: The pipeline has already been created and no longer borrows the temporary shader
    // modules, so they can be destroyed immediately on the same device.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(PipelinePair { layout, pipe })
}

fn create_mesh_pipeline(
    device: &Device,
    render_pass: vk::RenderPass,
    mode: BlendMode,
) -> Result<PipelinePair, Box<dyn Error>> {
    // Shaders (recompiled SPIR-V)
    let vert_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_mesh.vert.spv"));
    let frag_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_mesh.frag.spv"));
    let vert_module = create_shader_module(device, vert_shader_code)?;
    let frag_module = create_shader_module(device, frag_shader_code)?;
    let main_name = c"main";

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(main_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(main_name),
    ];

    let (binding_descriptions, attribute_descriptions) = vertex_input_descriptions_mesh();
    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment = color_blend_for(mode);
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment));

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(std::mem::size_of::<ProjPush>() as u32);

    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));

    // SAFETY: The create info borrows only stack data and describes a valid push-constant layout
    // for the mesh shaders.
    let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None)? };

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_info)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0);

    // SAFETY: All referenced shader modules, pipeline layout, and render pass are live and owned
    // by the caller; Vulkan copies the provided create info before returning.
    let pipe = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            .map_err(|e| e.1)?[0]
    };

    // SAFETY: The pipeline has already been created and no longer borrows the temporary shader
    // modules, so they can be destroyed immediately on the same device.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(PipelinePair { layout, pipe })
}

fn create_textured_mesh_pipeline(
    device: &Device,
    render_pass: vk::RenderPass,
    set_layout: vk::DescriptorSetLayout,
    mode: BlendMode,
) -> Result<PipelinePair, Box<dyn Error>> {
    let vert_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_tmesh.vert.spv"));
    let frag_shader_code = include_bytes!(concat!(env!("OUT_DIR"), "/vulkan_tmesh.frag.spv"));
    let vert_module = create_shader_module(device, vert_shader_code)?;
    let frag_module = create_shader_module(device, frag_shader_code)?;
    let main_name = c"main";

    let shader_stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_module)
            .name(main_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_module)
            .name(main_name),
    ];

    let (binding_descriptions, attribute_descriptions) = vertex_input_descriptions_tmesh();
    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);

    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
        .rasterization_samples(vk::SampleCountFlags::TYPE_1);

    let color_blend_attachment = color_blend_for(mode);
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&color_blend_attachment));

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state =
        vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(std::mem::size_of::<ProjPush>() as u32);

    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(std::slice::from_ref(&set_layout))
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));

    // SAFETY: The descriptor set layout and push-constant range are valid for this pipeline, and
    // the create info borrows only stack data for the duration of the call.
    let layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None)? };

    let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_stages)
        .vertex_input_state(&vertex_input_info)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterizer)
        .multisample_state(&multisampling)
        .color_blend_state(&color_blending)
        .dynamic_state(&dynamic_state)
        .layout(layout)
        .render_pass(render_pass)
        .subpass(0);

    // SAFETY: All referenced shader modules, pipeline layout, and render pass are live and owned
    // by the caller; Vulkan copies the provided create info before returning.
    let pipe = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            .map_err(|e| e.1)?[0]
    };

    // SAFETY: The pipeline has already been created and no longer borrows the temporary shader
    // modules, so they can be destroyed immediately on the same device.
    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(PipelinePair { layout, pipe })
}

#[inline(always)]
const fn next_pow2_usize(x: usize) -> usize {
    let mut v = if x == 0 { 1 } else { x - 1 };
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    #[cfg(target_pointer_width = "64")]
    {
        v |= v >> 32;
    }
    v + 1
}

fn ensure_instance_ring_capacity(
    state: &mut State,
    needed_instances: usize,
) -> Result<u32, Box<dyn Error>> {
    // Request at least 1 instance, round to next power of two.
    let requested_stride = next_pow2_usize(needed_instances.max(1));

    // Grow-only policy: never shrink the ring to avoid frequent realloc + stalls.
    let stride = if state.per_frame_stride_instances == 0 {
        requested_stride
    } else {
        state.per_frame_stride_instances.max(requested_stride)
    };

    let need_total_instances = stride * MAX_FRAMES_IN_FLIGHT;
    let bytes_per_instance = std::mem::size_of::<InstanceData>() as vk::DeviceSize;
    let need_bytes = (need_total_instances as u64) * (bytes_per_instance as u64);

    let dev = state.device.as_ref().unwrap();

    // Reallocate only if missing or too small.
    if state.instance_ring.is_none() || state.instance_capacity_instances < need_total_instances {
        // SAFETY: The old ring buffer may still be referenced by in-flight command buffers.
        // Waiting for the device to go idle guarantees those submissions are complete before
        // unmapping or freeing the old allocation.
        if let Some(old) = state.instance_ring.take() {
            unsafe {
                // Full idle here is fine — this path runs only when we *grow*.
                dev.device_wait_idle()?;
                if !state.instance_ring_ptr.is_null() {
                    dev.unmap_memory(old.memory);
                }
            }
            destroy_buffer(dev, &old);
            state.instance_ring_ptr = std::ptr::null_mut();
        }

        // Create a new HOST_VISIBLE | HOST_COHERENT VB and keep it persistently mapped.
        let (buf, mem) = create_gpu_buffer(
            &state.instance,
            dev,
            state.pdevice,
            need_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        // SAFETY: `mem` was allocated from this device with HOST_VISIBLE memory and we keep the
        // mapping alive until the ring is explicitly unmapped during growth or shutdown.
        let mapped = unsafe { dev.map_memory(mem, 0, need_bytes, vk::MemoryMapFlags::empty())? };

        state.instance_ring = Some(BufferResource {
            buffer: buf,
            memory: mem,
        });
        state.instance_ring_ptr = mapped.cast::<InstanceData>();
        state.instance_capacity_instances = need_total_instances;
        state.per_frame_stride_instances = stride;
    } else if state.per_frame_stride_instances != stride {
        // We decided to grow-only; keep the bigger existing stride.
        state.per_frame_stride_instances = stride;
    }

    // Base "firstInstance" for this frame’s slice of the ring.
    Ok((state.current_frame * state.per_frame_stride_instances) as u32)
}

fn ensure_mesh_ring_capacity(
    state: &mut State,
    needed_vertices: usize,
) -> Result<u32, Box<dyn Error>> {
    let requested_stride = next_pow2_usize(needed_vertices.max(1));

    let stride = if state.per_frame_stride_vertices == 0 {
        requested_stride
    } else {
        state.per_frame_stride_vertices.max(requested_stride)
    };

    let need_total_vertices = stride * MAX_FRAMES_IN_FLIGHT;
    let bytes_per_vertex = std::mem::size_of::<MeshVertex>() as vk::DeviceSize;
    let need_bytes = (need_total_vertices as u64) * (bytes_per_vertex as u64);

    let dev = state.device.as_ref().unwrap();

    if state.mesh_ring.is_none() || state.mesh_capacity_vertices < need_total_vertices {
        if let Some(old) = state.mesh_ring.take() {
            // SAFETY: The old mesh ring may still be referenced by in-flight command buffers.
            // Waiting for idle guarantees those submissions are complete before unmapping/freeing.
            unsafe {
                dev.device_wait_idle()?;
                if !state.mesh_ring_ptr.is_null() {
                    dev.unmap_memory(old.memory);
                }
            }
            destroy_buffer(dev, &old);
            state.mesh_ring_ptr = std::ptr::null_mut();
        }

        let (buf, mem) = create_gpu_buffer(
            &state.instance,
            dev,
            state.pdevice,
            need_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        // SAFETY: `mem` was allocated from this device with HOST_VISIBLE memory and we keep the
        // mapping alive until the ring is explicitly unmapped during growth or shutdown.
        let mapped = unsafe { dev.map_memory(mem, 0, need_bytes, vk::MemoryMapFlags::empty())? };

        state.mesh_ring = Some(BufferResource {
            buffer: buf,
            memory: mem,
        });
        state.mesh_ring_ptr = mapped.cast::<MeshVertex>();
        state.mesh_capacity_vertices = need_total_vertices;
        state.per_frame_stride_vertices = stride;
    } else if state.per_frame_stride_vertices != stride {
        state.per_frame_stride_vertices = stride;
    }

    Ok((state.current_frame * state.per_frame_stride_vertices) as u32)
}

fn ensure_tmesh_ring_capacity(
    state: &mut State,
    needed_vertices: usize,
) -> Result<u32, Box<dyn Error>> {
    let requested_stride = next_pow2_usize(needed_vertices.max(1));

    let stride = if state.per_frame_stride_tmesh_vertices == 0 {
        requested_stride
    } else {
        state.per_frame_stride_tmesh_vertices.max(requested_stride)
    };

    let need_total_vertices = stride * MAX_FRAMES_IN_FLIGHT;
    let bytes_per_vertex = std::mem::size_of::<TexturedMeshVertexGpu>() as vk::DeviceSize;
    let need_bytes = (need_total_vertices as u64) * (bytes_per_vertex as u64);

    let dev = state.device.as_ref().unwrap();

    if state.tmesh_ring.is_none() || state.tmesh_capacity_vertices < need_total_vertices {
        if let Some(old) = state.tmesh_ring.take() {
            // SAFETY: The old textured-mesh ring may still be referenced by in-flight command
            // buffers. Waiting for idle guarantees those submissions are complete first.
            unsafe {
                dev.device_wait_idle()?;
                if !state.tmesh_ring_ptr.is_null() {
                    dev.unmap_memory(old.memory);
                }
            }
            destroy_buffer(dev, &old);
            state.tmesh_ring_ptr = std::ptr::null_mut();
        }

        let (buf, mem) = create_gpu_buffer(
            &state.instance,
            dev,
            state.pdevice,
            need_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        // SAFETY: `mem` was allocated from this device with HOST_VISIBLE memory and we keep the
        // mapping alive until the ring is explicitly unmapped during growth or shutdown.
        let mapped = unsafe { dev.map_memory(mem, 0, need_bytes, vk::MemoryMapFlags::empty())? };

        state.tmesh_ring = Some(BufferResource {
            buffer: buf,
            memory: mem,
        });
        state.tmesh_ring_ptr = mapped.cast::<TexturedMeshVertexGpu>();
        state.tmesh_capacity_vertices = need_total_vertices;
        state.per_frame_stride_tmesh_vertices = stride;
    } else if state.per_frame_stride_tmesh_vertices != stride {
        state.per_frame_stride_tmesh_vertices = stride;
    }

    Ok((state.current_frame * state.per_frame_stride_tmesh_vertices) as u32)
}

fn ensure_tmesh_instance_ring_capacity(
    state: &mut State,
    needed_instances: usize,
) -> Result<u32, Box<dyn Error>> {
    let requested_stride = next_pow2_usize(needed_instances.max(1));

    let stride = if state.per_frame_stride_tmesh_instances == 0 {
        requested_stride
    } else {
        state.per_frame_stride_tmesh_instances.max(requested_stride)
    };

    let need_total_instances = stride * MAX_FRAMES_IN_FLIGHT;
    let bytes_per_instance = std::mem::size_of::<TexturedMeshInstanceGpu>() as vk::DeviceSize;
    let need_bytes = (need_total_instances as u64) * (bytes_per_instance as u64);

    let dev = state.device.as_ref().unwrap();

    if state.tmesh_instance_ring.is_none() || state.tmesh_capacity_instances < need_total_instances
    {
        if let Some(old) = state.tmesh_instance_ring.take() {
            // SAFETY: The old textured-mesh instance ring may still be referenced by in-flight
            // command buffers. Waiting for idle guarantees those submissions are complete first.
            unsafe {
                dev.device_wait_idle()?;
                if !state.tmesh_instance_ring_ptr.is_null() {
                    dev.unmap_memory(old.memory);
                }
            }
            destroy_buffer(dev, &old);
            state.tmesh_instance_ring_ptr = std::ptr::null_mut();
        }

        let (buf, mem) = create_gpu_buffer(
            &state.instance,
            dev,
            state.pdevice,
            need_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        // SAFETY: `mem` was allocated from this device with HOST_VISIBLE memory and we keep the
        // mapping alive until the ring is explicitly unmapped during growth or shutdown.
        let mapped = unsafe { dev.map_memory(mem, 0, need_bytes, vk::MemoryMapFlags::empty())? };

        state.tmesh_instance_ring = Some(BufferResource {
            buffer: buf,
            memory: mem,
        });
        state.tmesh_instance_ring_ptr = mapped.cast::<TexturedMeshInstanceGpu>();
        state.tmesh_capacity_instances = need_total_instances;
        state.per_frame_stride_tmesh_instances = stride;
    } else if state.per_frame_stride_tmesh_instances != stride {
        state.per_frame_stride_tmesh_instances = stride;
    }

    Ok((state.current_frame * state.per_frame_stride_tmesh_instances) as u32)
}

fn transition_image_layout_cmd(
    device: &Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) {
    let (src_access_mask, dst_access_mask, src_stage, dst_stage) = match (old_layout, new_layout) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::TRANSFER,
        ),
        (vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
            vk::AccessFlags::SHADER_READ,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::TRANSFER,
        ),
        (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::SHADER_READ,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
        ),
        _ => panic!("Unsupported layout transition!"),
    };

    let barrier = vk::ImageMemoryBarrier::default()
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1),
        )
        .src_access_mask(src_access_mask)
        .dst_access_mask(dst_access_mask);

    // SAFETY: `cmd` is currently recording on the same queue family that owns `image`, and the
    // barrier references only stack data alive for the duration of the call.
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[barrier],
        );
    }
}

fn begin_pending_texture_upload_cmd(
    state: &mut State,
) -> Result<vk::CommandBuffer, Box<dyn Error>> {
    if let Some(cmd) = state.pending_tex_upload_cmd {
        return Ok(cmd);
    }

    let device = state.device.as_ref().unwrap();
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(state.command_pool)
        .command_buffer_count(1);
    // SAFETY: `state.command_pool` belongs to this device and remains alive until the command
    // buffer is ended, submitted, and later retired from `submitted_tex_uploads`.
    let cmd = unsafe { device.allocate_command_buffers(&alloc_info)?[0] };
    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: `cmd` was just allocated from `state.command_pool` and has not been begun yet.
    unsafe {
        device.begin_command_buffer(cmd, &begin_info)?;
    }
    state.pending_tex_upload_cmd = Some(cmd);
    Ok(cmd)
}

fn retire_submitted_texture_uploads(state: &mut State, frame: usize) {
    let device = state.device.as_ref().unwrap();
    let mut keep = Vec::with_capacity(state.submitted_tex_uploads.len());
    for batch in mem::take(&mut state.submitted_tex_uploads) {
        if batch.frame != frame {
            keep.push(batch);
            continue;
        }
        unsafe {
            device.free_command_buffers(state.command_pool, &[batch.cmd]);
        }
        for staging in batch.staging {
            destroy_buffer(device, &staging);
        }
    }
    state.submitted_tex_uploads = keep;
}

fn retire_all_submitted_texture_uploads(state: &mut State) {
    let device = state.device.as_ref().unwrap();
    for batch in mem::take(&mut state.submitted_tex_uploads) {
        unsafe {
            device.free_command_buffers(state.command_pool, &[batch.cmd]);
        }
        for staging in batch.staging {
            destroy_buffer(device, &staging);
        }
    }
}

fn submit_pending_texture_uploads(state: &mut State, frame: usize) -> Result<(), Box<dyn Error>> {
    let Some(cmd) = state.pending_tex_upload_cmd.take() else {
        return Ok(());
    };

    let device = state.device.as_ref().unwrap();
    let staging = mem::take(&mut state.pending_tex_staging);
    // SAFETY: `cmd` is a primary command buffer allocated from `state.command_pool`, the upload
    // queue is the same graphics queue used for frame submissions, and the staged buffers are kept
    // alive in `submitted_tex_uploads` until the tagged frame slot is known complete.
    unsafe {
        device.end_command_buffer(cmd)?;
        let submit = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd));
        device.queue_submit(state.queue, &[submit], vk::Fence::null())?;
    }
    state.submitted_tex_uploads.push(SubmittedTextureUpload {
        frame,
        cmd,
        staging,
    });

    Ok(())
}

pub fn flush_pending_uploads(state: &mut State) -> Result<(), Box<dyn Error>> {
    submit_pending_texture_uploads(state, state.current_frame)
}

pub fn retire_submitted_uploads(state: &mut State) {
    retire_all_submitted_texture_uploads(state)
}

#[inline(always)]
fn completed_present_id(state: &State) -> u32 {
    state
        .present_telemetry
        .last_completed
        .map_or(0, |timing| timing.present_id)
}

fn retire_completed_textures(state: &mut State) {
    let completed = completed_present_id(state);
    if completed == 0 {
        return;
    }

    let mut keep = Vec::with_capacity(state.retired_textures.len());
    for retired in mem::take(&mut state.retired_textures) {
        if retired.retire_after_present_id != 0 && retired.retire_after_present_id > completed {
            keep.push(retired);
        }
    }
    state.retired_textures = keep;
}

pub fn retire_textures(state: &mut State, textures: Vec<Texture>) {
    if textures.is_empty() {
        return;
    }

    let retire_after_present_id = state.last_submitted_present_id;
    let completed = completed_present_id(state);
    if retire_after_present_id == 0 || retire_after_present_id <= completed {
        drop(textures);
        return;
    }

    state
        .retired_textures
        .extend(textures.into_iter().map(|texture| RetiredTexture {
            retire_after_present_id,
            _texture: texture,
        }));
}

pub fn retire_all_textures(state: &mut State) {
    state.retired_textures.clear();
}

pub fn create_texture(
    state: &mut State,
    image: &RgbaImage,
    sampler: SamplerDesc,
) -> Result<Texture, Box<dyn Error>> {
    let device_arc = state.device.as_ref().unwrap().clone();
    let device = device_arc.as_ref();

    let (width, height) = image.dimensions();
    let image_data = image.as_raw();
    let staging_size = image_data.len() as vk::DeviceSize;
    let (staging_buffer, staging_memory) = create_gpu_buffer(
        &state.instance,
        device,
        state.pdevice,
        staging_size,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    // SAFETY: The staging allocation is HOST_VISIBLE | HOST_COHERENT, `image_data` points to
    // initialized RGBA bytes, and we unmap the range before handing the buffer to Vulkan.
    unsafe {
        let mapped =
            device.map_memory(staging_memory, 0, staging_size, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(image_data.as_ptr(), mapped.cast::<u8>(), image_data.len());
        device.unmap_memory(staging_memory);
    }
    let staging = BufferResource {
        buffer: staging_buffer,
        memory: staging_memory,
    };

    let fmt = vk::Format::R8G8B8A8_UNORM;
    let (tex_image, tex_mem) = create_image(
        state,
        width,
        height,
        fmt,
        vk::ImageTiling::OPTIMAL,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    let cmd = begin_pending_texture_upload_cmd(state)?;

    transition_image_layout_cmd(
        device,
        cmd,
        tex_image,
        vk::ImageLayout::UNDEFINED,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    );

    let region = vk::BufferImageCopy::default()
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });

    // SAFETY: `cmd` is recording, `staging.buffer` and `tex_image` are live transfer-compatible
    // resources, and the image has already been transitioned to TRANSFER_DST_OPTIMAL above.
    unsafe {
        device.cmd_copy_buffer_to_image(
            cmd,
            staging.buffer,
            tex_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
    }

    transition_image_layout_cmd(
        device,
        cmd,
        tex_image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    );

    state.pending_tex_staging.push(staging);
    let view = create_image_view(device, tex_image, fmt)?;
    let sampler_default = get_sampler(state, sampler)?;
    let sampler_repeat = get_sampler(
        state,
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..sampler
        },
    )?;
    let (set, set_repeat, pool) =
        create_texture_descriptor_sets(state, view, sampler_default, sampler_repeat)?;

    Ok(Texture {
        device: device_arc.clone(),
        image: tex_image,
        memory: tex_mem,
        view,
        descriptor_set: set,
        descriptor_set_repeat: set_repeat,
        pool,
    })
}

pub fn update_texture(
    state: &mut State,
    texture: &mut Texture,
    image: &RgbaImage,
) -> Result<(), Box<dyn Error>> {
    let device = texture.device.as_ref();
    let (width, height) = image.dimensions();
    let image_data = image.as_raw();
    let staging_size = image_data.len() as vk::DeviceSize;
    let (staging_buffer, staging_memory) = create_gpu_buffer(
        &state.instance,
        device,
        state.pdevice,
        staging_size,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    // SAFETY: The staging allocation is HOST_VISIBLE | HOST_COHERENT, `image_data` points to
    // initialized RGBA bytes, and we unmap the range before handing the buffer to Vulkan.
    unsafe {
        let mapped =
            device.map_memory(staging_memory, 0, staging_size, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(image_data.as_ptr(), mapped.cast::<u8>(), image_data.len());
        device.unmap_memory(staging_memory);
    }
    state.pending_tex_staging.push(BufferResource {
        buffer: staging_buffer,
        memory: staging_memory,
    });

    let cmd = begin_pending_texture_upload_cmd(state)?;
    transition_image_layout_cmd(
        device,
        cmd,
        texture.image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
    );
    let region = vk::BufferImageCopy::default()
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        });
    // SAFETY: `cmd` is recording, `staging_buffer` and `texture.image` are live transfer-
    // compatible resources, and the image has already been transitioned to TRANSFER_DST_OPTIMAL.
    unsafe {
        device.cmd_copy_buffer_to_image(
            cmd,
            staging_buffer,
            texture.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[region],
        );
    }
    transition_image_layout_cmd(
        device,
        cmd,
        texture.image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
    );
    Ok(())
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

    let mut stats = DrawStats::default();

    if !state.swapchain_valid || state.window_size.width == 0 || state.window_size.height == 0 {
        return Ok(stats);
    }
    stats.present_stats.mode = vk_present_mode_trace(state.swapchain_resources.present_mode);
    stats.present_stats.refresh_ns = state.present_telemetry.refresh_ns;

    {
        let prep = &mut state.prep;
        let instance = &state.instance;
        let device = Arc::clone(state.device.as_ref().unwrap());
        let pdevice = state.pdevice;
        let cached_tmesh = &mut state.cached_tmesh;
        let cached_tmesh_bytes = &mut state.cached_tmesh_bytes;
        let _prep_stats =
            draw_prep::prepare(
                render_list,
                prep,
                |cache_key, vertices| match ensure_cached_tmesh(
                    instance,
                    device.as_ref(),
                    pdevice,
                    cached_tmesh,
                    cached_tmesh_bytes,
                    cache_key,
                    vertices,
                ) {
                    Ok(cached) => cached,
                    Err(e) => {
                        warn!("Failed to cache Vulkan textured mesh {cache_key:#x}: {e}");
                        false
                    }
                },
            );
    }

    let needed_instances = state.prep.sprite_instances.len();
    let needed_mesh_vertices = state.prep.mesh_vertices.len();
    let needed_tmesh_vertices = state.prep.tmesh_vertices.len();
    let needed_tmesh_instances = state.prep.tmesh_instances.len();

    let base_first_instance = if needed_instances > 0 {
        Some(ensure_instance_ring_capacity(state, needed_instances)?)
    } else {
        None
    };
    let base_first_vertex = if needed_mesh_vertices > 0 {
        Some(ensure_mesh_ring_capacity(state, needed_mesh_vertices)?)
    } else {
        None
    };
    let base_first_tmesh_vertex = if needed_tmesh_vertices > 0 {
        Some(ensure_tmesh_ring_capacity(state, needed_tmesh_vertices)?)
    } else {
        None
    };
    let base_first_tmesh_instance = if needed_tmesh_instances > 0 {
        Some(ensure_tmesh_instance_ring_capacity(
            state,
            needed_tmesh_instances,
        )?)
    } else {
        None
    };

    // SAFETY: We wait on the current frame fence before reusing its command buffer or writing into
    // this frame's ring-buffer slice, so the GPU is done reading prior submissions. All Vulkan
    // handles referenced below are owned by `state` and remain alive through submission/present,
    // and any screenshot staging buffer is kept alive until we wait for the queue and unmap/free it.
    unsafe {
        let mut waited_for_image = false;
        let mut back_pressure_waited = false;
        let mut queue_idle_waited = false;
        let fence = state.in_flight_fences[state.current_frame];
        let device = Arc::clone(state.device.as_ref().unwrap());
        let wait_started = Instant::now();
        device.wait_for_fences(&[fence], true, u64::MAX)?;
        stats.gpu_wait_us = stats
            .gpu_wait_us
            .saturating_add(elapsed_us_since(wait_started));
        retire_submitted_texture_uploads(state, state.current_frame);
        submit_pending_texture_uploads(state, state.current_frame)?;

        let acquire_started = Instant::now();
        let (image_index, acquired_suboptimal) = match state
            .swapchain_resources
            .swapchain_loader
            .acquire_next_image(
                state.swapchain_resources.swapchain,
                u64::MAX,
                state.image_available_semaphores[state.current_frame],
                vk::Fence::null(),
            ) {
            Ok(pair) => pair,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                stats.acquire_us = elapsed_us_since(acquire_started);
                recreate_swapchain_and_dependents(state)?;
                return Ok(stats);
            }
            Err(e) => return Err(e.into()),
        };
        stats.acquire_us = elapsed_us_since(acquire_started);
        record_cpu_present_completion(state, image_index);
        retire_completed_textures(state);

        let in_flight = state.images_in_flight[image_index as usize];
        if in_flight != vk::Fence::null() {
            let wait_started = Instant::now();
            device.wait_for_fences(&[in_flight], true, u64::MAX)?;
            let wait_us = elapsed_us_since(wait_started);
            stats.gpu_wait_us = stats.gpu_wait_us.saturating_add(wait_us);
            waited_for_image = wait_us >= VULKAN_IMAGE_WAIT_THRESHOLD_US;
        }
        state.images_in_flight[image_index as usize] = fence;

        let backend_setup_started = Instant::now();
        device.reset_fences(&[fence])?;
        let cmd = state.command_buffers[state.current_frame];
        device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        stats.backend_setup_us = elapsed_us_since(backend_setup_started);

        let backend_prepare_started = Instant::now();
        let inst_base_ptr = base_first_instance.map_or(std::ptr::null_mut(), |b| {
            state.instance_ring_ptr.add(b as usize)
        });
        let mesh_base_ptr = base_first_vertex.map_or(std::ptr::null_mut(), |b| {
            state.mesh_ring_ptr.add(b as usize)
        });
        let tmesh_base_ptr = base_first_tmesh_vertex.map_or(std::ptr::null_mut(), |b| {
            state.tmesh_ring_ptr.add(b as usize)
        });
        let tmesh_instance_base_ptr = base_first_tmesh_instance.map_or(std::ptr::null_mut(), |b| {
            state.tmesh_instance_ring_ptr.add(b as usize)
        });
        if needed_instances > 0 {
            debug_assert!(!inst_base_ptr.is_null(), "instance ring missing");
            std::ptr::copy_nonoverlapping(
                state.prep.sprite_instances.as_ptr(),
                inst_base_ptr,
                needed_instances,
            );
        }
        if needed_mesh_vertices > 0 {
            debug_assert!(!mesh_base_ptr.is_null(), "mesh ring missing");
            std::ptr::copy_nonoverlapping(
                state.prep.mesh_vertices.as_ptr(),
                mesh_base_ptr,
                needed_mesh_vertices,
            );
        }
        if needed_tmesh_vertices > 0 {
            debug_assert!(!tmesh_base_ptr.is_null(), "textured mesh ring missing");
            std::ptr::copy_nonoverlapping(
                state.prep.tmesh_vertices.as_ptr(),
                tmesh_base_ptr,
                needed_tmesh_vertices,
            );
        }
        if needed_tmesh_instances > 0 {
            debug_assert!(
                !tmesh_instance_base_ptr.is_null(),
                "textured mesh instance ring missing"
            );
            std::ptr::copy_nonoverlapping(
                state.prep.tmesh_instances.as_ptr(),
                tmesh_instance_base_ptr,
                needed_tmesh_instances,
            );
        }
        stats.backend_prepare_us = elapsed_us_since(backend_prepare_started);

        let backend_record_started = Instant::now();
        let c = render_list.clear_color;
        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [c[0], c[1], c[2], c[3]],
            },
        };
        let rp_info = vk::RenderPassBeginInfo::default()
            .render_pass(state.render_pass)
            .framebuffer(state.swapchain_resources.framebuffers[image_index as usize])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: state.swapchain_resources.extent,
            })
            .clear_values(std::slice::from_ref(&clear_value));
        device.cmd_begin_render_pass(cmd, &rp_info, vk::SubpassContents::INLINE);

        let vp = vk::Viewport {
            x: 0.0,
            y: state.swapchain_resources.extent.height as f32,
            width: state.swapchain_resources.extent.width as f32,
            height: -(state.swapchain_resources.extent.height as f32),
            min_depth: 0.0,
            max_depth: 1.0,
        };
        device.cmd_set_viewport(cmd, 0, &[vp]);
        let sc = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: state.swapchain_resources.extent,
        };
        device.cmd_set_scissor(cmd, 0, &[sc]);

        enum Bound {
            None,
            Sprite,
            Mesh,
            TexturedMesh,
        }
        let mut bound = Bound::None;
        let mut last_set = vk::DescriptorSet::null();
        let mut last_camera: Option<u8> = None;
        let mut last_tmesh_source: Option<TexturedMeshSource> = None;
        let mut vertices_drawn: u32 = 0;
        for op in &state.prep.ops {
            match op {
                DrawOp::Sprite(run) => {
                    let set_opt = textures.get(&run.texture_handle).and_then(|t| {
                        if let RendererTexture::Vulkan(tex) = t {
                            Some(tex.descriptor_set)
                        } else {
                            None
                        }
                    });
                    let Some(set) = set_opt else {
                        continue;
                    };
                    if !matches!(bound, Bound::Sprite) {
                        device.cmd_bind_pipeline(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            state.sprite_pipeline,
                        );
                        let vb0 = state.vertex_buffer.as_ref().unwrap().buffer;
                        let inst_buf = state.instance_ring.as_ref().unwrap().buffer;
                        device.cmd_bind_vertex_buffers(cmd, 0, &[vb0, inst_buf], &[0, 0]);
                        let ib = state.index_buffer.as_ref().unwrap().buffer;
                        device.cmd_bind_index_buffer(cmd, ib, 0, vk::IndexType::UINT16);
                        bound = Bound::Sprite;
                        last_set = vk::DescriptorSet::null();
                        last_camera = None;
                        last_tmesh_source = None;
                    }

                    if last_camera != Some(run.camera) {
                        let vp = render_list
                            .cameras
                            .get(run.camera as usize)
                            .copied()
                            .unwrap_or(state.projection);
                        let pc = ProjPush {
                            proj: vp.to_cols_array_2d(),
                        };
                        device.cmd_push_constants(
                            cmd,
                            state.sprite_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            bytemuck::bytes_of(&pc),
                        );
                        last_camera = Some(run.camera);
                    }

                    if last_set != set {
                        device.cmd_bind_descriptor_sets(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            state.sprite_pipeline_layout,
                            0,
                            &[set],
                            &[],
                        );
                        last_set = set;
                    }

                    let first_instance = base_first_instance.unwrap_or(0) + run.instance_start;
                    device.cmd_draw_indexed(cmd, 6, run.instance_count, 0, 0, first_instance);
                    vertices_drawn = vertices_drawn.saturating_add(4 * run.instance_count);
                }
                DrawOp::Mesh(draw) => {
                    if !matches!(bound, Bound::Mesh) {
                        device.cmd_bind_pipeline(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            state.mesh_pipeline,
                        );
                        let vb = state.mesh_ring.as_ref().unwrap().buffer;
                        device.cmd_bind_vertex_buffers(cmd, 0, &[vb], &[0]);
                        bound = Bound::Mesh;
                        last_camera = None;
                        last_tmesh_source = None;
                    }

                    if last_camera != Some(draw.camera) {
                        let vp = render_list
                            .cameras
                            .get(draw.camera as usize)
                            .copied()
                            .unwrap_or(state.projection);
                        let pc = ProjPush {
                            proj: vp.to_cols_array_2d(),
                        };
                        device.cmd_push_constants(
                            cmd,
                            state.mesh_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            bytemuck::bytes_of(&pc),
                        );
                        last_camera = Some(draw.camera);
                    }

                    let first_vertex = base_first_vertex.unwrap_or(0) + draw.vertex_start;
                    device.cmd_draw(cmd, draw.vertex_count, 1, first_vertex, 0);
                    vertices_drawn = vertices_drawn.saturating_add(draw.vertex_count);
                }
                DrawOp::TexturedMesh(draw) => {
                    let set_opt = textures.get(&draw.texture_handle).and_then(|t| {
                        if let RendererTexture::Vulkan(tex) = t {
                            Some(tex.descriptor_set_repeat)
                        } else {
                            None
                        }
                    });
                    let Some(set) = set_opt else {
                        continue;
                    };
                    if !matches!(bound, Bound::TexturedMesh) {
                        device.cmd_bind_pipeline(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            state.textured_mesh_pipeline,
                        );
                        let inst = state.tmesh_instance_ring.as_ref().unwrap().buffer;
                        device.cmd_bind_vertex_buffers(cmd, 1, &[inst], &[0]);
                        bound = Bound::TexturedMesh;
                        last_set = vk::DescriptorSet::null();
                        last_camera = None;
                        last_tmesh_source = None;
                    }

                    if last_tmesh_source != Some(draw.source) {
                        let vb = match draw.source {
                            TexturedMeshSource::Transient { .. } => {
                                let Some(vb) = state.tmesh_ring.as_ref().map(|ring| ring.buffer)
                                else {
                                    continue;
                                };
                                vb
                            }
                            TexturedMeshSource::Cached { cache_key, .. } => {
                                let Some(entry) = state.cached_tmesh.get(&cache_key) else {
                                    continue;
                                };
                                entry.buffer.buffer
                            }
                        };
                        device.cmd_bind_vertex_buffers(cmd, 0, &[vb], &[0]);
                        last_tmesh_source = Some(draw.source);
                    }

                    if last_camera != Some(draw.camera) {
                        let vp = render_list
                            .cameras
                            .get(draw.camera as usize)
                            .copied()
                            .unwrap_or(state.projection);
                        let pc = ProjPush {
                            proj: vp.to_cols_array_2d(),
                        };
                        device.cmd_push_constants(
                            cmd,
                            state.textured_mesh_pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            bytemuck::bytes_of(&pc),
                        );
                        last_camera = Some(draw.camera);
                    }

                    if last_set != set {
                        device.cmd_bind_descriptor_sets(
                            cmd,
                            vk::PipelineBindPoint::GRAPHICS,
                            state.textured_mesh_pipeline_layout,
                            0,
                            &[set],
                            &[],
                        );
                        last_set = set;
                    }

                    let first_vertex = match draw.source {
                        TexturedMeshSource::Transient { vertex_start, .. } => {
                            base_first_tmesh_vertex.unwrap_or(0) + vertex_start
                        }
                        TexturedMeshSource::Cached { .. } => 0,
                    };
                    let first_instance =
                        base_first_tmesh_instance.unwrap_or(0) + draw.instance_start;
                    device.cmd_draw(
                        cmd,
                        draw.source.vertex_count(),
                        draw.instance_count,
                        first_vertex,
                        first_instance,
                    );
                    let tri_count = draw.source.vertex_count() / 3;
                    vertices_drawn = vertices_drawn
                        .saturating_add(tri_count.saturating_mul(draw.instance_count));
                }
            }
        }

        device.cmd_end_render_pass(cmd);
        let screenshot_staging = if state.screenshot_requested {
            state.screenshot_requested = false;
            state.captured_frame = None;
            if state.swapchain_resources.supports_transfer_src {
                let width = state.swapchain_resources.extent.width;
                let height = state.swapchain_resources.extent.height;
                let bytes_per_row = width as usize * 4;
                let copy_size = (bytes_per_row * height as usize) as vk::DeviceSize;
                let (staging_buffer, staging_memory) = create_gpu_buffer(
                    &state.instance,
                    &device,
                    state.pdevice,
                    copy_size,
                    vk::BufferUsageFlags::TRANSFER_DST,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )?;
                let swap_image = state.swapchain_resources._images[image_index as usize];
                let subresource = vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1);
                let to_transfer = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(swap_image)
                    .subresource_range(subresource);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&to_transfer),
                );

                let copy_region = vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .buffer_row_length(0)
                    .buffer_image_height(0)
                    .image_subresource(vk::ImageSubresourceLayers {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        mip_level: 0,
                        base_array_layer: 0,
                        layer_count: 1,
                    })
                    .image_extent(vk::Extent3D {
                        width,
                        height,
                        depth: 1,
                    });
                device.cmd_copy_image_to_buffer(
                    cmd,
                    swap_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    staging_buffer,
                    std::slice::from_ref(&copy_region),
                );

                let to_present = vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .image(swap_image)
                    .subresource_range(subresource);
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&to_present),
                );

                Some((
                    BufferResource {
                        buffer: staging_buffer,
                        memory: staging_memory,
                    },
                    width,
                    height,
                    state.swapchain_resources.format.format,
                ))
            } else {
                warn!(
                    "Vulkan swapchain does not support transfer-src usage; screenshot unavailable"
                );
                None
            }
        } else {
            None
        };
        device.end_command_buffer(cmd)?;
        stats.backend_record_us = elapsed_us_since(backend_record_started);

        let wait = [state.image_available_semaphores[state.current_frame]];
        let sig = [state.render_finished_semaphores[state.current_frame]];
        let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let submit = vk::SubmitInfo::default()
            .wait_semaphores(&wait)
            .wait_dst_stage_mask(&stages)
            .command_buffers(std::slice::from_ref(&cmd))
            .signal_semaphores(&sig);
        let submit_started = Instant::now();
        device.queue_submit(state.queue, &[submit], fence)?;
        stats.submit_us = elapsed_us_since(submit_started);

        let submitted_present_id = next_present_id(state);
        let present_times = [vk::PresentTimeGOOGLE::default().present_id(submitted_present_id)];
        let mut present_times_info = vk::PresentTimesInfoGOOGLE::default().times(&present_times);
        let mut present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&sig)
            .swapchains(std::slice::from_ref(&state.swapchain_resources.swapchain))
            .image_indices(std::slice::from_ref(&image_index));
        if state.present_telemetry.display_timing.is_some() {
            present_info = present_info.push_next(&mut present_times_info);
        }
        let present_started = Instant::now();
        let present_result = state
            .swapchain_resources
            .swapchain_loader
            .queue_present(state.queue, &present_info);
        stats.present_us = elapsed_us_since(present_started);
        let present_suboptimal = match present_result {
            Ok(suboptimal) => suboptimal || acquired_suboptimal,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => true,
            Err(e) => return Err(e.into()),
        };
        if let Some(slot) = state
            .present_telemetry
            .cpu_image_present_ids
            .get_mut(image_index as usize)
        {
            *slot = submitted_present_id;
        }
        calibrate_present_clock(state);
        poll_past_presentation_timing(state);
        state.last_submitted_present_id = submitted_present_id;
        retire_completed_textures(state);
        stats.present_stats = snapshot_present_stats(
            state,
            waited_for_image,
            false,
            false,
            present_suboptimal,
            submitted_present_id,
        );
        if present_suboptimal {
            recreate_swapchain_and_dependents(state)?;
        }
        if apply_present_back_pressure && screenshot_staging.is_none() {
            // Match the wgpu Vulkan pacing path: when the app is running
            // uncapped, wait for this frame's GPU work to retire so the CPU
            // cannot build a long queue of stale Mailbox presents.
            let wait_started = Instant::now();
            device.wait_for_fences(&[fence], true, u64::MAX)?;
            let wait_us = elapsed_us_since(wait_started);
            stats.gpu_wait_us = stats.gpu_wait_us.saturating_add(wait_us);
            back_pressure_waited = wait_us >= VULKAN_BACK_PRESSURE_THRESHOLD_US;
        }
        if let Some((staging, width, height, format)) = screenshot_staging {
            let wait_started = Instant::now();
            device.queue_wait_idle(state.queue)?;
            stats.gpu_wait_us = stats
                .gpu_wait_us
                .saturating_add(elapsed_us_since(wait_started));
            queue_idle_waited = true;
            let map_size = (width as usize * height as usize * 4) as vk::DeviceSize;
            let mapped =
                match device.map_memory(staging.memory, 0, map_size, vk::MemoryMapFlags::empty()) {
                    Ok(ptr) => ptr,
                    Err(e) => {
                        destroy_buffer(&device, &staging);
                        return Err(e.into());
                    }
                };
            let src = std::slice::from_raw_parts(mapped.cast::<u8>(), map_size as usize);
            let row_bytes = width as usize * 4;
            let mut rgba = vec![0u8; row_bytes * height as usize];
            let swap_rb = matches!(
                format,
                vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB
            );
            for y in 0..height as usize {
                let src_row = y * row_bytes;
                // Vulkan image copy rows are already top-to-bottom for this swapchain path.
                let dst_row = y * row_bytes;
                if swap_rb {
                    let mut x = 0usize;
                    while x < width as usize {
                        let s = src_row + x * 4;
                        let d = dst_row + x * 4;
                        rgba[d] = src[s + 2];
                        rgba[d + 1] = src[s + 1];
                        rgba[d + 2] = src[s];
                        rgba[d + 3] = src[s + 3];
                        x += 1;
                    }
                } else {
                    rgba[dst_row..dst_row + row_bytes]
                        .copy_from_slice(&src[src_row..src_row + row_bytes]);
                }
            }
            device.unmap_memory(staging.memory);
            destroy_buffer(&device, &staging);
            state.captured_frame = RgbaImage::from_raw(width, height, rgba);
        }

        stats.present_stats.applied_back_pressure = back_pressure_waited;
        stats.present_stats.queue_idle_waited = queue_idle_waited;
        state.current_frame = (state.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        stats.vertices = vertices_drawn;
        Ok(stats)
    }
}

pub fn cleanup(state: &mut State) {
    info!("Cleaning up Vulkan resources...");
    if let Err(e) = submit_pending_texture_uploads(state, state.current_frame) {
        error!("Failed to submit pending texture uploads during cleanup: {e}");
    }
    // SAFETY: If a logical device still exists, waiting for idle guarantees no in-flight work
    // still references resources we are about to destroy below.
    unsafe {
        if let Some(device) = &state.device {
            let _ = device.device_wait_idle();
        }
    }
    retire_all_submitted_texture_uploads(state);
    retire_all_textures(state);

    // SAFETY: The device is idle, so it is valid to tear down swapchain resources, mapped rings,
    // pipelines, descriptor pools/layouts, the device, and finally the instance-owned objects.
    unsafe {
        cleanup_swapchain_and_dependents(state);

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_semaphore(state.render_finished_semaphores[i], None);
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_semaphore(state.image_available_semaphores[i], None);
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_fence(state.in_flight_fences[i], None);
        }

        if let Some(buffer) = state.vertex_buffer.take() {
            destroy_buffer(state.device.as_ref().unwrap(), &buffer);
        }
        if let Some(buffer) = state.index_buffer.take() {
            destroy_buffer(state.device.as_ref().unwrap(), &buffer);
        }

        // Persistently-mapped ring buffer
        if let Some(ring) = state.instance_ring.take() {
            if !state.instance_ring_ptr.is_null() {
                state.device.as_ref().unwrap().unmap_memory(ring.memory);
                state.instance_ring_ptr = std::ptr::null_mut();
            }
            destroy_buffer(state.device.as_ref().unwrap(), &ring);
        }

        if let Some(ring) = state.mesh_ring.take() {
            if !state.mesh_ring_ptr.is_null() {
                state.device.as_ref().unwrap().unmap_memory(ring.memory);
                state.mesh_ring_ptr = std::ptr::null_mut();
            }
            destroy_buffer(state.device.as_ref().unwrap(), &ring);
        }

        if let Some(ring) = state.tmesh_ring.take() {
            if !state.tmesh_ring_ptr.is_null() {
                state.device.as_ref().unwrap().unmap_memory(ring.memory);
                state.tmesh_ring_ptr = std::ptr::null_mut();
            }
            destroy_buffer(state.device.as_ref().unwrap(), &ring);
        }

        if let Some(ring) = state.tmesh_instance_ring.take() {
            if !state.tmesh_instance_ring_ptr.is_null() {
                state.device.as_ref().unwrap().unmap_memory(ring.memory);
                state.tmesh_instance_ring_ptr = std::ptr::null_mut();
            }
            destroy_buffer(state.device.as_ref().unwrap(), &ring);
        }
        for geom in state.cached_tmesh.drain().map(|(_, geom)| geom) {
            destroy_buffer(state.device.as_ref().unwrap(), &geom.buffer);
        }
        state.cached_tmesh_bytes = 0;
        for sampler in state.sampler_cache.values() {
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_sampler(*sampler, None);
        }
        state.sampler_cache.clear();
        for pool in state.descriptor_pools.drain(..) {
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_descriptor_pool(pool, None);
        }
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_descriptor_set_layout(state.descriptor_set_layout, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_pipeline(state.sprite_pipeline, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_pipeline_layout(state.sprite_pipeline_layout, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_pipeline(state.mesh_pipeline, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_pipeline_layout(state.mesh_pipeline_layout, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_pipeline(state.textured_mesh_pipeline, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_pipeline_layout(state.textured_mesh_pipeline_layout, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_render_pass(state.render_pass, None);
        state
            .device
            .as_ref()
            .unwrap()
            .destroy_command_pool(state.command_pool, None);
        state.surface_loader.destroy_surface(state.surface, None);

        if let (Some(loader), Some(messenger)) =
            (state.debug_loader.take(), state.debug_messenger.take())
        {
            loader.destroy_debug_utils_messenger(messenger, None);
        }

        if let Some(device_arc) = state.device.take() {
            device_arc.destroy_device(None);
        }

        state.instance.destroy_instance(None);
    }
    info!("Vulkan resources cleaned up.");
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    debug!("Vulkan resize requested to {width}x{height}");
    state.window_size = PhysicalSize::new(width, height);
    if width > 0 && height > 0 {
        state.projection = ortho_for_window(width, height);
        if let Err(e) = recreate_swapchain_and_dependents(state) {
            error!("Failed to recreate swapchain: {e}");
        }
    }
}

// --- ALL HELPER FUNCTIONS ---

fn create_image_view(
    device: &Device,
    image: vk::Image,
    format: vk::Format,
) -> Result<vk::ImageView, vk::Result> {
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });
    // SAFETY: `image` is a live image created on `device`, and the create info references only
    // stack data for the duration of the call.
    unsafe { device.create_image_view(&view_info, None) }
}

fn create_image(
    state: &State,
    width: u32,
    height: u32,
    format: vk::Format,
    tiling: vk::ImageTiling,
    usage: vk::ImageUsageFlags,
    properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Image, vk::DeviceMemory), vk::Result> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .format(format)
        .tiling(tiling)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(usage)
        .samples(vk::SampleCountFlags::TYPE_1)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    // SAFETY: All handles used here belong to the same live device/physical device pair. The
    // image is bound exactly once to the freshly allocated memory returned by this function.
    unsafe {
        let image = state
            .device
            .as_ref()
            .unwrap()
            .create_image(&image_info, None)?;
        let mem_requirements = state
            .device
            .as_ref()
            .unwrap()
            .get_image_memory_requirements(image);
        let mem_type_index = find_memory_type(
            &state.instance,
            state.pdevice,
            mem_requirements.memory_type_bits,
            properties,
        );
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(mem_type_index);
        let memory = state
            .device
            .as_ref()
            .unwrap()
            .allocate_memory(&alloc_info, None)?;
        state
            .device
            .as_ref()
            .unwrap()
            .bind_image_memory(image, memory, 0)?;
        Ok((image, memory))
    }
}

fn color_blend_for(mode: BlendMode) -> vk::PipelineColorBlendAttachmentState {
    match mode {
        BlendMode::Alpha => vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD),
        BlendMode::Add => vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD),
        BlendMode::Multiply => vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ZERO)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD),
        BlendMode::Subtract => vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::REVERSE_SUBTRACT)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
            .alpha_blend_op(vk::BlendOp::ADD),
    }
}

fn create_texture_descriptor_set_pair(
    state: &State,
    pool: vk::DescriptorPool,
) -> Result<[vk::DescriptorSet; 2], vk::Result> {
    let layouts = [state.descriptor_set_layout, state.descriptor_set_layout];
    let alloc_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(pool)
        .set_layouts(&layouts);
    // SAFETY: `pool` and `state.descriptor_set_layout` were created on the same live device and
    // remain valid for the duration of the allocation call.
    unsafe {
        state
            .device
            .as_ref()
            .unwrap()
            .allocate_descriptor_sets(&alloc_info)
            .map(|sets| [sets[0], sets[1]])
    }
}

fn create_texture_descriptor_sets(
    state: &mut State,
    texture_image_view: vk::ImageView,
    sampler_default: vk::Sampler,
    sampler_repeat: vk::Sampler,
) -> Result<(vk::DescriptorSet, vk::DescriptorSet, vk::DescriptorPool), Box<dyn Error>> {
    let (descriptor_set, descriptor_set_repeat, pool) = loop {
        let pool = *state
            .descriptor_pools
            .last()
            .ok_or("Vulkan descriptor pool list is empty")?;
        match create_texture_descriptor_set_pair(state, pool) {
            Ok([set, set_repeat]) => break (set, set_repeat, pool),
            Err(vk::Result::ERROR_OUT_OF_POOL_MEMORY | vk::Result::ERROR_FRAGMENTED_POOL) => {
                grow_descriptor_pool(state)?;
            }
            Err(err) => return Err(Box::new(err)),
        }
    };

    let image_info = vk::DescriptorImageInfo::default()
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image_view(texture_image_view)
        .sampler(sampler_default);
    let image_info_repeat = vk::DescriptorImageInfo::default()
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image_view(texture_image_view)
        .sampler(sampler_repeat);

    let descriptor_write = vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(std::slice::from_ref(&image_info));
    let descriptor_write_repeat = vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set_repeat)
        .dst_binding(0)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(std::slice::from_ref(&image_info_repeat));

    // SAFETY: Both descriptor sets were allocated from `pool`, and the image view/samplers remain
    // live for at least as long as those descriptor sets do.
    unsafe {
        state
            .device
            .as_ref()
            .unwrap()
            .update_descriptor_sets(&[descriptor_write, descriptor_write_repeat], &[]);
    }
    Ok((descriptor_set, descriptor_set_repeat, pool))
}

#[inline(always)]
fn vertex_input_descriptions_textured_instanced() -> (
    [vk::VertexInputBindingDescription; 2],
    [vk::VertexInputAttributeDescription; 11],
) {
    // binding 0: unit quad [x,y,u,v]
    let b0 = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<[f32; 4]>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);

    // binding 1: compact per-instance payload
    let b1 = vk::VertexInputBindingDescription::default()
        .binding(1)
        .stride(std::mem::size_of::<InstanceData>() as u32) // 88
        .input_rate(vk::VertexInputRate::INSTANCE);

    // per-vertex
    let a0 = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(0)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(0); // pos
    let a1 = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(1)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(8); // uv

    // per-instance
    let i_center = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(2)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(0);
    let i_size = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(3)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(16);
    let i_rot = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(4)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(24);
    let i_tint = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(5)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(32);
    let i_uvs = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(6)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(48);
    let i_uvo = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(7)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(56);
    let i_local_offset = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(8)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(64);
    let i_local_offset_rot = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(9)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(72);
    let i_fade = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(10)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(80);

    (
        [b0, b1],
        [
            a0,
            a1,
            i_center,
            i_size,
            i_rot,
            i_tint,
            i_uvs,
            i_uvo,
            i_local_offset,
            i_local_offset_rot,
            i_fade,
        ],
    )
}

#[inline(always)]
fn vertex_input_descriptions_mesh() -> (
    [vk::VertexInputBindingDescription; 1],
    [vk::VertexInputAttributeDescription; 2],
) {
    let b0 = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<MeshVertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);

    let a_pos = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(0)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(0);
    let a_color = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(1)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(8);

    ([b0], [a_pos, a_color])
}

#[inline(always)]
fn vertex_input_descriptions_tmesh() -> (
    [vk::VertexInputBindingDescription; 2],
    [vk::VertexInputAttributeDescription; 12],
) {
    let b0 = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<TexturedMeshVertexGpu>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);
    let b1 = vk::VertexInputBindingDescription::default()
        .binding(1)
        .stride(std::mem::size_of::<TexturedMeshInstanceGpu>() as u32)
        .input_rate(vk::VertexInputRate::INSTANCE);

    let a_pos = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(0)
        .format(vk::Format::R32G32B32_SFLOAT)
        .offset(0);
    let a_uv = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(1)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(12);
    let a_color = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(2)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(20);
    let a_tex_matrix_scale = vk::VertexInputAttributeDescription::default()
        .binding(0)
        .location(3)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(36);
    let a_model0 = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(4)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(0);
    let a_model1 = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(5)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(16);
    let a_model2 = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(6)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(32);
    let a_model3 = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(7)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(48);
    let a_tint = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(8)
        .format(vk::Format::R32G32B32A32_SFLOAT)
        .offset(64);
    let a_uv_scale = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(9)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(80);
    let a_uv_offset = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(10)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(88);
    let a_uv_tex_shift = vk::VertexInputAttributeDescription::default()
        .binding(1)
        .location(11)
        .format(vk::Format::R32G32_SFLOAT)
        .offset(96);

    (
        [b0, b1],
        [
            a_pos,
            a_uv,
            a_color,
            a_tex_matrix_scale,
            a_model0,
            a_model1,
            a_model2,
            a_model3,
            a_tint,
            a_uv_scale,
            a_uv_offset,
            a_uv_tex_shift,
        ],
    )
}

fn begin_single_time_commands(
    device: &Device,
    pool: vk::CommandPool,
) -> Result<vk::CommandBuffer, vk::Result> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(pool)
        .command_buffer_count(1);
    // SAFETY: `pool` belongs to `device` and outlives the allocated command buffer.
    let cmd = unsafe { device.allocate_command_buffers(&alloc_info)?[0] };
    let begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    // SAFETY: `cmd` was just allocated from `pool` and has not yet been begun.
    unsafe {
        device.begin_command_buffer(cmd, &begin_info)?;
    }
    Ok(cmd)
}

fn end_single_time_commands(
    device: &Device,
    pool: vk::CommandPool,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
) -> Result<(), Box<dyn Error>> {
    // SAFETY: `command_buffer` was allocated from `pool`, recorded for one-time use, submitted to
    // `queue`, and is freed only after `queue_wait_idle` guarantees completion.
    unsafe {
        device.end_command_buffer(command_buffer)?;
        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));
        device.queue_submit(queue, &[submit_info], vk::Fence::null())?;
        device.queue_wait_idle(queue)?;
        device.free_command_buffers(pool, &[command_buffer]);
    }
    Ok(())
}

fn create_buffer<T: Copy>(
    instance: &Instance,
    device: &Device,
    pdevice: vk::PhysicalDevice,
    pool: vk::CommandPool,
    queue: vk::Queue,
    usage: vk::BufferUsageFlags,
    properties: vk::MemoryPropertyFlags,
    data: Option<&[T]>,
) -> Result<BufferResource, Box<dyn Error>> {
    let buffer_size = (mem::size_of::<T>() * data.map_or(1, <[T]>::len)) as vk::DeviceSize;

    if let Some(slice) = data {
        let staging_usage = vk::BufferUsageFlags::TRANSFER_SRC;
        let staging_props =
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let (staging_buffer, staging_memory) = create_gpu_buffer(
            instance,
            device,
            pdevice,
            buffer_size,
            staging_usage,
            staging_props,
        )?;

        // SAFETY: The staging allocation is HOST_VISIBLE | HOST_COHERENT, `slice` points to
        // initialized `T` values, and the mapped range is unmapped before submission.
        unsafe {
            let mapped =
                device.map_memory(staging_memory, 0, buffer_size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(slice.as_ptr(), mapped.cast::<T>(), slice.len());
            device.unmap_memory(staging_memory);
        }

        let final_usage = usage | vk::BufferUsageFlags::TRANSFER_DST;
        let (device_buffer, device_memory) = create_gpu_buffer(
            instance,
            device,
            pdevice,
            buffer_size,
            final_usage,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        copy_buffer(
            device,
            pool,
            queue,
            staging_buffer,
            device_buffer,
            buffer_size,
        )?;

        // SAFETY: The transfer submission above waits for the queue to idle before returning, so
        // the temporary staging buffer and allocation are no longer in use here.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }

        Ok(BufferResource {
            buffer: device_buffer,
            memory: device_memory,
        })
    } else {
        let (buffer, memory) =
            create_gpu_buffer(instance, device, pdevice, buffer_size, usage, properties)?;
        Ok(BufferResource { buffer, memory })
    }
}

fn copy_buffer(
    device: &Device,
    pool: vk::CommandPool,
    queue: vk::Queue,
    src: vk::Buffer,
    dst: vk::Buffer,
    size: vk::DeviceSize,
) -> Result<(), Box<dyn Error>> {
    let cmd = begin_single_time_commands(device, pool)?;
    // SAFETY: `cmd` is a live recording command buffer, and `src`/`dst` are valid buffers with a
    // copy region bounded by `size`.
    unsafe {
        let region = vk::BufferCopy::default().size(size);
        device.cmd_copy_buffer(cmd, src, dst, &[region]);
    }
    end_single_time_commands(device, pool, queue, cmd)?;
    Ok(())
}

fn create_gpu_buffer(
    instance: &Instance,
    device: &Device,
    pdevice: vk::PhysicalDevice,
    size: vk::DeviceSize,
    usage: vk::BufferUsageFlags,
    properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory), Box<dyn Error>> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    // SAFETY: The create info references only stack data for the duration of the call and
    // describes a buffer to be created on this live device.
    let buffer = unsafe { device.create_buffer(&buffer_info, None)? };
    // SAFETY: `buffer` was just created on this device and is valid for querying memory
    // requirements.
    let mem_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let mem_type_index = find_memory_type(
        instance,
        pdevice,
        mem_requirements.memory_type_bits,
        properties,
    );
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(mem_requirements.size)
        .memory_type_index(mem_type_index);
    // SAFETY: The allocation info matches the queried buffer requirements and uses a compatible
    // memory type index chosen from this physical device.
    let memory = unsafe { device.allocate_memory(&alloc_info, None)? };
    // SAFETY: `buffer` and `memory` were both created on this device, and offset 0 satisfies the
    // alignment required by `mem_requirements`.
    unsafe { device.bind_buffer_memory(buffer, memory, 0)? };
    Ok((buffer, memory))
}

fn destroy_buffer(device: &Device, buffer: &BufferResource) {
    // SAFETY: `buffer` owns this Vulkan buffer/allocation pair and callers only invoke this after
    // GPU work that might reference it has completed.
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn ensure_cached_tmesh(
    instance: &Instance,
    device: &Device,
    pdevice: vk::PhysicalDevice,
    cached_tmesh: &mut FastU64Map<CachedTMeshGeom>,
    cached_tmesh_bytes: &mut usize,
    cache_key: TMeshCacheKey,
    vertices: &[crate::engine::gfx::TexturedMeshVertex],
) -> Result<bool, Box<dyn Error>> {
    if let Some(entry) = cached_tmesh.get(&cache_key) {
        return Ok(entry.vertex_count == vertices.len() as u32);
    }

    let bytes = vertices.len() * std::mem::size_of::<TexturedMeshVertexGpu>();
    if bytes > VULKAN_TMESH_CACHE_MAX_BYTES
        || cached_tmesh_bytes.saturating_add(bytes) > VULKAN_TMESH_CACHE_MAX_BYTES
    {
        return Ok(false);
    }

    let size = bytes as vk::DeviceSize;
    let (buffer, memory) = create_gpu_buffer(
        instance,
        device,
        pdevice,
        size,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;

    // SAFETY: `memory` is a HOST_VISIBLE allocation created on this device and the
    // mapped range is fully written with initialized vertex data before unmapping.
    unsafe {
        let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
        let dst = mapped.cast::<TexturedMeshVertexGpu>();
        for (ix, vertex) in vertices.iter().enumerate() {
            std::ptr::write(
                dst.add(ix),
                TexturedMeshVertexGpu {
                    pos: vertex.pos,
                    uv: vertex.uv,
                    color: vertex.color,
                    tex_matrix_scale: vertex.tex_matrix_scale,
                },
            );
        }
        device.unmap_memory(memory);
    }

    cached_tmesh.insert(
        cache_key,
        CachedTMeshGeom {
            buffer: BufferResource { buffer, memory },
            vertex_count: vertices.len() as u32,
        },
    );
    *cached_tmesh_bytes = cached_tmesh_bytes.saturating_add(bytes);
    Ok(true)
}

fn find_memory_type(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
    type_filter: u32,
    properties: vk::MemoryPropertyFlags,
) -> u32 {
    // SAFETY: `pdevice` comes from `instance`, so querying its memory properties is valid.
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(pdevice) };
    (0..mem_properties.memory_type_count)
        .find(|i| {
            let i_usize = *i as usize;
            (type_filter & (1 << i)) != 0
                && (mem_properties.memory_types[i_usize].property_flags & properties) == properties
        })
        .expect("Failed to find suitable memory type!")
}

const VALIDATION_LAYER_NAME: &ffi::CStr = c"VK_LAYER_KHRONOS_validation";

// SAFETY: Vulkan invokes this callback with the required ABI and pointer validity guarantees for
// the duration of each call; the body treats all incoming pointers as borrowed only transiently.
unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _p_user_data: *mut ffi::c_void,
) -> vk::Bool32 {
    let msg = if p_callback_data.is_null() {
        std::borrow::Cow::Borrowed("<null callback data>")
    } else {
        // SAFETY: Vulkan calls this callback with a valid pointer for the duration of the call
        // whenever `p_callback_data` is non-null.
        let p_message = unsafe { (*p_callback_data).p_message };
        if p_message.is_null() {
            std::borrow::Cow::Borrowed("<null message>")
        } else {
            // SAFETY: `p_message` is a NUL-terminated C string owned by the validation layer for
            // the duration of this callback.
            unsafe { ffi::CStr::from_ptr(p_message) }.to_string_lossy()
        }
    };
    if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        error!("Vulkan validation {message_type:?}: {msg}");
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING) {
        warn!("Vulkan validation {message_type:?}: {msg}");
    } else if message_severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::INFO) {
        debug!("Vulkan validation {message_type:?}: {msg}");
    } else {
        debug!("Vulkan validation {message_type:?}: {msg}");
    }
    vk::FALSE
}

fn supports_instance_extension(entry: &Entry, extension: &ffi::CStr) -> Result<bool, vk::Result> {
    // SAFETY: Enumerating instance extensions only reads immutable loader state and does not
    // retain borrowed pointers beyond the call.
    let exts = unsafe { entry.enumerate_instance_extension_properties(None)? };
    Ok(exts
        .iter()
        // SAFETY: Vulkan writes a NUL-terminated extension name into this fixed array.
        .any(|ext| unsafe { ffi::CStr::from_ptr(ext.extension_name.as_ptr()) == extension }))
}

fn supports_instance_layer(entry: &Entry, layer: &ffi::CStr) -> Result<bool, vk::Result> {
    // SAFETY: Enumerating instance layers only reads immutable loader state and does not retain
    // borrowed pointers beyond the call.
    let layers = unsafe { entry.enumerate_instance_layer_properties()? };
    Ok(layers
        .iter()
        // SAFETY: Vulkan writes a NUL-terminated layer name into this fixed array.
        .any(|prop| unsafe { ffi::CStr::from_ptr(prop.layer_name.as_ptr()) == layer }))
}

fn create_instance(
    entry: &Entry,
    window: &Window,
    gfx_debug_enabled: bool,
) -> Result<(Instance, bool), Box<dyn Error>> {
    let app_name = c"DeadSync";
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name)
        .application_version(vk::make_api_version(0, 1, 0, 0))
        .engine_name(c"DeadSync Engine")
        .engine_version(vk::make_api_version(0, 1, 0, 0))
        .api_version(vk::API_VERSION_1_3);

    let mut extension_names =
        ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?.to_vec();
    let mut debug_utils_enabled = false;
    let mut layers_names_raw: Vec<*const ffi::c_char> = vec![];
    if gfx_debug_enabled {
        if supports_instance_extension(entry, ash::ext::debug_utils::NAME)? {
            let debug_ptr = ash::ext::debug_utils::NAME.as_ptr();
            if !extension_names.contains(&debug_ptr) {
                extension_names.push(debug_ptr);
            }
            debug_utils_enabled = true;
        } else {
            warn!(
                "Vulkan debug requested but VK_EXT_debug_utils is unavailable; debug messenger disabled."
            );
        }

        if supports_instance_layer(entry, VALIDATION_LAYER_NAME)? {
            layers_names_raw.push(VALIDATION_LAYER_NAME.as_ptr());
        } else {
            warn!(
                "Vulkan debug requested but '{}' is unavailable; validation layers disabled.",
                VALIDATION_LAYER_NAME.to_string_lossy()
            );
        }
    }

    let mut create_flags = vk::InstanceCreateFlags::empty();
    if cfg!(target_os = "macos") {
        extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
        create_flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
    }

    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names)
        .enabled_layer_names(&layers_names_raw)
        .flags(create_flags);

    // SAFETY: All extension/layer name pointers come from static `CStr`s or winit-provided
    // extension arrays and stay valid for the duration of the call.
    let instance = unsafe { entry.create_instance(&create_info, None)? };
    if gfx_debug_enabled {
        debug!(
            "Vulkan debug config: validation_layers={}, debug_messenger={}.",
            !layers_names_raw.is_empty(),
            debug_utils_enabled
        );
    }
    Ok((instance, debug_utils_enabled))
}

fn setup_debug_messenger(
    entry: &Entry,
    instance: &Instance,
    debug_utils_enabled: bool,
) -> Result<
    (
        Option<ash::ext::debug_utils::Instance>,
        Option<vk::DebugUtilsMessengerEXT>,
    ),
    vk::Result,
> {
    if !debug_utils_enabled {
        return Ok((None, None));
    }
    let loader = ash::ext::debug_utils::Instance::new(entry, instance);
    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(vulkan_debug_callback));
    // SAFETY: `loader` is bound to the live `instance`, and the callback function remains valid
    // for the lifetime of the created messenger.
    let messenger = unsafe { loader.create_debug_utils_messenger(&create_info, None)? };
    Ok((Some(loader), Some(messenger)))
}

fn create_surface(
    entry: &Entry,
    instance: &Instance,
    window: &Window,
) -> Result<vk::SurfaceKHR, Box<dyn Error>> {
    // SAFETY: The raw display/window handles come directly from `window` and remain valid for the
    // duration of the call; the returned surface is owned by `instance`.
    unsafe {
        Ok(ash_window::create_surface(
            entry,
            instance,
            window.display_handle()?.as_raw(),
            window.window_handle()?.as_raw(),
            None,
        )?)
    }
}

fn select_physical_device(
    instance: &Instance,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<vk::PhysicalDevice, Box<dyn Error>> {
    // SAFETY: Enumerating physical devices only reads instance state and returns handles owned by
    // the instance.
    let pdevices = unsafe { instance.enumerate_physical_devices()? };
    pdevices
        .into_iter()
        .find(|pdevice| is_device_suitable(instance, *pdevice, surface_loader, surface))
        .ok_or_else(|| "Failed to find a suitable GPU!".into())
}

#[inline(always)]
fn vk_vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x13B5 => "ARM",
        0x5143 => "Qualcomm",
        0x1010 => "ImgTec",
        0x106B => "Apple",
        _ => "Unknown",
    }
}

#[inline(always)]
fn decode_driver_version(vendor_id: u32, driver_version: u32) -> String {
    if vendor_id == 0x10DE {
        let a = (driver_version >> 22) & 0x3ff;
        let b = (driver_version >> 14) & 0x0ff;
        let c = (driver_version >> 6) & 0x0ff;
        let d = driver_version & 0x03f;
        return format!("{a}.{b}.{c}.{d} (raw=0x{driver_version:08x})");
    }
    let major = vk::api_version_major(driver_version);
    let minor = vk::api_version_minor(driver_version);
    let patch = vk::api_version_patch(driver_version);
    format!("{major}.{minor}.{patch} (raw=0x{driver_version:08x})")
}

fn log_selected_device(instance: &Instance, pdevice: vk::PhysicalDevice) {
    // SAFETY: `pdevice` was enumerated from `instance`, so querying immutable device properties is
    // valid for the lifetime of the instance.
    let props = unsafe { instance.get_physical_device_properties(pdevice) };
    // SAFETY: Vulkan stores the device name as a NUL-terminated string in `props.device_name`.
    let name = unsafe { ffi::CStr::from_ptr(props.device_name.as_ptr()) }.to_string_lossy();
    let api_major = vk::api_version_major(props.api_version);
    let api_minor = vk::api_version_minor(props.api_version);
    let api_patch = vk::api_version_patch(props.api_version);
    let vendor_name = vk_vendor_name(props.vendor_id);
    let driver = decode_driver_version(props.vendor_id, props.driver_version);
    info!(
        "Vulkan GPU: {} [{}], driver {}, API {}.{}.{} (pci ven=0x{:04x} dev=0x{:04x})",
        name,
        vendor_name,
        driver,
        api_major,
        api_minor,
        api_patch,
        props.vendor_id,
        props.device_id
    );
}

#[cfg(windows)]
fn qpc_freq_hz() -> Option<u64> {
    let mut hz = 0i64;
    // SAFETY: Windows writes the performance-counter frequency into the provided out pointer.
    unsafe {
        Performance::QueryPerformanceFrequency(&mut hz).ok()?;
    }
    (hz > 0).then_some(hz as u64)
}

#[cfg(windows)]
#[inline(always)]
fn qpc_ticks_to_nanos(ticks: u64) -> Option<u64> {
    let hz = (*QPC_FREQ_HZ)?;
    Some(((ticks as u128) * 1_000_000_000u128 / hz as u128).min(u128::from(u64::MAX)) as u64)
}

#[cfg(target_os = "windows")]
#[inline(always)]
fn current_host_nanos() -> u64 {
    crate::engine::windows_rt::current_host_nanos()
}

#[cfg(not(target_os = "windows"))]
#[inline(always)]
fn current_host_nanos() -> u64 {
    crate::engine::host_time::now_nanos()
}

#[inline(always)]
fn vk_present_mode_trace(mode: vk::PresentModeKHR) -> PresentModeTrace {
    match mode {
        vk::PresentModeKHR::FIFO => PresentModeTrace::Fifo,
        vk::PresentModeKHR::FIFO_RELAXED => PresentModeTrace::FifoRelaxed,
        vk::PresentModeKHR::MAILBOX => PresentModeTrace::Mailbox,
        vk::PresentModeKHR::IMMEDIATE => PresentModeTrace::Immediate,
        _ => PresentModeTrace::Unknown,
    }
}

#[inline(always)]
fn vk_clock_domain_trace(domain: Option<vk::TimeDomainKHR>) -> ClockDomainTrace {
    match domain {
        Some(vk::TimeDomainKHR::DEVICE) => ClockDomainTrace::Device,
        Some(vk::TimeDomainKHR::CLOCK_MONOTONIC) => ClockDomainTrace::Monotonic,
        Some(vk::TimeDomainKHR::CLOCK_MONOTONIC_RAW) => ClockDomainTrace::MonotonicRaw,
        Some(vk::TimeDomainKHR::QUERY_PERFORMANCE_COUNTER) => ClockDomainTrace::Qpc,
        _ => ClockDomainTrace::Unknown,
    }
}

#[inline(always)]
fn time_domain_to_nanos(domain: vk::TimeDomainKHR, raw: u64) -> Option<u64> {
    match domain {
        vk::TimeDomainKHR::CLOCK_MONOTONIC | vk::TimeDomainKHR::CLOCK_MONOTONIC_RAW => Some(raw),
        vk::TimeDomainKHR::QUERY_PERFORMANCE_COUNTER => {
            #[cfg(windows)]
            {
                qpc_ticks_to_nanos(raw)
            }
            #[cfg(not(windows))]
            {
                None
            }
        }
        _ => None,
    }
}

#[inline(always)]
fn has_time_domain(domains: &[vk::TimeDomainKHR], want: vk::TimeDomainKHR) -> bool {
    domains.contains(&want)
}

fn pick_host_clock(domains: &[vk::TimeDomainKHR]) -> Option<vk::TimeDomainKHR> {
    #[cfg(windows)]
    if has_time_domain(domains, vk::TimeDomainKHR::QUERY_PERFORMANCE_COUNTER) {
        return Some(vk::TimeDomainKHR::QUERY_PERFORMANCE_COUNTER);
    }
    if has_time_domain(domains, vk::TimeDomainKHR::CLOCK_MONOTONIC_RAW) {
        return Some(vk::TimeDomainKHR::CLOCK_MONOTONIC_RAW);
    }
    if has_time_domain(domains, vk::TimeDomainKHR::CLOCK_MONOTONIC) {
        return Some(vk::TimeDomainKHR::CLOCK_MONOTONIC);
    }
    None
}

fn pick_display_clock(
    domains: &[vk::TimeDomainKHR],
    host_clock: Option<vk::TimeDomainKHR>,
) -> Option<vk::TimeDomainKHR> {
    if has_time_domain(domains, vk::TimeDomainKHR::CLOCK_MONOTONIC_RAW) {
        return Some(vk::TimeDomainKHR::CLOCK_MONOTONIC_RAW);
    }
    if has_time_domain(domains, vk::TimeDomainKHR::CLOCK_MONOTONIC) {
        return Some(vk::TimeDomainKHR::CLOCK_MONOTONIC);
    }
    host_clock.filter(|domain| *domain != vk::TimeDomainKHR::DEVICE)
}

#[inline(always)]
fn calibrate_offset_ns(host_ns: u64, display_ns: u64) -> i64 {
    let diff = i128::from(host_ns) - i128::from(display_ns);
    diff.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[inline(always)]
fn apply_display_to_host_offset(display_ns: u64, offset_ns: i64) -> u64 {
    let host = i128::from(display_ns) + i128::from(offset_ns);
    host.clamp(0, i128::from(u64::MAX)) as u64
}

#[inline(always)]
fn has_device_extension(props: &[vk::ExtensionProperties], name: &ffi::CStr) -> bool {
    props
        .iter()
        // SAFETY: Vulkan writes a NUL-terminated extension name into this fixed array.
        .any(|prop| unsafe { ffi::CStr::from_ptr(prop.extension_name.as_ptr()) == name })
}

fn query_device_exts(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
) -> Result<DeviceExts, Box<dyn Error>> {
    // SAFETY: Enumerating device extensions only reads immutable properties for `pdevice`.
    let props = unsafe { instance.enumerate_device_extension_properties(pdevice)? };
    Ok(DeviceExts {
        display_timing: has_device_extension(&props, display_timing::NAME),
        calibrated_timestamps: has_device_extension(&props, calibrated_timestamps::NAME),
    })
}

fn init_present_telemetry(
    entry: &Entry,
    instance: &Instance,
    device: &Device,
    pdevice: vk::PhysicalDevice,
    swapchain: vk::SwapchainKHR,
    exts: DeviceExts,
) -> PresentTelemetryState {
    let mut telemetry = PresentTelemetryState::default();
    if !VULKAN_PRESENT_DISPLAY_TIMING_TELEMETRY {
        info!("Vulkan present telemetry: CPU-only (display timing disabled)");
        return telemetry;
    }
    if !exts.display_timing {
        info!("Vulkan present telemetry: CPU-only (VK_GOOGLE_display_timing unavailable)");
        return telemetry;
    }
    let loader = display_timing::Device::new(instance, device);
    telemetry.display_timing = Some(loader);
    telemetry.refresh_ns = refresh_cycle_duration(&telemetry, swapchain).unwrap_or(0);
    if telemetry.refresh_ns > 0 {
        info!(
            "Vulkan present telemetry: VK_GOOGLE_display_timing enabled refresh_ms={:.3}",
            telemetry.refresh_ns as f64 / 1_000_000.0
        );
    } else {
        info!("Vulkan present telemetry: VK_GOOGLE_display_timing enabled");
    }
    if exts.calibrated_timestamps {
        let calib_instance = calibrated_timestamps::Instance::new(entry, instance);
        // SAFETY: The calibrated-timestamps loader is bound to this live instance and `pdevice`
        // was enumerated from that same instance.
        match unsafe { calib_instance.get_physical_device_calibrateable_time_domains(pdevice) } {
            Ok(domains) => {
                telemetry.host_clock = pick_host_clock(&domains);
                telemetry.display_clock = pick_display_clock(&domains, telemetry.host_clock);
                telemetry.calibrated_timestamps =
                    Some(calibrated_timestamps::Device::new(instance, device));
                info!(
                    "Vulkan present clock calibration: display={} host={}",
                    vk_clock_domain_trace(telemetry.display_clock),
                    vk_clock_domain_trace(telemetry.host_clock)
                );
            }
            Err(e) => {
                debug!("Vulkan present clock calibration unavailable: {e:?}");
            }
        }
    }
    telemetry
}

fn refresh_cycle_duration(
    telemetry: &PresentTelemetryState,
    swapchain: vk::SwapchainKHR,
) -> Result<u64, vk::Result> {
    let Some(loader) = telemetry.display_timing.as_ref() else {
        return Ok(0);
    };
    // SAFETY: `loader` is tied to the live device/swapchain pair stored in telemetry/state.
    let duration = unsafe { loader.get_refresh_cycle_duration(swapchain)? };
    Ok(duration.refresh_duration)
}

fn reset_present_telemetry(state: &mut State) {
    state.present_telemetry.next_present_id = 1;
    state.present_telemetry.last_seen_present_id = 0;
    state.present_telemetry.cpu_last_completed_present_id = 0;
    state.present_telemetry.cpu_last_completed_host_ns = 0;
    state.present_telemetry.last_completed = None;
    state.present_telemetry.host_minus_display_ns = 0;
    state.present_telemetry.calibration_error_ns = 0;
    state
        .present_telemetry
        .cpu_image_present_ids
        .resize(state.images_in_flight.len(), 0);
    state.present_telemetry.cpu_image_present_ids.fill(0);
    state.present_telemetry.scratch_timings.clear();
    state.present_telemetry.refresh_ns = refresh_cycle_duration(
        &state.present_telemetry,
        state.swapchain_resources.swapchain,
    )
    .unwrap_or(0);
}

#[inline(always)]
fn next_present_id(state: &mut State) -> u32 {
    let id = state.present_telemetry.next_present_id.max(1);
    state.present_telemetry.next_present_id = id.wrapping_add(1).max(1);
    id
}

fn record_cpu_present_completion(state: &mut State, image_index: u32) {
    if state.present_telemetry.display_timing.is_some() {
        return;
    }
    let idx = image_index as usize;
    if idx >= state.present_telemetry.cpu_image_present_ids.len() {
        state
            .present_telemetry
            .cpu_image_present_ids
            .resize(state.images_in_flight.len(), 0);
    }
    let Some(&present_id) = state.present_telemetry.cpu_image_present_ids.get(idx) else {
        return;
    };
    if present_id == 0 || present_id <= state.present_telemetry.cpu_last_completed_present_id {
        return;
    }
    let host_present_ns = current_host_nanos();
    if host_present_ns == 0 {
        return;
    }
    let interval_ns = if state.present_telemetry.cpu_last_completed_host_ns == 0 {
        0
    } else {
        host_present_ns.saturating_sub(state.present_telemetry.cpu_last_completed_host_ns)
    };
    state.present_telemetry.last_seen_present_id = present_id;
    state.present_telemetry.cpu_last_completed_present_id = present_id;
    state.present_telemetry.cpu_last_completed_host_ns = host_present_ns;
    state.present_telemetry.last_completed = Some(CompletedPresentTiming {
        present_id,
        actual_present_time_ns: 0,
        interval_ns,
        present_margin_ns: 0,
        host_present_time_ns: host_present_ns,
    });
}

fn calibrate_present_clock(state: &mut State) {
    let Some(loader) = state.present_telemetry.calibrated_timestamps.as_ref() else {
        return;
    };
    let Some(host_clock) = state.present_telemetry.host_clock else {
        return;
    };
    let display_clock = state.present_telemetry.display_clock.unwrap_or(host_clock);
    let host_info = vk::CalibratedTimestampInfoKHR::default().time_domain(host_clock);
    let display_info = vk::CalibratedTimestampInfoKHR::default().time_domain(display_clock);
    let (timestamps, max_deviation) = if display_clock == host_clock {
        // SAFETY: The loader is bound to the live device, and the input slice lives for the
        // duration of the call.
        match unsafe { loader.get_calibrated_timestamps(&[host_info]) } {
            Ok(pair) => pair,
            Err(e) => {
                debug!("Vulkan present clock calibration failed: {e:?}");
                return;
            }
        }
    } else {
        // SAFETY: The loader is bound to the live device, and the input slice lives for the
        // duration of the call.
        match unsafe { loader.get_calibrated_timestamps(&[host_info, display_info]) } {
            Ok(pair) => pair,
            Err(e) => {
                debug!("Vulkan present clock calibration failed: {e:?}");
                return;
            }
        }
    };
    let Some(host_ns) = time_domain_to_nanos(host_clock, timestamps[0]) else {
        return;
    };
    let display_ns = if display_clock == host_clock {
        host_ns
    } else if let Some(display_ns) = time_domain_to_nanos(display_clock, timestamps[1]) {
        display_ns
    } else {
        return;
    };
    state.present_telemetry.host_minus_display_ns = calibrate_offset_ns(host_ns, display_ns);
    state.present_telemetry.calibration_error_ns = max_deviation;
}

fn poll_past_presentation_timing(state: &mut State) {
    let Some(loader) = state.present_telemetry.display_timing.as_ref() else {
        return;
    };
    let mut count: u32 = 0;
    // SAFETY: Passing a null output pointer is the Vulkan-prescribed "count only" query form.
    let first = unsafe {
        (loader.fp().get_past_presentation_timing_google)(
            loader.device(),
            state.swapchain_resources.swapchain,
            &mut count,
            std::ptr::null_mut(),
        )
    };
    match first {
        vk::Result::SUCCESS | vk::Result::INCOMPLETE => {}
        vk::Result::ERROR_OUT_OF_DATE_KHR => return,
        other => {
            debug!("Vulkan present telemetry: timing query failed: {other:?}");
            return;
        }
    }
    if count == 0 {
        return;
    }
    let host_map_ready = state.present_telemetry.host_clock.is_some()
        && state.present_telemetry.display_clock.is_some();
    let host_minus_display_ns = state.present_telemetry.host_minus_display_ns;
    let timings = &mut state.present_telemetry.scratch_timings;
    timings.clear();
    if timings.capacity() < count as usize {
        timings.reserve(count as usize - timings.capacity());
    }
    // SAFETY: `timings` has capacity for `count` elements and Vulkan writes at most that many
    // initialized records into the buffer before returning.
    let second = unsafe {
        (loader.fp().get_past_presentation_timing_google)(
            loader.device(),
            state.swapchain_resources.swapchain,
            &mut count,
            timings.as_mut_ptr(),
        )
    };
    match second {
        // SAFETY: On SUCCESS/INCOMPLETE Vulkan initialized exactly `count` entries in `timings`.
        vk::Result::SUCCESS | vk::Result::INCOMPLETE => unsafe {
            timings.set_len(count as usize);
        },
        vk::Result::ERROR_OUT_OF_DATE_KHR => return,
        other => {
            debug!("Vulkan present telemetry: timing fetch failed: {other:?}");
            return;
        }
    }
    let mut prev_actual_ns = state
        .present_telemetry
        .last_completed
        .map_or(0, |timing| timing.actual_present_time_ns);
    for timing in timings.iter() {
        if timing.present_id <= state.present_telemetry.last_seen_present_id {
            continue;
        }
        let interval_ns = if prev_actual_ns == 0 {
            0
        } else {
            timing.actual_present_time.saturating_sub(prev_actual_ns)
        };
        prev_actual_ns = timing.actual_present_time;
        state.present_telemetry.last_seen_present_id = timing.present_id;
        state.present_telemetry.last_completed = Some(CompletedPresentTiming {
            present_id: timing.present_id,
            actual_present_time_ns: timing.actual_present_time,
            interval_ns,
            present_margin_ns: timing.present_margin,
            host_present_time_ns: if host_map_ready {
                apply_display_to_host_offset(timing.actual_present_time, host_minus_display_ns)
            } else {
                0
            },
        });
    }
}

fn snapshot_present_stats(
    state: &State,
    image_waited: bool,
    back_pressure_waited: bool,
    queue_idle_waited: bool,
    suboptimal: bool,
    submitted_present_id: u32,
) -> PresentStats {
    let mut stats = PresentStats {
        mode: vk_present_mode_trace(state.swapchain_resources.present_mode),
        display_clock: vk_clock_domain_trace(state.present_telemetry.display_clock),
        host_clock: if state.present_telemetry.host_clock.is_some() {
            vk_clock_domain_trace(state.present_telemetry.host_clock)
        } else if state
            .present_telemetry
            .last_completed
            .is_some_and(|completed| completed.host_present_time_ns != 0)
        {
            #[cfg(target_os = "windows")]
            {
                ClockDomainTrace::Qpc
            }
            #[cfg(not(target_os = "windows"))]
            {
                ClockDomainTrace::Monotonic
            }
        } else {
            ClockDomainTrace::Unknown
        },
        in_flight_images: state
            .images_in_flight
            .iter()
            .filter(|&&fence| fence != vk::Fence::null())
            .count()
            .min(usize::from(u8::MAX)) as u8,
        waited_for_image: image_waited,
        applied_back_pressure: back_pressure_waited,
        queue_idle_waited,
        suboptimal,
        submitted_present_id,
        refresh_ns: state.present_telemetry.refresh_ns,
        calibration_error_ns: state.present_telemetry.calibration_error_ns,
        ..PresentStats::default()
    };
    if let Some(completed) = state.present_telemetry.last_completed {
        stats.completed_present_id = completed.present_id;
        stats.actual_interval_ns = completed.interval_ns;
        stats.present_margin_ns = completed.present_margin_ns;
        stats.host_present_ns = completed.host_present_time_ns;
    }
    stats
}

fn is_device_suitable(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> bool {
    find_queue_family(instance, pdevice, surface_loader, surface).is_some()
}

fn find_queue_family(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> Option<u32> {
    // SAFETY: `pdevice` was enumerated from `instance`, so querying queue-family properties is
    // valid for the lifetime of the instance.
    let queue_families = unsafe { instance.get_physical_device_queue_family_properties(pdevice) };
    queue_families.iter().enumerate().find_map(|(i, family)| {
        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            // SAFETY: `surface` belongs to `surface_loader`'s instance, and `i` indexes the queue
            // family slice we just queried from the same physical device.
            && unsafe {
                surface_loader
                    .get_physical_device_surface_support(pdevice, i as u32, surface)
                    .unwrap_or(false)
            }
        {
            Some(i as u32)
        } else {
            None
        }
    })
}

fn create_logical_device(
    instance: &Instance,
    pdevice: vk::PhysicalDevice,
    surface_loader: &surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(Device, vk::Queue, u32, DeviceExts), Box<dyn Error>> {
    let queue_family_index = find_queue_family(instance, pdevice, surface_loader, surface)
        .ok_or("No suitable queue family found")?;
    let device_exts = query_device_exts(instance, pdevice)?;
    let queue_priorities = [1.0];
    let queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&queue_priorities);
    let mut device_extensions = Vec::with_capacity(3);
    device_extensions.push(swapchain::NAME.as_ptr());
    if VULKAN_PRESENT_DISPLAY_TIMING_TELEMETRY && device_exts.display_timing {
        device_extensions.push(display_timing::NAME.as_ptr());
    }
    if VULKAN_PRESENT_DISPLAY_TIMING_TELEMETRY && device_exts.calibrated_timestamps {
        device_extensions.push(calibrated_timestamps::NAME.as_ptr());
    }
    let features = vk::PhysicalDeviceFeatures::default();
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_create_info))
        .enabled_extension_names(&device_extensions)
        .enabled_features(&features);

    // SAFETY: `pdevice` was selected from `instance`, the queue info references stack data only
    // for this call, and enabled extension names come from static `CStr`s.
    let device = unsafe { instance.create_device(pdevice, &create_info, None)? };
    // SAFETY: We requested one queue from `queue_family_index`, so queue 0 is valid to fetch.
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
    Ok((device, queue, queue_family_index, device_exts))
}

fn create_swapchain(
    instance: &Instance,
    device: &Device,
    pdevice: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &surface::Instance,
    window_size: PhysicalSize<u32>,
    old_swapchain: Option<vk::SwapchainKHR>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
) -> Result<SwapchainResources, Box<dyn Error>> {
    // SAFETY: These surface queries only read immutable capabilities/formats/present modes for
    // the selected physical device and surface.
    let capabilities =
        unsafe { surface_loader.get_physical_device_surface_capabilities(pdevice, surface)? };
    // SAFETY: These surface queries only read immutable capabilities/formats/present modes for
    // the selected physical device and surface.
    let formats = unsafe { surface_loader.get_physical_device_surface_formats(pdevice, surface)? };
    let present_modes =
        // SAFETY: These surface queries only read immutable capabilities/formats/present modes for
        // the selected physical device and surface.
        unsafe { surface_loader.get_physical_device_surface_present_modes(pdevice, surface)? };

    let format = formats
        .iter()
        .find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
        .copied()
        .unwrap_or(formats[0]);

    let present_mode = if vsync_enabled {
        vk::PresentModeKHR::FIFO
    } else if present_mode_policy == PresentModePolicy::Immediate {
        if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
            vk::PresentModeKHR::IMMEDIATE
        } else if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            vk::PresentModeKHR::FIFO
        }
    } else if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
        vk::PresentModeKHR::MAILBOX
    } else if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
        vk::PresentModeKHR::IMMEDIATE
    } else {
        vk::PresentModeKHR::FIFO
    };

    let desired_images = if present_mode == vk::PresentModeKHR::MAILBOX {
        3
    } else {
        capabilities.min_image_count + 1
    };

    let image_count = match capabilities.max_image_count {
        0 => desired_images,
        max => desired_images.min(max),
    };
    debug!(
        "Vulkan swapchain config: vsync={} present_mode={:?} images={} surface_min={} surface_max={} extent={}x{}",
        vsync_enabled,
        present_mode,
        image_count,
        capabilities.min_image_count,
        capabilities.max_image_count,
        window_size.width,
        window_size.height
    );

    // Derive the swapchain extent, making sure we never create a swapchain
    // with a zero-sized surface (which is invalid and triggers validation errors
    // on some platforms when the window is minimized or not yet fully realized).
    let mut extent = if capabilities.current_extent.width == u32::MAX {
        vk::Extent2D {
            width: window_size.width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ),
            height: window_size.height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ),
        }
    } else {
        capabilities.current_extent
    };

    if extent.width == 0 || extent.height == 0 {
        // Some drivers can briefly report a zero-sized surface (e.g. during
        // startup or minimize). Creating a swapchain with a 0x0 extent is
        // invalid, so fall back to the minimum supported extent instead.
        let fallback = vk::Extent2D {
            width: capabilities.min_image_extent.width.max(1),
            height: capabilities.min_image_extent.height.max(1),
        };
        debug!(
            "Surface reported zero-sized extent ({}x{}); using fallback {}x{} for swapchain",
            extent.width, extent.height, fallback.width, fallback.height
        );
        extent = fallback;
    }

    let supports_transfer_src = capabilities
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_SRC);
    let mut image_usage = vk::ImageUsageFlags::COLOR_ATTACHMENT;
    if supports_transfer_src {
        image_usage |= vk::ImageUsageFlags::TRANSFER_SRC;
    }

    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(image_usage)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));

    let swapchain_loader = swapchain::Device::new(instance, device);
    // SAFETY: `create_info` references only stack data for the duration of the call and uses the
    // live surface/device pair associated with `swapchain_loader`.
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None)? };
    // SAFETY: `swapchain` was just created by `swapchain_loader` and is valid to query here.
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };
    let image_views = images
        .iter()
        .map(|&image| create_image_view(device, image, format.format))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SwapchainResources {
        swapchain_loader,
        swapchain,
        _images: images,
        image_views,
        framebuffers: vec![],
        extent,
        format,
        present_mode,
        supports_transfer_src,
    })
}

fn recreate_framebuffers(
    device: &Device,
    swapchain_resources: &mut SwapchainResources,
    render_pass: vk::RenderPass,
) -> Result<(), vk::Result> {
    swapchain_resources.framebuffers = swapchain_resources
        .image_views
        .iter()
        .map(|view| {
            let attachments = [*view];
            let create_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(swapchain_resources.extent.width)
                .height(swapchain_resources.extent.height)
                .layers(1);
            // SAFETY: The framebuffer create info references the live render pass and image view
            // for this swapchain image, and all borrowed data lives through the call.
            unsafe { device.create_framebuffer(&create_info, None) }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

fn create_render_pass(device: &Device, format: vk::Format) -> Result<vk::RenderPass, vk::Result> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
    let color_attachment_ref = vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(std::slice::from_ref(&color_attachment_ref));
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(std::slice::from_ref(&color_attachment))
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(std::slice::from_ref(&dependency));
    // SAFETY: The render-pass create info references only stack data for the duration of the
    // call and describes a single-color-attachment pass compatible with the swapchain format.
    unsafe { device.create_render_pass(&create_info, None) }
}

fn create_command_pool(
    device: &Device,
    queue_family_index: u32,
) -> Result<vk::CommandPool, vk::Result> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    // SAFETY: `queue_family_index` was selected from this device and the create info references
    // only stack data for the duration of the call.
    unsafe { device.create_command_pool(&create_info, None) }
}

fn create_command_buffers(
    device: &Device,
    pool: vk::CommandPool,
    count: usize,
) -> Result<Vec<vk::CommandBuffer>, vk::Result> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count as u32);
    // SAFETY: `pool` belongs to `device` and remains alive for the returned command buffers.
    unsafe { device.allocate_command_buffers(&alloc_info) }
}

fn create_shader_module(device: &Device, code: &[u8]) -> Result<vk::ShaderModule, vk::Result> {
    let code_u32 = ash::util::read_spv(&mut std::io::Cursor::new(code)).unwrap();
    let create_info = vk::ShaderModuleCreateInfo::default().code(&code_u32);
    // SAFETY: `code_u32` contains SPIR-V words parsed from `code`, and the create info borrows
    // that slice only for the duration of the call.
    unsafe { device.create_shader_module(&create_info, None) }
}

fn create_sync_objects(
    device: &Device,
) -> Result<(Vec<vk::Semaphore>, Vec<vk::Semaphore>, Vec<vk::Fence>), vk::Result> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
    let mut image_available = vec![];
    let mut render_finished = vec![];
    let mut in_flight_fences = vec![];
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        // SAFETY: These synchronization primitives are created on a live device and are later
        // destroyed during renderer cleanup after the device is idle.
        image_available.push(unsafe { device.create_semaphore(&semaphore_info, None)? });
        // SAFETY: These synchronization primitives are created on a live device and are later
        // destroyed during renderer cleanup after the device is idle.
        render_finished.push(unsafe { device.create_semaphore(&semaphore_info, None)? });
        // SAFETY: These synchronization primitives are created on a live device and are later
        // destroyed during renderer cleanup after the device is idle.
        in_flight_fences.push(unsafe { device.create_fence(&fence_info, None)? });
    }
    Ok((image_available, render_finished, in_flight_fences))
}

fn cleanup_swapchain_and_dependents(state: &mut State) {
    // SAFETY: Callers ensure the device is idle, or otherwise that no in-flight work still
    // references these framebuffers, image views, or the swapchain before destruction.
    unsafe {
        for &framebuffer in &state.swapchain_resources.framebuffers {
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_framebuffer(framebuffer, None);
        }
        for &view in &state.swapchain_resources.image_views {
            state
                .device
                .as_ref()
                .unwrap()
                .destroy_image_view(view, None);
        }
        state
            .swapchain_resources
            .swapchain_loader
            .destroy_swapchain(state.swapchain_resources.swapchain, None);
    }
}

fn recreate_swapchain_and_dependents(state: &mut State) -> Result<(), Box<dyn Error>> {
    debug!("Recreating swapchain...");
    let device = state.device.as_ref().unwrap();

    // Some platforms (notably under certain compositors or when minimized)
    // can temporarily report a surface with all extents set to zero. In that
    // state, any attempt to create a swapchain is invalid (imageExtent must be
    // within [minImageExtent, maxImageExtent], which would both be 0x0).
    // Instead of hammering vkCreateSwapchainKHR with illegal extents, mark the
    // swapchain as temporarily invalid and skip recreation; we'll try again
    // once the surface reports usable extents.
    // SAFETY: This query only reads immutable surface capabilities for the selected
    // physical-device/surface pair.
    let caps = unsafe {
        state
            .surface_loader
            .get_physical_device_surface_capabilities(state.pdevice, state.surface)?
    };
    if caps.current_extent.width == 0
        && caps.current_extent.height == 0
        && caps.min_image_extent.width == 0
        && caps.min_image_extent.height == 0
        && caps.max_image_extent.width == 0
        && caps.max_image_extent.height == 0
    {
        debug!(
            "Swapchain recreation skipped: surface capabilities report all-zero extents (likely minimized)"
        );
        state.swapchain_valid = false;
        return Ok(());
    }

    state.swapchain_valid = false;

    // SAFETY: Waiting for idle ensures no queue submission still references the old swapchain
    // resources before we replace and destroy them below.
    unsafe {
        device.device_wait_idle()?;
    }

    let old_swapchain = state.swapchain_resources.swapchain;

    let new_resources = create_swapchain(
        &state.instance,
        device,
        state.pdevice,
        state.surface,
        &state.surface_loader,
        state.window_size,
        Some(old_swapchain),
        state.vsync_enabled,
        state.present_mode_policy,
    )?;

    let old = std::mem::replace(&mut state.swapchain_resources, new_resources);

    recreate_framebuffers(device, &mut state.swapchain_resources, state.render_pass)?;

    // SAFETY: The device is idle, so the old swapchain image views/framebuffers/swapchain are no
    // longer referenced by in-flight work and can be destroyed here.
    unsafe {
        for fb in old.framebuffers {
            device.destroy_framebuffer(fb, None);
        }
        for view in old.image_views {
            device.destroy_image_view(view, None);
        }
        old.swapchain_loader.destroy_swapchain(old.swapchain, None);
    }

    state.images_in_flight = vec![vk::Fence::null(); state.swapchain_resources._images.len()];
    reset_present_telemetry(state);
    state.swapchain_valid = true;
    debug!("Swapchain recreated.");
    Ok(())
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
    if state.window_size.width > 0
        && state.window_size.height > 0
        && let Err(e) = recreate_swapchain_and_dependents(state)
    {
        warn!("Failed to apply Vulkan present config update: {e}");
    }
}
