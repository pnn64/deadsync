use crate::core::gfx::{BlendMode, ObjectType, RenderList, Texture as RendererTexture};
use crate::core::space::ortho_for_window;
use ash::{
    khr::{surface, swapchain},
    vk, Device, Entry, Instance,
};
use cgmath::Matrix4;
use image::RgbaImage;
use log::{debug, error, info, warn};
use std::{collections::HashMap, error::Error, ffi, mem, sync::Arc};
use winit::{
    dpi::PhysicalSize,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
    window::Window,
};

// --- Constants ---
const MAX_FRAMES_IN_FLIGHT: usize = 3;

// --- Structs ---

#[repr(C)]
struct ProjPush {
    proj: Matrix4<f32>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InstanceData {
    // 72 bytes total
    center:     [f32; 2], // offset 0
    size:       [f32; 2], // offset 8
    rot_sin_cos:[f32; 2], // offset 16  (sin, cos)
    tint:       [f32; 4], // offset 24
    uv_scale:   [f32; 2], // offset 40
    uv_offset:  [f32; 2], // offset 48
    edge_fade:  [f32; 4], // offset 56
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
    pool: vk::DescriptorPool,
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.free_descriptor_sets(self.pool, &[self.descriptor_set]);
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

struct SwapchainResources {
    swapchain_loader: swapchain::Device,
    swapchain: vk::SwapchainKHR,
    _images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    extent: vk::Extent2D,
    format: vk::SurfaceFormatKHR,
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
    vertex_buffer: Option<BufferResource>,
    index_buffer: Option<BufferResource>,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub sampler: vk::Sampler,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available_semaphores: Vec<vk::Semaphore>,
    render_finished_semaphores: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,
    images_in_flight: Vec<vk::Fence>,
    current_frame: usize,
    window_size: PhysicalSize<u32>,
    vsync_enabled: bool,
    projection: Matrix4<f32>,
    instance_ring: Option<BufferResource>,       // one big VB for all frames
    instance_ring_ptr: *mut InstanceData,        // persistently mapped pointer
    instance_capacity_instances: usize,          // total instances across ring
    per_frame_stride_instances: usize,           // instances reserved per frame
}

// --- Main Procedural Functions ---
pub fn init(window: &Window, vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing Vulkan backend...");
    let entry = Entry::linked();
    let instance = create_instance(&entry, window)?;
    let (debug_loader, debug_messenger) = setup_debug_messenger(&entry, &instance)?;
    let surface = create_surface(&entry, &instance, window)?;
    let surface_loader = surface::Instance::new(&entry, &instance);
    let pdevice = select_physical_device(&instance, &surface_loader, surface)?;
    let (device, queue, queue_family_index) =
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
    )?;
    let render_pass =
        create_render_pass(device.as_ref().unwrap(), swapchain_resources.format.format)?;
    recreate_framebuffers(device.as_ref().unwrap(), &mut swapchain_resources, render_pass)?;

    let sampler = create_sampler(device.as_ref().unwrap())?;
    let descriptor_set_layout = create_descriptor_set_layout(device.as_ref().unwrap())?;
    let descriptor_pool = create_descriptor_pool(device.as_ref().unwrap())?;

    let PipelinePair { layout: sprite_pipeline_layout, pipe: sprite_pipeline } =
        create_sprite_pipeline(
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
        vertex_buffer: None,
        index_buffer: None,
        descriptor_set_layout,
        descriptor_pool,
        sampler,
        command_buffers,
        image_available_semaphores,
        render_finished_semaphores,
        in_flight_fences,
        images_in_flight,
        current_frame: 0,
        window_size: initial_size,
        vsync_enabled,
        projection,
        instance_ring: None,
        instance_ring_ptr: std::ptr::null_mut(),
        instance_capacity_instances: 0,
        per_frame_stride_instances: 0,
    };

    // Static unit quad buffers
    let vertices: [[f32; 4]; 4] = [
        [-0.5, -0.5, 0.0, 1.0],
        [ 0.5, -0.5, 1.0, 1.0],
        [ 0.5,  0.5, 1.0, 0.0],
        [-0.5,  0.5, 0.0, 0.0],
    ];
    let indices: [u16; 6] = [0, 1, 2, 2, 3, 0];
    
    let device_arc = device.as_ref().unwrap();
    state.vertex_buffer = Some(create_buffer(
        &state.instance, device_arc, state.pdevice, state.command_pool, state.queue,
        vk::BufferUsageFlags::VERTEX_BUFFER, vk::MemoryPropertyFlags::DEVICE_LOCAL, Some(&vertices)
    )?);
    state.index_buffer = Some(create_buffer(
        &state.instance, device_arc, state.pdevice, state.command_pool, state.queue,
        vk::BufferUsageFlags::INDEX_BUFFER, vk::MemoryPropertyFlags::DEVICE_LOCAL, Some(&indices)
    )?);

    info!("Vulkan backend initialized successfully.");
    Ok(state)
}

fn create_sampler(device: &Device) -> Result<vk::Sampler, vk::Result> {
    let sampler_info = vk::SamplerCreateInfo::default()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::REPEAT)
        .address_mode_v(vk::SamplerAddressMode::REPEAT)
        .address_mode_w(vk::SamplerAddressMode::REPEAT)
        .anisotropy_enable(false)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false)
        .compare_op(vk::CompareOp::ALWAYS);
    unsafe { device.create_sampler(&sampler_info, None) }
}

fn create_descriptor_set_layout(device: &Device) -> Result<vk::DescriptorSetLayout, vk::Result> {
    let sampler_layout_binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT);

    let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
        .bindings(std::slice::from_ref(&sampler_layout_binding));

    unsafe { device.create_descriptor_set_layout(&layout_info, None) }
}

fn create_descriptor_pool(device: &Device) -> Result<vk::DescriptorPool, vk::Result> {
    let pool_size = vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(100);

    let pool_info = vk::DescriptorPoolCreateInfo::default()
        .pool_sizes(std::slice::from_ref(&pool_size))
        .max_sets(100)
        .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);

    unsafe { device.create_descriptor_pool(&pool_info, None) }
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
    let main_name = ffi::CStr::from_bytes_with_nul(b"main\0")?;

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
    let (binding_descriptions, attribute_descriptions) = vertex_input_descriptions_textured_instanced();
    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default()
        .vertex_binding_descriptions(&binding_descriptions)
        .vertex_attribute_descriptions(&attribute_descriptions);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let viewport_state = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1).scissor_count(1);

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
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::default()
        .dynamic_states(&dynamic_states);

    // Push constant: projection only
    #[repr(C)]
    struct ProjPush { proj: Matrix4<f32> }
    let push_constant_range = vk::PushConstantRange::default()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(std::mem::size_of::<ProjPush>() as u32);

    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(std::slice::from_ref(&set_layout))
        .push_constant_ranges(std::slice::from_ref(&push_constant_range));

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

    let pipe = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
            .map_err(|e| e.1)?[0]
    };

    unsafe {
        device.destroy_shader_module(vert_module, None);
        device.destroy_shader_module(frag_module, None);
    }

    Ok(PipelinePair { layout, pipe })
}

#[inline(always)]
fn next_pow2_usize(x: usize) -> usize {
    let mut v = if x == 0 { 1 } else { x - 1 };
    v |= v >> 1; v |= v >> 2; v |= v >> 4; v |= v >> 8; v |= v >> 16;
    if std::mem::size_of::<usize>() == 8 { v |= v >> 32; }
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
        // Safety: the old ring buffer may be referenced by in-flight command buffers.
        // Wait for the GPU to finish BEFORE destroying it.
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
            &state.instance, dev, state.pdevice,
            need_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let mapped = unsafe { dev.map_memory(mem, 0, need_bytes, vk::MemoryMapFlags::empty())? };

        state.instance_ring = Some(BufferResource { buffer: buf, memory: mem });
        state.instance_ring_ptr = mapped as *mut InstanceData;
        state.instance_capacity_instances = need_total_instances;
        state.per_frame_stride_instances = stride;
    } else if state.per_frame_stride_instances != stride {
        // We decided to grow-only; keep the bigger existing stride.
        state.per_frame_stride_instances = stride;
    }

    // Base "firstInstance" for this frame’s slice of the ring.
    Ok((state.current_frame * state.per_frame_stride_instances) as u32)
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

    unsafe {
        device.cmd_pipeline_barrier(cmd, src_stage, dst_stage, vk::DependencyFlags::empty(), &[], &[], &[barrier]);
    }
}

pub fn create_texture(state: &mut State, image: &RgbaImage) -> Result<Texture, Box<dyn Error>> {
    let device_arc = state.device.as_ref().unwrap().clone();
    let device = device_arc.as_ref();

    let (width, height) = image.dimensions();
    let image_data = image.as_raw();

    let staging = create_buffer(
        &state.instance, device, state.pdevice, state.command_pool, state.queue,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        Some(image_data),
    )?;

    let fmt = vk::Format::R8G8B8A8_UNORM;
    let (tex_image, tex_mem) = create_image(
        state, width, height, fmt, vk::ImageTiling::OPTIMAL,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;

    let cmd = begin_single_time_commands(device, state.command_pool)?;

    transition_image_layout_cmd(device, cmd, tex_image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL);

    let region = vk::BufferImageCopy::default()
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_extent(vk::Extent3D { width, height, depth: 1 });

    unsafe {
        device.cmd_copy_buffer_to_image(cmd, staging.buffer, tex_image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, &[region]);
    }

    transition_image_layout_cmd(device, cmd, tex_image, vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

    end_single_time_commands(device, state.command_pool, state.queue, cmd)?;

    destroy_buffer(device, &staging);
    let view = create_image_view(device, tex_image, fmt)?;
    let set  = create_texture_descriptor_set(state, view, state.sampler)?;

    Ok(Texture {
        device: device_arc.clone(),
        image: tex_image,
        memory: tex_mem,
        view,
        descriptor_set: set,
        pool: state.descriptor_pool,
    })
}

#[inline(always)]
unsafe fn bytes_of<T>(v: &T) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            (v as *const T) as *const u8,
            std::mem::size_of::<T>(),
        )
    }
}

pub fn draw(
    state: &mut State,
    render_list: &RenderList,
    textures: &HashMap<String, RendererTexture>,
) -> Result<u32, Box<dyn Error>> {
    if state.window_size.width == 0 || state.window_size.height == 0 {
        return Ok(0);
    }

    #[inline(always)]
    fn decompose_2d(m: [[f32; 4]; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
        let center = [m[3][0], m[3][1]];
        let c0 = [m[0][0], m[0][1]];
        let c1 = [m[1][0], m[1][1]];
        let sx = (c0[0]*c0[0] + c0[1]*c0[1]).sqrt().max(1e-12);
        let sy = (c1[0]*c1[0] + c1[1]*c1[1]).sqrt().max(1e-12);
        let cos_t = c0[0] / sx;
        let sin_t = c0[1] / sx;
        (center, [sx, sy], [sin_t, cos_t])
    }

    let needed_instances = render_list.objects.iter().filter(|o| {
        matches!(&o.object_type, ObjectType::Sprite { .. })
    }).count();

    if needed_instances == 0 {
        unsafe {
            let device = state.device.as_ref().unwrap();
            let fence = state.in_flight_fences[state.current_frame];
            device.wait_for_fences(&[fence], true, u64::MAX)?;

            let (image_index, acquired_suboptimal) =
                match state.swapchain_resources.swapchain_loader.acquire_next_image(
                    state.swapchain_resources.swapchain,
                    u64::MAX,
                    state.image_available_semaphores[state.current_frame],
                    vk::Fence::null(),
                ) {
                    Ok(pair) => pair,
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => { recreate_swapchain_and_dependents(state)?; return Ok(0); }
                    Err(e) => return Err(e.into()),
                };

            let in_flight = state.images_in_flight[image_index as usize];
            if in_flight != vk::Fence::null() {
                device.wait_for_fences(&[in_flight], true, u64::MAX)?;
            }
            state.images_in_flight[image_index as usize] = fence;

            device.reset_fences(&[fence])?;
            let cmd = state.command_buffers[state.current_frame];
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;

            device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;

            let c = render_list.clear_color;
            let clear_value = vk::ClearValue { color: vk::ClearColorValue { float32: [c[0], c[1], c[2], c[3]] } };
            let rp_info = vk::RenderPassBeginInfo::default()
                .render_pass(state.render_pass)
                .framebuffer(state.swapchain_resources.framebuffers[image_index as usize])
                .render_area(vk::Rect2D { offset: vk::Offset2D::default(), extent: state.swapchain_resources.extent })
                .clear_values(std::slice::from_ref(&clear_value));
            device.cmd_begin_render_pass(cmd, &rp_info, vk::SubpassContents::INLINE);
            device.cmd_end_render_pass(cmd);
            device.end_command_buffer(cmd)?;

            let wait = [state.image_available_semaphores[state.current_frame]];
            let sig  = [state.render_finished_semaphores[state.current_frame]];
            let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait).wait_dst_stage_mask(&stages)
                .command_buffers(std::slice::from_ref(&cmd)).signal_semaphores(&sig);
            device.queue_submit(state.queue, &[submit], fence)?;

            let present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&sig)
                .swapchains(std::slice::from_ref(&state.swapchain_resources.swapchain))
                .image_indices(std::slice::from_ref(&image_index));

            match state.swapchain_resources.swapchain_loader.queue_present(state.queue, &present_info) {
                Ok(suboptimal) if suboptimal || acquired_suboptimal => recreate_swapchain_and_dependents(state)?,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => recreate_swapchain_and_dependents(state)?,
                Ok(_) => {},
                Err(e) => return Err(e.into()),
            }
            state.current_frame = (state.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        }
        return Ok(0);
    }

    let base_first_instance = ensure_instance_ring_capacity(state, needed_instances)?;
    struct Run { set: vk::DescriptorSet, start: u32, count: u32 }

    let mut runs: Vec<Run> = Vec::new();
    let mut written: u32 = 0;

    unsafe {
        let dst_base = state.instance_ring_ptr.add(base_first_instance as usize);
        let mut last_set = vk::DescriptorSet::null();

        for obj in &render_list.objects {
            let (texture_id, tint, uv_scale, uv_offset, edge_fade) = match &obj.object_type {
                ObjectType::Sprite { texture_id, tint, uv_scale, uv_offset, edge_fade } => {
                    (texture_id, tint, uv_scale, uv_offset, edge_fade)
                }
            };

            let set_opt = textures.get(texture_id).and_then(|t| {
                if let RendererTexture::Vulkan(tex) = t {
                    Some(tex.descriptor_set)
                } else {
                    None
                }
            });
            let set = match set_opt { Some(s) => s, None => continue };

            let model: [[f32;4];4] = obj.transform.into();
            let (center, size, sincos) = decompose_2d(model);
            let dst_ptr = dst_base.add(written as usize);
            std::ptr::write(dst_ptr, InstanceData { center, size, rot_sin_cos: sincos, tint: *tint, uv_scale: *uv_scale, uv_offset: *uv_offset, edge_fade: *edge_fade });

            if runs.is_empty() || set != last_set {
                runs.push(Run { set, start: written, count: 1 });
                last_set = set;
            } else {
                if let Some(r) = runs.last_mut() {
                    r.count += 1;
                }
            }
            written += 1;
        }

        if written == 0 {
            // ... (clear-only path, same as above)
            return Ok(0);
        }

        let device_arc = state.device.as_ref().unwrap().clone();
        let device = device_arc.as_ref();

        let fence = state.in_flight_fences[state.current_frame];
        device.wait_for_fences(&[fence], true, u64::MAX)?;

        let (image_index, acquired_suboptimal) =
            match state.swapchain_resources.swapchain_loader.acquire_next_image(
                state.swapchain_resources.swapchain,
                u64::MAX,
                state.image_available_semaphores[state.current_frame],
                vk::Fence::null(),
            ) {
                Ok(pair) => pair,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => { recreate_swapchain_and_dependents(state)?; return Ok(0); }
                Err(e) => return Err(e.into()),
            };

        let in_flight = state.images_in_flight[image_index as usize];
        if in_flight != vk::Fence::null() {
            device.wait_for_fences(&[in_flight], true, u64::MAX)?;
        }
        state.images_in_flight[image_index as usize] = fence;

        device.reset_fences(&[fence])?;
        let cmd = state.command_buffers[state.current_frame];
        device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT))?;

        let c = render_list.clear_color;
        let clear_value = vk::ClearValue { color: vk::ClearColorValue { float32: [c[0], c[1], c[2], c[3]] } };
        let rp_info = vk::RenderPassBeginInfo::default()
            .render_pass(state.render_pass)
            .framebuffer(state.swapchain_resources.framebuffers[image_index as usize])
            .render_area(vk::Rect2D { offset: vk::Offset2D::default(), extent: state.swapchain_resources.extent })
            .clear_values(std::slice::from_ref(&clear_value));
        device.cmd_begin_render_pass(cmd, &rp_info, vk::SubpassContents::INLINE);

        let vp = vk::Viewport { x: 0.0, y: state.swapchain_resources.extent.height as f32, width: state.swapchain_resources.extent.width as f32, height: -(state.swapchain_resources.extent.height as f32), min_depth: 0.0, max_depth: 1.0, };
        device.cmd_set_viewport(cmd, 0, &[vp]);
        let sc = vk::Rect2D { offset: vk::Offset2D::default(), extent: state.swapchain_resources.extent };
        device.cmd_set_scissor(cmd, 0, &[sc]);

        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, state.sprite_pipeline);
        let pc = ProjPush { proj: state.projection };
        device.cmd_push_constants(cmd, state.sprite_pipeline_layout, vk::ShaderStageFlags::VERTEX, 0, bytes_of(&pc));
        let vb0 = state.vertex_buffer.as_ref().unwrap().buffer;
        let inst_buf = state.instance_ring.as_ref().unwrap().buffer;
        device.cmd_bind_vertex_buffers(cmd, 0, &[vb0, inst_buf], &[0, 0]);
        let ib = state.index_buffer.as_ref().unwrap().buffer;
        device.cmd_bind_index_buffer(cmd, ib, 0, vk::IndexType::UINT16);

        let mut last_set = vk::DescriptorSet::null();
        let mut vertices_drawn: u32 = 0;
        for run in runs {
            if last_set != run.set {
                device.cmd_bind_descriptor_sets(cmd, vk::PipelineBindPoint::GRAPHICS, state.sprite_pipeline_layout, 0, &[run.set], &[]);
                last_set = run.set;
            }
            let first_instance = base_first_instance + run.start;
            device.cmd_draw_indexed(cmd, 6, run.count, 0, 0, first_instance);
            vertices_drawn = vertices_drawn.saturating_add(4 * run.count);
        }

        device.cmd_end_render_pass(cmd);
        device.end_command_buffer(cmd)?;

        let wait = [state.image_available_semaphores[state.current_frame]];
        let sig  = [state.render_finished_semaphores[state.current_frame]];
        let stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let submit = vk::SubmitInfo::default().wait_semaphores(&wait).wait_dst_stage_mask(&stages).command_buffers(std::slice::from_ref(&cmd)).signal_semaphores(&sig);
        device.queue_submit(state.queue, &[submit], fence)?;

        let present_info = vk::PresentInfoKHR::default().wait_semaphores(&sig).swapchains(std::slice::from_ref(&state.swapchain_resources.swapchain)).image_indices(std::slice::from_ref(&image_index));
        match state.swapchain_resources.swapchain_loader.queue_present(state.queue, &present_info) {
            Ok(suboptimal) if suboptimal || acquired_suboptimal => recreate_swapchain_and_dependents(state)?,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR | vk::Result::SUBOPTIMAL_KHR) => recreate_swapchain_and_dependents(state)?,
            Ok(_) => {},
            Err(e) => return Err(e.into()),
        }

        state.current_frame = (state.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        Ok(vertices_drawn)
    }
}

pub fn cleanup(state: &mut State) {
    info!("Cleaning up Vulkan resources...");
    unsafe {
        if let Some(device) = &state.device {
            let _ = device.device_wait_idle();
        }
    }

    unsafe {
        cleanup_swapchain_and_dependents(state);

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            state.device.as_ref().unwrap().destroy_semaphore(state.render_finished_semaphores[i], None);
            state.device.as_ref().unwrap().destroy_semaphore(state.image_available_semaphores[i], None);
            state.device.as_ref().unwrap().destroy_fence(state.in_flight_fences[i], None);
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

        state.device.as_ref().unwrap().destroy_sampler(state.sampler, None);
        state.device.as_ref().unwrap().destroy_descriptor_pool(state.descriptor_pool, None);
        state.device.as_ref().unwrap().destroy_descriptor_set_layout(state.descriptor_set_layout, None);
        state.device.as_ref().unwrap().destroy_pipeline(state.sprite_pipeline, None);
        state.device.as_ref().unwrap().destroy_pipeline_layout(state.sprite_pipeline_layout, None);
        state.device.as_ref().unwrap().destroy_render_pass(state.render_pass, None);
        state.device.as_ref().unwrap().destroy_command_pool(state.command_pool, None);
        state.surface_loader.destroy_surface(state.surface, None);

        if let (Some(loader), Some(messenger)) = (state.debug_loader.take(), state.debug_messenger.take()) {
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
    info!("Vulkan resize requested to {}x{}", width, height);
    state.window_size = PhysicalSize::new(width, height);
    if width > 0 && height > 0 {
        state.projection = ortho_for_window(width, height);
        if let Err(e) = recreate_swapchain_and_dependents(state) {
            error!("Failed to recreate swapchain: {}", e);
        }
    }
}

// --- ALL HELPER FUNCTIONS ---

fn create_image_view(device: &Device, image: vk::Image, format: vk::Format) -> Result<vk::ImageView, vk::Result> {
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
    unsafe { device.create_image_view(&view_info, None) }
}

fn create_image(
    state: &State, width: u32, height: u32, format: vk::Format, tiling: vk::ImageTiling,
    usage: vk::ImageUsageFlags, properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Image, vk::DeviceMemory), vk::Result> {
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .extent(vk::Extent3D { width, height, depth: 1 })
        .mip_levels(1)
        .array_layers(1)
        .format(format)
        .tiling(tiling)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(usage)
        .samples(vk::SampleCountFlags::TYPE_1)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    unsafe {
        let image = state.device.as_ref().unwrap().create_image(&image_info, None)?;
        let mem_requirements = state.device.as_ref().unwrap().get_image_memory_requirements(image);
        let mem_type_index = find_memory_type(&state.instance, state.pdevice, mem_requirements.memory_type_bits, properties);
        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_requirements.size)
            .memory_type_index(mem_type_index);
        let memory = state.device.as_ref().unwrap().allocate_memory(&alloc_info, None)?;
        state.device.as_ref().unwrap().bind_image_memory(image, memory, 0)?;
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

fn create_texture_descriptor_set(
    state: &State,
    texture_image_view: vk::ImageView,
    sampler: vk::Sampler,
) -> Result<vk::DescriptorSet, vk::Result> {
    let layouts = [state.descriptor_set_layout];
    let alloc_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(state.descriptor_pool)
        .set_layouts(&layouts);
    let descriptor_set = unsafe { state.device.as_ref().unwrap().allocate_descriptor_sets(&alloc_info)?[0] };

    let image_info = vk::DescriptorImageInfo::default()
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .image_view(texture_image_view)
        .sampler(sampler);

    let descriptor_write = vk::WriteDescriptorSet::default()
        .dst_set(descriptor_set)
        .dst_binding(0)
        .dst_array_element(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(std::slice::from_ref(&image_info));

    unsafe {
        state.device.as_ref().unwrap().update_descriptor_sets(&[descriptor_write], &[]);
    }
    Ok(descriptor_set)
}

#[inline(always)]
fn vertex_input_descriptions_textured_instanced() -> (
    [vk::VertexInputBindingDescription; 2],
    [vk::VertexInputAttributeDescription; 9],
) {
    // binding 0: unit quad [x,y,u,v]
    let b0 = vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(std::mem::size_of::<[f32; 4]>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX);

    // binding 1: compact per-instance payload
    let b1 = vk::VertexInputBindingDescription::default()
        .binding(1)
        .stride(std::mem::size_of::<InstanceData>() as u32) // 72
        .input_rate(vk::VertexInputRate::INSTANCE);

    // per-vertex
    let a0 = vk::VertexInputAttributeDescription::default()
        .binding(0).location(0).format(vk::Format::R32G32_SFLOAT).offset(0);  // pos
    let a1 = vk::VertexInputAttributeDescription::default()
        .binding(0).location(1).format(vk::Format::R32G32_SFLOAT).offset(8);  // uv

    // per-instance
    let i_center = vk::VertexInputAttributeDescription::default()
        .binding(1).location(2).format(vk::Format::R32G32_SFLOAT).offset(0);
    let i_size = vk::VertexInputAttributeDescription::default()
        .binding(1).location(3).format(vk::Format::R32G32_SFLOAT).offset(8);
    let i_rot = vk::VertexInputAttributeDescription::default()
        .binding(1).location(4).format(vk::Format::R32G32_SFLOAT).offset(16);
    let i_tint = vk::VertexInputAttributeDescription::default()
        .binding(1).location(5).format(vk::Format::R32G32B32A32_SFLOAT).offset(24);
    let i_uvs = vk::VertexInputAttributeDescription::default()
        .binding(1).location(6).format(vk::Format::R32G32_SFLOAT).offset(40);
    let i_uvo = vk::VertexInputAttributeDescription::default()
        .binding(1).location(7).format(vk::Format::R32G32_SFLOAT).offset(48);
    let i_fade = vk::VertexInputAttributeDescription::default()
        .binding(1).location(8).format(vk::Format::R32G32B32A32_SFLOAT).offset(56);

    ([b0, b1], [a0, a1, i_center, i_size, i_rot, i_tint, i_uvs, i_uvo, i_fade])
}

fn begin_single_time_commands(device: &Device, pool: vk::CommandPool) -> Result<vk::CommandBuffer, vk::Result> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(pool)
        .command_buffer_count(1);
    let cmd = unsafe { device.allocate_command_buffers(&alloc_info)?[0] };
    let begin_info = vk::CommandBufferBeginInfo::default()
        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
    unsafe {
        device.begin_command_buffer(cmd, &begin_info)?;
    }
    Ok(cmd)
}

fn end_single_time_commands(device: &Device, pool: vk::CommandPool, queue: vk::Queue, command_buffer: vk::CommandBuffer) -> Result<(), Box<dyn Error>> {
    unsafe {
        device.end_command_buffer(command_buffer)?;
        let submit_info = vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&command_buffer));
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
    let buffer_size = (mem::size_of::<T>() * data.map_or(1, |d| d.len())) as vk::DeviceSize;

    if let Some(slice) = data {
        let staging_usage = vk::BufferUsageFlags::TRANSFER_SRC;
        let staging_props = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let (staging_buffer, staging_memory) = create_gpu_buffer(instance, device, pdevice, buffer_size, staging_usage, staging_props)?;
        
        unsafe {
            let mapped = device.map_memory(staging_memory, 0, buffer_size, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(slice.as_ptr(), mapped as *mut T, slice.len());
            device.unmap_memory(staging_memory);
        }

        let final_usage = usage | vk::BufferUsageFlags::TRANSFER_DST;
        let (device_buffer, device_memory) = create_gpu_buffer(instance, device, pdevice, buffer_size, final_usage, vk::MemoryPropertyFlags::DEVICE_LOCAL)?;
        
        copy_buffer(device, pool, queue, staging_buffer, device_buffer, buffer_size)?;

        unsafe {
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_memory, None);
        }
        
        Ok(BufferResource { buffer: device_buffer, memory: device_memory })
    } else {
        let (buffer, memory) = create_gpu_buffer(instance, device, pdevice, buffer_size, usage, properties)?;
        Ok(BufferResource { buffer, memory })
    }
}


fn copy_buffer(
    device: &Device, pool: vk::CommandPool, queue: vk::Queue,
    src: vk::Buffer, dst: vk::Buffer, size: vk::DeviceSize,
) -> Result<(), Box<dyn Error>> {
    let cmd = begin_single_time_commands(device, pool)?;
    unsafe {
        let region = vk::BufferCopy::default().size(size);
        device.cmd_copy_buffer(cmd, src, dst, &[region]);
    }
    end_single_time_commands(device, pool, queue, cmd)?;
    Ok(())
}

fn create_gpu_buffer(
    instance: &Instance, device: &Device, pdevice: vk::PhysicalDevice, size: vk::DeviceSize,
    usage: vk::BufferUsageFlags, properties: vk::MemoryPropertyFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory), Box<dyn Error>> {
    let buffer_info = vk::BufferCreateInfo::default()
        .size(size)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&buffer_info, None)? };
    let mem_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let mem_type_index = find_memory_type(instance, pdevice, mem_requirements.memory_type_bits, properties);
    let alloc_info = vk::MemoryAllocateInfo::default()
        .allocation_size(mem_requirements.size)
        .memory_type_index(mem_type_index);
    let memory = unsafe { device.allocate_memory(&alloc_info, None)? };
    unsafe { device.bind_buffer_memory(buffer, memory, 0)? };
    Ok((buffer, memory))
}

fn destroy_buffer(device: &Device, buffer: &BufferResource) {
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn find_memory_type(
    instance: &Instance, pdevice: vk::PhysicalDevice, type_filter: u32,
    properties: vk::MemoryPropertyFlags,
) -> u32 {
    let mem_properties = unsafe { instance.get_physical_device_memory_properties(pdevice) };
    (0..mem_properties.memory_type_count)
        .find(|i| {
            let i_usize = *i as usize;
            (type_filter & (1 << i)) != 0
                && (mem_properties.memory_types[i_usize].property_flags & properties) == properties
        })
        .expect("Failed to find suitable memory type!")
}

fn create_instance(entry: &Entry, window: &Window) -> Result<Instance, Box<dyn Error>> {
    let app_name = ffi::CStr::from_bytes_with_nul(b"DeadSync\0")?;
    let app_info = vk::ApplicationInfo::default()
        .application_name(app_name)
        .application_version(vk::make_api_version(0, 1, 0, 0))
        .engine_name(ffi::CStr::from_bytes_with_nul(b"DeadSync Engine\0")?)
        .engine_version(vk::make_api_version(0, 1, 0, 0))
        .api_version(vk::API_VERSION_1_3);

    let mut extension_names = ash_window::enumerate_required_extensions(window.display_handle()?.as_raw())?.to_vec();
    if cfg!(debug_assertions) {
        extension_names.push(ash::ext::debug_utils::NAME.as_ptr());
    }

    let layers_names_raw: Vec<*const ffi::c_char> = if cfg!(debug_assertions) {
        vec![ffi::CStr::from_bytes_with_nul(b"VK_LAYER_KHRONOS_validation\0")?.as_ptr()]
    } else {
        vec![]
    };

    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extension_names)
        .enabled_layer_names(&layers_names_raw);

    unsafe { Ok(entry.create_instance(&create_info, None)?) }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut ffi::c_void,
) -> vk::Bool32 {
    let message = unsafe { ffi::CStr::from_ptr((*p_callback_data).p_message) };
    let severity = format!("{:?}", message_severity).to_lowercase();
    let ty = format!("{:?}", message_type).to_lowercase();
    let log_message = format!("[vulkan_{}_{}] {}", severity, ty, message.to_string_lossy());

    match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => error!("{}", log_message),
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => warn!("{}", log_message),
        _ => debug!("{}", log_message),
    }
    vk::FALSE
}

fn setup_debug_messenger(
    entry: &Entry,
    instance: &Instance,
) -> Result<(Option<ash::ext::debug_utils::Instance>, Option<vk::DebugUtilsMessengerEXT>), vk::Result> {
    if !cfg!(debug_assertions) { return Ok((None, None)); }
    let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::WARNING | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR)
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(vulkan_debug_callback));
    let loader = ash::ext::debug_utils::Instance::new(entry, instance);
    let messenger = unsafe { loader.create_debug_utils_messenger(&create_info, None)? };
    Ok((Some(loader), Some(messenger)))
}

fn create_surface(
    entry: &Entry,
    instance: &Instance,
    window: &Window,
) -> Result<vk::SurfaceKHR, Box<dyn Error>> {
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
    let pdevices = unsafe { instance.enumerate_physical_devices()? };
    pdevices
        .into_iter()
        .find(|pdevice| is_device_suitable(instance, *pdevice, surface_loader, surface))
        .ok_or_else(|| "Failed to find a suitable GPU!".into())
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
    let queue_families = unsafe { instance.get_physical_device_queue_family_properties(pdevice) };
    queue_families.iter().enumerate().find_map(|(i, family)| {
        if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
            && unsafe { surface_loader.get_physical_device_surface_support(pdevice, i as u32, surface).unwrap_or(false) }
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
) -> Result<(Device, vk::Queue, u32), Box<dyn Error>> {
    let queue_family_index = find_queue_family(instance, pdevice, surface_loader, surface)
        .ok_or("No suitable queue family found")?;
    let queue_priorities = [1.0];
    let queue_create_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family_index)
        .queue_priorities(&queue_priorities);
    let device_extensions = [swapchain::NAME.as_ptr()];
    let features = vk::PhysicalDeviceFeatures::default();
    let create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(std::slice::from_ref(&queue_create_info))
        .enabled_extension_names(&device_extensions)
        .enabled_features(&features);

    let device = unsafe { instance.create_device(pdevice, &create_info, None)? };
    let queue = unsafe { device.get_device_queue(queue_family_index, 0) };
    Ok((device, queue, queue_family_index))
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
) -> Result<SwapchainResources, Box<dyn Error>> {
    let capabilities = unsafe { surface_loader.get_physical_device_surface_capabilities(pdevice, surface)? };
    let formats = unsafe { surface_loader.get_physical_device_surface_formats(pdevice, surface)? };
    let present_modes = unsafe { surface_loader.get_physical_device_surface_present_modes(pdevice, surface)? };

    let format = formats.iter().find(|f| f.format == vk::Format::B8G8R8A8_UNORM)
        .cloned()
        .unwrap_or(formats[0]);
    
    let present_mode = if vsync_enabled {
        vk::PresentModeKHR::FIFO
    } else if present_modes.contains(&vk::PresentModeKHR::MAILBOX) {
        vk::PresentModeKHR::MAILBOX
    } else if present_modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
        vk::PresentModeKHR::IMMEDIATE
    } else {
        vk::PresentModeKHR::FIFO
    };

    let desired_images =
        if present_mode == vk::PresentModeKHR::MAILBOX { 3 } else { capabilities.min_image_count + 1 };

    let image_count = match capabilities.max_image_count {
        0 => desired_images,
        max => desired_images.min(max),
    };

    let extent = if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        vk::Extent2D {
            width: window_size.width.clamp(capabilities.min_image_extent.width, capabilities.max_image_extent.width),
            height: window_size.height.clamp(capabilities.min_image_extent.height, capabilities.max_image_extent.height),
        }
    };

    let create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface).min_image_count(image_count).image_format(format.format)
        .image_color_space(format.color_space).image_extent(extent).image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT).image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(capabilities.current_transform).composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode).clipped(true).old_swapchain(old_swapchain.unwrap_or(vk::SwapchainKHR::null()));

    let swapchain_loader = swapchain::Device::new(instance, device);
    let swapchain = unsafe { swapchain_loader.create_swapchain(&create_info, None)? };
    let images = unsafe { swapchain_loader.get_swapchain_images(swapchain)? };
    let image_views = images.iter().map(|&image| create_image_view(device, image, format.format)).collect::<Result<Vec<_>, _>>()?;

    Ok(SwapchainResources { swapchain_loader, swapchain, _images: images, image_views, framebuffers: vec![], extent, format })
}

fn recreate_framebuffers(
    device: &Device,
    swapchain_resources: &mut SwapchainResources,
    render_pass: vk::RenderPass,
) -> Result<(), vk::Result> {
    swapchain_resources.framebuffers = swapchain_resources.image_views.iter().map(|view| {
        let attachments = [*view];
        let create_info = vk::FramebufferCreateInfo::default()
            .render_pass(render_pass)
            .attachments(&attachments)
            .width(swapchain_resources.extent.width)
            .height(swapchain_resources.extent.height)
            .layers(1);
        unsafe { device.create_framebuffer(&create_info, None) }
    }).collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

fn create_render_pass(device: &Device, format: vk::Format) -> Result<vk::RenderPass, vk::Result> {
    let color_attachment = vk::AttachmentDescription::default()
        .format(format).samples(vk::SampleCountFlags::TYPE_1).load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE).stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE).initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);
    let color_attachment_ref = vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(std::slice::from_ref(&color_attachment_ref));
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL).dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);
    let create_info = vk::RenderPassCreateInfo::default()
        .attachments(std::slice::from_ref(&color_attachment))
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(std::slice::from_ref(&dependency));
    unsafe { device.create_render_pass(&create_info, None) }
}

fn create_command_pool(device: &Device, queue_family_index: u32) -> Result<vk::CommandPool, vk::Result> {
    let create_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family_index)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    unsafe { device.create_command_pool(&create_info, None) }
}

fn create_command_buffers(device: &Device, pool: vk::CommandPool, count: usize) -> Result<Vec<vk::CommandBuffer>, vk::Result> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(count as u32);
    unsafe { device.allocate_command_buffers(&alloc_info) }
}

fn create_shader_module(device: &Device, code: &[u8]) -> Result<vk::ShaderModule, vk::Result> {
    let code_u32 = ash::util::read_spv(&mut std::io::Cursor::new(code)).unwrap();
    let create_info = vk::ShaderModuleCreateInfo::default().code(&code_u32);
    unsafe { device.create_shader_module(&create_info, None) }
}

fn create_sync_objects(device: &Device) -> Result<(Vec<vk::Semaphore>, Vec<vk::Semaphore>, Vec<vk::Fence>), vk::Result> {
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
    let mut image_available = vec![];
    let mut render_finished = vec![];
    let mut in_flight_fences = vec![];
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        image_available.push(unsafe { device.create_semaphore(&semaphore_info, None)? });
        render_finished.push(unsafe { device.create_semaphore(&semaphore_info, None)? });
        in_flight_fences.push(unsafe { device.create_fence(&fence_info, None)? });
    }
    Ok((image_available, render_finished, in_flight_fences))
}

fn cleanup_swapchain_and_dependents(state: &mut State) {
    unsafe {
        for &framebuffer in &state.swapchain_resources.framebuffers {
            state.device.as_ref().unwrap().destroy_framebuffer(framebuffer, None);
        }
        for &view in &state.swapchain_resources.image_views {
            state.device.as_ref().unwrap().destroy_image_view(view, None);
        }
        state.swapchain_resources.swapchain_loader.destroy_swapchain(state.swapchain_resources.swapchain, None);
    }
}

fn recreate_swapchain_and_dependents(state: &mut State) -> Result<(), Box<dyn Error>> {
    debug!("Recreating swapchain...");
    let device = state.device.as_ref().unwrap();

    unsafe { device.device_wait_idle()?; }

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
    )?;

    let old = std::mem::replace(&mut state.swapchain_resources, new_resources);

    recreate_framebuffers(device, &mut state.swapchain_resources, state.render_pass)?;

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
    debug!("Swapchain recreated.");
    Ok(())
}
