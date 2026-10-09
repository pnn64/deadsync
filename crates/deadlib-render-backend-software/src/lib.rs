use deadlib_render_core::{
    BlendMode, DrawOp, DrawStats, MeshSampler, RenderFrame, RenderTargetFrame,
    SOFTWARE_MESH_STORAGE_SLOT, SOFTWARE_OBJECTS_STORAGE_SLOT, SOFTWARE_TMESH_STORAGE_SLOT,
    SamplerDesc, SamplerFilter, SamplerWrap, TextureHandle, TexturedMeshGeometry,
    TexturedMeshInstanceRaw, Yuv420Upload, draw_storage_stats, is_render_target_texture,
    render_target_base_handle, render_target_uses_nearest, texture_sampler_desc,
};
use glam::{Mat4 as Matrix4, Vec4 as Vector4};
use image::RgbaImage;
use log::info;
use rayon::prelude::*;
use std::{error::Error, num::NonZeroU32, sync::Arc, time::Instant};
use winit::{dpi::PhysicalSize, window::Window};

mod texture_upload;
use texture_upload::{texture_is_opaque, update_yuv420_image, yuv420_to_rgba};

const SOFTWARE_ROW_CHUNK: usize = 32;
// Staging wins once enough row workers would otherwise repeat every transform.
const MIN_STAGE_MESH_STRIPES: usize = 12;
// Covers the current two-player density graph while bounding retained memory.
// Frames that exceed either buffer render through the direct path instead.
const MESH_STAGE_VERTEX_CAP: usize = 36 * 1024;
const U8_TO_F32: f32 = 1.0 / 255.0;

pub struct Texture {
    pub image: RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    yuv420: bool,
    // Empty for uploaded byte images; captures retain physical RGBA16F texels.
    half_pixels: Vec<u64>,
}

pub trait TextureLookup {
    fn software_texture(&self, handle: TextureHandle) -> Option<&Texture>;
}

pub struct State {
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    window_size: PhysicalSize<u32>,
    surface_resize_pending: bool,
    projection: Matrix4,
    thread_hint: Option<usize>,
    available_threads: usize,
    worker_pool: Option<WorkerPool>,
    prepared_objects: Vec<PreparedObject>,
    prepared_mesh_triangles: Vec<PreparedTriangle<ScreenVertexColor>>,
    prepared_tmesh_triangles: Vec<PreparedTriangle<ScreenVertexTexColor>>,
    stripe_bins: StripeBins,
    // Sized at window creation/resize, split with pixel stripes for exclusive
    // worker access, reused each frame, and freed with the renderer. Stripes
    // are reset only when a depth-tested draw reaches them.
    depth: Vec<f32>,
    offscreen_targets: Vec<OffscreenTarget>,
}

/// Render-thread-owned, song-reused software `ActorFrameTexture` storage.
/// Slots are bounded by the largest active graph, allocated at graph warmup,
/// and replaced only when its handle, dimensions, color format or depth flag changes.
/// Gameplay redraws reuse pixel buffers without eviction, pruning, I/O, or
/// destruction; the renderer owns and frees them at shutdown. A pass miss is a
/// transparent texture, and per-frame work is bounded by target pixels plus its
/// draw list. Pixel/depth capacities cover the entire backing image; their
/// lengths cover the viewport. Viewport changes reload at most the backing
/// pixel count without reallocating. Clears include the backing padding.
/// Float slots retain 8-byte RGBA16F texels in both backing and viewport
/// buffers, plus the 4-byte decoded-image view. Byte slots retain their
/// original color buffers. Depth-enabled slots retain two backing-sized f32
/// allocations (compact viewport rows and physical image rows); depth-disabled
/// slots retain none. Capture comparisons quantize to native 16-bit depth.
/// There is no per-pixel allocation; clear, reload
/// and image-copy work each visit at most the backing pixel count. Formats,
/// buffer pointers and capacities are covered by the warmed capture tests.
struct OffscreenTarget {
    handle: TextureHandle,
    width: u32,
    height: u32,
    viewport: [u32; 2],
    texture: Texture,
    pixels: Vec<u32>,
    half_pixels: Vec<u64>,
    float_color: bool,
    depth: Vec<f32>,
    depth_image: Vec<f32>,
    with_depth: bool,
    initialized: bool,
}

struct DepthRows<'a> {
    pixels: &'a mut [f32],
    unorm16: bool,
}

impl DepthRows<'_> {
    fn test(&mut self, index: usize, z: f32) -> bool {
        if !(0.0..=1.0).contains(&z) {
            return false;
        }
        if self.pixels.is_empty() {
            return true;
        }
        let z = if self.unorm16 {
            (z * 65535.0).round() / 65535.0
        } else {
            z
        };
        if z > self.pixels[index] {
            return false;
        }
        self.pixels[index] = z;
        true
    }
}

#[derive(Clone, Copy)]
struct SoftwarePass<'a> {
    depth: bool,
    reset_depth: bool,
    depth_unorm: bool,
    cameras: &'a [Matrix4],
    sprite_instances: &'a [deadlib_render_core::SpriteInstanceRaw],
    mesh_vertices: &'a [deadlib_render_core::MeshVertex],
    tmesh_instances: &'a [TexturedMeshInstanceRaw],
    tmesh_geometries: &'a [TexturedMeshGeometry],
    ops: &'a [DrawOp],
}

impl<'a> From<&'a RenderFrame> for SoftwarePass<'a> {
    fn from(frame: &'a RenderFrame) -> Self {
        Self {
            depth: true,
            reset_depth: true,
            depth_unorm: false,
            cameras: &frame.cameras,
            sprite_instances: &frame.sprite_instances,
            mesh_vertices: &frame.mesh_vertices,
            tmesh_instances: &frame.tmesh_instances,
            tmesh_geometries: &frame.tmesh_geometries,
            ops: &frame.ops,
        }
    }
}

impl<'a> From<&'a RenderTargetFrame> for SoftwarePass<'a> {
    fn from(frame: &'a RenderTargetFrame) -> Self {
        Self {
            depth: frame.depth,
            reset_depth: false,
            depth_unorm: true,
            cameras: &frame.cameras,
            sprite_instances: &frame.sprite_instances,
            mesh_vertices: &frame.mesh_vertices,
            tmesh_instances: &frame.tmesh_instances,
            tmesh_geometries: &frame.tmesh_geometries,
            ops: &frame.ops,
        }
    }
}

struct WorkerPool {
    threads: usize,
    pool: rayon::ThreadPool,
}

/// Frame-local raster input built once before parallel row stripes execute.
///
/// Prepared vertices, conservative row intervals, and sprite triangle
/// reciprocals are derived during the existing frame preparation pass. The
/// renderer-owned vectors retain their session high-water capacities, while
/// entries are cleared and rebuilt without allocation on warmed frames.
enum PreparedObject {
    ClearDepth,
    Sprite {
        vertices: [ScreenVertex; 4],
        rows: ScreenRows,
        inv_denom: [Option<f32>; 2],
        tint: [f32; 4],
        texture_mask: bool,
        blend: BlendMode,
        texture_handle: TextureHandle,
    },
    Mesh {
        triangle_start: u32,
        triangle_count: u32,
        rows: ScreenRows,
        blend: BlendMode,
    },
    DirectMesh {
        vertex_start: u32,
        vertex_count: u32,
        projection: Matrix4,
        blend: BlendMode,
    },
    TexturedMesh {
        triangle_start: u32,
        triangle_count: u32,
        rows: ScreenRows,
        texture_mask: bool,
        blend: BlendMode,
        depth_test: bool,
        texture_handle: TextureHandle,
        sampler: Option<MeshSampler>,
    },
    DirectTexturedMesh {
        geometry: u32,
        instance: u32,
        mvp: Matrix4,
        blend: BlendMode,
        depth_test: bool,
        texture_handle: TextureHandle,
        sampler: Option<MeshSampler>,
    },
}

impl PreparedObject {
    #[inline(always)]
    const fn rows(&self, height: usize) -> ScreenRows {
        match self {
            Self::Sprite { rows, .. }
            | Self::Mesh { rows, .. }
            | Self::TexturedMesh { rows, .. } => *rows,
            Self::ClearDepth | Self::DirectMesh { .. } | Self::DirectTexturedMesh { .. } => {
                ScreenRows {
                    start: 0,
                    end: height as u32,
                }
            }
        }
    }
}

/// Frame-local painter-order membership for the software worker stripes.
///
/// The render thread owns and rebuilds it after frame preparation; Rayon
/// workers only read it. Its lifetime is one frame, while the three vectors
/// retain a session high-water capacity (64 stripes and 4,096 memberships are
/// prewarmed). A larger frame may grow them during preparation, never during
/// parallel drawing. There are no misses, eviction, pruning, synchronization,
/// or deferred destruction: overflow grows at the frame boundary and cleanup
/// occurs with renderer state. The lengths and capacities are observable in
/// tests, and build work is bounded by objects plus their covered stripes.
#[derive(Default)]
struct StripeBins {
    offsets: Vec<u32>,
    cursors: Vec<u32>,
    items: Vec<StripeItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StripeItem(u32);

impl StripeItem {
    const WHOLE_OBJECT: u32 = 1 << 31;
    const TEXTURED: u32 = 1 << 30;
    const INDEX_MASK: u32 = Self::TEXTURED - 1;

    #[inline(always)]
    const fn whole(object: u32) -> Self {
        debug_assert!(object <= Self::INDEX_MASK);
        Self(Self::WHOLE_OBJECT | object)
    }

    #[inline(always)]
    const fn mesh(triangle: u32) -> Self {
        debug_assert!(triangle <= Self::INDEX_MASK);
        Self(triangle)
    }

    #[inline(always)]
    const fn tmesh(triangle: u32) -> Self {
        debug_assert!(triangle <= Self::INDEX_MASK);
        Self(Self::TEXTURED | triangle)
    }

    #[inline(always)]
    const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    #[inline(always)]
    const fn is_whole(self) -> bool {
        self.0 & Self::WHOLE_OBJECT != 0
    }

    #[inline(always)]
    const fn is_tmesh(self) -> bool {
        self.0 & Self::TEXTURED != 0
    }
}

impl StripeBins {
    fn warmed() -> Self {
        Self {
            offsets: Vec::with_capacity(65),
            cursors: Vec::with_capacity(64),
            items: Vec::with_capacity(4_096),
        }
    }

    fn build(
        &mut self,
        objects: &[PreparedObject],
        mesh_triangles: &[PreparedTriangle<ScreenVertexColor>],
        tmesh_triangles: &[PreparedTriangle<ScreenVertexTexColor>],
        height: usize,
    ) {
        let stripe_count = height.div_ceil(SOFTWARE_ROW_CHUNK);
        self.offsets.clear();
        self.offsets.resize(stripe_count + 1, 0);
        for object in objects {
            match object {
                PreparedObject::Mesh {
                    triangle_start,
                    triangle_count,
                    ..
                } => {
                    let start = *triangle_start as usize;
                    let end = start + *triangle_count as usize;
                    for triangle in &mesh_triangles[start..end] {
                        Self::count_rows(&mut self.offsets, triangle.setup.rows(), stripe_count);
                    }
                }
                PreparedObject::TexturedMesh {
                    triangle_start,
                    triangle_count,
                    ..
                } => {
                    let start = *triangle_start as usize;
                    let end = start + *triangle_count as usize;
                    for triangle in &tmesh_triangles[start..end] {
                        Self::count_rows(&mut self.offsets, triangle.setup.rows(), stripe_count);
                    }
                }
                _ => Self::count_rows(&mut self.offsets, object.rows(height), stripe_count),
            }
        }
        // Convert range boundaries to stripe counts and then to item offsets.
        let mut covered = 0u32;
        let mut total = 0;
        for offset in &mut self.offsets {
            covered = covered.wrapping_add(*offset);
            *offset = total;
            total += covered;
        }

        self.items
            .resize(self.offsets[stripe_count] as usize, StripeItem(0));
        self.cursors.clear();
        self.cursors
            .extend_from_slice(&self.offsets[..stripe_count]);
        for (index, object) in objects.iter().enumerate() {
            let object_index = index as u32;
            match object {
                PreparedObject::Mesh {
                    triangle_start,
                    triangle_count,
                    ..
                } => {
                    let start = *triangle_start as usize;
                    let end = start + *triangle_count as usize;
                    for (offset, prepared) in mesh_triangles[start..end].iter().enumerate() {
                        let triangle = start + offset;
                        self.insert_rows(
                            prepared.setup.rows(),
                            StripeItem::mesh(triangle as u32),
                            stripe_count,
                        );
                    }
                }
                PreparedObject::TexturedMesh {
                    triangle_start,
                    triangle_count,
                    ..
                } => {
                    let start = *triangle_start as usize;
                    let end = start + *triangle_count as usize;
                    for (offset, prepared) in tmesh_triangles[start..end].iter().enumerate() {
                        let triangle = start + offset;
                        self.insert_rows(
                            prepared.setup.rows(),
                            StripeItem::tmesh(triangle as u32),
                            stripe_count,
                        );
                    }
                }
                _ => self.insert_rows(
                    object.rows(height),
                    StripeItem::whole(object_index),
                    stripe_count,
                ),
            }
        }
    }

    #[inline(always)]
    fn stripe(&self, index: usize) -> &[StripeItem] {
        let start = self.offsets[index] as usize;
        let end = self.offsets[index + 1] as usize;
        &self.items[start..end]
    }

    #[inline]
    fn count_rows(offsets: &mut [u32], rows: ScreenRows, stripe_count: usize) {
        let first = rows.start as usize / SOFTWARE_ROW_CHUNK;
        let end = (rows.end as usize)
            .div_ceil(SOFTWARE_ROW_CHUNK)
            .min(stripe_count);
        if first < end {
            // Record a range once; the prefix scan recovers each stripe's count.
            // Wrapping encodes negative end deltas in the existing u32 storage.
            offsets[first] = offsets[first].wrapping_add(1);
            offsets[end] = offsets[end].wrapping_sub(1);
        }
    }

    #[inline]
    fn insert_rows(&mut self, rows: ScreenRows, item: StripeItem, stripe_count: usize) {
        let first = rows.start as usize / SOFTWARE_ROW_CHUNK;
        let end = (rows.end as usize)
            .div_ceil(SOFTWARE_ROW_CHUNK)
            .min(stripe_count);
        for stripe in first..end {
            let slot = self.cursors[stripe] as usize;
            self.items[slot] = item;
            self.cursors[stripe] += 1;
        }
    }
}

pub fn init(
    window: Arc<Window>,
    projection: Matrix4,
    _vsync_enabled: bool,
) -> Result<State, Box<dyn Error>> {
    info!("Initializing software renderer backend (softbuffer)...");

    let window_size = window.inner_size();

    let context = softbuffer::Context::new(window.clone())?;
    let surface = softbuffer::Surface::new(&context, window)?;
    let available_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .max(1);

    Ok(State {
        _context: context,
        surface,
        window_size,
        surface_resize_pending: true,
        projection,
        thread_hint: None,
        available_threads,
        worker_pool: None,
        prepared_objects: Vec::with_capacity(1024),
        prepared_mesh_triangles: Vec::with_capacity(MESH_STAGE_VERTEX_CAP / 3),
        prepared_tmesh_triangles: Vec::with_capacity(MESH_STAGE_VERTEX_CAP / 3),
        stripe_bins: StripeBins::warmed(),
        depth: vec![1.0; window_size.width as usize * window_size.height as usize],
        offscreen_targets: Vec::with_capacity(4),
    })
}

pub const fn set_thread_hint(state: &mut State, threads: Option<usize>) {
    state.thread_hint = threads;
}

fn ensure_worker_pool(state: &mut State, threads: usize) -> Result<(), Box<dyn Error>> {
    if threads <= 1 {
        state.worker_pool = None;
        return Ok(());
    }
    if state
        .worker_pool
        .as_ref()
        .is_some_and(|pool| pool.threads == threads)
    {
        return Ok(());
    }
    state.worker_pool = Some(WorkerPool {
        threads,
        pool: rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("software-render-{index}"))
            .build()?,
    });
    Ok(())
}

#[inline(always)]
pub const fn request_screenshot(_state: &mut State) {}

pub fn create_texture(image: &RgbaImage, sampler: SamplerDesc) -> Result<Texture, Box<dyn Error>> {
    Ok(Texture {
        image: image.clone(),
        sampler,
        opaque: texture_is_opaque(image),
        yuv420: false,
        half_pixels: Vec::new(),
    })
}

pub fn update_texture(texture: &mut Texture, image: &RgbaImage) -> Result<(), Box<dyn Error>> {
    texture.image.clone_from(image);
    texture.opaque = texture_is_opaque(image);
    texture.yuv420 = false;
    texture.half_pixels.clear();
    Ok(())
}

pub fn create_yuv420_texture(
    upload: Yuv420Upload<'_>,
    sampler: SamplerDesc,
) -> Result<Texture, Box<dyn Error>> {
    let image = yuv420_to_rgba(upload)?;
    Ok(Texture {
        image,
        sampler,
        opaque: true,
        yuv420: true,
        half_pixels: Vec::new(),
    })
}

pub fn update_yuv420_texture(
    texture: &mut Texture,
    upload: Yuv420Upload<'_>,
) -> Result<(), Box<dyn Error>> {
    update_yuv420_image(&mut texture.image, upload)?;
    texture.opaque = true;
    texture.yuv420 = true;
    Ok(())
}

#[inline(always)]
#[must_use]
pub const fn texture_is_yuv420(texture: &Texture) -> bool {
    texture.yuv420
}

struct ResolvedTextures<'a, T: TextureLookup + Sync> {
    external: &'a T,
    targets: &'a [OffscreenTarget],
}

impl<T: TextureLookup + Sync> TextureLookup for ResolvedTextures<'_, T> {
    fn software_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        if is_render_target_texture(handle) {
            let handle = render_target_base_handle(handle);
            return self
                .targets
                .iter()
                .find(|target| target.handle == handle)
                .map(|target| &target.texture);
        }
        self.external.software_texture(handle)
    }
}

#[inline(always)]
const fn effective_sampler(texture: &Texture, handle: TextureHandle) -> SamplerDesc {
    if render_target_uses_nearest(handle) {
        SamplerDesc {
            filter: SamplerFilter::Nearest,
            ..texture.sampler
        }
    } else {
        texture.sampler
    }
}

fn create_offscreen_target(
    handle: TextureHandle,
    width: u32,
    height: u32,
    viewport: [u32; 2],
    float_color: bool,
    with_depth: bool,
) -> OffscreenTarget {
    let width = width.max(1);
    let height = height.max(1);
    let len = width as usize * height as usize;
    let viewport = [viewport[0].clamp(1, width), viewport[1].clamp(1, height)];
    let viewport_len = viewport[0] as usize * viewport[1] as usize;
    // Retain backing-sized capacity so viewport changes reuse the buffers.
    let mut pixels = vec![0; if float_color { 0 } else { len }];
    let mut half_pixels = vec![0; if float_color { len } else { 0 }];
    let mut depth = vec![1.0; if with_depth { len } else { 0 }];
    pixels.truncate(viewport_len);
    half_pixels.truncate(viewport_len);
    depth.truncate(viewport_len);
    OffscreenTarget {
        handle,
        width,
        height,
        viewport,
        texture: Texture {
            image: RgbaImage::new(width, height),
            sampler: SamplerDesc {
                filter: SamplerFilter::Linear,
                wrap: SamplerWrap::Clamp,
                mipmaps: false,
            },
            opaque: false,
            yuv420: false,
            half_pixels: vec![0; if float_color { len } else { 0 }],
        },
        pixels,
        half_pixels,
        float_color,
        depth,
        depth_image: vec![1.0; if with_depth { len } else { 0 }],
        with_depth,
        initialized: false,
    }
}

fn ensure_offscreen_targets(targets: &mut Vec<OffscreenTarget>, frame: &RenderFrame) {
    for (index, pass) in frame.render_targets.iter().enumerate() {
        let viewport = deadlib_render_core::render_target_viewport(pass);
        let matches = targets.get(index).is_some_and(|target| {
            target.handle == pass.texture_handle
                && target.width == pass.width.max(1)
                && target.height == pass.height.max(1)
                && target.float_color == pass.float_color
                && target.with_depth == pass.depth
        });
        if matches {
            let target = &mut targets[index];
            if target.viewport != viewport {
                let len = viewport[0] as usize * viewport[1] as usize;
                if target.float_color {
                    target.half_pixels.resize(len, 0);
                } else {
                    target.pixels.resize(len, 0);
                }
                if pass.depth {
                    target.depth.resize(len, 1.0);
                    for (source, dest) in target
                        .depth_image
                        .chunks_exact(target.width as usize)
                        .zip(target.depth.chunks_exact_mut(viewport[0] as usize))
                    {
                        dest.copy_from_slice(&source[..dest.len()]);
                    }
                }
                target.viewport = viewport;
                if target.initialized && target.float_color {
                    for (source, dest) in target
                        .texture
                        .half_pixels
                        .chunks_exact(target.width as usize)
                        .zip(target.half_pixels.chunks_exact_mut(viewport[0] as usize))
                    {
                        dest.copy_from_slice(&source[..dest.len()]);
                    }
                } else if target.initialized {
                    for (source, dest) in target
                        .texture
                        .image
                        .as_raw()
                        .chunks_exact(target.width as usize * 4)
                        .zip(target.pixels.chunks_exact_mut(viewport[0] as usize))
                    {
                        for (rgba, pixel) in source.as_chunks::<4>().0.iter().zip(dest) {
                            *pixel = u32::from_be_bytes([rgba[3], rgba[0], rgba[1], rgba[2]]);
                        }
                    }
                }
            }
            continue;
        }
        let target = create_offscreen_target(
            pass.texture_handle,
            pass.width,
            pass.height,
            viewport,
            pass.float_color,
            pass.depth,
        );
        if index < targets.len() {
            targets[index] = target;
        } else {
            targets.push(target);
        }
    }
}

fn copy_target_pixels<const PRESERVE_ALPHA: bool>(target: &mut OffscreenTarget) {
    if target.with_depth {
        for (source, dest) in target
            .depth
            .chunks_exact(target.viewport[0] as usize)
            .zip(target.depth_image.chunks_exact_mut(target.width as usize))
        {
            dest[..source.len()].copy_from_slice(source);
        }
    }

    if target.float_color {
        for (source, dest) in target
            .half_pixels
            .chunks_exact(target.viewport[0] as usize)
            .zip(
                target
                    .texture
                    .half_pixels
                    .chunks_exact_mut(target.width as usize),
            )
        {
            dest[..source.len()].copy_from_slice(source);
        }
        for (rgba, &pixel) in target
            .texture
            .image
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&target.texture.half_pixels)
        {
            *rgba = unpack_half(pixel).map(|v| (clamp01(v) * 255.0).round() as u8);
        }
        return;
    }

    if !PRESERVE_ALPHA {
        for rgba in target.texture.image.as_mut().as_chunks_mut::<4>().0 {
            rgba[3] = 255;
        }
    }
    for (row, pixels) in target
        .texture
        .image
        .as_mut()
        .chunks_exact_mut(target.width as usize * 4)
        .zip(target.pixels.chunks_exact_mut(target.viewport[0] as usize))
    {
        for (rgba, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            if !PRESERVE_ALPHA {
                *pixel |= 0xff00_0000;
            }
            let pixel = *pixel;
            rgba[0] = (pixel >> 16) as u8;
            rgba[1] = (pixel >> 8) as u8;
            rgba[2] = pixel as u8;
            rgba[3] = (pixel >> 24) as u8;
        }
    }
}

fn draw_offscreen_targets(
    state: &mut State,
    frame: &RenderFrame,
    textures: &(impl TextureLookup + Sync),
) -> u32 {
    let mut targets = std::mem::take(&mut state.offscreen_targets);
    ensure_offscreen_targets(&mut targets, frame);
    let mut vertices = 0u32;
    for (index, pass) in frame.render_targets.iter().enumerate() {
        let [width, height] = targets[index].viewport.map(|value| value as usize);
        let initialized = targets[index].initialized;
        let mut pixels = std::mem::take(&mut targets[index].pixels);
        let mut half_pixels = std::mem::take(&mut targets[index].half_pixels);
        let mut depth = std::mem::take(&mut targets[index].depth);
        if !pass.preserve || !initialized {
            depth.fill(1.0);
            targets[index].depth_image.fill(1.0);
            pixels.fill(if pass.alpha { 0 } else { 0xff00_0000 });
            let clear = pack_half([0.0, 0.0, 0.0, if pass.alpha { 0.0 } else { 1.0 }]);
            half_pixels.fill(clear);
            targets[index].texture.half_pixels.fill(clear);
            if pass.alpha {
                targets[index].texture.image.as_mut().fill(0);
            } else {
                for rgba in targets[index].texture.image.as_mut().as_chunks_mut::<4>().0 {
                    *rgba = [0, 0, 0, 255];
                }
            }
        }
        let resolved = ResolvedTextures {
            external: textures,
            targets: &targets,
        };
        let software_pass = SoftwarePass::from(pass);
        let fixed_vertices = prepare_objects(
            software_pass,
            state.projection,
            &resolved,
            width,
            height,
            &mut state.prepared_objects,
            &mut state.prepared_mesh_triangles,
            &mut state.prepared_tmesh_triangles,
            false,
        );
        if pass.float_color {
            vertices = vertices.saturating_add(draw_rows(
                software_pass,
                &state.prepared_objects,
                None,
                &state.prepared_mesh_triangles,
                &state.prepared_tmesh_triangles,
                &resolved,
                width,
                height,
                0,
                height,
                &mut half_writer(&mut half_pixels, pass.alpha),
                fixed_vertices,
                &mut depth,
            ));
        } else {
            vertices = vertices.saturating_add(draw_rows(
                software_pass,
                &state.prepared_objects,
                None,
                &state.prepared_mesh_triangles,
                &state.prepared_tmesh_triangles,
                &resolved,
                width,
                height,
                0,
                height,
                &mut byte_writer(&mut pixels),
                fixed_vertices,
                &mut depth,
            ));
        }
        targets[index].pixels = pixels;
        targets[index].half_pixels = half_pixels;
        targets[index].depth = depth;
        targets[index].initialized = true;
        if pass.alpha {
            copy_target_pixels::<true>(&mut targets[index]);
        } else {
            copy_target_pixels::<false>(&mut targets[index]);
        }
    }
    state.offscreen_targets = targets;
    vertices
}

/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn draw(
    state: &mut State,
    frame: &RenderFrame,
    textures: &(impl TextureLookup + Sync),
    _apply_present_back_pressure: bool,
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

    let PhysicalSize { width, height } = state.window_size;
    if width == 0 || height == 0 {
        return Ok(DrawStats::default());
    }

    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return Ok(DrawStats::default());
    }

    let default_proj = state.projection;
    let offscreen_started = Instant::now();
    let offscreen_vertices = draw_offscreen_targets(state, frame, textures);
    let offscreen_us = elapsed_us_since(offscreen_started);
    let threads = match state.thread_hint {
        Some(threads) if threads >= 1 => threads.min(state.available_threads),
        _ => state.available_threads,
    };
    let use_parallel = threads > 1 && h >= SOFTWARE_ROW_CHUNK * 2 && !frame.ops.is_empty();
    let stage_meshes = use_parallel && h.div_ceil(SOFTWARE_ROW_CHUNK) >= MIN_STAGE_MESH_STRIPES;
    let backend_prepare_started = Instant::now();
    ensure_worker_pool(state, threads)?;
    let resolved_textures = ResolvedTextures {
        external: textures,
        targets: &state.offscreen_targets,
    };
    let software_frame = SoftwarePass::from(frame);
    let fixed_vertices = prepare_objects(
        software_frame,
        default_proj,
        &resolved_textures,
        w,
        h,
        &mut state.prepared_objects,
        &mut state.prepared_mesh_triangles,
        &mut state.prepared_tmesh_triangles,
        stage_meshes,
    );
    if use_parallel {
        state.stripe_bins.build(
            &state.prepared_objects,
            &state.prepared_mesh_triangles,
            &state.prepared_tmesh_triangles,
            h,
        );
    }
    let backend_prepare_us = elapsed_us_since(backend_prepare_started).saturating_add(offscreen_us);

    let backend_setup_started = Instant::now();
    if state.surface_resize_pending {
        let resize_w = NonZeroU32::new(width).unwrap();
        let resize_h = NonZeroU32::new(height).unwrap();
        state.surface.resize(resize_w, resize_h)?;
        state.surface_resize_pending = false;
    }

    let worker_pool = if use_parallel {
        state.worker_pool.as_ref().map(|worker| &worker.pool)
    } else {
        None
    };
    let mut buffer = state.surface.buffer_mut()?;
    let backend_setup_us = elapsed_us_since(backend_setup_started);
    let backend_record_started = Instant::now();
    let clear = pack_rgba(frame.clear_color);

    let prepared_objects = state.prepared_objects.as_slice();
    let prepared_mesh_triangles = state.prepared_mesh_triangles.as_slice();
    let prepared_tmesh_triangles = state.prepared_tmesh_triangles.as_slice();
    let stripe_bins = &state.stripe_bins;
    let vertices = if let Some(worker_pool) = worker_pool {
        let pixels: &mut [u32] = &mut buffer;
        worker_pool.install(|| {
            pixels
                .par_chunks_mut(w * SOFTWARE_ROW_CHUNK)
                .zip(state.depth.par_chunks_mut(w * SOFTWARE_ROW_CHUNK))
                .enumerate()
                .map(|(chunk_index, (stripe, depth))| {
                    stripe.fill(clear);
                    let y_start = chunk_index * SOFTWARE_ROW_CHUNK;
                    let y_end = y_start + stripe.len() / w;
                    draw_rows(
                        software_frame,
                        prepared_objects,
                        Some(stripe_bins.stripe(chunk_index)),
                        prepared_mesh_triangles,
                        prepared_tmesh_triangles,
                        &resolved_textures,
                        w,
                        h,
                        y_start,
                        y_end,
                        &mut byte_writer(stripe),
                        fixed_vertices,
                        depth,
                    )
                })
                .reduce(|| 0, u32::saturating_add)
        })
    } else {
        buffer.fill(clear);
        let depth = &mut state.depth;
        draw_rows(
            software_frame,
            prepared_objects,
            None,
            prepared_mesh_triangles,
            prepared_tmesh_triangles,
            &resolved_textures,
            w,
            h,
            0,
            h,
            &mut byte_writer(&mut buffer),
            fixed_vertices,
            depth,
        )
    };
    let backend_record_us = elapsed_us_since(backend_record_started);

    let present_started = Instant::now();
    buffer.present()?;

    // The software path retains its own prepared-object and projected-vertex storage.
    let mut storage = draw_storage_stats(frame, None);
    storage.capacities[SOFTWARE_OBJECTS_STORAGE_SLOT] =
        state.prepared_objects.capacity().min(u32::MAX as usize) as u32;
    storage.capacities[SOFTWARE_MESH_STORAGE_SLOT] = state
        .prepared_mesh_triangles
        .capacity()
        .saturating_mul(3)
        .min(u32::MAX as usize) as u32;
    storage.capacities[SOFTWARE_TMESH_STORAGE_SLOT] = state
        .prepared_tmesh_triangles
        .capacity()
        .saturating_mul(3)
        .min(u32::MAX as usize) as u32;
    Ok(DrawStats {
        vertices: vertices.saturating_add(offscreen_vertices),
        present_us: elapsed_us_since(present_started),
        backend_setup_us,
        backend_prepare_us,
        backend_record_us,
        storage,
        ..DrawStats::default()
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_objects(
    frame: SoftwarePass<'_>,
    default_proj: Matrix4,
    textures: &(impl TextureLookup + Sync),
    width: usize,
    height: usize,
    prepared: &mut Vec<PreparedObject>,
    mesh_triangles: &mut Vec<PreparedTriangle<ScreenVertexColor>>,
    tmesh_triangles: &mut Vec<PreparedTriangle<ScreenVertexTexColor>>,
    stage_meshes: bool,
) -> u32 {
    prepared.clear();
    prepared.reserve(
        frame
            .sprite_instances
            .len()
            .saturating_add(frame.tmesh_instances.len()),
    );
    mesh_triangles.clear();
    tmesh_triangles.clear();
    let mut fixed_vertices = 0u32;

    for op in frame.ops {
        if frame.depth && matches!(op, DrawOp::TexturedMesh(run) if run.clear_depth) {
            prepared.push(PreparedObject::ClearDepth);
        }
        match *op {
            DrawOp::Sprite(run) => {
                if textures.software_texture(run.texture_handle).is_none() {
                    continue;
                }
                let projection = frame
                    .cameras
                    .get(run.camera as usize)
                    .copied()
                    .unwrap_or(default_proj);
                let end = run.instance_start.saturating_add(run.instance_count);
                for sprite_index in run.instance_start..end {
                    let Some(sprite) = frame.sprite_instances.get(sprite_index as usize) else {
                        continue;
                    };
                    if sprite.tint[3] <= 0.0 {
                        continue;
                    }
                    let Some(vertices) = prepare_sprite_vertices(
                        &projection,
                        sprite.center,
                        sprite.size,
                        sprite.rot_sin_cos,
                        sprite.uv_scale,
                        sprite.uv_offset,
                        sprite.local_offset,
                        sprite.local_offset_rot_sin_cos,
                        width,
                        height,
                    ) else {
                        continue;
                    };
                    fixed_vertices = fixed_vertices.saturating_add(4);
                    prepared.push(PreparedObject::Sprite {
                        rows: sprite_rows(&vertices, height),
                        inv_denom: sprite_inv_denom(&vertices),
                        vertices,
                        tint: sprite.tint,
                        texture_mask: sprite.texture_mask != 0.0,
                        blend: run.blend,
                        texture_handle: run.texture_handle,
                    });
                }
            }
            DrawOp::Mesh(run) => {
                let projection = frame
                    .cameras
                    .get(run.camera as usize)
                    .copied()
                    .unwrap_or(default_proj);
                let start = run.vertex_start as usize;
                let end = start.saturating_add(run.vertex_count as usize);
                let Some(vertices) = frame.mesh_vertices.get(start..end) else {
                    continue;
                };
                if !stage_meshes
                    || vertices.len().div_ceil(3)
                        > mesh_triangles
                            .capacity()
                            .saturating_sub(mesh_triangles.len())
                {
                    prepared.push(PreparedObject::DirectMesh {
                        vertex_start: run.vertex_start,
                        vertex_count: run.vertex_count,
                        projection,
                        blend: run.blend,
                    });
                    continue;
                }
                let Some((triangle_start, triangle_count, projected_count, rows)) =
                    prepare_mesh_triangles(
                        mesh_triangles,
                        prepared.len() as u32,
                        &projection,
                        vertices,
                        width,
                        height,
                    )
                else {
                    continue;
                };
                fixed_vertices = fixed_vertices.saturating_add(projected_count);
                prepared.push(PreparedObject::Mesh {
                    triangle_start,
                    triangle_count,
                    rows,
                    blend: run.blend,
                });
            }
            DrawOp::TexturedMesh(run) => {
                let Some(geometry) = frame.tmesh_geometries.get(run.geometry as usize) else {
                    continue;
                };
                if textures.software_texture(run.texture_handle).is_none() {
                    continue;
                }
                let projection = frame
                    .cameras
                    .get(run.camera as usize)
                    .copied()
                    .unwrap_or(default_proj);
                let end = run.instance_start.saturating_add(run.instance_count);
                for instance_index in run.instance_start..end {
                    let Some(instance) = frame.tmesh_instances.get(instance_index as usize) else {
                        continue;
                    };
                    let mvp = projection * instance.transform();
                    // One source triangle can become two after near-plane clipping.
                    if !stage_meshes
                        || geometry.vertices.len().div_ceil(3).saturating_mul(2)
                            > tmesh_triangles
                                .capacity()
                                .saturating_sub(tmesh_triangles.len())
                        || geometry
                            .vertices
                            .first()
                            .is_some_and(|vertex| vertex.normal[3] != 0.0)
                    {
                        prepared.push(PreparedObject::DirectTexturedMesh {
                            geometry: run.geometry,
                            instance: instance_index,
                            mvp,
                            blend: run.blend,
                            depth_test: frame.depth && run.depth_test,
                            texture_handle: run.texture_handle,
                            sampler: run.sampler,
                        });
                        continue;
                    }
                    let Some((triangle_start, triangle_count, projected_count, rows)) =
                        prepare_tmesh_triangles(
                            tmesh_triangles,
                            prepared.len() as u32,
                            &mvp,
                            instance.tint,
                            instance.uv_scale,
                            instance.uv_offset,
                            instance.uv_tex_shift,
                            geometry.vertices.as_ref(),
                            width,
                            height,
                            instance.cull_back > 0.5,
                        )
                    else {
                        continue;
                    };
                    fixed_vertices = fixed_vertices.saturating_add(projected_count);
                    prepared.push(PreparedObject::TexturedMesh {
                        triangle_start,
                        triangle_count,
                        rows,
                        texture_mask: instance.texture_mask != 0.0,
                        blend: run.blend,
                        depth_test: frame.depth && run.depth_test,
                        texture_handle: run.texture_handle,
                        sampler: run.sampler,
                    });
                }
            }
        }
    }
    fixed_vertices
}

fn draw_rows(
    frame: SoftwarePass<'_>,
    prepared_objects: &[PreparedObject],
    stripe_items: Option<&[StripeItem]>,
    mesh_triangles: &[PreparedTriangle<ScreenVertexColor>],
    tmesh_triangles: &[PreparedTriangle<ScreenVertexTexColor>],
    textures: &(impl TextureLookup + Sync),
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    fixed_vertices: u32,

    depth: &mut [f32],
) -> u32 {
    let mut vertices_drawn = fixed_vertices;
    let mut texture_cache = None;
    // `depth` still holds a previous pass; it is reset before its first use.
    let mut depth_reset = frame.reset_depth;
    let mut depth = DepthRows {
        pixels: depth,
        unorm16: frame.depth_unorm,
    };
    if let Some(items) = stripe_items {
        for &item in items {
            let (object, triangle) = if item.is_whole() {
                (item.index(), None)
            } else if item.is_tmesh() {
                let triangle = item.index();
                (tmesh_triangles[triangle].object as usize, Some(triangle))
            } else {
                let triangle = item.index();
                (mesh_triangles[triangle].object as usize, Some(triangle))
            };
            let prepared = &prepared_objects[object];
            if !apply_depth_reset(prepared, depth.pixels, &mut depth_reset) {
                continue;
            }
            if let Some(triangle) = triangle {
                draw_prepared_triangle(
                    prepared,
                    triangle as u32,
                    mesh_triangles,
                    tmesh_triangles,
                    textures,
                    &mut texture_cache,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                    width,
                    &mut depth,
                );
            } else {
                vertices_drawn = vertices_drawn.saturating_add(draw_prepared(
                    prepared,
                    true,
                    frame,
                    mesh_triangles,
                    tmesh_triangles,
                    textures,
                    &mut texture_cache,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                    &mut depth,
                ));
            }
        }
    } else {
        for prepared in prepared_objects {
            if !apply_depth_reset(prepared, depth.pixels, &mut depth_reset) {
                continue;
            }
            vertices_drawn = vertices_drawn.saturating_add(draw_prepared(
                prepared,
                false,
                frame,
                mesh_triangles,
                tmesh_triangles,
                textures,
                &mut texture_cache,
                width,
                height,
                stripe_y_start,
                stripe_y_end,
                buffer,
                &mut depth,
            ));
        }
    }
    vertices_drawn
}

/// Defers depth resets to the next depth-tested draw in these rows. Only those
/// draws read or write depth, so rows they never reach skip the fill. Returns
/// false for reset markers, which draw nothing.
#[inline(always)]
fn apply_depth_reset(prepared: &PreparedObject, depth: &mut [f32], pending: &mut bool) -> bool {
    match prepared {
        PreparedObject::ClearDepth => {
            *pending = true;
            false
        }
        PreparedObject::TexturedMesh {
            depth_test: true, ..
        }
        | PreparedObject::DirectTexturedMesh {
            depth_test: true, ..
        } => {
            if std::mem::take(pending) {
                depth.fill(1.0);
            }
            true
        }
        _ => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_prepared<'a>(
    prepared: &PreparedObject,
    rows_known_visible: bool,
    frame: SoftwarePass<'_>,
    mesh_triangles: &[PreparedTriangle<ScreenVertexColor>],
    tmesh_triangles: &[PreparedTriangle<ScreenVertexTexColor>],
    textures: &'a (impl TextureLookup + Sync),
    texture_cache: &mut Option<(TextureHandle, Option<&'a Texture>)>,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),

    depth: &mut DepthRows<'_>,
) -> u32 {
    let mut no_depth = DepthRows {
        pixels: &mut [],
        unorm16: false,
    };
    match prepared {
        // `draw_rows` applies resets before dispatching depth-tested draws.
        PreparedObject::ClearDepth => 0,
        PreparedObject::Sprite {
            vertices,
            rows,
            inv_denom,
            tint,
            texture_mask,
            blend,
            texture_handle,
        } => {
            if rows_known_visible || rows.overlaps(stripe_y_start, stripe_y_end) {
                let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                    return 0;
                };
                rasterize_prepared_sprite(
                    vertices,
                    *inv_denom,
                    *tint,
                    *texture_mask,
                    *blend,
                    tex.texels(),
                    effective_sampler(tex, *texture_handle),
                    tex.opaque,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                );
            }
            0
        }
        PreparedObject::Mesh {
            triangle_start,
            triangle_count,
            rows,
            blend,
            ..
        } => {
            if rows_known_visible || rows.overlaps(stripe_y_start, stripe_y_end) {
                let start = *triangle_start as usize;
                let end = start + *triangle_count as usize;
                rasterize_prepared_mesh(
                    &mesh_triangles[start..end],
                    *blend,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                    width,
                );
            }
            0
        }
        PreparedObject::DirectMesh {
            vertex_start,
            vertex_count,
            projection,
            blend,
        } => {
            let start = *vertex_start as usize;
            let end = start.saturating_add(*vertex_count as usize);
            let Some(vertices) = frame.mesh_vertices.get(start..end) else {
                return 0;
            };
            rasterize_mesh_triangles(
                projection,
                vertices,
                *blend,
                width,
                height,
                stripe_y_start,
                stripe_y_end,
                buffer,
            )
        }
        PreparedObject::TexturedMesh {
            triangle_start,
            triangle_count,
            rows,
            texture_mask,
            blend,
            depth_test,
            texture_handle,
            sampler,
            ..
        } => {
            if rows_known_visible || rows.overlaps(stripe_y_start, stripe_y_end) {
                let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                    return 0;
                };
                let start = *triangle_start as usize;
                let end = start + *triangle_count as usize;
                rasterize_prepared_tmesh(
                    &tmesh_triangles[start..end],
                    *texture_mask,
                    *blend,
                    tex.texels(),
                    texture_sampler_desc(tex.sampler, *texture_handle, true, *sampler),
                    tex.opaque,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                    width,
                    if *depth_test { depth } else { &mut no_depth },
                );
            }
            0
        }
        PreparedObject::DirectTexturedMesh {
            geometry,
            instance,
            mvp,
            blend,
            depth_test,
            texture_handle,
            sampler,
        } => {
            let Some(geometry) = frame.tmesh_geometries.get(*geometry as usize) else {
                return 0;
            };
            let Some(instance) = frame.tmesh_instances.get(*instance as usize) else {
                return 0;
            };
            let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                return 0;
            };
            if geometry
                .vertices
                .first()
                .is_some_and(|vertex| vertex.normal[3] != 0.0)
            {
                let additive = textures.software_texture(instance.additive_texture);
                return rasterize_environment(
                    mvp,
                    geometry.vertices.as_ref(),
                    *instance,
                    *blend,
                    tex,
                    texture_sampler_desc(tex.sampler, *texture_handle, true, *sampler),
                    additive,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                    if *depth_test { depth } else { &mut no_depth },
                );
            }
            rasterize_textured_mesh_triangles(
                mvp,
                geometry.vertices.as_ref(),
                instance.tint,
                instance.uv_scale,
                instance.uv_offset,
                instance.uv_tex_shift,
                instance.texture_mask != 0.0,
                *blend,
                tex.texels(),
                texture_sampler_desc(tex.sampler, *texture_handle, true, *sampler),
                tex.opaque,
                width,
                height,
                stripe_y_start,
                stripe_y_end,
                buffer,
                instance.cull_back > 0.5,
                if *depth_test { depth } else { &mut no_depth },
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_prepared_triangle<'a>(
    prepared: &PreparedObject,
    triangle: u32,
    mesh_triangles: &[PreparedTriangle<ScreenVertexColor>],
    tmesh_triangles: &[PreparedTriangle<ScreenVertexTexColor>],
    textures: &'a (impl TextureLookup + Sync),
    texture_cache: &mut Option<(TextureHandle, Option<&'a Texture>)>,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    let mut no_depth = DepthRows {
        pixels: &mut [],
        unorm16: false,
    };
    match prepared {
        PreparedObject::Mesh { blend, .. } => {
            let triangle = &mesh_triangles[triangle as usize];
            rasterize_triangle_color_prepared(
                &triangle.vertices,
                triangle.setup,
                *blend,
                stripe_y_start,
                stripe_y_end,
                buffer,
                width,
            );
        }
        PreparedObject::TexturedMesh {
            texture_mask,
            blend,
            depth_test,
            texture_handle,
            sampler,
            ..
        } => {
            let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                return;
            };
            let triangle = &tmesh_triangles[triangle as usize];
            rasterize_triangle_tex_color_prepared(
                &triangle.vertices,
                triangle.setup,
                *blend,
                *texture_mask,
                tex.texels(),
                texture_sampler_desc(tex.sampler, *texture_handle, true, *sampler),
                tex.opaque,
                stripe_y_start,
                stripe_y_end,
                buffer,
                width,
                if *depth_test { depth } else { &mut no_depth },
            );
        }
        _ => debug_assert!(false, "whole objects must use whole-object stripe items"),
    }
}

#[inline(always)]
fn resolve_texture<'a>(
    textures: &'a (impl TextureLookup + Sync),
    cache: &mut Option<(TextureHandle, Option<&'a Texture>)>,
    handle: TextureHandle,
) -> Option<&'a Texture> {
    if let Some((cached_handle, texture)) = *cache
        && cached_handle == handle
    {
        return texture;
    }
    let texture = textures.software_texture(handle);
    *cache = Some((handle, texture));
    texture
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    let window_size = PhysicalSize::new(width, height);
    state.surface_resize_pending |= state.window_size != window_size;
    state.window_size = window_size;
    state.depth.resize(width as usize * height as usize, 1.0);
}

pub const fn set_default_projection(state: &mut State, projection: Matrix4) {
    state.projection = projection;
}

pub fn cleanup(_state: &mut State) {
    info!("Software renderer backend cleanup.");
}

#[inline(always)]
const fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[inline(always)]
const fn pack_rgba(c: [f32; 4]) -> u32 {
    u32::from_be_bytes([
        c[3].mul_add(255.0, 0.5) as u8,
        c[0].mul_add(255.0, 0.5) as u8,
        c[1].mul_add(255.0, 0.5) as u8,
        c[2].mul_add(255.0, 0.5) as u8,
    ])
}

#[derive(Clone, Copy)]
struct ScreenVertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
}

#[derive(Clone, Copy)]
struct ScreenVertexColor {
    x: f32,
    y: f32,
    color: [f32; 4],
}

#[derive(Clone, Copy)]
struct ScreenVertexTexColor {
    inv_w: f32,
    z: f32,
    x: f32,
    y: f32,
    u: f32,
    v: f32,
    color: [f32; 4],
}

#[derive(Clone, Copy)]
struct ClipVertexTexColor {
    clip: Vector4,
    u: f32,
    v: f32,
    color: [f32; 4],
}

#[derive(Clone, Copy)]
struct RasterSetup {
    min_x: i32,
    max_x: i32,
    min_y: i32,
    max_y: i32,
    inv_denom: f32,
}

#[derive(Clone, Copy)]
struct PreparedTriangle<V> {
    object: u32,
    vertices: [V; 3],
    setup: RasterSetup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScreenRows {
    start: u32,
    end: u32,
}

impl ScreenRows {
    #[inline(always)]
    fn from_bounds(min_y: f32, max_y: f32, height: usize) -> Self {
        debug_assert!(height > 0);
        let start = min_y as u32;
        let max = max_y.ceil().min((height - 1) as f32) as u32;
        if start > max {
            Self { start: 0, end: 0 }
        } else {
            Self {
                start,
                end: max + 1,
            }
        }
    }

    #[inline(always)]
    const fn overlaps(self, start: usize, end: usize) -> bool {
        self.start < end as u32 && self.end > start as u32
    }
}

#[inline(always)]
fn sprite_rows(vertices: &[ScreenVertex; 4], height: usize) -> ScreenRows {
    let min_y = vertices
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::INFINITY, f32::min);
    let max_y = vertices
        .iter()
        .map(|vertex| vertex.y)
        .fold(f32::NEG_INFINITY, f32::max);
    ScreenRows::from_bounds(min_y, max_y, height)
}

#[inline(always)]
fn sprite_inv_denom(vertices: &[ScreenVertex; 4]) -> [Option<f32>; 2] {
    [
        triangle_inv_denom(&vertices[0], &vertices[1], &vertices[2]),
        triangle_inv_denom(&vertices[0], &vertices[2], &vertices[3]),
    ]
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn prepare_sprite_vertices(
    proj: &Matrix4,
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
    width: usize,
    height: usize,
) -> Option<[ScreenVertex; 4]> {
    if width == 0 || height == 0 {
        return None;
    }

    let mut adjusted_center = center;
    if local_offset[0] != 0.0 || local_offset[1] != 0.0 {
        let s = local_offset_rot_sin_cos[0];
        let c = local_offset_rot_sin_cos[1];
        let ox = c.mul_add(local_offset[0], -(s * local_offset[1]));
        let oy = s.mul_add(local_offset[0], c * local_offset[1]);
        adjusted_center[0] += ox;
        adjusted_center[1] += oy;
    }

    const POS: [(f32, f32); 4] = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    const UV_BASE: [(f32, f32); 4] = [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)];

    let mut v = [ScreenVertex {
        x: 0.0,
        y: 0.0,
        u: 0.0,
        v: 0.0,
    }; 4];

    for i in 0..4 {
        let (lx, ly) = POS[i];
        let local_x = lx * size[0];
        let local_y = ly * size[1];
        let world = Vector4::new(
            rot_sin_cos[1].mul_add(local_x, -(rot_sin_cos[0] * local_y) + adjusted_center[0]),
            rot_sin_cos[0].mul_add(local_x, rot_sin_cos[1].mul_add(local_y, adjusted_center[1])),
            adjusted_center[2],
            1.0,
        );
        let clip = *proj * world;
        if clip.w == 0.0 {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;

        let sx = f32::midpoint(ndc_x, 1.0) * (width as f32);
        let sy = ((1.0 - ndc_y) * 0.5) * (height as f32);

        let (u0, v0) = UV_BASE[i];
        let u = u0.mul_add(uv_scale[0], uv_offset[0]);
        let vv = v0.mul_add(uv_scale[1], uv_offset[1]);

        v[i] = ScreenVertex {
            x: sx,
            y: sy,
            u,
            v: vv,
        };
    }

    Some(v)
}

fn prepare_mesh_triangles(
    out: &mut Vec<PreparedTriangle<ScreenVertexColor>>,
    object: u32,
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::MeshVertex],
    width: usize,
    height: usize,
) -> Option<(u32, u32, u32, ScreenRows)> {
    if vertices.is_empty() || width == 0 || height == 0 {
        return None;
    }
    let start = out.len();
    let mut projected_count = 0u32;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    'tri: for chunk in vertices.as_chunks::<3>().0 {
        let mut tri = [ScreenVertexColor {
            x: 0.0,
            y: 0.0,
            color: [0.0; 4],
        }; 3];
        for i in 0..3 {
            let p = chunk[i].pos;
            let clip = *mvp * Vector4::new(p[0], p[1], 0.0, 1.0);
            if clip.w == 0.0 {
                continue 'tri;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            if !ndc_x.is_finite() || !ndc_y.is_finite() {
                continue 'tri;
            }
            tri[i] = ScreenVertexColor {
                x: f32::midpoint(ndc_x, 1.0) * width as f32,
                y: ((1.0 - ndc_y) * 0.5) * height as f32,
                color: chunk[i].color,
            };
        }
        projected_count = projected_count.saturating_add(3);
        let Some(setup) = triangle_setup(
            [tri[0].x, tri[1].x, tri[2].x],
            [tri[0].y, tri[1].y, tri[2].y],
            width,
            height,
        ) else {
            continue;
        };
        min_y = min_y.min(setup.min_y);
        max_y = max_y.max(setup.max_y);
        out.push(PreparedTriangle {
            object,
            vertices: tri,
            setup,
        });
    }
    let rows = if min_y > max_y {
        ScreenRows { start: 0, end: 0 }
    } else {
        ScreenRows {
            start: min_y as u32,
            end: max_y as u32 + 1,
        }
    };
    Some((
        start as u32,
        (out.len() - start) as u32,
        projected_count,
        rows,
    ))
}

/// Clips one triangle against OpenGL's homogeneous near plane (`z >= -w`).
/// A single plane produces at most four vertices, all kept on the stack.
#[inline]
fn clip_tmesh_near(triangle: [ClipVertexTexColor; 3]) -> ([ClipVertexTexColor; 4], usize) {
    let distances = triangle.map(|vertex| vertex.clip.z + vertex.clip.w);
    if distances.iter().all(|distance| *distance >= 0.0) {
        return ([triangle[0], triangle[1], triangle[2], triangle[0]], 3);
    }
    if distances.iter().all(|distance| *distance < 0.0) {
        return ([triangle[0]; 4], 0);
    }

    let mut out = [triangle[0]; 4];
    let mut len = 0usize;
    let mut previous = triangle[2];
    let mut previous_distance = distances[2];
    let mut previous_inside = previous_distance >= 0.0;

    for (current, current_distance) in triangle.into_iter().zip(distances) {
        let current_inside = current_distance >= 0.0;
        if current_inside != previous_inside {
            let t = previous_distance / (previous_distance - current_distance);
            out[len] = ClipVertexTexColor {
                clip: previous.clip + (current.clip - previous.clip) * t,
                u: (current.u - previous.u).mul_add(t, previous.u),
                v: (current.v - previous.v).mul_add(t, previous.v),
                color: std::array::from_fn(|channel| {
                    (current.color[channel] - previous.color[channel])
                        .mul_add(t, previous.color[channel])
                }),
            };
            len += 1;
        }
        if current_inside {
            out[len] = current;
            len += 1;
        }
        previous = current;
        previous_distance = current_distance;
        previous_inside = current_inside;
    }
    (out, len)
}

#[allow(clippy::too_many_arguments)]
fn project_tmesh_polygon(
    mvp: &Matrix4,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    width: usize,
    height: usize,
    cull_back: bool,
) -> Option<([ScreenVertexTexColor; 4], usize)> {
    debug_assert_eq!(vertices.len(), 3);
    let mut triangle = [ClipVertexTexColor {
        clip: Vector4::ZERO,
        u: 0.0,
        v: 0.0,
        color: [0.0; 4],
    }; 3];
    for i in 0..3 {
        let vertex = vertices[i];
        let p = vertex.pos;
        let clip = *mvp * Vector4::new(p[0], p[1], p[2], 1.0);
        if !clip.is_finite() {
            return None;
        }
        triangle[i] = ClipVertexTexColor {
            clip,
            u: uv_tex_shift[0].mul_add(
                vertex.tex_matrix_scale[0] - 1.0,
                vertex.uv[0].mul_add(uv_scale[0], uv_offset[0]),
            ),
            v: uv_tex_shift[1].mul_add(
                vertex.tex_matrix_scale[1] - 1.0,
                vertex.uv[1].mul_add(uv_scale[1], uv_offset[1]),
            ),
            color: [
                vertex.color[0] * tint[0],
                vertex.color[1] * tint[1],
                vertex.color[2] * tint[2],
                vertex.color[3] * tint[3],
            ],
        };
    }

    let (clipped, len) = clip_tmesh_near(triangle);
    let polygon = &clipped[..len];
    let mut projected = [ScreenVertexTexColor {
        inv_w: 1.0,
        z: 0.0,
        x: 0.0,
        y: 0.0,
        u: 0.0,
        v: 0.0,
        color: [0.0; 4],
    }; 4];
    for (i, vertex) in polygon.iter().copied().enumerate() {
        if vertex.clip.w == 0.0 {
            return None;
        }
        let ndc_x = vertex.clip.x / vertex.clip.w;
        let ndc_y = vertex.clip.y / vertex.clip.w;
        if !ndc_x.is_finite() || !ndc_y.is_finite() {
            return None;
        }
        projected[i] = ScreenVertexTexColor {
            inv_w: vertex.clip.w.recip(),
            z: (vertex.clip.z / vertex.clip.w + 1.0) * 0.5,
            x: f32::midpoint(ndc_x, 1.0) * width as f32,
            y: ((1.0 - ndc_y) * 0.5) * height as f32,
            u: vertex.u,
            v: vertex.v,
            color: vertex.color,
        };
    }
    if cull_back && polygon.len() >= 3 {
        let [a, b, c] = [projected[0], projected[1], projected[2]];
        // Screen Y points down, so a CCW clip-space face has negative area.
        if (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x) >= 0.0 {
            return None;
        }
    }
    Some((projected, polygon.len()))
}

#[allow(clippy::too_many_arguments)]
fn prepare_tmesh_triangles(
    out: &mut Vec<PreparedTriangle<ScreenVertexTexColor>>,
    object: u32,
    mvp: &Matrix4,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    width: usize,
    height: usize,
    cull_back: bool,
) -> Option<(u32, u32, u32, ScreenRows)> {
    if vertices.is_empty() || width == 0 || height == 0 {
        return None;
    }
    let start = out.len();
    let mut projected_count = 0u32;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for chunk in vertices.as_chunks::<3>().0 {
        let Some((polygon, polygon_len)) = project_tmesh_polygon(
            mvp,
            tint,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            chunk,
            width,
            height,
            cull_back,
        ) else {
            continue;
        };
        projected_count = projected_count.saturating_add(3);
        for index in 1..polygon_len.saturating_sub(1) {
            let tri = [polygon[0], polygon[index], polygon[index + 1]];
            let Some(setup) = triangle_setup(
                [tri[0].x, tri[1].x, tri[2].x],
                [tri[0].y, tri[1].y, tri[2].y],
                width,
                height,
            ) else {
                continue;
            };
            min_y = min_y.min(setup.min_y);
            max_y = max_y.max(setup.max_y);
            out.push(PreparedTriangle {
                object,
                vertices: tri,
                setup,
            });
        }
    }
    let rows = if min_y > max_y {
        ScreenRows { start: 0, end: 0 }
    } else {
        ScreenRows {
            start: min_y as u32,
            end: max_y as u32 + 1,
        }
    };
    Some((
        start as u32,
        (out.len() - start) as u32,
        projected_count,
        rows,
    ))
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn rasterize_prepared_sprite(
    vertices: &[ScreenVertex; 4],
    inv_denom: [Option<f32>; 2],
    tint: [f32; 4],
    texture_mask: bool,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    if width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return;
    }

    if let Some(inv_denom) = inv_denom[0] {
        rasterize_triangle_with_inv(
            &vertices[0],
            &vertices[1],
            &vertices[2],
            inv_denom,
            tint,
            texture_mask,
            blend,
            image,
            sampler,
            opaque,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
    }
    if let Some(inv_denom) = inv_denom[1] {
        rasterize_triangle_with_inv(
            &vertices[0],
            &vertices[2],
            &vertices[3],
            inv_denom,
            tint,
            texture_mask,
            blend,
            image,
            sampler,
            opaque,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
    }
}

fn rasterize_prepared_mesh(
    triangles: &[PreparedTriangle<ScreenVertexColor>],
    blend: BlendMode,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,
) {
    for triangle in triangles {
        rasterize_triangle_color_prepared(
            &triangle.vertices,
            triangle.setup,
            blend,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn rasterize_prepared_tmesh(
    triangles: &[PreparedTriangle<ScreenVertexTexColor>],
    texture_mask: bool,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    for triangle in triangles {
        rasterize_triangle_tex_color_prepared(
            &triangle.vertices,
            triangle.setup,
            blend,
            texture_mask,
            image,
            sampler,
            opaque,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        );
    }
}

fn rasterize_mesh_triangles(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::MeshVertex],
    blend: BlendMode,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) -> u32 {
    if vertices.len() < 3 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
    }

    let mut tri: [ScreenVertexColor; 3] = [ScreenVertexColor {
        x: 0.0,
        y: 0.0,
        color: [0.0; 4],
    }; 3];

    let mut verts_drawn = 0u32;
    'tri: for chunk in vertices.as_chunks::<3>().0 {
        for i in 0..3 {
            let p = chunk[i].pos;
            let clip = *mvp * Vector4::new(p[0], p[1], 0.0, 1.0);
            if clip.w == 0.0 {
                continue 'tri;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            if !ndc_x.is_finite() || !ndc_y.is_finite() {
                continue 'tri;
            }

            let sx = f32::midpoint(ndc_x, 1.0) * (width as f32);
            let sy = ((1.0 - ndc_y) * 0.5) * (height as f32);
            tri[i] = ScreenVertexColor {
                x: sx,
                y: sy,
                color: chunk[i].color,
            };
        }

        rasterize_triangle_color(
            &tri[0],
            &tri[1],
            &tri[2],
            blend,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
        verts_drawn = verts_drawn.saturating_add(3);
    }

    verts_drawn
}

// Environment coordinates are generated at vertices, then perspective-correctly
// interpolated. Stack triangles also preserve clipping without transient buffers.
#[allow(clippy::too_many_arguments)]
fn rasterize_environment(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    instance: deadlib_render_core::TexturedMeshInstanceRaw,
    blend: BlendMode,
    primary: &Texture,
    primary_sampler: SamplerDesc,
    additive: Option<&Texture>,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    depth: &mut DepthRows<'_>,
) -> u32 {
    let mut count = 0;
    for triangle in vertices.as_chunks::<3>().0 {
        let mut first = *triangle;
        let mut second = *triangle;
        for i in 0..3 {
            let uv = deadlib_render_core::textured_mesh_uvs(triangle[i], instance);
            first[i].uv = uv[0];
            second[i].uv = uv[1];
        }
        let Some((p, len)) = project_tmesh_polygon(
            mvp,
            instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &first,
            width,
            height,
            instance.cull_back > 0.5,
        ) else {
            continue;
        };
        let Some((q, _)) = project_tmesh_polygon(
            mvp,
            instance.tint,
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &second,
            width,
            height,
            instance.cull_back > 0.5,
        ) else {
            continue;
        };
        count += 3;
        for i in 1..len.saturating_sub(1) {
            let p = [p[0], p[i], p[i + 1]];
            let q = [q[0], q[i], q[i + 1]];
            let Some(setup) = triangle_setup(p.map(|v| v.x), p.map(|v| v.y), width, height) else {
                continue;
            };
            let Some((min_x, max_x, min_y, max_y, start)) =
                setup.stripe_bounds(stripe_y_start, stripe_y_end)
            else {
                continue;
            };
            let sample = |texture: &Texture, sampler: SamplerDesc, uv: [f32; 2]| {
                let image = texture.texels();
                if sampler.filter == SamplerFilter::Linear {
                    sample_tex_linear::<false>(
                        image,
                        image.width,
                        image.height,
                        uv[0],
                        uv[1],
                        sampler,
                    )
                } else {
                    sample_tex_nearest::<false>(
                        image,
                        image.width,
                        image.height,
                        uv[0],
                        uv[1],
                        sampler,
                    )
                }
                .unwrap_or([0.0; 4])
            };
            let edges = [
                owns_edge(p[1].x, p[1].y, p[2].x, p[2].y, setup.inv_denom),
                owns_edge(p[2].x, p[2].y, p[0].x, p[0].y, setup.inv_denom),
                owns_edge(p[0].x, p[0].y, p[1].x, p[1].y, setup.inv_denom),
            ];
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let px = x as f32 + 0.5;
                    let py = y as f32 + 0.5;
                    let a = edge_function(p[1].x, p[1].y, p[2].x, p[2].y, px, py) * setup.inv_denom;
                    let b = edge_function(p[2].x, p[2].y, p[0].x, p[0].y, px, py) * setup.inv_denom;
                    let c = 1.0 - a - b;
                    if !covered([a, b, c], edges) {
                        continue;
                    }
                    let z = a * p[0].z + b * p[1].z + c * p[2].z;
                    let sum = a * p[0].inv_w + b * p[1].inv_w + c * p[2].inv_w;
                    let w = [
                        a * p[0].inv_w / sum,
                        b * p[1].inv_w / sum,
                        c * p[2].inv_w / sum,
                    ];
                    let uv = |v: &[ScreenVertexTexColor; 3]| {
                        [
                            w[0] * v[0].u + w[1] * v[1].u + w[2] * v[2].u,
                            w[0] * v[0].v + w[1] * v[1].v + w[2] * v[2].v,
                        ]
                    };
                    let texel = sample(primary, primary_sampler, uv(&p));
                    let tint: [f32; 4] = std::array::from_fn(|i| {
                        w[0] * p[0].color[i] + w[1] * p[1].color[i] + w[2] * p[2].color[i]
                    });
                    let mut color: [f32; 4] = std::array::from_fn(|i| texel[i] * tint[i]);
                    if instance.texture_mask > 0.5 {
                        color[..3].copy_from_slice(&tint[..3]);
                    } else if triangle[0].normal[3] as u8 & 4 != 0 {
                        let reflection = sample(
                            additive.unwrap_or(primary),
                            SamplerDesc {
                                wrap: SamplerWrap::Repeat,
                                ..additive.unwrap_or(primary).sampler
                            },
                            uv(&q),
                        );
                        for i in 0..3 {
                            color[i] = (color[i] + reflection[i]).min(1.0);
                        }
                        color[3] *= reflection[3];
                    }
                    if color[3] <= 1.0 / 256.0 {
                        continue;
                    }
                    let index = (y - start) as usize * width + x as usize;
                    if !depth.test(index, z) {
                        continue;
                    }
                    buffer(index, color, blend);
                }
            }
        }
    }
    count
}

fn rasterize_textured_mesh_triangles(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: bool,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    cull_back: bool,

    depth: &mut DepthRows<'_>,
) -> u32 {
    if vertices.len() < 3 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
    }

    let mut verts_drawn = 0u32;
    for chunk in vertices.as_chunks::<3>().0 {
        let Some((polygon, polygon_len)) = project_tmesh_polygon(
            mvp,
            tint,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            chunk,
            width,
            height,
            cull_back,
        ) else {
            continue;
        };
        verts_drawn = verts_drawn.saturating_add(3);
        for index in 1..polygon_len.saturating_sub(1) {
            rasterize_triangle_tex_color(
                &polygon[0],
                &polygon[index],
                &polygon[index + 1],
                blend,
                texture_mask,
                image,
                sampler,
                opaque,
                width,
                height,
                stripe_y_start,
                stripe_y_end,
                buffer,
                depth,
            );
        }
    }

    verts_drawn
}

#[inline(always)]
fn rasterize_triangle_with_inv(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
    texture_mask: bool,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    match (texture_mask, opaque) {
        (false, false) => rasterize_triangle_mode::<false, false>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (false, true) => rasterize_triangle_mode::<false, true>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (true, false) => rasterize_triangle_mode::<true, false>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (true, true) => rasterize_triangle_mode::<true, true>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            blend,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn rasterize_triangle_mode<const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    match sampler.filter {
        SamplerFilter::Nearest => rasterize_triangle_impl::<false, MASK, OPAQUE>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            blend,
            buffer,
        ),
        SamplerFilter::Linear => rasterize_triangle_impl::<true, MASK, OPAQUE>(
            v0,
            v1,
            v2,
            inv_denom,
            tint,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            blend,
            buffer,
        ),
    }
}

#[inline(always)]
fn rasterize_triangle_tex_color(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    blend: BlendMode,
    texture_mask: bool,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),

    depth: &mut DepthRows<'_>,
) {
    let Some(setup) = triangle_setup_in_rows(
        [v0.x, v1.x, v2.x],
        [v0.y, v1.y, v2.y],
        width,
        height,
        stripe_y_start,
        stripe_y_end,
    ) else {
        return;
    };
    rasterize_triangle_tex_color_prepared(
        &[*v0, *v1, *v2],
        setup,
        blend,
        texture_mask,
        image,
        sampler,
        opaque,
        stripe_y_start,
        stripe_y_end,
        buffer,
        width,
        depth,
    );
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn rasterize_triangle_tex_color_prepared(
    vertices: &[ScreenVertexTexColor; 3],
    setup: RasterSetup,
    blend: BlendMode,
    texture_mask: bool,
    image: Texels<'_>,
    sampler: SamplerDesc,
    opaque: bool,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    let [v0, v1, v2] = vertices;
    match (texture_mask, opaque) {
        (false, false) => rasterize_triangle_tex_color_mode::<false, false>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
        (false, true) => rasterize_triangle_tex_color_mode::<false, true>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
        (true, false) => rasterize_triangle_tex_color_mode::<true, false>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
        (true, true) => rasterize_triangle_tex_color_mode::<true, true>(
            v0,
            v1,
            v2,
            setup,
            blend,
            image,
            sampler,
            stripe_y_start,
            stripe_y_end,
            buffer,
            width,
            depth,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn rasterize_triangle_tex_color_mode<const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    setup: RasterSetup,
    blend: BlendMode,
    image: Texels<'_>,
    sampler: SamplerDesc,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,

    depth: &mut DepthRows<'_>,
) {
    match sampler.filter {
        SamplerFilter::Nearest => {
            rasterize_triangle_tex_color_impl::<false, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                blend,
                buffer,
                depth,
            );
        }
        SamplerFilter::Linear => {
            rasterize_triangle_tex_color_impl::<true, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                blend,
                buffer,
                depth,
            );
        }
    }
}

#[inline(always)]
fn rasterize_triangle_color(
    v0: &ScreenVertexColor,
    v1: &ScreenVertexColor,
    v2: &ScreenVertexColor,
    blend: BlendMode,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    let Some(setup) = triangle_setup_in_rows(
        [v0.x, v1.x, v2.x],
        [v0.y, v1.y, v2.y],
        width,
        height,
        stripe_y_start,
        stripe_y_end,
    ) else {
        return;
    };
    rasterize_triangle_color_prepared(
        &[*v0, *v1, *v2],
        setup,
        blend,
        stripe_y_start,
        stripe_y_end,
        buffer,
        width,
    );
}

#[inline(always)]
fn rasterize_triangle_color_prepared(
    vertices: &[ScreenVertexColor; 3],
    setup: RasterSetup,
    blend: BlendMode,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
    width: usize,
) {
    let [v0, v1, v2] = vertices;
    rasterize_triangle_color_impl(
        v0,
        v1,
        v2,
        setup,
        width,
        stripe_y_start,
        stripe_y_end,
        blend,
        buffer,
    );
}

#[inline(always)]
fn wrap_uv(u: f32, wrap: SamplerWrap) -> f32 {
    match wrap {
        SamplerWrap::Clamp => clamp01(u),
        SamplerWrap::Repeat => {
            let mut f = u.fract();
            if f < 0.0 {
                f += 1.0;
            }
            f
        }
    }
}

#[inline(always)]
fn wrap_index(i: i32, max: usize, wrap: SamplerWrap) -> usize {
    match wrap {
        SamplerWrap::Clamp => i.clamp(0, max.saturating_sub(1) as i32) as usize,
        SamplerWrap::Repeat => {
            if max.is_power_of_two() {
                return i as usize & (max - 1);
            }
            let m = max as i32;
            if m == 0 {
                0
            } else {
                let mut v = i % m;
                if v < 0 {
                    v += m;
                }
                v as usize
            }
        }
    }
}

#[inline(always)]
fn sample_tex_nearest<const OPAQUE: bool>(
    tex_data: Texels<'_>,
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<[f32; 4]> {
    let tx = wrap_index(
        (wrap_uv(u, sampler.wrap) * tex_w as f32) as i32,
        tex_w,
        sampler.wrap,
    );
    let ty = wrap_index(
        (wrap_uv(v, sampler.wrap) * tex_h as f32) as i32,
        tex_h,
        sampler.wrap,
    );
    let mut color = tex_data.pixel(ty * tex_w + tx)?;
    if OPAQUE {
        color[3] = 1.0;
    }
    Some(color)
}

#[inline(always)]
fn sample_alpha_nearest(
    tex_data: Texels<'_>,
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<f32> {
    sample_tex_nearest::<false>(tex_data, tex_w, tex_h, u, v, sampler).map(|c| c[3])
}

#[inline(always)]
fn sample_tex_linear<const OPAQUE: bool>(
    tex_data: Texels<'_>,
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<[f32; 4]> {
    let x = wrap_uv(u, sampler.wrap).mul_add(tex_w as f32, -0.5);
    let y = wrap_uv(v, sampler.wrap).mul_add(tex_h as f32, -0.5);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = clamp01(x - x0 as f32);
    let fy = clamp01(y - y0 as f32);
    let ix0 = wrap_index(x0, tex_w, sampler.wrap);
    let ix1 = wrap_index(x0 + 1, tex_w, sampler.wrap);
    let iy0 = wrap_index(y0, tex_h, sampler.wrap);
    let iy1 = wrap_index(y0 + 1, tex_h, sampler.wrap);
    let mut colors = [
        tex_data.pixel(iy0 * tex_w + ix0)?,
        tex_data.pixel(iy0 * tex_w + ix1)?,
        tex_data.pixel(iy1 * tex_w + ix0)?,
        tex_data.pixel(iy1 * tex_w + ix1)?,
    ];
    if OPAQUE {
        for c in &mut colors {
            c[3] = 1.0;
        }
    } else if colors.iter().all(|c| c[3] == 0.0) {
        return None;
    }
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    Some(std::array::from_fn(|i| {
        lerp(
            lerp(colors[0][i], colors[1][i], fx),
            lerp(colors[2][i], colors[3][i], fx),
            fy,
        )
    }))
}

#[inline(always)]
fn sample_alpha_linear(
    tex_data: Texels<'_>,
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<f32> {
    sample_tex_linear::<false>(tex_data, tex_w, tex_h, u, v, sampler)
        .map(|c| c[3])
        .or(Some(0.0))
}

#[derive(Clone, Copy)]
struct Texels<'a> {
    bytes: &'a [u8],
    half: &'a [u64],
    width: usize,
    height: usize,
}

impl Texture {
    fn texels(&self) -> Texels<'_> {
        Texels {
            half: &self.half_pixels,
            ..(&self.image).into()
        }
    }
}

impl<'a> From<&'a RgbaImage> for Texels<'a> {
    fn from(image: &'a RgbaImage) -> Self {
        Self {
            bytes: image.as_raw(),
            half: &[],
            width: image.width() as usize,
            height: image.height() as usize,
        }
    }
}

impl Texels<'_> {
    fn pixel(self, index: usize) -> Option<[f32; 4]> {
        if self.half.is_empty() {
            let pixel = self.bytes.get(index * 4..index * 4 + 4)?;
            Some(std::array::from_fn(|i| f32::from(pixel[i]) * U8_TO_F32))
        } else {
            self.half.get(index).copied().map(unpack_half)
        }
    }
}

// IEEE binary16 conversion, round to nearest with ties to even. Every
// attachment write is rounded, so chained captures retain GPU precision.
fn encode_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let magnitude = bits & 0x7fff_ffff;
    if magnitude >= 0x7f80_0000 {
        return sign | 0x7c00 | if magnitude == 0x7f80_0000 { 0 } else { 0x0200 };
    }
    let exponent = ((magnitude >> 23) & 0xff) as i32 - 127;
    if exponent > 15 {
        return sign | 0x7c00;
    }
    if exponent >= -14 {
        let rounded = magnitude + 0x0fff + ((magnitude >> 13) & 1);
        return sign | ((rounded >> 13) - 0x1c000) as u16;
    }
    if exponent < -25 {
        return sign;
    }
    let mantissa = (magnitude & 0x007f_ffff) | 0x0080_0000;
    let shift = (-exponent - 1) as u32;
    let rounded = mantissa + ((1 << (shift - 1)) - 1) + ((mantissa >> shift) & 1);
    sign | (rounded >> shift) as u16
}

fn decode_half(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 0x1f);
    let mantissa = u32::from(bits & 0x03ff);
    let magnitude = match exponent {
        0 if mantissa == 0 => 0,
        0 => {
            let shift = mantissa.leading_zeros() - 21;
            ((113 - shift) << 23) | (((mantissa << shift) & 0x03ff) << 13)
        }
        31 => 0x7f80_0000 | (mantissa << 13),
        _ => ((exponent + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(sign | magnitude)
}

fn pack_half(color: [f32; 4]) -> u64 {
    let [r, g, b, a] = color.map(encode_half).map(u64::from);
    r | (g << 16) | (b << 32) | (a << 48)
}

fn unpack_half(pixel: u64) -> [f32; 4] {
    std::array::from_fn(|i| decode_half((pixel >> (i * 16)) as u16))
}

fn blend_color(dst: [f32; 4], src: [f32; 4], mode: BlendMode) -> [f32; 4] {
    let alpha = clamp01(src[3]);
    let inv = 1.0 - alpha;
    let mut color = std::array::from_fn(|i| match mode {
        BlendMode::Alpha => src[i].mul_add(alpha, dst[i] * inv),
        BlendMode::Add => src[i].mul_add(alpha, dst[i]),
        BlendMode::Multiply => src[i] * dst[i],
        BlendMode::Subtract => dst[i].mul_add(inv, -(src[i] * alpha)),
    });
    color[3] = if mode == BlendMode::Subtract {
        dst[3].mul_add(inv, -src[3])
    } else {
        src[3] + dst[3] * inv
    };
    color
}

fn byte_writer(buffer: &mut [u32]) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
    move |index, color, mode| {
        if mode == BlendMode::Alpha && color[3] >= 1.0 {
            buffer[index] = pack_rgba([color[0], color[1], color[2], 1.0]);
            return;
        }
        let dst = buffer[index];
        let dst = [
            ((dst >> 16) & 255) as f32 * U8_TO_F32,
            ((dst >> 8) & 255) as f32 * U8_TO_F32,
            (dst & 255) as f32 * U8_TO_F32,
            (dst >> 24) as f32 * U8_TO_F32,
        ];
        // Normalized attachments clamp fragment inputs before blending.
        buffer[index] = pack_rgba(blend_color(dst, color.map(clamp01), mode));
    }
}

fn half_writer(buffer: &mut [u64], alpha: bool) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
    move |index, color, mode| {
        let mut color = blend_color(unpack_half(buffer[index]), color, mode);
        if !alpha {
            color[3] = 1.0;
        }
        buffer[index] = pack_half(color);
    }
}

#[inline(always)]
fn raster_bounds(
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
) -> Option<(i32, i32, i32, i32, i32)> {
    let min_x = min_x.max(0.0) as i32;
    let max_x = max_x.ceil().min((width - 1) as f32) as i32;
    let mut min_y = min_y.max(0.0) as i32;
    let mut max_y = max_y.ceil().min((height - 1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return None;
    }

    let stripe_start = stripe_y_start as i32;
    let stripe_end = stripe_y_end as i32 - 1;
    if stripe_start > stripe_end || max_y < stripe_start || min_y > stripe_end {
        return None;
    }
    min_y = min_y.max(stripe_start);
    max_y = max_y.min(stripe_end);
    Some((min_x, max_x, min_y, max_y, stripe_start))
}

#[inline(always)]
fn triangle_setup(x: [f32; 3], y: [f32; 3], width: usize, height: usize) -> Option<RasterSetup> {
    triangle_setup_in_rows(x, y, width, height, 0, height)
}

#[inline(always)]
fn triangle_setup_in_rows(
    x: [f32; 3],
    y: [f32; 3],
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
) -> Option<RasterSetup> {
    let (min_x, max_x, min_y, max_y, _) = raster_bounds(
        x[0].min(x[1]).min(x[2]),
        x[0].max(x[1]).max(x[2]),
        y[0].min(y[1]).min(y[2]),
        y[0].max(y[1]).max(y[2]),
        width,
        height,
        stripe_y_start,
        stripe_y_end,
    )?;
    let denom = edge_function(x[0], y[0], x[1], y[1], x[2], y[2]);
    if denom == 0.0 {
        return None;
    }
    Some(RasterSetup {
        min_x,
        max_x,
        min_y,
        max_y,
        inv_denom: 1.0 / denom,
    })
}

impl RasterSetup {
    #[inline(always)]
    const fn rows(self) -> ScreenRows {
        ScreenRows {
            start: self.min_y as u32,
            end: self.max_y as u32 + 1,
        }
    }

    #[inline(always)]
    fn stripe_bounds(
        self,
        stripe_y_start: usize,
        stripe_y_end: usize,
    ) -> Option<(i32, i32, i32, i32, i32)> {
        let stripe_start = stripe_y_start as i32;
        let stripe_end = stripe_y_end as i32 - 1;
        if stripe_start > stripe_end || self.max_y < stripe_start || self.min_y > stripe_end {
            return None;
        }
        Some((
            self.min_x,
            self.max_x,
            self.min_y.max(stripe_start),
            self.max_y.min(stripe_end),
            stripe_start,
        ))
    }
}

#[inline(always)]
fn rasterize_triangle_impl<const LINEAR: bool, const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
    image: Texels<'_>,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    blend: BlendMode,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) = raster_bounds(
        v0.x.min(v1.x).min(v2.x),
        v0.x.max(v1.x).max(v2.x),
        v0.y.min(v1.y).min(v2.y),
        v0.y.max(v1.y).max(v2.y),
        width,
        height,
        stripe_y_start,
        stripe_y_end,
    ) else {
        return;
    };

    let tex_w = image.width.max(1);
    let tex_h = image.height.max(1);
    let tex_data = image;

    let edges = [
        owns_edge(v1.x, v1.y, v2.x, v2.y, inv_denom),
        owns_edge(v2.x, v2.y, v0.x, v0.y, inv_denom),
        owns_edge(v0.x, v0.y, v1.x, v1.y, inv_denom),
    ];
    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if !covered([w0, w1, w2], edges) {
                continue;
            }

            let sampled = if MASK && OPAQUE {
                Some([0.0, 0.0, 0.0, 1.0])
            } else {
                let u = v2.u.mul_add(w2, v0.u.mul_add(w0, v1.u * w1));
                let v = v2.v.mul_add(w2, v0.v.mul_add(w0, v1.v * w1));
                if MASK {
                    let alpha = if LINEAR {
                        sample_alpha_linear(tex_data, tex_w, tex_h, u, v, sampler)
                    } else {
                        sample_alpha_nearest(tex_data, tex_w, tex_h, u, v, sampler)
                    };
                    alpha.map(|alpha| [0.0, 0.0, 0.0, alpha])
                } else if LINEAR {
                    sample_tex_linear::<OPAQUE>(tex_data, tex_w, tex_h, u, v, sampler)
                } else {
                    sample_tex_nearest::<OPAQUE>(tex_data, tex_w, tex_h, u, v, sampler)
                }
            };
            let Some(sampled) = sampled else {
                continue;
            };
            if sampled[3] == 0.0 && blend != BlendMode::Multiply {
                continue;
            }

            let sr = if MASK { tint[0] } else { sampled[0] * tint[0] };
            let sg = if MASK { tint[1] } else { sampled[1] * tint[1] };
            let sb = if MASK { tint[2] } else { sampled[2] * tint[2] };
            let sa = sampled[3] * tint[3];
            if sa == 0.0 && blend != BlendMode::Multiply {
                continue;
            }

            let dst_idx = row * width + x as usize;
            buffer(dst_idx, [sr, sg, sb, sa], blend);
        }
    }
}

#[inline(always)]
fn rasterize_triangle_tex_color_impl<const LINEAR: bool, const MASK: bool, const OPAQUE: bool>(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    setup: RasterSetup,
    image: Texels<'_>,
    sampler: SamplerDesc,
    width: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    blend: BlendMode,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),

    depth: &mut DepthRows<'_>,
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) =
        setup.stripe_bounds(stripe_y_start, stripe_y_end)
    else {
        return;
    };
    let inv_denom = setup.inv_denom;
    let tex_w = image.width.max(1);
    let tex_h = image.height.max(1);
    let tex_data = image;

    let edges = [
        owns_edge(v1.x, v1.y, v2.x, v2.y, inv_denom),
        owns_edge(v2.x, v2.y, v0.x, v0.y, inv_denom),
        owns_edge(v0.x, v0.y, v1.x, v1.y, inv_denom),
    ];
    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if !covered([w0, w1, w2], edges) {
                continue;
            }

            let sampled = if MASK && OPAQUE {
                Some([0.0, 0.0, 0.0, 1.0])
            } else {
                let u = v2.u.mul_add(w2, v0.u.mul_add(w0, v1.u * w1));
                let v = v2.v.mul_add(w2, v0.v.mul_add(w0, v1.v * w1));
                if MASK {
                    let alpha = if LINEAR {
                        sample_alpha_linear(tex_data, tex_w, tex_h, u, v, sampler)
                    } else {
                        sample_alpha_nearest(tex_data, tex_w, tex_h, u, v, sampler)
                    };
                    alpha.map(|alpha| [0.0, 0.0, 0.0, alpha])
                } else if LINEAR {
                    sample_tex_linear::<OPAQUE>(tex_data, tex_w, tex_h, u, v, sampler)
                } else {
                    sample_tex_nearest::<OPAQUE>(tex_data, tex_w, tex_h, u, v, sampler)
                }
            };
            let Some(sampled) = sampled else {
                continue;
            };
            if sampled[3] == 0.0 && blend != BlendMode::Multiply {
                continue;
            }

            let cr = v2.color[0].mul_add(w2, v0.color[0].mul_add(w0, v1.color[0] * w1));
            let cg = v2.color[1].mul_add(w2, v0.color[1].mul_add(w0, v1.color[1] * w1));
            let cb = v2.color[2].mul_add(w2, v0.color[2].mul_add(w0, v1.color[2] * w1));
            let ca = v2.color[3].mul_add(w2, v0.color[3].mul_add(w0, v1.color[3] * w1));

            let sr = if MASK { cr } else { sampled[0] * cr };
            let sg = if MASK { cg } else { sampled[1] * cg };
            let sb = if MASK { cb } else { sampled[2] * cb };
            let sa = sampled[3] * ca;
            if sa <= 1.0 / 256.0 {
                continue;
            }

            let dst_idx = row * width + x as usize;
            let z = v2.z.mul_add(w2, v0.z.mul_add(w0, v1.z * w1));
            if !depth.test(dst_idx, z) {
                continue;
            }
            buffer(dst_idx, [sr, sg, sb, sa], blend);
        }
    }
}

#[inline(always)]
fn rasterize_triangle_color_impl(
    v0: &ScreenVertexColor,
    v1: &ScreenVertexColor,
    v2: &ScreenVertexColor,
    setup: RasterSetup,
    width: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    blend: BlendMode,
    buffer: &mut impl FnMut(usize, [f32; 4], BlendMode),
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) =
        setup.stripe_bounds(stripe_y_start, stripe_y_end)
    else {
        return;
    };
    let inv_denom = setup.inv_denom;

    let edges = [
        owns_edge(v1.x, v1.y, v2.x, v2.y, inv_denom),
        owns_edge(v2.x, v2.y, v0.x, v0.y, inv_denom),
        owns_edge(v0.x, v0.y, v1.x, v1.y, inv_denom),
    ];
    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if !covered([w0, w1, w2], edges) {
                continue;
            }

            let sr = v2.color[0].mul_add(w2, v0.color[0].mul_add(w0, v1.color[0] * w1));
            let sg = v2.color[1].mul_add(w2, v0.color[1].mul_add(w0, v1.color[1] * w1));
            let sb = v2.color[2].mul_add(w2, v0.color[2].mul_add(w0, v1.color[2] * w1));
            let sa = v2.color[3].mul_add(w2, v0.color[3].mul_add(w0, v1.color[3] * w1));
            if sa == 0.0 && blend != BlendMode::Multiply {
                continue;
            }

            let dst_idx = row * width + x as usize;
            buffer(dst_idx, [sr, sg, sb, sa], blend);
        }
    }
}

fn owns_edge(x0: f32, y0: f32, x1: f32, y1: f32, inv: f32) -> bool {
    let dy = (y1 - y0) * inv;
    let dx = (x1 - x0) * inv;
    dy > 0.0 || (dy == 0.0 && dx < 0.0)
}

fn covered(weights: [f32; 3], edges: [bool; 3]) -> bool {
    weights
        .into_iter()
        .zip(edges)
        .all(|(w, edge)| w > 0.0 || (w == 0.0 && edge))
}

#[inline(always)]
fn edge_function(x0: f32, y0: f32, x1: f32, y1: f32, px: f32, py: f32) -> f32 {
    (px - x0).mul_add(y1 - y0, -((py - y0) * (x1 - x0)))
}

#[inline(always)]
fn triangle_inv_denom(v0: &ScreenVertex, v1: &ScreenVertex, v2: &ScreenVertex) -> Option<f32> {
    let denom = edge_function(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    (denom != 0.0).then(|| 1.0 / denom)
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]

    fn assert_depth_captures(state: &mut State) {
        let textures = TestTextures {
            texture: create_texture(
                &RgbaImage::from_pixel(1, 1, Rgba([255; 4])),
                SamplerDesc::default(),
            )
            .unwrap(),
            lookups: AtomicUsize::new(0),
        };
        let model = depth_frame();
        let projection = Matrix4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        let mut frame = RenderFrame {
            render_targets: vec![RenderTargetFrame {
                texture_handle: deadlib_render_core::render_target_texture_handle(18),
                width: 64,
                height: 64,
                viewport: [64, 64],
                alpha: true,
                depth: true,
                preserve: false,
                float_color: false,
                cameras: vec![projection],
                sprite_instances: vec![],
                mesh_vertices: vec![],
                tmesh_instances: model.tmesh_instances.clone(),
                tmesh_geometries: model.tmesh_geometries.clone(),
                ops: model.ops.clone(),
            }],
            ..model.clone()
        };
        let mut check = |frame: &RenderFrame, samples: &[(u32, u32, [u8; 3])], label: &str| {
            draw_offscreen_targets(state, frame, &textures);
            let target = &state.offscreen_targets[0];
            for &(x, y, expected) in samples {
                assert_eq!(
                    target.texture.image.get_pixel(x, y).0[..3],
                    expected,
                    "{label}: {x},{y}"
                );
            }
            assert_eq!(
                target.depth.len(),
                if frame.render_targets[0].depth {
                    64 * 64
                } else {
                    0
                }
            );
            assert_eq!(
                target.depth_image.len(),
                if frame.render_targets[0].depth {
                    64 * 64
                } else {
                    0
                }
            );
        };
        for float_color in [false, true] {
            frame.render_targets[0].float_color = float_color;
            for alpha in [false, true] {
                frame.render_targets[0].alpha = alpha;
                for depth in [true, false, true, false] {
                    frame.render_targets[0].depth = depth;
                    check(
                        &frame,
                        &[
                            (20, 25, if depth { [0, 255, 0] } else { [255, 0, 0] }),
                            (32, 32, [0, 0, 255]),
                            (50, 32, [255, 0, 0]),
                        ],
                        "optional capture depth",
                    );
                }
            }
        }
        drop(check);
        let mut check = |frame: &RenderFrame, expected: [u8; 3], label: &str| {
            draw_offscreen_targets(state, frame, &textures);
            assert_eq!(
                state.offscreen_targets[0].texture.image.get_pixel(20, 25).0[..3],
                expected,
                "{label}"
            );
            let target = &state.offscreen_targets[0];
            (target.depth.as_ptr(), target.depth_image.as_ptr())
        };
        for float_color in [false, true] {
            let target = &mut frame.render_targets[0];
            target.texture_handle = deadlib_render_core::render_target_texture_handle(19);
            target.float_color = float_color;
            target.alpha = true;
            target.depth = true;
            target.preserve = true;
            target.viewport = [64, 64];
            target.tmesh_instances.clone_from(&model.tmesh_instances);
            target.tmesh_geometries.clone_from(&model.tmesh_geometries);
            target.ops = vec![model.ops[0].clone()];
            target.mesh_vertices = [
                [-1.0, -1.0],
                [1.0, -1.0],
                [1.0, 1.0],
                [-1.0, -1.0],
                [1.0, 1.0],
                [-1.0, 1.0],
            ]
            .map(|pos| deadlib_render_core::MeshVertex {
                pos,
                color: [0.0, 0.0, 0.0, 1.0],
            })
            .to_vec();
            check(&frame, [0, 255, 0], "first preserved capture");
            frame.render_targets[0].ops = vec![model.ops[1].clone()];
            // Both compact and physical depth buffers retain their allocations.
            let pointers = check(&frame, [0, 255, 0], "depth retained across captures");
            frame.render_targets[0].viewport = [32, 32];
            frame.render_targets[0].ops = vec![DrawOp::Mesh(deadlib_render_core::MeshRun {
                vertex_start: 0,
                vertex_count: 6,
                blend: BlendMode::Alpha,
                camera: 0,
            })];
            check(&frame, [0, 0, 0], "color overwritten in reduced viewport");
            frame.render_targets[0].viewport = [64, 64];
            frame.render_targets[0].ops = vec![model.ops[1].clone()];
            assert_eq!(
                check(&frame, [0, 0, 0], "depth retained through viewport changes"),
                pointers
            );
            let DrawOp::TexturedMesh(run) = &mut frame.render_targets[0].ops[0] else {
                unreachable!()
            };
            run.clear_depth = true;
            check(
                &frame,
                [255, 0, 0],
                "explicit depth clear on preserved capture",
            );
            frame.render_targets[0].preserve = false;
            frame.render_targets[0].ops = vec![model.ops[1].clone()];
            check(&frame, [255, 0, 0], "nonpreserved depth clear");
            let front = 1.0 - 2.0 * (20000.0 / 65535.0);
            frame.render_targets[0].tmesh_instances[0].model_col3[2] = front - 0.4;
            frame.render_targets[0].tmesh_instances[1].model_col3[2] = front - 0.000005;
            frame.render_targets[0].ops = model.ops[..2].to_vec();
            check(&frame, [255, 0, 0], "equal quantized capture depths");
            // Clip planes still apply when no depth attachment is present.
            frame.render_targets[0].depth = false;
            frame.render_targets[0].ops = vec![model.ops[1].clone()];
            for environment in [false, true] {
                frame.render_targets[0].tmesh_geometries[1].vertices =
                    TexturedMeshVertices::Shared(Arc::from(
                        model.tmesh_geometries[1]
                            .vertices
                            .as_ref()
                            .iter()
                            .copied()
                            .map(|mut vertex| {
                                vertex.normal = if environment {
                                    [0.0, 0.0, 1.0, 1.0]
                                } else {
                                    [0.0; 4]
                                };
                                vertex
                            })
                            .collect::<Vec<_>>(),
                    ));
                for z in [-2.0, 2.0, 0.0] {
                    frame.render_targets[0].tmesh_instances[1].model_col3[2] = z;
                    check(
                        &frame,
                        if z == 0.0 { [255, 0, 0] } else { [0, 0, 0] },
                        "depth-disabled clip planes",
                    );
                }
            }
        }
    }

    fn assert_float_captures(state: &mut State) {
        set_default_projection(state, Matrix4::IDENTITY);
        let textures = test_textures();
        let mut pixels = vec![0; 64 * 64];
        state.depth.resize(64 * 64, 1.0);

        let handle = deadlib_render_core::render_target_texture_handle(15);
        let mut frame = RenderFrame {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            render_targets: vec![],
            cameras: vec![Matrix4::IDENTITY],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            tmesh_instances: vec![],
            tmesh_geometries: vec![],
            ops: vec![],
        };
        frame.tmesh_instances = vec![TexturedMeshInstanceRaw::new(
            Matrix4::IDENTITY,
            [0.25, 0.25, 0.25, 1.0],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            false,
        )];
        frame.tmesh_geometries = vec![TexturedMeshGeometry {
            cache_key: 0,
            vertices: TexturedMeshVertices::Shared(Arc::from(
                [
                    [-1.0, -1.0, 0.0],
                    [1.0, -1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [-1.0, -1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [-1.0, 1.0, 0.0],
                ]
                .map(|pos| TexturedMeshVertex {
                    pos,
                    normal: [0.0; 4],
                    uv: [0.5; 2],
                    color: [1.0; 4],
                    tex_matrix_scale: [1.0; 2],
                }),
            )),
        }];
        frame.sprite_instances = vec![SpriteInstanceRaw {
            center: [0.0; 4],
            size: [2.0; 2],
            rot_sin_cos: [0.0, 1.0],
            tint: [0.25, 0.25, 0.25, 1.0],
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            local_offset: [0.0; 2],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        }];
        frame.render_targets = vec![RenderTargetFrame {
            texture_handle: handle,
            width: 64,
            height: 64,
            viewport: [64, 64],
            float_color: true,
            alpha: false,
            depth: true,
            preserve: false,
            cameras: vec![Matrix4::IDENTITY],
            sprite_instances: vec![],
            mesh_vertices: [
                [-1.0, -1.0],
                [1.0, -1.0],
                [1.0, 1.0],
                [-1.0, -1.0],
                [1.0, 1.0],
                [-1.0, 1.0],
            ]
            .map(|pos| MeshVertex {
                pos,
                color: [1.0, 0.0, 0.0, 1.0],
            })
            .to_vec(),
            tmesh_instances: vec![],
            tmesh_geometries: vec![],
            ops: vec![
                DrawOp::Mesh(MeshRun {
                    vertex_start: 0,
                    vertex_count: 6,
                    blend: BlendMode::Add,
                    camera: 0
                });
                2
            ],
        }];
        // The float capture holds RGB=2 and uses native source-over alpha. Sampling it
        // at lower intensity must recover those values, rather than a clamped 1.
        // Toggle format on the same handle to verify cache identity, then redraw it.
        for alpha in [false, true] {
            frame.render_targets[0].alpha = alpha;
            frame.sprite_instances[0].tint[3] = if alpha { 0.5 } else { 1.0 };
            frame.tmesh_instances[0].tint[3] = if alpha { 0.5 } else { 1.0 };
            for float_color in [true, false, true] {
                frame.render_targets[0].float_color = float_color;
                for textured in [false, true] {
                    frame.ops = vec![if textured {
                        DrawOp::TexturedMesh(TexturedMeshRun {
                            sampler: None,
                            additive_texture: 0,
                            geometry: 0,
                            instance_start: 0,
                            instance_count: 1,
                            blend: BlendMode::Add,
                            texture_handle: handle,
                            camera: 0,
                            depth_test: false,
                            clear_depth: false,
                        })
                    } else {
                        DrawOp::Sprite(SpriteRun {
                            instance_start: 0,
                            instance_count: 1,
                            blend: BlendMode::Add,
                            texture_handle: handle,
                            camera: 0,
                        })
                    }];
                    draw_offscreen_targets(state, &frame, &textures);
                    if float_color {
                        assert!(
                            state.offscreen_targets[0]
                                .texture
                                .half_pixels
                                .iter()
                                .all(|&pixel| unpack_half(pixel) == [2.0, 0.0, 0.0, 1.0]),
                            "quad diagonals must shade each capture pixel once per draw"
                        );
                    }
                    let resolved = ResolvedTextures {
                        external: &textures,
                        targets: &state.offscreen_targets,
                    };
                    let fixed = prepare_objects(
                        (&frame).into(),
                        Matrix4::IDENTITY,
                        &resolved,
                        64,
                        64,
                        &mut state.prepared_objects,
                        &mut state.prepared_mesh_triangles,
                        &mut state.prepared_tmesh_triangles,
                        true,
                    );
                    pixels.fill(pack_rgba(frame.clear_color));
                    draw_rows(
                        (&frame).into(),
                        &state.prepared_objects,
                        None,
                        &state.prepared_mesh_triangles,
                        &state.prepared_tmesh_triangles,
                        &resolved,
                        64,
                        64,
                        0,
                        64,
                        &mut byte_writer(&mut pixels),
                        fixed,
                        &mut state.depth,
                    );
                    let [a, r, g, b] = pixels[20 * 64 + 32].to_be_bytes();
                    let pixel = [r, g, b, a];
                    let red = if float_color {
                        if alpha { 64 } else { 128 }
                    } else if alpha {
                        32
                    } else {
                        64
                    };
                    assert!(
                        pixel[0].abs_diff(red) <= 1 && pixel[1] == 0 && pixel[2] == 0,
                        "software: float={float_color}, alpha={alpha}, textured={textured}, pixel={pixel:?}, expected red={red}"
                    );
                    if float_color && !alpha && !textured {
                        frame.sprite_instances[0].tint = [1.0, 1.0, 1.0, 0.5];
                        let fixed = prepare_objects(
                            (&frame).into(),
                            Matrix4::IDENTITY,
                            &resolved,
                            64,
                            64,
                            &mut state.prepared_objects,
                            &mut state.prepared_mesh_triangles,
                            &mut state.prepared_tmesh_triangles,
                            true,
                        );
                        pixels.fill(pack_rgba(frame.clear_color));
                        draw_rows(
                            (&frame).into(),
                            &state.prepared_objects,
                            None,
                            &state.prepared_mesh_triangles,
                            &state.prepared_tmesh_triangles,
                            &resolved,
                            64,
                            64,
                            0,
                            64,
                            &mut byte_writer(&mut pixels),
                            fixed,
                            &mut state.depth,
                        );
                        assert_eq!(
                            pixels[20 * 64 + 32],
                            0xff80_0000,
                            "normalized fragment clamp"
                        );
                        frame.sprite_instances[0].tint = [0.25, 0.25, 0.25, 1.0];
                    }
                }
            }
        }
        // RageDisplay_OGL uses ONE / ONE_MINUS_SRC_ALPHA for the alpha channel,
        // independently of normal/additive RGB factors. Two half-alpha layers
        // therefore retain 0.75 alpha, which must survive sampling the capture.
        frame.render_targets[0].float_color = true;
        frame.render_targets[0].alpha = true;
        for vertex in &mut frame.render_targets[0].mesh_vertices {
            vertex.color[3] = 0.5;
        }
        frame.sprite_instances[0].tint = [1.0; 4];
        frame.ops = vec![DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: handle,
            camera: 0,
        })];
        for (blend, expected) in [(BlendMode::Alpha, 143_u8), (BlendMode::Add, 191)] {
            for op in &mut frame.render_targets[0].ops {
                let DrawOp::Mesh(run) = op else {
                    unreachable!()
                };
                run.blend = blend;
            }
            draw_offscreen_targets(state, &frame, &textures);
            let resolved = ResolvedTextures {
                external: &textures,
                targets: &state.offscreen_targets,
            };
            let fixed = prepare_objects(
                (&frame).into(),
                Matrix4::IDENTITY,
                &resolved,
                64,
                64,
                &mut state.prepared_objects,
                &mut state.prepared_mesh_triangles,
                &mut state.prepared_tmesh_triangles,
                true,
            );
            pixels.fill(pack_rgba(frame.clear_color));
            draw_rows(
                (&frame).into(),
                &state.prepared_objects,
                None,
                &state.prepared_mesh_triangles,
                &state.prepared_tmesh_triangles,
                &resolved,
                64,
                64,
                0,
                64,
                &mut byte_writer(&mut pixels),
                fixed,
                &mut state.depth,
            );
            let [a, r, g, b] = pixels[20 * 64 + 32].to_be_bytes();
            let pixel = [r, g, b, a];
            assert!(
                pixel[0].abs_diff(expected) <= 1 && pixel[1] == 0 && pixel[2] == 0,
                "software: captured alpha, blend={blend:?}, pixel={pixel:?}, expected red={expected}"
            );
        }
        // Float attachments also retain negative subtraction results. Subtracting
        // twice gives -0.75, then adding one restores 0.25. An 8-bit target clips
        // the intermediate negative result and incorrectly restores one instead.
        frame.render_targets[0].alpha = false;
        let red = frame.render_targets[0]
            .mesh_vertices
            .iter()
            .copied()
            .map(|mut vertex| {
                vertex.color[3] = 1.0;
                vertex
            })
            .collect::<Vec<_>>();
        frame.render_targets[0].mesh_vertices.extend(red);
        for (index, vertex) in frame.render_targets[0].mesh_vertices.iter_mut().enumerate() {
            vertex.color = if index < 6 {
                [0.8, 0.0, 0.0, 1.0]
            } else {
                [0.5, 0.0, 0.0, 0.5]
            };
        }
        frame.render_targets[0].ops = vec![
            DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 6,
                blend: BlendMode::Alpha,
                camera: 0,
            }),
            DrawOp::Mesh(MeshRun {
                vertex_start: 6,
                vertex_count: 6,
                blend: BlendMode::Multiply,
                camera: 0,
            }),
        ];
        for float_color in [true, false] {
            frame.render_targets[0].float_color = float_color;
            draw_offscreen_targets(state, &frame, &textures);
            let pixel = state.offscreen_targets[0].texture.image.get_pixel(32, 20).0;
            assert_eq!(pixel, [102, 0, 0, 255], "multiply float={float_color}");
        }
        for (index, vertex) in frame.render_targets[0].mesh_vertices.iter_mut().enumerate() {
            vertex.color = [1.0, 0.0, 0.0, if index < 6 { 0.5 } else { 1.0 }];
        }
        frame.render_targets[0].ops = vec![
            DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 6,
                blend: BlendMode::Subtract,
                camera: 0,
            }),
            DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 6,
                blend: BlendMode::Subtract,
                camera: 0,
            }),
            DrawOp::Mesh(MeshRun {
                vertex_start: 6,
                vertex_count: 6,
                blend: BlendMode::Add,
                camera: 0,
            }),
        ];
        for float_color in [true, false] {
            frame.render_targets[0].float_color = float_color;
            draw_offscreen_targets(state, &frame, &textures);
            let resolved = ResolvedTextures {
                external: &textures,
                targets: &state.offscreen_targets,
            };
            let fixed = prepare_objects(
                (&frame).into(),
                Matrix4::IDENTITY,
                &resolved,
                64,
                64,
                &mut state.prepared_objects,
                &mut state.prepared_mesh_triangles,
                &mut state.prepared_tmesh_triangles,
                true,
            );
            pixels.fill(pack_rgba(frame.clear_color));
            draw_rows(
                (&frame).into(),
                &state.prepared_objects,
                None,
                &state.prepared_mesh_triangles,
                &state.prepared_tmesh_triangles,
                &resolved,
                64,
                64,
                0,
                64,
                &mut byte_writer(&mut pixels),
                fixed,
                &mut state.depth,
            );
            let [a, r, g, b] = pixels[20 * 64 + 32].to_be_bytes();
            let pixel = [r, g, b, a];
            let expected = if float_color { 64_u8 } else { 255 };
            assert!(
                pixel[0].abs_diff(expected) <= 1 && pixel[1] == 0 && pixel[2] == 0,
                "software: negative color, float={float_color}, pixel={pixel:?}, expected red={expected}"
            );
        }
        let pass = &mut frame.render_targets[0];
        pass.float_color = true;
        pass.viewport = [40, 24];
        pass.ops = vec![DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Alpha,
            camera: 0,
        })];
        for vertex in &mut pass.mesh_vertices[..6] {
            vertex.color = [2.0, -0.5, 0.25, 0.5];
        }
        for alpha in [false, true] {
            frame.render_targets[0].alpha = alpha;
            frame.render_targets[0].preserve = false;
            draw_offscreen_targets(state, &frame, &textures);
            let target = &state.offscreen_targets[0];
            let expected = target.texture.half_pixels.clone();
            let buffers = (
                target.half_pixels.as_ptr(),
                target.depth.as_ptr(),
                target.texture.half_pixels.as_ptr(),
                target.texture.image.as_ptr(),
            );
            let color = [1.0, -0.25, 0.125, if alpha { 0.5 } else { 1.0 }];
            let clear = [0.0, 0.0, 0.0, if alpha { 0.0 } else { 1.0 }];
            for (index, &pixel) in expected.iter().enumerate() {
                assert_eq!(
                    unpack_half(pixel),
                    if index % 64 < 40 && index / 64 < 24 {
                        color
                    } else {
                        clear
                    }
                );
            }
            frame.render_targets[0].ops.clear();
            frame.render_targets[0].preserve = true;
            for viewport in [[20, 16], [60, 48], [40, 24]] {
                frame.render_targets[0].viewport = viewport;
                draw_offscreen_targets(state, &frame, &textures);
                let target = &state.offscreen_targets[0];
                assert_eq!(
                    target.texture.half_pixels, expected,
                    "preserve {viewport:?}"
                );
                assert_eq!(
                    (
                        target.half_pixels.as_ptr(),
                        target.depth.as_ptr(),
                        target.texture.half_pixels.as_ptr(),
                        target.texture.image.as_ptr()
                    ),
                    buffers
                );
                assert!(target.half_pixels.capacity() >= 64 * 64);
            }
            frame.render_targets[0].preserve = false;
            draw_offscreen_targets(state, &frame, &textures);
            assert!(
                state.offscreen_targets[0]
                    .texture
                    .half_pixels
                    .iter()
                    .all(|&pixel| unpack_half(pixel) == clear),
                "clear includes padding"
            );
            frame.render_targets[0].ops = vec![DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 6,
                blend: BlendMode::Alpha,
                camera: 0,
            })];
        }
    }

    use super::*;
    use deadlib_render_core::{
        INVALID_TMESH_CACHE_KEY, MeshRun, MeshVertex, SpriteInstanceRaw, SpriteRun,
        TexturedMeshGeometry, TexturedMeshInstanceRaw, TexturedMeshRun, TexturedMeshVertex,
        TexturedMeshVertices,
    };
    use glam::Vec3;
    use image::Rgba;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const WIDTH: usize = 96;
    const HEIGHT: usize = 80;
    const TEXTURE_HANDLE: TextureHandle = 7;
    const MISSING_TEXTURE_HANDLE: TextureHandle = 99;

    #[cfg(target_os = "windows")]
    #[test]
    fn resize_preserves_projection_and_offscreen_cameras_override_it() {
        use winit::platform::windows::EventLoopBuilderExtWindows;

        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .expect("create test event loop");
        #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
        let window = event_loop
            .create_window(Window::default_attributes().with_visible(false))
            .expect("create hidden test window");
        let projection =
            glam::camera::rh::proj::opengl::orthographic(-10.0, 10.0, -5.0, 5.0, -1.0, 1.0);
        let mut state =
            init(Arc::new(window), projection, false).expect("create software renderer");
        assert_eq!(state.projection, projection);
        let custom = Matrix4::from_translation(Vec3::new(0.25, -0.5, 0.0)) * projection;
        set_default_projection(&mut state, custom);
        for (width, height) in [
            (640, 480),
            (1920, 1080),
            (3840, 780),
            (0, 480),
            (640, 0),
            (80, 60),
        ] {
            resize(&mut state, width, height);
            assert_eq!(state.projection, custom);
            assert_eq!(state.window_size, PhysicalSize::new(width, height));
        }
        set_default_projection(&mut state, projection);
        let mut frame = RenderFrame {
            clear_color: [0.0; 4],
            render_targets: vec![RenderTargetFrame {
                texture_handle: deadlib_render_core::render_target_texture_handle(1),
                width: 40,
                height: 24,
                viewport: [40, 24],
                float_color: false,
                alpha: false,
                depth: false,
                preserve: false,
                cameras: Vec::new(),
                sprite_instances: Vec::new(),
                mesh_vertices: [
                    (-5.0, -2.5),
                    (5.0, -2.5),
                    (5.0, 2.5),
                    (-5.0, -2.5),
                    (5.0, 2.5),
                    (-5.0, 2.5),
                ]
                .map(|(x, y)| MeshVertex {
                    pos: [x, y],
                    color: [1.0; 4],
                })
                .to_vec(),
                tmesh_instances: Vec::new(),
                tmesh_geometries: Vec::new(),
                ops: vec![DrawOp::Mesh(MeshRun {
                    vertex_start: 0,
                    vertex_count: 6,
                    blend: BlendMode::Alpha,
                    camera: 0,
                })],
            }],
            cameras: Vec::new(),
            sprite_instances: Vec::new(),
            mesh_vertices: Vec::new(),
            tmesh_instances: Vec::new(),
            tmesh_geometries: Vec::new(),
            ops: Vec::new(),
        };
        for (width, height) in [(40, 24), (80, 16)] {
            frame.render_targets[0].width = width;
            frame.render_targets[0].height = height;
            frame.render_targets[0].viewport = [width, height];
            // Explicit cameras change coverage; absent and invalid indices use the default.
            for (cameras, camera, inset) in [
                (vec![], 0, 4),
                (
                    vec![projection * Matrix4::from_scale(Vec3::new(0.5, 0.5, 1.0))],
                    0,
                    8,
                ),
                (vec![Matrix4::IDENTITY], 1, 4),
            ] {
                frame.render_targets[0].cameras = cameras;
                let DrawOp::Mesh(run) = &mut frame.render_targets[0].ops[0] else {
                    panic!("mesh fixture");
                };
                run.camera = camera;
                draw_offscreen_targets(&mut state, &frame, &test_textures());
                for (index, pixel) in state.offscreen_targets[0].pixels.iter().enumerate() {
                    let x = index as u32 % width;
                    let y = index as u32 / width;
                    let inside = (width / 2 - width / inset..width / 2 + width / inset)
                        .contains(&x)
                        && (height / 2 - height / inset..height / 2 + height / inset).contains(&y);
                    assert_eq!(
                        *pixel,
                        if inside { 0xffff_ffff } else { 0xff00_0000 },
                        "target {width}x{height}, camera {camera}, pixel {x},{y}"
                    );
                }
            }
        }
        let pass = &mut frame.render_targets[0];
        pass.width = 128;
        pass.height = 64;
        pass.viewport = [80, 48];
        pass.alpha = true;
        pass.cameras = vec![projection];
        let DrawOp::Mesh(run) = &mut pass.ops[0] else {
            panic!("mesh fixture");
        };
        run.camera = 0;
        draw_offscreen_targets(&mut state, &frame, &test_textures());
        let target = &state.offscreen_targets[0];
        let expected = target.texture.image.clone();
        for (x, y, pixel) in expected.enumerate_pixels() {
            let inside = (20..60).contains(&x) && (12..36).contains(&y);
            assert_eq!(
                pixel.0,
                if inside { [255; 4] } else { [0; 4] },
                "padded capture at {x},{y}"
            );
        }
        let buffers = (
            target.pixels.as_ptr(),
            target.depth.as_ptr(),
            target.texture.image.as_ptr(),
        );
        frame.render_targets[0].ops.clear();
        frame.render_targets[0].preserve = true;
        for viewport in [[40, 24], [80, 48]] {
            frame.render_targets[0].viewport = viewport;
            draw_offscreen_targets(&mut state, &frame, &test_textures());
            let target = &state.offscreen_targets[0];
            assert_eq!(target.texture.image, expected, "preserve {viewport:?}");
            assert_eq!(
                (
                    target.pixels.as_ptr(),
                    target.depth.as_ptr(),
                    target.texture.image.as_ptr(),
                ),
                buffers,
                "viewport change reallocates backing storage"
            );
        }
        frame.render_targets[0].preserve = false;
        for alpha in [true, false] {
            frame.render_targets[0].alpha = alpha;
            draw_offscreen_targets(&mut state, &frame, &test_textures());
            for pixel in state.offscreen_targets[0].texture.image.pixels() {
                assert_eq!(pixel.0, if alpha { [0; 4] } else { [0, 0, 0, 255] });
            }
        }
        assert_float_captures(&mut state);
        assert_depth_captures(&mut state);
    }

    struct TestTextures {
        texture: Texture,
        lookups: AtomicUsize,
    }

    #[test]
    fn sphere_material_rotation_and_alpha_match_gl_stages() {
        let texture = |image| Texture {
            image,
            sampler: SamplerDesc::default(),
            opaque: false,
            yuv420: false,
            half_pixels: Vec::new(),
        };
        let gradient = texture(RgbaImage::from_fn(128, 128, |x, y| {
            image::Rgba([(x * 2) as u8, (y * 2) as u8, 0, 255])
        }));
        let base = texture(RgbaImage::from_pixel(1, 1, image::Rgba([40, 60, 80, 128])));
        let reflection = texture(RgbaImage::from_pixel(1, 1, image::Rgba([80, 40, 20, 128])));
        let vertices = [
            [-0.8, -0.8, 0.0],
            [0.8, -0.8, 0.0],
            [0.8, 0.8, 0.0],
            [-0.8, -0.8, 0.0],
            [0.8, 0.8, 0.0],
            [-0.8, 0.8, 0.0],
        ]
        .map(|pos| deadlib_render_core::TexturedMeshVertex {
            pos,
            normal: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            uv: [0.1, 0.2],
            ..Default::default()
        });
        let mut instance = TexturedMeshInstanceRaw::new(
            Matrix4::IDENTITY,
            [1.0; 4],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            false,
        );
        let mut pixels = vec![0; 64 * 64];
        for (angle, expected) in [
            (0.0_f32, [127, 127, 0]),
            (45.0, [218, 127, 0]),
            (-45.0, [37, 127, 0]),
        ] {
            let mut eye = Matrix4::from_rotation_y(angle.to_radians());
            eye.w_axis.z = -1000.0;
            instance.sphere_rows = [
                eye.row(0).to_array(),
                eye.row(1).to_array(),
                eye.row(2).to_array(),
            ];
            pixels.fill(0xff14_283c);
            rasterize_environment(
                &Matrix4::IDENTITY,
                &vertices,
                instance,
                BlendMode::Alpha,
                &gradient,
                SamplerDesc {
                    wrap: SamplerWrap::Repeat,
                    ..gradient.sampler
                },
                None,
                64,
                64,
                0,
                64,
                &mut byte_writer(&mut pixels),
                &mut DepthRows {
                    pixels: &mut [],
                    unorm16: false,
                },
            );
            let actual = pixels[32 * 64 + 32];
            for (i, expected) in expected.into_iter().enumerate() {
                assert!(
                    (((actual >> (16 - i * 8)) & 255) as u8).abs_diff(expected) <= 3,
                    "{angle}: {actual:08x}"
                );
            }
        }
        let vertices = vertices.map(|mut v| {
            v.normal[3] = 6.0;
            v
        });
        instance.tint = [0.5, 0.75, 1.0, 0.5];
        for (mask, expected) in [(0.0, [30, 46, 65]), (1.0, [47, 78, 109])] {
            instance.texture_mask = mask;
            pixels.fill(0xff14_283c);
            rasterize_environment(
                &Matrix4::IDENTITY,
                &vertices,
                instance,
                BlendMode::Alpha,
                &base,
                SamplerDesc {
                    wrap: SamplerWrap::Repeat,
                    ..base.sampler
                },
                Some(&reflection),
                64,
                64,
                0,
                64,
                &mut byte_writer(&mut pixels),
                &mut DepthRows {
                    pixels: &mut [],
                    unorm16: false,
                },
            );
            let actual = pixels[32 * 64 + 32];
            for (i, expected) in expected.into_iter().enumerate() {
                assert!(
                    (((actual >> (16 - i * 8)) & 255) as u8).abs_diff(expected) <= 3,
                    "mask={mask}: {actual:08x}"
                );
            }
        }
    }

    impl TextureLookup for TestTextures {
        fn software_texture(&self, handle: TextureHandle) -> Option<&Texture> {
            self.lookups.fetch_add(1, Ordering::Relaxed);
            (handle == TEXTURE_HANDLE).then_some(&self.texture)
        }
    }

    #[test]
    fn half_values_and_rounding() {
        for bits in 0u16..=u16::MAX {
            if bits & 0x7c00 == 0x7c00 && bits & 0x03ff != 0 {
                continue;
            }
            assert_eq!(encode_half(decode_half(bits)), bits, "half {bits:04x}");
        }
        for (value, bits) in [
            (1.0, 0x3c00),
            (-0.75, 0xba00),
            (65504.0, 0x7bff),
            (65520.0, 0x7c00),
            (2.0_f32.powi(-24), 1),
            (2.0_f32.powi(-25), 0),
            (3.0 * 2.0_f32.powi(-25), 2),
            (1.0 + 2.0_f32.powi(-11), 0x3c00),
            (1.0 + 3.0 * 2.0_f32.powi(-11), 0x3c02),
        ] {
            assert_eq!(encode_half(value), bits, "value {value}");
        }
    }

    #[test]
    fn repeat_wrap_matches_euclidean_modulo_for_all_texture_sizes() {
        for max in 1usize..=513 {
            for index in -2_052i32..=2_052 {
                assert_eq!(
                    wrap_index(index, max, SamplerWrap::Repeat),
                    index.rem_euclid(max as i32) as usize
                );
            }
        }
    }

    #[test]
    fn offscreen_copy_preserves_pixels_across_alpha_mode_changes() {
        for (width, height) in [(1, 1), (17, 17)] {
            for modes in [[false, false], [false, true], [true, false], [true, true]] {
                let mut target =
                    create_offscreen_target(7, width, height, [width, height], false, false);
                for (index, pixel) in target.pixels.iter_mut().enumerate() {
                    *pixel = u32::from_be_bytes([index as u8, 17, (index * 31) as u8, 209]);
                }
                let mut expected = target.pixels.clone();
                for alpha in modes {
                    // Each pass blends over the retained previous pass.
                    for index in 0..expected.len() {
                        byte_writer(&mut target.pixels)(
                            index,
                            [0.2, 0.4, 0.7, 0.3],
                            BlendMode::Alpha,
                        );
                        byte_writer(&mut expected)(index, [0.2, 0.4, 0.7, 0.3], BlendMode::Alpha);
                    }
                    if !alpha {
                        for pixel in &mut expected {
                            *pixel |= 0xff00_0000;
                        }
                    }
                    if alpha {
                        copy_target_pixels::<true>(&mut target);
                    } else {
                        copy_target_pixels::<false>(&mut target);
                    }
                    assert_eq!(target.pixels, expected);
                    let expected_rgba: Vec<_> = expected
                        .iter()
                        .flat_map(|pixel| {
                            let [a, r, g, b] = pixel.to_be_bytes();
                            [r, g, b, a]
                        })
                        .collect();
                    assert_eq!(target.texture.image.as_raw(), &expected_rgba);
                }
            }
        }
    }

    #[test]
    fn texture_opacity_tracks_create_and_update_pixels() {
        let sampler = SamplerDesc::default();
        let opaque_image = RgbaImage::from_pixel(4, 3, Rgba([12, 34, 56, 255]));
        let mut texture = create_texture(&opaque_image, sampler).expect("texture creation works");
        assert!(texture.opaque);

        let translucent = RgbaImage::from_fn(4, 3, |x, y| {
            Rgba([
                x as u8,
                y as u8,
                99,
                if x == 2 && y == 1 { 254 } else { 255 },
            ])
        });
        update_texture(&mut texture, &translucent).expect("texture update works");
        assert!(!texture.opaque);

        update_texture(&mut texture, &opaque_image).expect("texture update works");
        assert!(texture.opaque);
        assert!(!texture_is_opaque(&RgbaImage::new(0, 0)));
    }

    #[test]
    fn bt709_limited_video_conversion_hits_reference_neutrals() {
        let sampler = SamplerDesc::default();
        let levels = [255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0, -128.0 / 224.0];
        let coeffs = [1.5748, -0.187_324, -0.468_124, 1.8556];
        let black = create_yuv420_texture(
            Yuv420Upload {
                width: 2,
                height: 2,
                y: &[16; 4],
                u: &[128],
                v: &[128],
                levels,
                coeffs,
            },
            sampler,
        )
        .unwrap();
        assert!(black.image.pixels().all(|pixel| pixel.0 == [0, 0, 0, 255]));
        assert!(texture_is_yuv420(&black));

        let white = create_yuv420_texture(
            Yuv420Upload {
                width: 2,
                height: 2,
                y: &[235; 4],
                u: &[128],
                v: &[128],
                levels,
                coeffs,
            },
            sampler,
        )
        .unwrap();
        assert!(
            white
                .image
                .pixels()
                .all(|pixel| pixel.0 == [255, 255, 255, 255])
        );
    }

    #[test]
    fn specialized_texture_samples_preserve_visible_results_exactly() {
        let opaque = RgbaImage::from_fn(8, 8, |x, y| {
            Rgba([
                (x * 29 + y * 7) as u8,
                (x * 11 + y * 31) as u8,
                (x * 19 + y * 17) as u8,
                255,
            ])
        });
        let mixed = RgbaImage::from_fn(8, 8, |x, y| {
            Rgba([
                (x * 37 + y * 13) as u8,
                (x * 5 + y * 41) as u8,
                (x * 23 + y * 3) as u8,
                if (x + y) % 4 == 0 {
                    0
                } else {
                    (32 + x * 17 + y * 11) as u8
                },
            ])
        });
        let coordinates = [
            [-3.25, -1.75],
            [-0.01, 0.0],
            [0.0, 0.0],
            [0.13, 0.49],
            [0.5, 0.5],
            [0.999, 1.0],
            [1.0, 1.0],
            [2.75, 4.125],
        ];
        for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
            let sampler = SamplerDesc {
                filter: SamplerFilter::Linear,
                wrap,
                mipmaps: false,
            };
            for [u, v] in coordinates {
                assert_eq!(
                    sample_tex_nearest::<true>((&opaque).into(), 8, 8, u, v, sampler),
                    sample_tex_nearest::<false>((&opaque).into(), 8, 8, u, v, sampler),
                );
                assert_eq!(
                    sample_tex_linear::<true>((&opaque).into(), 8, 8, u, v, sampler),
                    sample_tex_linear::<false>((&opaque).into(), 8, 8, u, v, sampler),
                );

                let nearest = sample_tex_nearest::<false>((&mixed).into(), 8, 8, u, v, sampler)
                    .expect("nonempty image samples");
                assert_eq!(
                    sample_alpha_nearest((&mixed).into(), 8, 8, u, v, sampler),
                    Some(nearest[3]),
                );
                let alpha = sample_alpha_linear((&mixed).into(), 8, 8, u, v, sampler)
                    .expect("nonempty image samples");
                match sample_tex_linear::<false>((&mixed).into(), 8, 8, u, v, sampler) {
                    Some(sample) => assert_eq!(alpha, sample[3]),
                    None => assert_eq!(alpha, 0.0),
                }
            }
        }
    }

    #[test]
    fn raster_modes_preserve_mask_and_opaque_pixels() {
        let opaque_a = RgbaImage::from_fn(8, 8, |x, y| {
            Rgba([
                (x * 29 + y * 7) as u8,
                (x * 11 + y * 31) as u8,
                (x * 19 + y * 17) as u8,
                255,
            ])
        });
        let opaque_b = RgbaImage::from_fn(8, 8, |x, y| {
            Rgba([
                255u8.wrapping_sub((x * 13 + y * 37) as u8),
                (x * 43 + y * 3) as u8,
                (x * 5 + y * 47) as u8,
                255,
            ])
        });
        let alpha_a = RgbaImage::from_fn(8, 8, |x, y| {
            Rgba([
                (x * 17 + y * 41) as u8,
                (x * 23 + y * 5) as u8,
                (x * 31 + y * 11) as u8,
                (31 + x * 19 + y * 13) as u8,
            ])
        });
        let alpha_b = RgbaImage::from_fn(8, 8, |x, y| {
            let alpha = (31 + x * 19 + y * 13) as u8;
            Rgba([255, 17, 203, alpha])
        });
        let vertices = [
            ScreenVertex {
                x: 8.0,
                y: 7.0,
                u: -0.2,
                v: 0.1,
            },
            ScreenVertex {
                x: 9.0,
                y: 70.0,
                u: 0.15,
                v: 1.3,
            },
            ScreenVertex {
                x: 87.0,
                y: 12.0,
                u: 1.2,
                v: -0.15,
            },
        ];
        let inv_denom = triangle_inv_denom(&vertices[0], &vertices[1], &vertices[2])
            .expect("test triangle is not degenerate");
        let render = |image: &RgbaImage,
                      sampler: SamplerDesc,
                      texture_mask: bool,
                      opaque: bool,
                      blend: BlendMode| {
            let mut pixels = vec![pack_rgba([0.03, 0.05, 0.07, 1.0]); WIDTH * HEIGHT];
            rasterize_triangle_with_inv(
                &vertices[0],
                &vertices[1],
                &vertices[2],
                inv_denom,
                [0.63, 0.72, 0.81, 0.68],
                texture_mask,
                blend,
                image.into(),
                sampler,
                opaque,
                WIDTH,
                HEIGHT,
                0,
                HEIGHT,
                &mut byte_writer(&mut pixels),
            );
            pixels
        };

        for filter in [SamplerFilter::Nearest, SamplerFilter::Linear] {
            for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
                let sampler = SamplerDesc {
                    filter,
                    wrap,
                    mipmaps: false,
                };
                for blend in [BlendMode::Alpha, BlendMode::Add] {
                    assert_eq!(
                        render(&opaque_a, sampler, false, true, blend),
                        render(&opaque_a, sampler, false, false, blend),
                        "opaque color specialization changed pixels"
                    );
                    assert_eq!(
                        render(&opaque_a, sampler, true, true, blend),
                        render(&opaque_b, sampler, true, true, blend),
                        "opaque masks must ignore every texture channel"
                    );
                    assert_eq!(
                        render(&alpha_a, sampler, true, false, blend),
                        render(&alpha_b, sampler, true, false, blend),
                        "alpha masks must ignore texture RGB"
                    );
                    assert_ne!(
                        render(&alpha_a, sampler, false, false, blend),
                        render(&alpha_b, sampler, false, false, blend),
                        "the fixture must expose non-mask RGB sampling"
                    );
                }
            }
        }
    }

    #[test]
    fn transparent_bilinear_samples_discard_nonzero_rgb() {
        let image = RgbaImage::from_fn(2, 2, |x, y| {
            Rgba([50 + x as u8 * 70, 80 + y as u8 * 60, 210, 0])
        });
        for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
            let sampler = SamplerDesc {
                filter: SamplerFilter::Linear,
                wrap,
                mipmaps: false,
            };
            for [u, v] in [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0], [-1.25, 2.75]] {
                assert_eq!(
                    sample_tex_linear::<false>((&image).into(), 2, 2, u, v, sampler),
                    None,
                );
                assert_eq!(
                    sample_alpha_linear((&image).into(), 2, 2, u, v, sampler),
                    Some(0.0),
                );
            }
        }
    }

    #[test]
    fn textured_mesh_near_clip_interpolates_edges_without_heap_storage() {
        let vertex = |clip: [f32; 4], u: f32, color: [f32; 4]| ClipVertexTexColor {
            clip: Vector4::from_array(clip),
            u,
            v: u * 2.0,
            color,
        };
        let a = vertex([-0.5, -0.5, 0.0, 1.0], 0.0, [0.0, 0.2, 0.4, 0.6]);
        let b = vertex([0.5, -0.5, 0.0, 1.0], 1.0, [1.0, 0.8, 0.6, 0.4]);
        let outside = vertex([0.0, 0.5, -2.0, 1.0], 0.5, [0.5; 4]);

        let (clipped, len) = clip_tmesh_near([a, b, outside]);
        assert_eq!(len, 4);
        assert_eq!(clipped[0].clip.to_array(), [-0.25, 0.0, -1.0, 1.0]);
        assert_eq!(clipped[1].clip.to_array(), a.clip.to_array());
        assert_eq!(clipped[2].clip.to_array(), b.clip.to_array());
        assert_eq!(clipped[3].clip.to_array(), [0.25, 0.0, -1.0, 1.0]);
        assert_eq!(clipped[0].u, 0.25);
        assert_eq!(clipped[3].u, 0.75);
        assert_eq!(clipped[0].v, 0.5);
        assert_eq!(clipped[3].v, 1.5);
        assert_eq!(clipped[0].color, [0.25, 0.35, 0.45, 0.55]);
        assert_eq!(clipped[3].color, [0.75, 0.65, 0.55, 0.45]);
    }

    fn depth_frame() -> RenderFrame {
        let geometries =
            [(0.5, 0.4), (0.8, 0.0), (0.15, -0.4)].map(|(r, z)| TexturedMeshGeometry {
                cache_key: 0,
                vertices: TexturedMeshVertices::Shared(Arc::from(
                    [
                        [-r, -r, z],
                        [r, -r, z],
                        [r, r, z],
                        [-r, -r, z],
                        [r, r, z],
                        [-r, r, z],
                    ]
                    .map(|pos| TexturedMeshVertex {
                        normal: [0.0; 4],
                        pos,
                        uv: [0.5; 2],
                        color: [1.0; 4],
                        tex_matrix_scale: [1.0; 2],
                    }),
                )),
            });
        RenderFrame {
            clear_color: [0.0; 4],
            render_targets: vec![],
            cameras: vec![],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            tmesh_geometries: geometries.to_vec(),
            tmesh_instances: [
                [0.0, 1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
            ]
            .map(|tint| {
                TexturedMeshInstanceRaw::new(
                    Matrix4::IDENTITY,
                    tint,
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    false,
                )
            })
            .to_vec(),
            ops: (0..3)
                .map(|i| {
                    DrawOp::TexturedMesh(TexturedMeshRun {
                        sampler: None,
                        additive_texture: 0,
                        geometry: i,
                        instance_start: i,
                        instance_count: 1,
                        texture_handle: TEXTURE_HANDLE,
                        blend: BlendMode::Alpha,
                        camera: 0,
                        depth_test: true,
                        // Runs 0 and 1 share depth; run 2 is isolated from them.
                        clear_depth: i == 2,
                    })
                })
                .collect(),
        }
    }

    #[test]
    fn model_depth_groups() {
        let textures = TestTextures {
            texture: create_texture(
                &RgbaImage::from_pixel(1, 1, Rgba([255; 4])),
                SamplerDesc::default(),
            )
            .unwrap(),
            lookups: AtomicUsize::new(0),
        };
        let mut frame = depth_frame();
        // Positive Z is nearer. The green foreground comes first in authored order;
        // the blue foreground belongs to a later note despite being farther away.
        let projection = Matrix4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        let mut overlay = TexturedMeshInstanceRaw::new(
            Matrix4::IDENTITY,
            [1.0, 1.0, 0.0, 1.0],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            false,
        );
        overlay.model_col3[2] = -0.8;
        frame.tmesh_instances.push(overlay);
        for (transparent, overlay) in [(false, false), (true, false), (false, true)] {
            frame.ops.truncate(3);
            if overlay {
                frame.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                    sampler: None,
                    additive_texture: 0,
                    geometry: 1,
                    instance_start: 3,
                    instance_count: 1,
                    texture_handle: TEXTURE_HANDLE,
                    blend: BlendMode::Alpha,
                    camera: 0,
                    depth_test: true,
                    clear_depth: true,
                }));
            }
            frame.tmesh_instances[0].tint[3] = if transparent { 0.0 } else { 1.0 };
            for staged in [false, true] {
                let mut prepared = Vec::new();
                let mut mesh = Vec::with_capacity(16);
                let mut tmesh = Vec::with_capacity(16);
                let fixed = prepare_objects(
                    (&frame).into(),
                    projection,
                    &textures,
                    WIDTH,
                    HEIGHT,
                    &mut prepared,
                    &mut mesh,
                    &mut tmesh,
                    staged,
                );
                let mut bins = StripeBins::warmed();
                bins.build(&prepared, &mesh, &tmesh, HEIGHT);
                for indexed in [false, true] {
                    let mut pixels = vec![0; WIDTH * HEIGHT];
                    for (stripe_index, stripe) in
                        pixels.chunks_mut(WIDTH * SOFTWARE_ROW_CHUNK).enumerate()
                    {
                        let start = stripe_index * SOFTWARE_ROW_CHUNK;
                        let end = start + stripe.len() / WIDTH;
                        // Stale depth nearer than every model; rows reset it first.
                        let mut depth = vec![0.0; stripe.len()];
                        draw_rows(
                            (&frame).into(),
                            &prepared,
                            indexed.then(|| bins.stripe(stripe_index)),
                            &mesh,
                            &tmesh,
                            &textures,
                            WIDTH,
                            HEIGHT,
                            start,
                            end,
                            &mut byte_writer(stripe),
                            fixed,
                            &mut depth,
                        );
                    }
                    for (x, expected) in [
                        (WIDTH / 3, if transparent { 0xff0000 } else { 0x00ff00 }),
                        (WIDTH / 2, 0x0000ff),
                        (WIDTH * 4 / 5, 0xff0000),
                    ] {
                        assert_eq!(
                            pixels[HEIGHT / 2 * WIDTH + x] & 0xffffff,
                            if overlay { 0xffff00 } else { expected },
                            "staged={staged}, indexed={indexed}, transparent={transparent}, x={x}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn model_sampler_pixels_match_in_every_raster_path() {
        use deadlib_render_core::MeshSampler;
        let image = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let textures = TestTextures {
            texture: create_texture(&image, SamplerDesc::default()).unwrap(),
            lookups: AtomicUsize::new(0),
        };
        let cases = [
            (
                Some(MeshSampler {
                    filter: SamplerFilter::Nearest,
                    wrap: SamplerWrap::Repeat,
                }),
                0.5,
                [0, 0, 255],
            ),
            (
                Some(MeshSampler {
                    filter: SamplerFilter::Linear,
                    wrap: SamplerWrap::Repeat,
                }),
                0.5,
                [128, 0, 128],
            ),
            (
                Some(MeshSampler {
                    filter: SamplerFilter::Nearest,
                    wrap: SamplerWrap::Repeat,
                }),
                0.5,
                [0, 0, 255],
            ),
            (
                Some(MeshSampler {
                    filter: SamplerFilter::Nearest,
                    wrap: SamplerWrap::Clamp,
                }),
                1.25,
                [0, 0, 255],
            ),
            (
                Some(MeshSampler {
                    filter: SamplerFilter::Nearest,
                    wrap: SamplerWrap::Repeat,
                }),
                1.25,
                [255, 0, 0],
            ),
            (None, 0.5, [128, 0, 128]),
        ];
        for staged in [false, true] {
            for indexed in [false, true] {
                for environment in [false, true] {
                    for (sampler, u, expected) in cases {
                        let vertices = [
                            [-1.0, -1.0],
                            [1.0, -1.0],
                            [1.0, 1.0],
                            [-1.0, -1.0],
                            [1.0, 1.0],
                            [-1.0, 1.0],
                        ]
                        .map(|[x, y]| TexturedMeshVertex {
                            normal: [0.0, 0.0, 1.0, if environment { 2.0 } else { 0.0 }],
                            pos: [x, y, 0.0],
                            uv: [u, 0.5],
                            tex_matrix_scale: [1.0; 2],
                            color: [1.0; 4],
                        });
                        let frame = RenderFrame {
                            clear_color: [0.0; 4],
                            render_targets: vec![],
                            cameras: vec![Matrix4::IDENTITY],
                            sprite_instances: vec![],
                            mesh_vertices: vec![],
                            tmesh_geometries: vec![TexturedMeshGeometry {
                                cache_key: 0,
                                vertices: TexturedMeshVertices::Shared(Arc::from(vertices)),
                            }],
                            tmesh_instances: vec![TexturedMeshInstanceRaw::new(
                                Matrix4::IDENTITY,
                                [1.0; 4],
                                [1.0; 2],
                                [0.0; 2],
                                [0.0; 2],
                                false,
                            )],
                            ops: vec![DrawOp::TexturedMesh(TexturedMeshRun {
                                sampler,
                                additive_texture: 0,
                                geometry: 0,
                                instance_start: 0,
                                instance_count: 1,
                                texture_handle: TEXTURE_HANDLE,
                                blend: BlendMode::Alpha,
                                camera: 0,
                                depth_test: false,
                                clear_depth: false,
                            })],
                        };
                        let mut prepared = Vec::new();
                        let mut mesh = Vec::with_capacity(16);
                        let mut tmesh = Vec::with_capacity(16);
                        let fixed = prepare_objects(
                            (&frame).into(),
                            Matrix4::IDENTITY,
                            &textures,
                            64,
                            64,
                            &mut prepared,
                            &mut mesh,
                            &mut tmesh,
                            staged,
                        );
                        let mut bins = StripeBins::warmed();
                        bins.build(&prepared, &mesh, &tmesh, 64);
                        let mut pixels = vec![0; 64 * 64];
                        for (stripe_index, stripe) in
                            pixels.chunks_mut(64 * SOFTWARE_ROW_CHUNK).enumerate()
                        {
                            let start = stripe_index * SOFTWARE_ROW_CHUNK;
                            draw_rows(
                                (&frame).into(),
                                &prepared,
                                indexed.then(|| bins.stripe(stripe_index)),
                                &mesh,
                                &tmesh,
                                &textures,
                                64,
                                64,
                                start,
                                start + stripe.len() / 64,
                                &mut byte_writer(stripe),
                                fixed,
                                &mut [],
                            );
                        }
                        let actual = pixels[32 * 64 + 32];
                        for (channel, expected) in expected.into_iter().enumerate() {
                            let value = ((actual >> (16 - channel * 8)) & 255) as u8;
                            assert!(
                                value.abs_diff(expected) <= 1,
                                "staged={staged} indexed={indexed} environment={environment} sampler={sampler:?} u={u}: {actual:08x}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn depth_resets_skip_rows_without_depth_draws() {
        let textures = TestTextures {
            texture: create_texture(
                &RgbaImage::from_pixel(1, 1, Rgba([255; 4])),
                SamplerDesc::default(),
            )
            .unwrap(),
            lookups: AtomicUsize::new(0),
        };
        let quad = |[x0, y0, x1, y1]: [f32; 4]| TexturedMeshGeometry {
            cache_key: 0,
            vertices: TexturedMeshVertices::Shared(Arc::from(
                [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]].map(|[x, y]| {
                    TexturedMeshVertex {
                        normal: [0.0; 4],
                        pos: [x, y, 0.0],
                        uv: [0.5; 2],
                        color: [1.0; 4],
                        tex_matrix_scale: [1.0; 2],
                    }
                }),
            )),
        };
        let run = |geometry, instance_start, depth_test, clear_depth| {
            DrawOp::TexturedMesh(TexturedMeshRun {
                sampler: None,
                additive_texture: 0,
                geometry,
                instance_start,
                instance_count: 1,
                texture_handle: TEXTURE_HANDLE,
                blend: BlendMode::Alpha,
                camera: 0,
                depth_test,
                clear_depth,
            })
        };
        // A near green model in the first stripe, a full-screen blue quad that
        // ignores depth, and a farther red model that must pass its reset.
        let frame = RenderFrame {
            clear_color: [0.0; 4],
            render_targets: vec![],
            cameras: vec![],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            tmesh_geometries: vec![quad([-0.5, 0.5, 0.5, 0.9]), quad([-1.0, -1.0, 1.0, 1.0])],
            tmesh_instances: [
                ([1.0, 0.0, 0.0, 1.0], 0.2),
                ([0.0, 0.0, 1.0, 1.0], 0.0),
                ([0.0, 1.0, 0.0, 1.0], -0.4),
            ]
            .map(|(tint, z)| {
                TexturedMeshInstanceRaw::new(
                    Matrix4::from_translation(Vec3::new(0.0, 0.0, z)),
                    tint,
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    false,
                )
            })
            .to_vec(),
            ops: vec![
                run(0, 2, true, false),
                run(1, 1, false, false),
                run(0, 0, true, true),
            ],
        };
        for staged in [false, true] {
            let mut prepared = Vec::new();
            let mut mesh = Vec::with_capacity(16);
            let mut tmesh = Vec::with_capacity(16);
            let fixed = prepare_objects(
                (&frame).into(),
                Matrix4::IDENTITY,
                &textures,
                WIDTH,
                HEIGHT,
                &mut prepared,
                &mut mesh,
                &mut tmesh,
                staged,
            );
            let mut bins = StripeBins::warmed();
            bins.build(&prepared, &mesh, &tmesh, HEIGHT);
            let mut pixels = vec![0; WIDTH * HEIGHT];
            // Stale depth rejects both models unless their rows are reset.
            let mut depth = vec![0.25; WIDTH * HEIGHT];
            for (stripe_index, (stripe, depth)) in pixels
                .chunks_mut(WIDTH * SOFTWARE_ROW_CHUNK)
                .zip(depth.chunks_mut(WIDTH * SOFTWARE_ROW_CHUNK))
                .enumerate()
            {
                let start = stripe_index * SOFTWARE_ROW_CHUNK;
                draw_rows(
                    (&frame).into(),
                    &prepared,
                    Some(bins.stripe(stripe_index)),
                    &mesh,
                    &tmesh,
                    &textures,
                    WIDTH,
                    HEIGHT,
                    start,
                    start + stripe.len() / WIDTH,
                    &mut byte_writer(stripe),
                    fixed,
                    depth,
                );
            }
            let (inside, outside) = (12 * WIDTH + WIDTH / 2, 12 * WIDTH + 4);
            assert_eq!(pixels[inside] & 0xffffff, 0xff0000, "staged={staged}");
            assert_eq!(pixels[outside] & 0xffffff, 0x0000ff, "staged={staged}");
            assert!((depth[inside] - 0.6).abs() < 1e-5, "staged={staged}");
            assert_eq!(depth[outside], 1.0, "staged={staged}");
            // Whole-object direct meshes visit every stripe; staged triangles
            // reach only the first, so the others never touch depth.
            let untouched = &depth[SOFTWARE_ROW_CHUNK * WIDTH..];
            assert_eq!(
                untouched.iter().all(|&z| z == 0.25),
                staged,
                "staged={staged}"
            );
        }
    }

    #[test]
    fn backfaces_do_not_cover_textured_front() {
        // A green front and a white rear, authored last, reproduce a closed
        // model without depending on any installed noteskin or its texture.
        let mut vertices = Vec::new();
        for (points, uv) in [
            (
                [[-0.8, -0.8, 0.2], [0.8, -0.8, 0.2], [0.0, 0.8, 0.2]],
                [0.25, 0.5],
            ),
            (
                [[-0.8, -0.8, -0.2], [0.0, 0.8, -0.2], [0.8, -0.8, -0.2]],
                [0.75, 0.5],
            ),
        ] {
            for pos in points {
                vertices.push(TexturedMeshVertex {
                    normal: [0.0; 4],
                    pos,
                    uv,
                    color: [1.0; 4],
                    tex_matrix_scale: [1.0; 2],
                });
            }
        }
        let image = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([0, 255, 0, 255])
            } else {
                Rgba([255; 4])
            }
        });
        let sampler = SamplerDesc {
            filter: SamplerFilter::Nearest,
            wrap: SamplerWrap::Repeat,
            mipmaps: false,
        };
        for (mvp, expected) in [
            (Matrix4::IDENTITY, 0x00ff00),
            (
                Matrix4::from_rotation_z(std::f32::consts::FRAC_PI_2),
                0x00ff00,
            ),
            (Matrix4::from_rotation_y(std::f32::consts::PI), 0xffffff),
        ] {
            for cull in [false, true] {
                let mut direct = vec![0; WIDTH * HEIGHT];
                rasterize_textured_mesh_triangles(
                    &mvp,
                    &vertices,
                    [1.0; 4],
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    false,
                    BlendMode::Alpha,
                    (&image).into(),
                    sampler,
                    true,
                    WIDTH,
                    HEIGHT,
                    0,
                    HEIGHT,
                    &mut byte_writer(&mut direct),
                    cull,
                    &mut DepthRows {
                        pixels: &mut [],
                        unorm16: false,
                    },
                );
                let mut prepared = Vec::with_capacity(4);
                prepare_tmesh_triangles(
                    &mut prepared,
                    0,
                    &mvp,
                    [1.0; 4],
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    &vertices,
                    WIDTH,
                    HEIGHT,
                    cull,
                )
                .unwrap();
                let mut retained = vec![0; WIDTH * HEIGHT];
                rasterize_prepared_tmesh(
                    &prepared,
                    false,
                    BlendMode::Alpha,
                    (&image).into(),
                    sampler,
                    true,
                    0,
                    HEIGHT,
                    &mut byte_writer(&mut retained),
                    WIDTH,
                    &mut DepthRows {
                        pixels: &mut [],
                        unorm16: false,
                    },
                );
                assert_eq!(retained, direct, "staged and direct culling must agree");
                assert_eq!(
                    direct[HEIGHT / 2 * WIDTH + WIDTH / 2] & 0xffffff,
                    if cull { expected } else { 0xffffff }
                );
                assert_eq!(prepared.len(), if cull { 1 } else { 2 });
            }
        }
        // Clipping preserves the facing test; it must not depend on any
        // triangle being wholly in front of the homogeneous near plane.
        vertices[2].pos[2] = -2.0;
        assert!(
            project_tmesh_polygon(
                &Matrix4::IDENTITY,
                [1.0; 4],
                [1.0; 2],
                [0.0; 2],
                [0.0; 2],
                &vertices[..3],
                WIDTH,
                HEIGHT,
                true
            )
            .is_some()
        );
    }

    #[test]
    fn visible_textured_mesh_projects_without_changing_vertices() {
        let vertices = [
            textured_vertex([-0.5, -0.5, 0.0], [0.0, 1.0]),
            textured_vertex([0.5, -0.5, 0.0], [1.0, 1.0]),
            textured_vertex([0.0, 0.5, 0.0], [0.5, 0.0]),
        ];
        let (projected, len) = project_tmesh_polygon(
            &Matrix4::IDENTITY,
            [1.0; 4],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &vertices,
            WIDTH,
            HEIGHT,
            false,
        )
        .expect("fully visible triangle projects");

        assert_eq!(len, 3);
        assert_eq!(
            projected.map(|vertex| [vertex.x, vertex.y]),
            [[24.0, 60.0], [72.0, 60.0], [48.0, 20.0], [0.0, 0.0]]
        );
        assert_eq!(
            projected.map(|vertex| [vertex.u, vertex.v]),
            [[0.0, 1.0], [1.0, 1.0], [0.5, 0.0], [0.0, 0.0]]
        );
        assert_eq!(
            projected.map(|vertex| vertex.color),
            [
                [0.9, 0.8, 0.7, 0.85],
                [0.9, 0.8, 0.7, 0.85],
                [0.9, 0.8, 0.7, 0.85],
                [0.0; 4],
            ]
        );
    }

    #[test]
    fn near_clipped_background_model_matches_retained_and_direct_paths() {
        let textures = test_textures();
        let vertices = [
            textured_vertex([-0.5, -0.5, 0.0], [0.0, 1.0]),
            textured_vertex([0.5, -0.5, 0.0], [1.0, 1.0]),
            textured_vertex([0.0, 0.5, -2.0], [0.5, 0.0]),
        ];
        let mut prepared = Vec::with_capacity(2);
        let (start, triangle_count, projected_count, rows) = prepare_tmesh_triangles(
            &mut prepared,
            0,
            &Matrix4::IDENTITY,
            [1.0; 4],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            &vertices,
            WIDTH,
            HEIGHT,
            false,
        )
        .expect("crossing triangle projects after clipping");
        assert_eq!(start, 0);
        assert_eq!(triangle_count, 2);
        assert_eq!(projected_count, 3);
        assert_eq!(rows, ScreenRows { start: 40, end: 61 });
        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared.capacity(), 2);

        let clear = pack_rgba([0.02, 0.03, 0.04, 1.0]);
        let mut retained = vec![clear; WIDTH * HEIGHT];
        let mut direct = retained.clone();
        rasterize_prepared_tmesh(
            &prepared,
            false,
            BlendMode::Alpha,
            textures.texture.texels(),
            textures.texture.sampler,
            textures.texture.opaque,
            0,
            HEIGHT,
            &mut byte_writer(&mut retained),
            WIDTH,
            &mut DepthRows {
                pixels: &mut [],
                unorm16: false,
            },
        );
        assert_eq!(
            rasterize_textured_mesh_triangles(
                &Matrix4::IDENTITY,
                &vertices,
                [1.0; 4],
                [1.0; 2],
                [0.0; 2],
                [0.0; 2],
                false,
                BlendMode::Alpha,
                textures.texture.texels(),
                textures.texture.sampler,
                textures.texture.opaque,
                WIDTH,
                HEIGHT,
                0,
                HEIGHT,
                &mut byte_writer(&mut direct),
                false,
                &mut DepthRows {
                    pixels: &mut [],
                    unorm16: false
                },
            ),
            3
        );
        assert_eq!(retained, direct);

        let mut changed = 0usize;
        for (index, pixel) in retained.into_iter().enumerate() {
            if pixel == clear {
                continue;
            }
            changed += 1;
            let x = index % WIDTH;
            let y = index / WIDTH;
            assert!((24..=72).contains(&x), "clipped pixel escaped in x: {x}");
            assert!((40..=60).contains(&y), "clipped pixel escaped in y: {y}");
        }
        assert!(changed > 100);
    }

    #[test]
    fn prepared_objects_preserve_striped_mixed_rendering() {
        let textures = test_textures();
        let frame = mixed_frame();
        let fallback = Matrix4::from_scale_rotation_translation(
            Vec3::splat(0.93),
            glam::Quat::from_rotation_z(0.04),
            Vec3::new(0.03, -0.02, 0.0),
        ) * frame.cameras[0];
        let clear = pack_rgba([0.025, 0.05, 0.075, 1.0]);
        let mut staged_pixels = vec![clear; WIDTH * HEIGHT];
        let mut direct_pixels = vec![clear; WIDTH * HEIGHT];

        let mut prepared = Vec::new();
        let mut prepared_mesh = Vec::with_capacity(MESH_STAGE_VERTEX_CAP);
        let mut prepared_tmesh = Vec::with_capacity(MESH_STAGE_VERTEX_CAP);
        let staged_fixed_vertices = prepare_objects(
            (&frame).into(),
            fallback,
            &textures,
            WIDTH,
            HEIGHT,
            &mut prepared,
            &mut prepared_mesh,
            &mut prepared_tmesh,
            true,
        );
        assert!(!prepared_mesh.is_empty());
        assert_eq!(prepared_tmesh.len(), 1);
        let staged_vertices = render_prepared_stripes(
            &frame,
            &prepared,
            &prepared_mesh,
            &prepared_tmesh,
            &textures,
            &mut staged_pixels,
            staged_fixed_vertices,
        );
        let direct_fixed_vertices = prepare_objects(
            (&frame).into(),
            fallback,
            &textures,
            WIDTH,
            HEIGHT,
            &mut prepared,
            &mut prepared_mesh,
            &mut prepared_tmesh,
            false,
        );
        assert!(prepared_mesh.is_empty());
        assert!(prepared_tmesh.is_empty());
        assert!(
            prepared
                .iter()
                .any(|object| matches!(object, PreparedObject::DirectMesh { .. }))
        );
        assert!(
            prepared
                .iter()
                .any(|object| matches!(object, PreparedObject::DirectTexturedMesh { .. }))
        );
        let direct_vertices = render_prepared_stripes(
            &frame,
            &prepared,
            &prepared_mesh,
            &prepared_tmesh,
            &textures,
            &mut direct_pixels,
            direct_fixed_vertices,
        );

        assert_eq!(staged_vertices, direct_vertices);
        assert!(staged_vertices > 0);
        assert_eq!(staged_pixels, direct_pixels);
        assert!(staged_pixels.iter().any(|pixel| *pixel != clear));
    }

    #[test]
    fn stripe_bins_preserve_pixels_counts_and_painter_order() {
        let textures = test_textures();
        let frame = mixed_frame();
        let clear = pack_rgba(frame.clear_color);
        let mut prepared = Vec::new();
        let mut prepared_mesh = Vec::with_capacity(MESH_STAGE_VERTEX_CAP / 3);
        let mut prepared_tmesh = Vec::with_capacity(MESH_STAGE_VERTEX_CAP / 3);
        let fixed_vertices = prepare_objects(
            (&frame).into(),
            Matrix4::IDENTITY,
            &textures,
            WIDTH,
            HEIGHT,
            &mut prepared,
            &mut prepared_mesh,
            &mut prepared_tmesh,
            true,
        );

        let mut bins = StripeBins::warmed();
        bins.build(&prepared, &prepared_mesh, &prepared_tmesh, HEIGHT);
        let stripe_count = HEIGHT.div_ceil(SOFTWARE_ROW_CHUNK);
        assert!(bins.items.len() < prepared.len() * stripe_count);
        let item_order = |item: StripeItem| {
            if item.is_whole() {
                (item.index() as u32, u32::MAX)
            } else if item.is_tmesh() {
                let index = item.index();
                (prepared_tmesh[index].object, index as u32)
            } else {
                let index = item.index();
                (prepared_mesh[index].object, index as u32)
            }
        };
        for stripe in 0..stripe_count {
            assert!(
                bins.stripe(stripe)
                    .windows(2)
                    .all(|pair| item_order(pair[0]) < item_order(pair[1]))
            );
        }

        let mut scanned = vec![clear; WIDTH * HEIGHT];
        let scanned_vertices = render_prepared_stripes(
            &frame,
            &prepared,
            &prepared_mesh,
            &prepared_tmesh,
            &textures,
            &mut scanned,
            fixed_vertices,
        );
        textures.lookups.store(0, Ordering::Relaxed);
        let mut indexed = vec![0xdead_beef; WIDTH * HEIGHT];
        let indexed_vertices = render_indexed_stripes(
            &frame,
            &prepared,
            &prepared_mesh,
            &prepared_tmesh,
            &bins,
            &textures,
            &mut indexed,
            clear,
            fixed_vertices,
        );

        assert_eq!(indexed_vertices, scanned_vertices);
        assert_eq!(indexed, scanned);
        let lookups = textures.lookups.load(Ordering::Relaxed);
        assert!(lookups > 0);
        assert!(lookups <= stripe_count);
    }

    #[test]
    fn mesh_staging_saturates_without_growing_buffers() {
        let textures = test_textures();
        let frame = mixed_frame();
        let mut prepared = Vec::new();
        let mut prepared_mesh = Vec::with_capacity(1);
        let mut prepared_tmesh = Vec::with_capacity(1);
        let mesh_capacity = prepared_mesh.capacity();
        let tmesh_capacity = prepared_tmesh.capacity();

        prepare_objects(
            (&frame).into(),
            Matrix4::IDENTITY,
            &textures,
            WIDTH,
            HEIGHT,
            &mut prepared,
            &mut prepared_mesh,
            &mut prepared_tmesh,
            true,
        );

        assert!(prepared_mesh.is_empty());
        assert!(prepared_tmesh.is_empty());
        assert_eq!(prepared_mesh.capacity(), mesh_capacity);
        assert_eq!(prepared_tmesh.capacity(), tmesh_capacity);
        assert!(
            prepared
                .iter()
                .any(|object| matches!(object, PreparedObject::DirectMesh { .. }))
        );
        assert!(
            prepared
                .iter()
                .any(|object| matches!(object, PreparedObject::DirectTexturedMesh { .. }))
        );
    }

    fn render_prepared_stripes(
        frame: &RenderFrame,
        prepared: &[PreparedObject],
        mesh_triangles: &[PreparedTriangle<ScreenVertexColor>],
        tmesh_triangles: &[PreparedTriangle<ScreenVertexTexColor>],
        textures: &TestTextures,
        pixels: &mut [u32],
        fixed_vertices: u32,
    ) -> u32 {
        pixels
            .chunks_mut(WIDTH * SOFTWARE_ROW_CHUNK)
            .enumerate()
            .map(|(chunk_index, stripe)| {
                let y_start = chunk_index * SOFTWARE_ROW_CHUNK;
                let y_end = y_start + stripe.len() / WIDTH;
                draw_rows(
                    frame.into(),
                    prepared,
                    None,
                    mesh_triangles,
                    tmesh_triangles,
                    textures,
                    WIDTH,
                    HEIGHT,
                    y_start,
                    y_end,
                    &mut byte_writer(stripe),
                    fixed_vertices,
                    &mut [],
                )
            })
            .sum()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_indexed_stripes(
        frame: &RenderFrame,
        prepared: &[PreparedObject],
        mesh_triangles: &[PreparedTriangle<ScreenVertexColor>],
        tmesh_triangles: &[PreparedTriangle<ScreenVertexTexColor>],
        bins: &StripeBins,
        textures: &TestTextures,
        pixels: &mut [u32],
        clear: u32,
        fixed_vertices: u32,
    ) -> u32 {
        pixels
            .chunks_mut(WIDTH * SOFTWARE_ROW_CHUNK)
            .enumerate()
            .map(|(chunk_index, stripe)| {
                stripe.fill(clear);
                let y_start = chunk_index * SOFTWARE_ROW_CHUNK;
                let y_end = y_start + stripe.len() / WIDTH;
                draw_rows(
                    frame.into(),
                    prepared,
                    Some(bins.stripe(chunk_index)),
                    mesh_triangles,
                    tmesh_triangles,
                    textures,
                    WIDTH,
                    HEIGHT,
                    y_start,
                    y_end,
                    &mut byte_writer(stripe),
                    fixed_vertices,
                    &mut [],
                )
            })
            .sum()
    }

    fn mixed_frame() -> RenderFrame {
        let mesh_vertices = vec![
            MeshVertex {
                pos: [-120.0, -80.0],
                color: [1.0, 0.2, 0.1, 0.7],
            },
            MeshVertex {
                pos: [20.0, -70.0],
                color: [0.1, 1.0, 0.2, 0.8],
            },
            MeshVertex {
                pos: [-35.0, 90.0],
                color: [0.2, 0.3, 1.0, 0.9],
            },
            MeshVertex {
                pos: [f32::NAN, 0.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [0.0, 0.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [1.0, 1.0],
                color: [1.0; 4],
            },
        ];
        let textured_vertices: Arc<[TexturedMeshVertex]> = vec![
            textured_vertex([-80.0, -60.0, 0.0], [0.0, 1.0]),
            textured_vertex([75.0, -55.0, 0.0], [1.0, 1.0]),
            textured_vertex([5.0, 85.0, 0.0], [0.5, 0.0]),
            textured_vertex([f32::NAN, 0.0, 0.0], [0.0, 0.0]),
            textured_vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            textured_vertex([1.0, 1.0, 0.0], [0.0, 0.0]),
        ]
        .into();
        RenderFrame {
            clear_color: [0.025, 0.05, 0.075, 1.0],
            render_targets: Vec::new(),
            cameras: vec![glam::camera::rh::proj::opengl::orthographic(
                -288.0, 288.0, -240.0, 240.0, -1.0, 1.0,
            )],
            sprite_instances: vec![
                sprite([-30.0, 15.0], 0.17, 0.92),
                sprite([0.0, 0.0], -0.31, 0.0),
                sprite([25.0, -25.0], 0.43, 0.75),
                sprite([30.0, 20.0], -0.12, 0.68),
            ],
            mesh_vertices,
            tmesh_instances: vec![
                TexturedMeshInstanceRaw::new(
                    Matrix4::from_translation(Vec3::new(50.0, 5.0, 0.0)),
                    [0.7, 0.8, 1.0, 0.72],
                    [0.85, 0.9],
                    [0.07, 0.11],
                    [0.2, 0.3],
                    false,
                ),
                TexturedMeshInstanceRaw::new(
                    Matrix4::from_translation(Vec3::new(-45.0, 20.0, 0.0)),
                    [0.8, 0.7, 0.9, 0.65],
                    [0.9, 0.85],
                    [0.03, 0.08],
                    [0.1, 0.2],
                    false,
                ),
            ],
            tmesh_geometries: vec![TexturedMeshGeometry {
                vertices: TexturedMeshVertices::Shared(textured_vertices),
                cache_key: INVALID_TMESH_CACHE_KEY,
            }],
            ops: vec![
                DrawOp::Mesh(MeshRun {
                    vertex_start: 0,
                    vertex_count: 6,
                    blend: BlendMode::Alpha,
                    camera: 0,
                }),
                DrawOp::Sprite(SpriteRun {
                    instance_start: 0,
                    instance_count: 2,
                    blend: BlendMode::Alpha,
                    texture_handle: TEXTURE_HANDLE,
                    camera: 0,
                }),
                DrawOp::Sprite(SpriteRun {
                    instance_start: 2,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: MISSING_TEXTURE_HANDLE,
                    camera: 0,
                }),
                DrawOp::TexturedMesh(TexturedMeshRun {
                    sampler: None,
                    additive_texture: 0,
                    geometry: 0,
                    instance_start: 0,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: TEXTURE_HANDLE,
                    camera: 0,
                    depth_test: false,
                    clear_depth: false,
                }),
                DrawOp::TexturedMesh(TexturedMeshRun {
                    sampler: None,
                    additive_texture: 0,
                    geometry: 0,
                    instance_start: 1,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: MISSING_TEXTURE_HANDLE,
                    camera: 0,
                    depth_test: false,
                    clear_depth: false,
                }),
                DrawOp::Sprite(SpriteRun {
                    instance_start: 3,
                    instance_count: 1,
                    blend: BlendMode::Add,
                    texture_handle: TEXTURE_HANDLE,
                    camera: 99,
                }),
            ],
        }
    }

    fn textured_vertex(pos: [f32; 3], uv: [f32; 2]) -> TexturedMeshVertex {
        TexturedMeshVertex {
            normal: [0.0; 4],
            pos,
            uv,
            color: [0.9, 0.8, 0.7, 0.85],
            tex_matrix_scale: [1.2, 0.8],
        }
    }

    fn sprite(center: [f32; 2], angle: f32, alpha: f32) -> SpriteInstanceRaw {
        let local_angle = angle * -0.7;
        SpriteInstanceRaw {
            center: [center[0], center[1], 0.0, 1.0],
            size: [185.0, 145.0],
            rot_sin_cos: [angle.sin(), angle.cos()],
            tint: [0.75, 0.85, 0.95, alpha],
            uv_scale: [0.72, 0.81],
            uv_offset: [0.13, 0.09],
            local_offset: [9.0, -6.0],
            local_offset_rot_sin_cos: [local_angle.sin(), local_angle.cos()],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        }
    }

    fn test_textures() -> TestTextures {
        TestTextures {
            texture: Texture {
                image: RgbaImage::from_fn(8, 8, |x, y| {
                    Rgba([
                        (x * 27 + y * 5) as u8,
                        (x * 9 + y * 23) as u8,
                        (x * 17 + y * 13) as u8,
                        160 + ((x + y) % 4) as u8 * 25,
                    ])
                }),
                sampler: SamplerDesc {
                    filter: SamplerFilter::Nearest,
                    wrap: SamplerWrap::Clamp,
                    mipmaps: false,
                },
                opaque: false,
                yuv420: false,
                half_pixels: Vec::new(),
            },
            lookups: AtomicUsize::new(0),
        }
    }
}
