#![cfg(target_os = "macos")]

use core_graphics_types::geometry::CGSize;
use deadlib_render::{
    BlendMode, ClockDomainTrace, DrawOp, DrawStats, FastU64Map, MeshVertex, PresentModePolicy,
    PresentModeTrace, PresentStats, RenderFrame, SamplerDesc, SamplerFilter, SamplerWrap,
    SpriteInstanceRaw, TMeshCacheKey, TextureHandle, TexturedMeshBufferCache,
    TexturedMeshInstanceRaw, TexturedMeshUploads, TexturedMeshVertex, draw_storage_stats,
    resolve_textured_meshes,
};
use foreign_types::ForeignType;
use glam::Mat4 as Matrix4;
use image::RgbaImage;
use log::{debug, info, warn};
use metal::*;
use objc::{
    msg_send,
    rc::autoreleasepool,
    runtime::{Object, YES},
    sel, sel_impl,
};
use std::{borrow::Cow, error::Error, mem, ptr, sync::Arc, time::Instant};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

const FRAMES_IN_FLIGHT: usize = 3;
const IMAGE_WAIT_THRESHOLD_US: u32 = 1_000;
const BACK_PRESSURE_THRESHOLD_US: u32 = 1_000;
const TMESH_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;
const LOGICAL_HEIGHT: f32 = 480.0;
const DESIGN_WIDTH_16_9: f32 = 854.0;
const COLOR_FORMAT: MTLPixelFormat = MTLPixelFormat::BGRA8Unorm;
const DEPTH_FORMAT: MTLPixelFormat = MTLPixelFormat::Depth32Float;
const SHADER: &str = include_str!("shaders/renderer.metal");

const _: () = assert!(mem::size_of::<SpriteInstanceRaw>() == 100);
const _: () = assert!(mem::size_of::<MeshVertex>() == 24);
const _: () = assert!(mem::size_of::<TexturedMeshVertex>() == 44);
const _: () = assert!(mem::size_of::<TexturedMeshInstanceRaw>() == 108);

pub struct Texture {
    id: u64,
    raw: metal::Texture,
    sampler: SamplerState,
    repeat_sampler: SamplerState,
    mipmaps: bool,
}

pub trait TextureLookup {
    fn metal_texture(&self, handle: TextureHandle) -> Option<&Texture>;
}

struct PipelineSet {
    alpha: RenderPipelineState,
    add: RenderPipelineState,
    multiply: RenderPipelineState,
    subtract: RenderPipelineState,
}

impl PipelineSet {
    #[inline(always)]
    fn get(&self, blend: BlendMode) -> &RenderPipelineStateRef {
        match blend {
            BlendMode::Alpha => &self.alpha,
            BlendMode::Add => &self.add,
            BlendMode::Multiply => &self.multiply,
            BlendMode::Subtract => &self.subtract,
        }
    }
}

struct DynamicBuffer {
    raw: Buffer,
    capacity: usize,
}

impl DynamicBuffer {
    fn new(device: &DeviceRef, capacity: usize) -> Self {
        let capacity = capacity.max(4).next_power_of_two();
        Self {
            raw: device.new_buffer(capacity as u64, MTLResourceOptions::StorageModeShared),
            capacity,
        }
    }

    fn upload<T: bytemuck::Pod>(&mut self, device: &DeviceRef, values: &[T]) {
        let bytes: &[u8] = bytemuck::cast_slice(values);
        if bytes.len() > self.capacity {
            *self = Self::new(device, bytes.len());
        }
        if !bytes.is_empty() {
            // SAFETY: Metal allocated at least `capacity` writable shared bytes and the
            // source is a valid byte view whose length was checked above.
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), self.raw.contents().cast(), bytes.len());
            }
        }
    }
}

struct FrameBuffers {
    sprites: DynamicBuffer,
    meshes: DynamicBuffer,
    tmeshes: DynamicBuffer,
    tmesh_instances: DynamicBuffer,
    command: Option<CommandBuffer>,
    submitted_id: u32,
}

impl FrameBuffers {
    fn new(device: &DeviceRef) -> Self {
        Self {
            sprites: DynamicBuffer::new(device, 64 * mem::size_of::<SpriteInstanceRaw>()),
            meshes: DynamicBuffer::new(device, 1024 * mem::size_of::<MeshVertex>()),
            tmeshes: DynamicBuffer::new(device, 1024 * mem::size_of::<TexturedMeshVertex>()),
            tmesh_instances: DynamicBuffer::new(
                device,
                64 * mem::size_of::<TexturedMeshInstanceRaw>(),
            ),
            command: None,
            submitted_id: 0,
        }
    }
}

struct CachedTMesh {
    buffer: Buffer,
    vertex_count: u32,
}

#[derive(Default)]
struct CacheStats {
    hits: u64,
    misses: u64,
    saturated: u64,
}

/// Direct Metal renderer state, owned and used only by the render thread.
///
/// The three dynamic upload sets live for the session and are reused only after
/// their command buffer completes. Retained textured-mesh geometry is warmed on
/// first draw, capped at 16 MiB, and saturates instead of pruning. A saturated
/// miss falls back to the current frame's bounded upload buffer. Cache entries
/// are freed by the render thread during cleanup; hit/miss/saturation counters
/// are logged then. Per-frame maintenance is O(draw ops + visible geometry), and
/// the only unbounded GPU wait is explicit back pressure or reuse of one of the
/// three in-flight frame slots.
pub struct State {
    window: Arc<Window>,
    device: Device,
    queue: CommandQueue,
    layer: MetalLayer,
    sprite_pipelines: PipelineSet,
    mesh_pipelines: PipelineSet,
    tmesh_pipelines: PipelineSet,
    depth_disabled: DepthStencilState,
    depth_enabled: DepthStencilState,
    depth: metal::Texture,
    frames: [FrameBuffers; FRAMES_IN_FLIGHT],
    frame_index: usize,
    window_size: (u32, u32),
    projection: Matrix4,
    uploads: TexturedMeshUploads,
    cached_tmeshes: FastU64Map<CachedTMesh>,
    cached_tmesh_bytes: usize,
    cache_stats: CacheStats,
    next_texture_id: u64,
    next_present_id: u32,
    completed_present_id: u32,
    completed_host_ns: u64,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    screenshot_requested: bool,
    captured_frame: Option<RgbaImage>,
}

pub fn init(
    window: Arc<Window>,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
    gfx_debug_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing native Metal backend...");
    let device = Device::system_default()
        .ok_or_else(|| std::io::Error::other("No Metal device is available"))?;
    info!("Using Metal device: {}", device.name());
    if gfx_debug_enabled {
        info!("Metal API validation is controlled by the MTL_DEBUG_LAYER environment variable");
    }

    let mut layer = MetalLayer::new();
    layer.set_device(&device);
    layer.set_pixel_format(COLOR_FORMAT);
    layer.set_presents_with_transaction(false);
    layer.set_framebuffer_only(false);
    layer.set_maximum_drawable_count(FRAMES_IN_FLIGHT as u64);
    layer.set_contents_scale(window.scale_factor());
    attach_layer(&window, &mut layer)?;

    let size = window.inner_size();
    let window_size = (size.width, size.height);
    set_layer_size(&layer, size.width, size.height);
    set_layer_present_config(&layer, vsync_enabled);

    let options = CompileOptions::new();
    let library = device
        .new_library_with_source(SHADER, &options)
        .map_err(std::io::Error::other)?;
    let sprite_pipelines =
        build_pipeline_set(&device, &library, "sprite_vertex", "sprite_fragment")?;
    let mesh_pipelines = build_pipeline_set(&device, &library, "mesh_vertex", "mesh_fragment")?;
    let tmesh_pipelines = build_pipeline_set(
        &device,
        &library,
        "textured_mesh_vertex",
        "textured_mesh_fragment",
    )?;
    let (depth_disabled, depth_enabled) = build_depth_states(&device);
    let depth = create_depth_target(&device, size.width, size.height);
    let queue = device.new_command_queue_with_max_command_buffer_count(FRAMES_IN_FLIGHT as u64);
    queue.set_label("DeadSync native Metal queue");
    let frames = std::array::from_fn(|_| FrameBuffers::new(&device));

    Ok(State {
        window,
        device,
        queue,
        layer,
        sprite_pipelines,
        mesh_pipelines,
        tmesh_pipelines,
        depth_disabled,
        depth_enabled,
        depth,
        frames,
        frame_index: 0,
        window_size,
        projection: ortho_for_window(size.width, size.height),
        uploads: TexturedMeshUploads::with_capacity(1024, 64),
        cached_tmeshes: FastU64Map::default(),
        cached_tmesh_bytes: 0,
        cache_stats: CacheStats::default(),
        next_texture_id: 1,
        next_present_id: 1,
        completed_present_id: 0,
        completed_host_ns: 0,
        vsync_enabled,
        present_mode_policy,
        screenshot_requested: false,
        captured_frame: None,
    })
}

pub fn create_texture(
    state: &mut State,
    image: &RgbaImage,
    sampler_desc: SamplerDesc,
) -> Result<Texture, Box<dyn Error>> {
    validate_image(image)?;
    let desc = TextureDescriptor::new();
    desc.set_texture_type(MTLTextureType::D2);
    desc.set_pixel_format(MTLPixelFormat::RGBA8Unorm);
    desc.set_width(image.width() as u64);
    desc.set_height(image.height() as u64);
    desc.set_mipmap_level_count(mip_level_count(image, sampler_desc.mipmaps));
    desc.set_storage_mode(MTLStorageMode::Private);
    desc.set_usage(MTLTextureUsage::ShaderRead);
    let raw = state.device.new_texture(&desc);
    upload_texture(&state.queue, &raw, image, sampler_desc.mipmaps);
    let sampler = create_sampler(&state.device, sampler_desc);
    let repeat_sampler = create_sampler(
        &state.device,
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..sampler_desc
        },
    );
    let id = state.next_texture_id;
    state.next_texture_id = state.next_texture_id.wrapping_add(1).max(1);
    Ok(Texture {
        id,
        raw,
        sampler,
        repeat_sampler,
        mipmaps: sampler_desc.mipmaps,
    })
}

pub fn update_texture(
    state: &mut State,
    texture: &mut Texture,
    image: &RgbaImage,
) -> Result<(), Box<dyn Error>> {
    validate_image(image)?;
    if texture.raw.width() != image.width() as u64 || texture.raw.height() != image.height() as u64
    {
        return Err(std::io::Error::other("Metal texture update dimensions do not match").into());
    }
    upload_texture(&state.queue, &texture.raw, image, texture.mipmaps);
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
    frame: &RenderFrame,
    textures: &impl TextureLookup,
    apply_present_back_pressure: bool,
) -> Result<DrawStats, Box<dyn Error>> {
    autoreleasepool(|| draw_inner(state, frame, textures, apply_present_back_pressure))
}

fn draw_inner(
    state: &mut State,
    frame: &RenderFrame,
    textures: &impl TextureLookup,
    apply_present_back_pressure: bool,
) -> Result<DrawStats, Box<dyn Error>> {
    let mut stats = DrawStats::default();
    let (width, height) = state.window_size;
    if width == 0 || height == 0 {
        return Ok(stats);
    }

    let prepare_started = Instant::now();
    {
        let device = &state.device;
        let cache = &mut state.cached_tmeshes;
        let cache_bytes = &mut state.cached_tmesh_bytes;
        let cache_stats = &mut state.cache_stats;
        resolve_textured_meshes(frame, &mut state.uploads, |key, vertices| {
            ensure_cached_tmesh(device, cache, cache_bytes, cache_stats, key, vertices)
        });
        stats.storage = draw_storage_stats(frame, Some(&state.uploads));
    }
    stats.backend_prepare_us = elapsed_us(prepare_started);

    let slot_index = state.frame_index;
    let upload_started = Instant::now();
    retire_frame_slot(state, slot_index, &mut stats);
    {
        let slot = &mut state.frames[slot_index];
        slot.sprites.upload(&state.device, &frame.sprite_instances);
        slot.meshes.upload(&state.device, &frame.mesh_vertices);
        slot.tmeshes.upload(&state.device, &state.uploads.vertices);
        slot.tmesh_instances
            .upload(&state.device, &frame.tmesh_instances);
    }
    stats.backend_upload_us = elapsed_us(upload_started);

    let acquire_started = Instant::now();
    let Some(drawable) = state.layer.next_drawable().map(ToOwned::to_owned) else {
        stats.acquire_us = elapsed_us(acquire_started);
        return Ok(stats);
    };
    stats.acquire_us = elapsed_us(acquire_started);
    let waited_for_image = stats.acquire_us >= IMAGE_WAIT_THRESHOLD_US;
    let submitted_id = next_present_id(state);

    let setup_started = Instant::now();
    let pass = render_pass(drawable.texture(), &state.depth, frame.clear_color);
    let command = state.queue.new_command_buffer();
    command.set_label("DeadSync native Metal frame");
    let encoder = command.new_render_command_encoder(pass);
    encoder.set_label("DeadSync native Metal render pass");
    encoder.set_viewport(MTLViewport {
        originX: 0.0,
        originY: 0.0,
        width: f64::from(width),
        height: f64::from(height),
        znear: 0.0,
        zfar: 1.0,
    });
    encoder.set_front_facing_winding(MTLWinding::CounterClockwise);
    stats.backend_setup_us = elapsed_us(setup_started);

    let record_started = Instant::now();
    let mut vertices_drawn = 0u32;
    let mut last_kind = None;
    let mut last_blend = None;
    let mut last_camera = None;
    let mut last_texture = None;
    let mut last_depth = None;
    let mut tmesh_buffer_cache = TexturedMeshBufferCache::default();
    for op in &frame.ops {
        match op {
            DrawOp::Sprite(run) => {
                let Some(texture) = textures.metal_texture(run.texture_handle) else {
                    continue;
                };
                if last_kind != Some(0) {
                    encoder.set_cull_mode(MTLCullMode::Back);
                    encoder.set_depth_stencil_state(&state.depth_disabled);
                    last_kind = Some(0);
                    last_blend = None;
                    last_camera = None;
                    last_texture = None;
                    last_depth = Some(false);
                    tmesh_buffer_cache.reset();
                }
                if last_blend != Some(run.blend) {
                    encoder.set_render_pipeline_state(state.sprite_pipelines.get(run.blend));
                    last_blend = Some(run.blend);
                }
                set_camera(
                    encoder,
                    1,
                    run.camera,
                    &frame.cameras,
                    state.projection,
                    &mut last_camera,
                );
                if last_texture != Some((texture.id, false)) {
                    bind_texture(encoder, texture, false);
                    last_texture = Some((texture.id, false));
                }
                encoder.set_vertex_buffer(
                    0,
                    Some(&state.frames[slot_index].sprites.raw),
                    run.instance_start as u64 * mem::size_of::<SpriteInstanceRaw>() as u64,
                );
                encoder.draw_primitives_instanced(
                    MTLPrimitiveType::Triangle,
                    0,
                    6,
                    run.instance_count as u64,
                );
                vertices_drawn = vertices_drawn.saturating_add(4 * run.instance_count);
            }
            DrawOp::Mesh(run) => {
                if run.vertex_count == 0 {
                    continue;
                }
                if last_kind != Some(1) {
                    encoder.set_cull_mode(MTLCullMode::None);
                    encoder.set_depth_stencil_state(&state.depth_disabled);
                    encoder.set_vertex_buffer(0, Some(&state.frames[slot_index].meshes.raw), 0);
                    last_kind = Some(1);
                    last_blend = None;
                    last_camera = None;
                    last_texture = None;
                    last_depth = Some(false);
                    tmesh_buffer_cache.reset();
                }
                if last_blend != Some(run.blend) {
                    encoder.set_render_pipeline_state(state.mesh_pipelines.get(run.blend));
                    last_blend = Some(run.blend);
                }
                set_camera(
                    encoder,
                    1,
                    run.camera,
                    &frame.cameras,
                    state.projection,
                    &mut last_camera,
                );
                encoder.draw_primitives(
                    MTLPrimitiveType::Triangle,
                    run.vertex_start as u64,
                    run.vertex_count as u64,
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
                let Some(texture) = textures.metal_texture(run.texture_handle) else {
                    continue;
                };
                if last_kind != Some(2) {
                    last_kind = Some(2);
                    last_blend = None;
                    last_camera = None;
                    last_texture = None;
                    last_depth = None;
                    tmesh_buffer_cache.reset();
                }
                if last_blend != Some(run.blend) {
                    encoder.set_render_pipeline_state(state.tmesh_pipelines.get(run.blend));
                    last_blend = Some(run.blend);
                }
                if last_depth != Some(run.depth_test) {
                    encoder.set_depth_stencil_state(if run.depth_test {
                        &state.depth_enabled
                    } else {
                        &state.depth_disabled
                    });
                    encoder.set_cull_mode(if run.depth_test {
                        MTLCullMode::Back
                    } else {
                        MTLCullMode::None
                    });
                    last_depth = Some(run.depth_test);
                }
                set_camera(
                    encoder,
                    2,
                    run.camera,
                    &frame.cameras,
                    state.projection,
                    &mut last_camera,
                );
                if last_texture != Some((texture.id, true)) {
                    bind_texture(encoder, texture, true);
                    last_texture = Some((texture.id, true));
                }
                if tmesh_buffer_cache.update_required(source) {
                    if let Some(key) = source.cache_key() {
                        let Some(cached) = state.cached_tmeshes.get(&key) else {
                            tmesh_buffer_cache.reset();
                            continue;
                        };
                        encoder.set_vertex_buffer(0, Some(&cached.buffer), 0);
                    } else {
                        encoder.set_vertex_buffer(
                            0,
                            Some(&state.frames[slot_index].tmeshes.raw),
                            0,
                        );
                    }
                }
                encoder.set_vertex_buffer(
                    1,
                    Some(&state.frames[slot_index].tmesh_instances.raw),
                    run.instance_start as u64 * mem::size_of::<TexturedMeshInstanceRaw>() as u64,
                );
                encoder.draw_primitives_instanced(
                    MTLPrimitiveType::Triangle,
                    source.vertex_start() as u64,
                    source.vertex_count() as u64,
                    run.instance_count as u64,
                );
                vertices_drawn = vertices_drawn
                    .saturating_add((source.vertex_count() / 3).saturating_mul(run.instance_count));
            }
        }
    }
    encoder.end_encoding();

    let screenshot = if state.screenshot_requested {
        state.screenshot_requested = false;
        Some(encode_screenshot(
            command,
            drawable.texture(),
            width,
            height,
        ))
    } else {
        None
    };
    stats.backend_record_us = elapsed_us(record_started);

    let present_started = Instant::now();
    command.present_drawable(&drawable);
    stats.present_us = elapsed_us(present_started);
    let owned_command = command.to_owned();
    let submit_started = Instant::now();
    command.commit();
    stats.submit_us = elapsed_us(submit_started);
    let mut applied_back_pressure = false;
    let mut queue_idle_waited = false;
    if apply_present_back_pressure || screenshot.is_some() {
        let wait_started = Instant::now();
        owned_command.wait_until_completed();
        let waited = elapsed_us(wait_started);
        stats.gpu_wait_us = stats.gpu_wait_us.saturating_add(waited);
        applied_back_pressure = apply_present_back_pressure && waited >= BACK_PRESSURE_THRESHOLD_US;
        queue_idle_waited = screenshot.is_some() && waited != 0;
        mark_completed(state, submitted_id);
    }
    if let Some((buffer, row_bytes)) = screenshot {
        state.captured_frame = read_screenshot(&buffer, width, height, row_bytes);
    }
    state.frames[slot_index].command = Some(owned_command);
    state.frames[slot_index].submitted_id = submitted_id;
    state.frame_index = (slot_index + 1) % FRAMES_IN_FLIGHT;
    poll_completions(state);

    let in_flight_images = submitted_id
        .saturating_sub(state.completed_present_id)
        .min(u32::from(u8::MAX)) as u8;
    stats.present_stats = PresentStats {
        mode: present_mode(state.vsync_enabled, state.present_mode_policy),
        display_clock: ClockDomainTrace::Unknown,
        host_clock: ClockDomainTrace::Monotonic,
        in_flight_images,
        waited_for_image,
        applied_back_pressure,
        queue_idle_waited,
        suboptimal: false,
        submitted_present_id: submitted_id,
        completed_present_id: state.completed_present_id,
        refresh_ns: 0,
        actual_interval_ns: 0,
        present_margin_ns: 0,
        host_present_ns: state.completed_host_ns,
        calibration_error_ns: 0,
    };
    stats.vertices = vertices_drawn;
    Ok(stats)
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    state.window_size = (width, height);
    state.projection = ortho_for_window(width, height);
    state.layer.set_contents_scale(state.window.scale_factor());
    set_layer_size(&state.layer, width, height);
    state.depth = create_depth_target(&state.device, width, height);
}

pub fn set_present_config(
    state: &mut State,
    vsync_enabled: bool,
    present_mode_policy: PresentModePolicy,
) {
    state.vsync_enabled = vsync_enabled;
    state.present_mode_policy = present_mode_policy;
    set_layer_present_config(&state.layer, vsync_enabled);
}

pub fn wait_for_idle(state: &mut State) {
    for index in 0..FRAMES_IN_FLIGHT {
        if let Some(command) = state.frames[index].command.take() {
            command.wait_until_completed();
            mark_completed(state, state.frames[index].submitted_id);
        }
    }
}

pub fn cleanup(state: &mut State) {
    wait_for_idle(state);
    debug!(
        "Native Metal textured-mesh cache: {} hits, {} misses, {} saturated, {} bytes",
        state.cache_stats.hits,
        state.cache_stats.misses,
        state.cache_stats.saturated,
        state.cached_tmesh_bytes,
    );
    state.cached_tmeshes.clear();
    state.cached_tmesh_bytes = 0;
    if let Err(error) = detach_layer(&state.window) {
        warn!("Failed to detach native Metal layer: {error}");
    }
}

fn attach_layer(window: &Window, layer: &mut MetalLayer) -> Result<(), Box<dyn Error>> {
    let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
        return Err(std::io::Error::other("Native Metal requires an AppKit window").into());
    };
    let view = handle.ns_view.as_ptr().cast::<Object>();
    // SAFETY: winit owns a live NSView on the main thread. AppKit retains the
    // CAMetalLayer assigned to it, while `State` also keeps an owning reference.
    unsafe {
        let layer_ptr = layer.as_ptr().cast::<Object>();
        let _: () = msg_send![view, setWantsLayer: YES];
        let _: () = msg_send![view, setLayer: layer_ptr];
    }
    Ok(())
}

fn detach_layer(window: &Window) -> Result<(), Box<dyn Error>> {
    let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
        return Ok(());
    };
    let view = handle.ns_view.as_ptr().cast::<Object>();
    // SAFETY: This reverses `attach_layer` on the same live winit NSView.
    unsafe {
        let _: () = msg_send![view, setLayer: ptr::null_mut::<Object>()];
    }
    Ok(())
}

fn set_layer_size(layer: &MetalLayerRef, width: u32, height: u32) {
    layer.set_drawable_size(CGSize::new(
        f64::from(width.max(1)),
        f64::from(height.max(1)),
    ));
}

fn set_layer_present_config(layer: &MetalLayerRef, vsync_enabled: bool) {
    layer.set_display_sync_enabled(vsync_enabled);
}

fn build_pipeline_set(
    device: &DeviceRef,
    library: &LibraryRef,
    vertex_name: &str,
    fragment_name: &str,
) -> Result<PipelineSet, Box<dyn Error>> {
    let vertex = library
        .get_function(vertex_name, None)
        .map_err(std::io::Error::other)?;
    let fragment = library
        .get_function(fragment_name, None)
        .map_err(std::io::Error::other)?;
    Ok(PipelineSet {
        alpha: build_pipeline(device, &vertex, &fragment, BlendMode::Alpha)?,
        add: build_pipeline(device, &vertex, &fragment, BlendMode::Add)?,
        multiply: build_pipeline(device, &vertex, &fragment, BlendMode::Multiply)?,
        subtract: build_pipeline(device, &vertex, &fragment, BlendMode::Subtract)?,
    })
}

fn build_pipeline(
    device: &DeviceRef,
    vertex: &FunctionRef,
    fragment: &FunctionRef,
    blend: BlendMode,
) -> Result<RenderPipelineState, Box<dyn Error>> {
    let desc = RenderPipelineDescriptor::new();
    desc.set_vertex_function(Some(vertex));
    desc.set_fragment_function(Some(fragment));
    desc.set_depth_attachment_pixel_format(DEPTH_FORMAT);
    let attachment = desc
        .color_attachments()
        .object_at(0)
        .ok_or_else(|| std::io::Error::other("Metal pipeline has no color attachment"))?;
    attachment.set_pixel_format(COLOR_FORMAT);
    attachment.set_write_mask(
        MTLColorWriteMask::Red | MTLColorWriteMask::Green | MTLColorWriteMask::Blue,
    );
    configure_blend(attachment, blend);
    device
        .new_render_pipeline_state(&desc)
        .map_err(|error| std::io::Error::other(error).into())
}

fn configure_blend(attachment: &RenderPipelineColorAttachmentDescriptorRef, blend: BlendMode) {
    let (src, dst, op, src_alpha, dst_alpha, alpha_op) = match blend {
        BlendMode::Alpha => (
            MTLBlendFactor::SourceAlpha,
            MTLBlendFactor::OneMinusSourceAlpha,
            MTLBlendOperation::Add,
            MTLBlendFactor::SourceAlpha,
            MTLBlendFactor::OneMinusSourceAlpha,
            MTLBlendOperation::Add,
        ),
        BlendMode::Add => (
            MTLBlendFactor::SourceAlpha,
            MTLBlendFactor::One,
            MTLBlendOperation::Add,
            MTLBlendFactor::SourceAlpha,
            MTLBlendFactor::One,
            MTLBlendOperation::Add,
        ),
        BlendMode::Multiply => (
            MTLBlendFactor::DestinationColor,
            MTLBlendFactor::Zero,
            MTLBlendOperation::Add,
            MTLBlendFactor::DestinationAlpha,
            MTLBlendFactor::Zero,
            MTLBlendOperation::Add,
        ),
        BlendMode::Subtract => (
            MTLBlendFactor::One,
            MTLBlendFactor::One,
            MTLBlendOperation::ReverseSubtract,
            MTLBlendFactor::One,
            MTLBlendFactor::One,
            MTLBlendOperation::ReverseSubtract,
        ),
    };
    attachment.set_blending_enabled(true);
    attachment.set_source_rgb_blend_factor(src);
    attachment.set_destination_rgb_blend_factor(dst);
    attachment.set_rgb_blend_operation(op);
    attachment.set_source_alpha_blend_factor(src_alpha);
    attachment.set_destination_alpha_blend_factor(dst_alpha);
    attachment.set_alpha_blend_operation(alpha_op);
}

fn build_depth_states(device: &DeviceRef) -> (DepthStencilState, DepthStencilState) {
    let disabled_desc = DepthStencilDescriptor::new();
    disabled_desc.set_depth_compare_function(MTLCompareFunction::Always);
    disabled_desc.set_depth_write_enabled(false);
    let disabled = device.new_depth_stencil_state(&disabled_desc);
    let enabled_desc = DepthStencilDescriptor::new();
    enabled_desc.set_depth_compare_function(MTLCompareFunction::LessEqual);
    enabled_desc.set_depth_write_enabled(true);
    let enabled = device.new_depth_stencil_state(&enabled_desc);
    (disabled, enabled)
}

fn create_depth_target(device: &DeviceRef, width: u32, height: u32) -> metal::Texture {
    let desc = TextureDescriptor::new();
    desc.set_texture_type(MTLTextureType::D2);
    desc.set_pixel_format(DEPTH_FORMAT);
    desc.set_width(width.max(1) as u64);
    desc.set_height(height.max(1) as u64);
    desc.set_storage_mode(MTLStorageMode::Private);
    desc.set_usage(MTLTextureUsage::RenderTarget);
    device.new_texture(&desc)
}

fn render_pass<'a>(
    color: &TextureRef,
    depth: &TextureRef,
    clear: [f32; 4],
) -> &'a RenderPassDescriptorRef {
    let pass = RenderPassDescriptor::new();
    let color_attachment = pass.color_attachments().object_at(0).expect("attachment 0");
    color_attachment.set_texture(Some(color));
    color_attachment.set_load_action(MTLLoadAction::Clear);
    color_attachment.set_store_action(MTLStoreAction::Store);
    color_attachment.set_clear_color(MTLClearColor::new(
        f64::from(clear[0]),
        f64::from(clear[1]),
        f64::from(clear[2]),
        1.0,
    ));
    let depth_attachment = pass.depth_attachment().expect("depth attachment");
    depth_attachment.set_texture(Some(depth));
    depth_attachment.set_load_action(MTLLoadAction::Clear);
    depth_attachment.set_store_action(MTLStoreAction::DontCare);
    depth_attachment.set_clear_depth(1.0);
    pass
}

fn create_sampler(device: &DeviceRef, sampler: SamplerDesc) -> SamplerState {
    let desc = SamplerDescriptor::new();
    let filter = match sampler.filter {
        SamplerFilter::Linear => MTLSamplerMinMagFilter::Linear,
        SamplerFilter::Nearest => MTLSamplerMinMagFilter::Nearest,
    };
    let address = match sampler.wrap {
        SamplerWrap::Clamp => MTLSamplerAddressMode::ClampToEdge,
        SamplerWrap::Repeat => MTLSamplerAddressMode::Repeat,
    };
    desc.set_min_filter(filter);
    desc.set_mag_filter(filter);
    desc.set_mip_filter(if sampler.mipmaps {
        MTLSamplerMipFilter::Linear
    } else {
        MTLSamplerMipFilter::NotMipmapped
    });
    desc.set_address_mode_s(address);
    desc.set_address_mode_t(address);
    device.new_sampler(&desc)
}

fn validate_image(image: &RgbaImage) -> Result<(), Box<dyn Error>> {
    if image.width() == 0 || image.height() == 0 {
        Err(std::io::Error::other("Metal textures must have non-zero dimensions").into())
    } else {
        Ok(())
    }
}

fn mip_level_count(image: &RgbaImage, mipmaps: bool) -> u64 {
    if !mipmaps {
        return 1;
    }
    u64::from(u32::BITS - image.width().max(image.height()).leading_zeros())
}

fn upload_texture(queue: &CommandQueueRef, texture: &TextureRef, image: &RgbaImage, mipmaps: bool) {
    let packed_row_bytes = image.width() as usize * 4;
    let row_bytes = packed_row_bytes.next_multiple_of(256);
    let staging_bytes = if row_bytes == packed_row_bytes {
        Cow::Borrowed(image.as_raw().as_slice())
    } else {
        let mut padded = vec![0; row_bytes * image.height() as usize];
        for (source, destination) in image
            .as_raw()
            .chunks_exact(packed_row_bytes)
            .zip(padded.chunks_exact_mut(row_bytes))
        {
            destination[..packed_row_bytes].copy_from_slice(source);
        }
        Cow::Owned(padded)
    };
    autoreleasepool(|| {
        let staging = queue.device().new_buffer_with_data(
            staging_bytes.as_ptr().cast(),
            staging_bytes.len() as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let command = queue.new_command_buffer();
        let blit = command.new_blit_command_encoder();
        blit.copy_from_buffer_to_texture(
            &staging,
            0,
            row_bytes as u64,
            staging_bytes.len() as u64,
            MTLSize::new(image.width() as u64, image.height() as u64, 1),
            texture,
            0,
            0,
            MTLOrigin::default(),
            MTLBlitOption::None,
        );
        if mipmaps {
            blit.generate_mipmaps(texture);
        }
        blit.end_encoding();
        command.commit();
    });
}

fn ensure_cached_tmesh(
    device: &DeviceRef,
    cache: &mut FastU64Map<CachedTMesh>,
    cache_bytes: &mut usize,
    stats: &mut CacheStats,
    key: TMeshCacheKey,
    vertices: &[TexturedMeshVertex],
) -> bool {
    if let Some(entry) = cache.get(&key) {
        stats.hits = stats.hits.saturating_add(1);
        return entry.vertex_count == vertices.len() as u32;
    }
    stats.misses = stats.misses.saturating_add(1);
    let bytes = mem::size_of_val(vertices);
    if bytes == 0
        || bytes > TMESH_CACHE_MAX_BYTES
        || cache_bytes.saturating_add(bytes) > TMESH_CACHE_MAX_BYTES
    {
        stats.saturated = stats.saturated.saturating_add(1);
        return false;
    }
    let buffer = device.new_buffer_with_data(
        vertices.as_ptr().cast(),
        bytes as u64,
        MTLResourceOptions::StorageModeShared,
    );
    cache.insert(
        key,
        CachedTMesh {
            buffer,
            vertex_count: vertices.len() as u32,
        },
    );
    *cache_bytes = cache_bytes.saturating_add(bytes);
    true
}

fn set_camera(
    encoder: &RenderCommandEncoderRef,
    buffer_index: u64,
    camera: u8,
    cameras: &[Matrix4],
    fallback: Matrix4,
    last_camera: &mut Option<u8>,
) {
    if *last_camera == Some(camera) {
        return;
    }
    let projection = cameras.get(camera as usize).copied().unwrap_or(fallback);
    let columns = projection.to_cols_array();
    encoder.set_vertex_bytes(
        buffer_index,
        mem::size_of_val(&columns) as u64,
        columns.as_ptr().cast(),
    );
    *last_camera = Some(camera);
}

fn bind_texture(encoder: &RenderCommandEncoderRef, texture: &Texture, repeat: bool) {
    encoder.set_fragment_texture(0, Some(&texture.raw));
    encoder.set_fragment_sampler_state(
        0,
        Some(if repeat {
            &texture.repeat_sampler
        } else {
            &texture.sampler
        }),
    );
}

fn retire_frame_slot(state: &mut State, index: usize, stats: &mut DrawStats) {
    let Some(command) = state.frames[index].command.take() else {
        return;
    };
    let wait_started = Instant::now();
    if !command_complete(&command) {
        command.wait_until_completed();
    }
    stats.gpu_wait_us = stats.gpu_wait_us.saturating_add(elapsed_us(wait_started));
    mark_completed(state, state.frames[index].submitted_id);
}

fn poll_completions(state: &mut State) {
    let mut completed = state.completed_present_id;
    for frame in &state.frames {
        if frame
            .command
            .as_ref()
            .is_some_and(|command| command_complete(command))
        {
            completed = completed.max(frame.submitted_id);
        }
    }
    if completed > state.completed_present_id {
        mark_completed(state, completed);
    }
}

fn command_complete(command: &CommandBufferRef) -> bool {
    matches!(
        command.status(),
        MTLCommandBufferStatus::Completed | MTLCommandBufferStatus::Error
    )
}

fn mark_completed(state: &mut State, present_id: u32) {
    if present_id > state.completed_present_id {
        state.completed_present_id = present_id;
        state.completed_host_ns = deadlib_platform::host_time::now_nanos();
    }
}

fn next_present_id(state: &mut State) -> u32 {
    let id = state.next_present_id.max(1);
    state.next_present_id = id.wrapping_add(1).max(1);
    id
}

fn encode_screenshot(
    command: &CommandBufferRef,
    source: &TextureRef,
    width: u32,
    height: u32,
) -> (Buffer, usize) {
    let row_bytes = (width as usize * 4).next_multiple_of(256);
    let buffer = source.device().new_buffer(
        (row_bytes * height as usize) as u64,
        MTLResourceOptions::StorageModeShared,
    );
    let blit = command.new_blit_command_encoder();
    blit.copy_from_texture_to_buffer(
        source,
        0,
        0,
        MTLOrigin::default(),
        MTLSize::new(width as u64, height as u64, 1),
        &buffer,
        0,
        row_bytes as u64,
        (row_bytes * height as usize) as u64,
        MTLBlitOption::None,
    );
    blit.end_encoding();
    (buffer, row_bytes)
}

fn read_screenshot(
    buffer: &BufferRef,
    width: u32,
    height: u32,
    row_bytes: usize,
) -> Option<RgbaImage> {
    let width = width as usize;
    let height = height as usize;
    let mut rgba = vec![0; width * height * 4];
    // SAFETY: The completed blit filled `row_bytes * height` bytes in this
    // shared buffer, and each source row access remains inside that allocation.
    unsafe {
        let bytes = std::slice::from_raw_parts(buffer.contents().cast::<u8>(), row_bytes * height);
        for y in 0..height {
            let src = y * row_bytes;
            let dst = y * width * 4;
            for x in 0..width {
                let s = src + x * 4;
                let d = dst + x * 4;
                rgba[d] = bytes[s + 2];
                rgba[d + 1] = bytes[s + 1];
                rgba[d + 2] = bytes[s];
                rgba[d + 3] = bytes[s + 3];
            }
        }
    }
    RgbaImage::from_raw(width as u32, height as u32, rgba)
}

#[inline(always)]
fn elapsed_us(started: Instant) -> u32 {
    started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32
}

#[inline(always)]
fn present_mode(vsync: bool, policy: PresentModePolicy) -> PresentModeTrace {
    if vsync {
        PresentModeTrace::Fifo
    } else if policy == PresentModePolicy::Immediate {
        PresentModeTrace::Immediate
    } else {
        PresentModeTrace::Mailbox
    }
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
