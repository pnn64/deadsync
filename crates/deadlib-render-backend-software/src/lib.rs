use deadlib_render_core::{
    BlendMode, DrawOp, DrawStats, RenderFrame, RenderTargetFrame, SOFTWARE_MESH_STORAGE_SLOT,
    SOFTWARE_OBJECTS_STORAGE_SLOT, SOFTWARE_TMESH_STORAGE_SLOT, SamplerDesc, SamplerFilter,
    SamplerWrap, TextureHandle, TexturedMeshGeometry, TexturedMeshInstanceRaw, Yuv420Upload,
    draw_storage_stats, is_render_target_texture, render_target_base_handle,
    render_target_uses_nearest,
};
use glam::{Mat4 as Matrix4, Vec4 as Vector4};
use image::RgbaImage;
use log::info;
use rayon::prelude::*;
use std::{error::Error, num::NonZeroU32, sync::Arc, time::Instant};
use winit::{dpi::PhysicalSize, window::Window};

const SOFTWARE_ROW_CHUNK: usize = 32;
// Staging wins once enough row workers would otherwise repeat every transform.
const MIN_STAGE_MESH_STRIPES: usize = 12;
// Covers the current two-player density graph while bounding retained memory.
// Frames that exceed either buffer render through the direct path instead.
const MESH_STAGE_VERTEX_CAP: usize = 36 * 1024;
const U8_TO_F32: f32 = 1.0 / 255.0;
const LOGICAL_HEIGHT: f32 = 480.0;
const DESIGN_WIDTH_16_9: f32 = 854.0;

pub struct Texture {
    pub image: RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    yuv420: bool,
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
    offscreen_targets: Vec<OffscreenTarget>,
}

/// Render-thread-owned, song-reused software `ActorFrameTexture` storage.
/// Slots are bounded by the largest active graph, allocated at graph warmup,
/// and replaced only when its handle or dimensions change. Gameplay redraws
/// reuse the pixel buffers without lookup, eviction, pruning, I/O, or
/// destruction; the renderer owns and frees them at shutdown. A pass miss is a
/// transparent texture, and per-frame work is bounded by target pixels plus its
/// draw list.
struct OffscreenTarget {
    handle: TextureHandle,
    width: u32,
    height: u32,
    texture: Texture,
    pixels: Vec<u32>,
    initialized: bool,
}

#[derive(Clone, Copy)]
struct SoftwarePass<'a> {
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
        projected_count: u32,
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
        projected_count: u32,
        rows: ScreenRows,
        texture_mask: bool,
        blend: BlendMode,
        texture_handle: TextureHandle,
    },
    DirectTexturedMesh {
        geometry: u32,
        instance: u32,
        mvp: Matrix4,
        blend: BlendMode,
        texture_handle: TextureHandle,
    },
}

impl PreparedObject {
    #[inline(always)]
    const fn rows(&self, height: usize) -> ScreenRows {
        match self {
            Self::Sprite { rows, .. }
            | Self::Mesh { rows, .. }
            | Self::TexturedMesh { rows, .. } => *rows,
            Self::DirectMesh { .. } | Self::DirectTexturedMesh { .. } => ScreenRows {
                start: 0,
                end: height as u32,
            },
        }
    }

    #[inline(always)]
    const fn fixed_vertices(&self) -> u32 {
        match self {
            Self::Sprite { .. } => 4,
            Self::Mesh {
                projected_count, ..
            }
            | Self::TexturedMesh {
                projected_count, ..
            } => *projected_count,
            Self::DirectMesh { .. } | Self::DirectTexturedMesh { .. } => 0,
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
        self.0 & Self::TEXTURED != 0 && !self.is_whole()
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
        for stripe in 0..stripe_count {
            self.offsets[stripe + 1] += self.offsets[stripe];
        }

        self.items.clear();
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
        for stripe in first.min(stripe_count)..end {
            offsets[stripe + 1] += 1;
        }
    }

    #[inline]
    fn insert_rows(&mut self, rows: ScreenRows, item: StripeItem, stripe_count: usize) {
        let first = rows.start as usize / SOFTWARE_ROW_CHUNK;
        let end = (rows.end as usize)
            .div_ceil(SOFTWARE_ROW_CHUNK)
            .min(stripe_count);
        for stripe in first.min(stripe_count)..end {
            let slot = self.cursors[stripe] as usize;
            self.items[slot] = item;
            self.cursors[stripe] += 1;
        }
    }
}

pub fn init(window: Arc<Window>, _vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing software renderer backend (softbuffer)...");

    let window_size = window.inner_size();
    let projection = ortho_for_window(window_size.width, window_size.height);

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
    })
}

pub fn update_texture(texture: &mut Texture, image: &RgbaImage) -> Result<(), Box<dyn Error>> {
    texture.image.clone_from(image);
    texture.opaque = texture_is_opaque(image);
    texture.yuv420 = false;
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
    })
}

pub fn update_yuv420_texture(
    texture: &mut Texture,
    upload: Yuv420Upload<'_>,
) -> Result<(), Box<dyn Error>> {
    texture.image = yuv420_to_rgba(upload)?;
    texture.opaque = true;
    texture.yuv420 = true;
    Ok(())
}

#[inline(always)]
#[must_use]
pub const fn texture_is_yuv420(texture: &Texture) -> bool {
    texture.yuv420
}

fn yuv420_to_rgba(upload: Yuv420Upload<'_>) -> Result<RgbaImage, Box<dyn Error>> {
    if !upload.is_valid() {
        return Err(std::io::Error::other("invalid YUV420 planes").into());
    }

    let width = upload.width as usize;
    let height = upload.height as usize;
    let luma_len = width * height;

    let mut rgba = vec![0; luma_len * 4];
    for row in 0..height {
        let chroma_row = row / 2 * (width / 2);
        for col in 0..width {
            let pixel = row * width + col;
            let chroma = chroma_row + col / 2;
            let y =
                (f32::from(upload.y[pixel]) / 255.0).mul_add(upload.levels[0], upload.levels[1]);
            let u =
                (f32::from(upload.u[chroma]) / 255.0).mul_add(upload.levels[2], upload.levels[3]);
            let v =
                (f32::from(upload.v[chroma]) / 255.0).mul_add(upload.levels[2], upload.levels[3]);
            let out = &mut rgba[pixel * 4..pixel * 4 + 4];
            out[0] = (upload.coeffs[0].mul_add(v, y).clamp(0.0, 1.0) * 255.0).round() as u8;
            out[1] = (upload.coeffs[2]
                .mul_add(v, upload.coeffs[1].mul_add(u, y))
                .clamp(0.0, 1.0)
                * 255.0)
                .round() as u8;
            out[2] = (upload.coeffs[3].mul_add(u, y).clamp(0.0, 1.0) * 255.0).round() as u8;
            out[3] = 255;
        }
    }
    RgbaImage::from_raw(upload.width, upload.height, rgba)
        .ok_or_else(|| std::io::Error::other("invalid converted YUV420 image").into())
}

#[inline]
fn texture_is_opaque(image: &RgbaImage) -> bool {
    image.width() != 0
        && image.height() != 0
        && image
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
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

fn create_offscreen_target(handle: TextureHandle, width: u32, height: u32) -> OffscreenTarget {
    let width = width.max(1);
    let height = height.max(1);
    let len = width as usize * height as usize;
    OffscreenTarget {
        handle,
        width,
        height,
        texture: Texture {
            image: RgbaImage::new(width, height),
            sampler: SamplerDesc {
                filter: SamplerFilter::Linear,
                wrap: SamplerWrap::Clamp,
                mipmaps: false,
            },
            opaque: false,
            yuv420: false,
        },
        pixels: vec![0; len],
        initialized: false,
    }
}

fn ensure_offscreen_targets(targets: &mut Vec<OffscreenTarget>, frame: &RenderFrame) {
    for (index, pass) in frame.render_targets.iter().enumerate() {
        let matches = targets.get(index).is_some_and(|target| {
            target.handle == pass.texture_handle
                && target.width == pass.width.max(1)
                && target.height == pass.height.max(1)
        });
        if matches {
            continue;
        }
        let target = create_offscreen_target(pass.texture_handle, pass.width, pass.height);
        if index < targets.len() {
            targets[index] = target;
        } else {
            targets.push(target);
        }
    }
}

fn copy_target_pixels(target: &mut OffscreenTarget) {
    for (rgba, pixel) in target
        .texture
        .image
        .as_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(target.pixels.iter().copied())
    {
        rgba[0] = (pixel >> 16) as u8;
        rgba[1] = (pixel >> 8) as u8;
        rgba[2] = pixel as u8;
        rgba[3] = (pixel >> 24) as u8;
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
        let width = pass.width.max(1) as usize;
        let height = pass.height.max(1) as usize;
        let initialized = targets[index].initialized;
        let mut pixels = std::mem::take(&mut targets[index].pixels);
        if !pass.preserve || !initialized {
            pixels.fill(if pass.alpha { 0 } else { 0xff00_0000 });
        }
        let resolved = ResolvedTextures {
            external: textures,
            targets: &targets,
        };
        let software_pass = SoftwarePass::from(pass);
        prepare_objects(
            software_pass,
            pass.cameras
                .first()
                .copied()
                .unwrap_or_else(|| ortho_for_window(pass.width.max(1), pass.height.max(1))),
            &resolved,
            width,
            height,
            &mut state.prepared_objects,
            &mut state.prepared_mesh_triangles,
            &mut state.prepared_tmesh_triangles,
            false,
        );
        let fixed_vertices = state.prepared_objects.iter().fold(0u32, |sum, object| {
            sum.saturating_add(object.fixed_vertices())
        });
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
            &mut pixels,
            fixed_vertices,
        ));
        if !pass.alpha {
            for pixel in &mut pixels {
                *pixel |= 0xff00_0000;
            }
        }
        targets[index].pixels = pixels;
        targets[index].initialized = true;
        copy_target_pixels(&mut targets[index]);
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
    prepare_objects(
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
    let fixed_vertices = state.prepared_objects.iter().fold(0u32, |sum, object| {
        sum.saturating_add(object.fixed_vertices())
    });
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
                .enumerate()
                .map(|(chunk_index, stripe)| {
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
                        stripe,
                        fixed_vertices,
                    )
                })
                .reduce(|| 0, u32::saturating_add)
        })
    } else {
        buffer.fill(clear);
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
            &mut buffer,
            fixed_vertices,
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
) {
    prepared.clear();
    prepared.reserve(
        frame
            .sprite_instances
            .len()
            .saturating_add(frame.tmesh_instances.len()),
    );
    mesh_triangles.clear();
    tmesh_triangles.clear();

    for op in frame.ops {
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
                        [1.0; 4],
                        vertices,
                        width,
                        height,
                    )
                else {
                    continue;
                };
                prepared.push(PreparedObject::Mesh {
                    triangle_start,
                    triangle_count,
                    projected_count,
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
                    {
                        prepared.push(PreparedObject::DirectTexturedMesh {
                            geometry: run.geometry,
                            instance: instance_index,
                            mvp,
                            blend: run.blend,
                            texture_handle: run.texture_handle,
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
                        )
                    else {
                        continue;
                    };
                    prepared.push(PreparedObject::TexturedMesh {
                        triangle_start,
                        triangle_count,
                        projected_count,
                        rows,
                        texture_mask: instance.texture_mask != 0.0,
                        blend: run.blend,
                        texture_handle: run.texture_handle,
                    });
                }
            }
        }
    }
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
    buffer: &mut [u32],
    fixed_vertices: u32,
) -> u32 {
    let mut vertices_drawn = fixed_vertices;
    let mut texture_cache = None;
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
            let drawn = if let Some(triangle) = triangle {
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
                )
            } else {
                draw_prepared(
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
                )
            };
            vertices_drawn = vertices_drawn.saturating_add(drawn);
        }
    } else {
        for prepared in prepared_objects {
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
            ));
        }
    }
    vertices_drawn
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
    buffer: &mut [u32],
) -> u32 {
    match prepared {
        PreparedObject::Sprite {
            vertices,
            rows,
            inv_denom,
            tint,
            texture_mask,
            blend,
            texture_handle,
        } => {
            let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                return 0;
            };
            if rows_known_visible || rows.overlaps(stripe_y_start, stripe_y_end) {
                rasterize_prepared_sprite(
                    vertices,
                    *inv_denom,
                    *tint,
                    *texture_mask,
                    *blend,
                    &tex.image,
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
                [1.0; 4],
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
            texture_handle,
            ..
        } => {
            let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                return 0;
            };
            if rows_known_visible || rows.overlaps(stripe_y_start, stripe_y_end) {
                let start = *triangle_start as usize;
                let end = start + *triangle_count as usize;
                rasterize_prepared_tmesh(
                    &tmesh_triangles[start..end],
                    *texture_mask,
                    *blend,
                    &tex.image,
                    effective_sampler(tex, *texture_handle),
                    tex.opaque,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                    width,
                );
            }
            0
        }
        PreparedObject::DirectTexturedMesh {
            geometry,
            instance,
            mvp,
            blend,
            texture_handle,
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
            rasterize_textured_mesh_triangles(
                mvp,
                geometry.vertices.as_ref(),
                instance.tint,
                instance.uv_scale,
                instance.uv_offset,
                instance.uv_tex_shift,
                instance.texture_mask != 0.0,
                *blend,
                &tex.image,
                effective_sampler(tex, *texture_handle),
                tex.opaque,
                width,
                height,
                stripe_y_start,
                stripe_y_end,
                buffer,
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
    buffer: &mut [u32],
    width: usize,
) -> u32 {
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
            texture_handle,
            ..
        } => {
            let Some(tex) = resolve_texture(textures, texture_cache, *texture_handle) else {
                return 0;
            };
            let triangle = &tmesh_triangles[triangle as usize];
            rasterize_triangle_tex_color_prepared(
                &triangle.vertices,
                triangle.setup,
                *blend,
                *texture_mask,
                &tex.image,
                SamplerDesc {
                    wrap: SamplerWrap::Repeat,
                    ..effective_sampler(tex, *texture_handle)
                },
                tex.opaque,
                stripe_y_start,
                stripe_y_end,
                buffer,
                width,
            );
        }
        _ => debug_assert!(false, "whole objects must use whole-object stripe items"),
    }
    0
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
    if width == 0 || height == 0 {
        return;
    }
    state.projection = ortho_for_window(width, height);
}

pub const fn set_default_projection(state: &mut State, projection: Matrix4) {
    state.projection = projection;
}

pub fn cleanup(_state: &mut State) {
    info!("Software renderer backend cleanup.");
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

#[inline(always)]
const fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[inline(always)]
const fn pack_rgba(c: [f32; 4]) -> u32 {
    let r = clamp01(c[0]).mul_add(255.0, 0.5) as u32;
    let g = clamp01(c[1]).mul_add(255.0, 0.5) as u32;
    let b = clamp01(c[2]).mul_add(255.0, 0.5) as u32;
    let a = clamp01(c[3]).mul_add(255.0, 0.5) as u32;

    (a << 24) | (r << 16) | (g << 8) | b
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
        let start = min_y.floor().max(0.0) as u32;
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
    tint: [f32; 4],
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
                color: [
                    chunk[i].color[0] * tint[0],
                    chunk[i].color[1] * tint[1],
                    chunk[i].color[2] * tint[2],
                    chunk[i].color[3] * tint[3],
                ],
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

    let clipped;
    let polygon: &[ClipVertexTexColor] = if triangle
        .iter()
        .all(|vertex| vertex.clip.z + vertex.clip.w >= 0.0)
    {
        &triangle
    } else {
        let result = clip_tmesh_near(triangle);
        clipped = result.0;
        &clipped[..result.1]
    };
    let mut projected = [ScreenVertexTexColor {
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
            x: f32::midpoint(ndc_x, 1.0) * width as f32,
            y: ((1.0 - ndc_y) * 0.5) * height as f32,
            u: vertex.u,
            v: vertex.v,
            color: vertex.color,
        };
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
    image: &RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) -> u32 {
    if tint[3] <= 0.0 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
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

    4
}

fn rasterize_prepared_mesh(
    triangles: &[PreparedTriangle<ScreenVertexColor>],
    blend: BlendMode,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
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
    image: &RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
    width: usize,
) {
    let sampler = SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..sampler
    };
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
        );
    }
}

fn rasterize_mesh_triangles(
    mvp: &Matrix4,
    tint: [f32; 4],
    vertices: &[deadlib_render_core::MeshVertex],
    blend: BlendMode,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
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
                color: [
                    chunk[i].color[0] * tint[0],
                    chunk[i].color[1] * tint[1],
                    chunk[i].color[2] * tint[2],
                    chunk[i].color[3] * tint[3],
                ],
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

fn rasterize_textured_mesh_triangles(
    mvp: &Matrix4,
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: bool,
    blend: BlendMode,
    image: &RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) -> u32 {
    if vertices.len() < 3 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
    }

    let sampler = SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..sampler
    };

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
    image: &RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
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
    image: &RgbaImage,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) {
    match (sampler.filter, matches!(blend, BlendMode::Add)) {
        (SamplerFilter::Nearest, true) => rasterize_triangle_impl::<false, true, MASK, OPAQUE>(
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
            buffer,
        ),
        (SamplerFilter::Nearest, false) => rasterize_triangle_impl::<false, false, MASK, OPAQUE>(
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
            buffer,
        ),
        (SamplerFilter::Linear, true) => rasterize_triangle_impl::<true, true, MASK, OPAQUE>(
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
            buffer,
        ),
        (SamplerFilter::Linear, false) => rasterize_triangle_impl::<true, false, MASK, OPAQUE>(
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
    image: &RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
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
    );
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn rasterize_triangle_tex_color_prepared(
    vertices: &[ScreenVertexTexColor; 3],
    setup: RasterSetup,
    blend: BlendMode,
    texture_mask: bool,
    image: &RgbaImage,
    sampler: SamplerDesc,
    opaque: bool,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
    width: usize,
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
    image: &RgbaImage,
    sampler: SamplerDesc,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
    width: usize,
) {
    match (sampler.filter, matches!(blend, BlendMode::Add)) {
        (SamplerFilter::Nearest, true) => {
            rasterize_triangle_tex_color_impl::<false, true, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                buffer,
            );
        }
        (SamplerFilter::Nearest, false) => {
            rasterize_triangle_tex_color_impl::<false, false, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                buffer,
            );
        }
        (SamplerFilter::Linear, true) => {
            rasterize_triangle_tex_color_impl::<true, true, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                buffer,
            );
        }
        (SamplerFilter::Linear, false) => {
            rasterize_triangle_tex_color_impl::<true, false, MASK, OPAQUE>(
                v0,
                v1,
                v2,
                setup,
                image,
                sampler,
                width,
                stripe_y_start,
                stripe_y_end,
                buffer,
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
    buffer: &mut [u32],
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
    buffer: &mut [u32],
    width: usize,
) {
    let [v0, v1, v2] = vertices;
    if matches!(blend, BlendMode::Add) {
        rasterize_triangle_color_impl::<true>(
            v0,
            v1,
            v2,
            setup,
            width,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
    } else {
        rasterize_triangle_color_impl::<false>(
            v0,
            v1,
            v2,
            setup,
            width,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
    }
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
    tex_data: &[u8],
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<[f32; 4]> {
    let tx = wrap_index(
        (wrap_uv(u, sampler.wrap) * tex_w as f32).floor() as i32,
        tex_w,
        sampler.wrap,
    );
    let ty = wrap_index(
        (wrap_uv(v, sampler.wrap) * tex_h as f32).floor() as i32,
        tex_h,
        sampler.wrap,
    );
    let idx = (ty * tex_w + tx) * 4;
    if idx + 3 >= tex_data.len() {
        return None;
    }
    Some([
        f32::from(tex_data[idx]) * U8_TO_F32,
        f32::from(tex_data[idx + 1]) * U8_TO_F32,
        f32::from(tex_data[idx + 2]) * U8_TO_F32,
        if OPAQUE {
            1.0
        } else {
            f32::from(tex_data[idx + 3]) * U8_TO_F32
        },
    ])
}

#[inline(always)]
fn sample_alpha_nearest(
    tex_data: &[u8],
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<f32> {
    let tx = wrap_index(
        (wrap_uv(u, sampler.wrap) * tex_w as f32).floor() as i32,
        tex_w,
        sampler.wrap,
    );
    let ty = wrap_index(
        (wrap_uv(v, sampler.wrap) * tex_h as f32).floor() as i32,
        tex_h,
        sampler.wrap,
    );
    tex_data
        .get((ty * tex_w + tx) * 4 + 3)
        .map(|alpha| f32::from(*alpha) * U8_TO_F32)
}

#[inline(always)]
fn sample_tex_linear<const OPAQUE: bool>(
    tex_data: &[u8],
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
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let fx = clamp01(x - x0 as f32);
    let fy = clamp01(y - y0 as f32);

    let ix0 = wrap_index(x0, tex_w, sampler.wrap);
    let ix1 = wrap_index(x1, tex_w, sampler.wrap);
    let iy0 = wrap_index(y0, tex_h, sampler.wrap);
    let iy1 = wrap_index(y1, tex_h, sampler.wrap);

    let idx00 = (iy0 * tex_w + ix0) * 4;
    let idx10 = (iy0 * tex_w + ix1) * 4;
    let idx01 = (iy1 * tex_w + ix0) * 4;
    let idx11 = (iy1 * tex_w + ix1) * 4;
    if idx11 + 3 >= tex_data.len() {
        return None;
    }
    if !OPAQUE
        && tex_data[idx00 + 3] == 0
        && tex_data[idx10 + 3] == 0
        && tex_data[idx01 + 3] == 0
        && tex_data[idx11 + 3] == 0
    {
        return None;
    }

    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    let c00 = [
        f32::from(tex_data[idx00]) * U8_TO_F32,
        f32::from(tex_data[idx00 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx00 + 2]) * U8_TO_F32,
        if OPAQUE {
            1.0
        } else {
            f32::from(tex_data[idx00 + 3]) * U8_TO_F32
        },
    ];
    let c10 = [
        f32::from(tex_data[idx10]) * U8_TO_F32,
        f32::from(tex_data[idx10 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx10 + 2]) * U8_TO_F32,
        if OPAQUE {
            1.0
        } else {
            f32::from(tex_data[idx10 + 3]) * U8_TO_F32
        },
    ];
    let c01 = [
        f32::from(tex_data[idx01]) * U8_TO_F32,
        f32::from(tex_data[idx01 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx01 + 2]) * U8_TO_F32,
        if OPAQUE {
            1.0
        } else {
            f32::from(tex_data[idx01 + 3]) * U8_TO_F32
        },
    ];
    let c11 = [
        f32::from(tex_data[idx11]) * U8_TO_F32,
        f32::from(tex_data[idx11 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx11 + 2]) * U8_TO_F32,
        if OPAQUE {
            1.0
        } else {
            f32::from(tex_data[idx11 + 3]) * U8_TO_F32
        },
    ];

    let r0 = lerp(c00[0], c10[0], fx);
    let g0 = lerp(c00[1], c10[1], fx);
    let b0 = lerp(c00[2], c10[2], fx);
    let a0 = lerp(c00[3], c10[3], fx);
    let r1 = lerp(c01[0], c11[0], fx);
    let g1 = lerp(c01[1], c11[1], fx);
    let b1 = lerp(c01[2], c11[2], fx);
    let a1 = lerp(c01[3], c11[3], fx);
    Some([
        lerp(r0, r1, fy),
        lerp(g0, g1, fy),
        lerp(b0, b1, fy),
        lerp(a0, a1, fy),
    ])
}

#[inline(always)]
fn sample_alpha_linear(
    tex_data: &[u8],
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<f32> {
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
    let idx00 = (iy0 * tex_w + ix0) * 4 + 3;
    let idx10 = (iy0 * tex_w + ix1) * 4 + 3;
    let idx01 = (iy1 * tex_w + ix0) * 4 + 3;
    let idx11 = (iy1 * tex_w + ix1) * 4 + 3;
    if idx11 >= tex_data.len() {
        return None;
    }
    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    let a00 = f32::from(tex_data[idx00]) * U8_TO_F32;
    let a10 = f32::from(tex_data[idx10]) * U8_TO_F32;
    let a01 = f32::from(tex_data[idx01]) * U8_TO_F32;
    let a11 = f32::from(tex_data[idx11]) * U8_TO_F32;
    Some(lerp(lerp(a00, a10, fx), lerp(a01, a11, fx), fy))
}

#[inline(always)]
fn blend_src_over(dst: u32, sr: f32, sg: f32, sb: f32, sa: f32) -> u32 {
    if sa >= 1.0 {
        return pack_rgba([sr, sg, sb, 1.0]);
    }
    blend_src_over_general(dst, sr, sg, sb, sa)
}

#[inline(always)]
fn blend_src_over_general(dst: u32, sr: f32, sg: f32, sb: f32, sa: f32) -> u32 {
    let dr = ((dst >> 16) & 0xFF) as f32 * U8_TO_F32;
    let dg = ((dst >> 8) & 0xFF) as f32 * U8_TO_F32;
    let db = (dst & 0xFF) as f32 * U8_TO_F32;
    let da = ((dst >> 24) & 0xFF) as f32 * U8_TO_F32;
    let inv = 1.0 - sa;
    pack_rgba([
        sr.mul_add(sa, dr * inv),
        sg.mul_add(sa, dg * inv),
        sb.mul_add(sa, db * inv),
        sa + da * inv,
    ])
}

#[inline(always)]
fn blend_add(dst: u32, sr: f32, sg: f32, sb: f32, sa: f32) -> u32 {
    let dr = ((dst >> 16) & 0xFF) as f32 * U8_TO_F32;
    let dg = ((dst >> 8) & 0xFF) as f32 * U8_TO_F32;
    let db = (dst & 0xFF) as f32 * U8_TO_F32;
    let da = ((dst >> 24) & 0xFF) as f32 * U8_TO_F32;
    pack_rgba([
        sr.mul_add(sa, dr).min(1.0),
        sg.mul_add(sa, dg).min(1.0),
        sb.mul_add(sa, db).min(1.0),
        (da + sa).min(1.0),
    ])
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
    let min_x = min_x.floor().max(0.0) as i32;
    let max_x = max_x.ceil().min((width - 1) as f32) as i32;
    let mut min_y = min_y.floor().max(0.0) as i32;
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
fn rasterize_triangle_impl<
    const LINEAR: bool,
    const ADD: bool,
    const MASK: bool,
    const OPAQUE: bool,
>(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    inv_denom: f32,
    tint: [f32; 4],
    image: &RgbaImage,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
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

    let tex_w = image.width().max(1) as usize;
    let tex_h = image.height().max(1) as usize;
    let tex_data = image.as_raw();

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
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
            if sampled[3] <= 0.0 {
                continue;
            }

            let sr = clamp01(if MASK { tint[0] } else { sampled[0] * tint[0] });
            let sg = clamp01(if MASK { tint[1] } else { sampled[1] * tint[1] });
            let sb = clamp01(if MASK { tint[2] } else { sampled[2] * tint[2] });
            let sa = clamp01(sampled[3] * tint[3]);
            if sa <= 0.0 {
                continue;
            }

            let dst_idx = row * width + x as usize;
            buffer[dst_idx] = if ADD {
                blend_add(buffer[dst_idx], sr, sg, sb, sa)
            } else {
                blend_src_over(buffer[dst_idx], sr, sg, sb, sa)
            };
        }
    }
}

#[inline(always)]
fn rasterize_triangle_tex_color_impl<
    const LINEAR: bool,
    const ADD: bool,
    const MASK: bool,
    const OPAQUE: bool,
>(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    setup: RasterSetup,
    image: &RgbaImage,
    sampler: SamplerDesc,
    width: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) =
        setup.stripe_bounds(stripe_y_start, stripe_y_end)
    else {
        return;
    };
    let inv_denom = setup.inv_denom;
    let tex_w = image.width().max(1) as usize;
    let tex_h = image.height().max(1) as usize;
    let tex_data = image.as_raw();

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
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
            if sampled[3] <= 0.0 {
                continue;
            }

            let cr = clamp01(v2.color[0].mul_add(w2, v0.color[0].mul_add(w0, v1.color[0] * w1)));
            let cg = clamp01(v2.color[1].mul_add(w2, v0.color[1].mul_add(w0, v1.color[1] * w1)));
            let cb = clamp01(v2.color[2].mul_add(w2, v0.color[2].mul_add(w0, v1.color[2] * w1)));
            let ca = clamp01(v2.color[3].mul_add(w2, v0.color[3].mul_add(w0, v1.color[3] * w1)));

            let sr = clamp01(if MASK { cr } else { sampled[0] * cr });
            let sg = clamp01(if MASK { cg } else { sampled[1] * cg });
            let sb = clamp01(if MASK { cb } else { sampled[2] * cb });
            let sa = clamp01(sampled[3] * ca);
            if sa <= 0.0 {
                continue;
            }

            let dst_idx = row * width + x as usize;
            buffer[dst_idx] = if ADD {
                blend_add(buffer[dst_idx], sr, sg, sb, sa)
            } else {
                blend_src_over(buffer[dst_idx], sr, sg, sb, sa)
            };
        }
    }
}

#[inline(always)]
fn rasterize_triangle_color_impl<const ADD: bool>(
    v0: &ScreenVertexColor,
    v1: &ScreenVertexColor,
    v2: &ScreenVertexColor,
    setup: RasterSetup,
    width: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) {
    let Some((min_x, max_x, min_y, max_y, stripe_start)) =
        setup.stripe_bounds(stripe_y_start, stripe_y_end)
    else {
        return;
    };
    let inv_denom = setup.inv_denom;

    for y in min_y..=max_y {
        let py = y as f32 + 0.5;
        let row = (y - stripe_start) as usize;
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let sr = clamp01(v2.color[0].mul_add(w2, v0.color[0].mul_add(w0, v1.color[0] * w1)));
            let sg = clamp01(v2.color[1].mul_add(w2, v0.color[1].mul_add(w0, v1.color[1] * w1)));
            let sb = clamp01(v2.color[2].mul_add(w2, v0.color[2].mul_add(w0, v1.color[2] * w1)));
            let sa = clamp01(v2.color[3].mul_add(w2, v0.color[3].mul_add(w0, v1.color[3] * w1)));
            if sa <= 0.0 {
                continue;
            }

            let dst_idx = row * width + x as usize;
            buffer[dst_idx] = if ADD {
                blend_add(buffer[dst_idx], sr, sg, sb, sa)
            } else {
                blend_src_over(buffer[dst_idx], sr, sg, sb, sa)
            };
        }
    }
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

    struct TestTextures {
        texture: Texture,
        lookups: AtomicUsize,
    }

    impl TextureLookup for TestTextures {
        fn software_texture(&self, handle: TextureHandle) -> Option<&Texture> {
            self.lookups.fetch_add(1, Ordering::Relaxed);
            (handle == TEXTURE_HANDLE).then_some(&self.texture)
        }
    }

    #[test]
    fn opaque_src_over_matches_general_blend_exactly() {
        let destinations = [0, 0x1020_3040, 0x7f83_4127, 0xffff_ffff];
        let channels = [0.0, 0.125, 0.5, 0.875, 1.0];
        for dst in destinations {
            for &sr in &channels {
                for &sg in &channels {
                    for &sb in &channels {
                        assert_eq!(
                            blend_src_over(dst, sr, sg, sb, 1.0),
                            blend_src_over_general(dst, sr, sg, sb, 1.0)
                        );
                    }
                }
            }
        }
        for &sa in &[0.0, 0.125, 0.5, 0.875, 0.999] {
            assert_eq!(
                blend_src_over(0x7f83_4127, 0.17, 0.43, 0.91, sa),
                blend_src_over_general(0x7f83_4127, 0.17, 0.43, 0.91, sa)
            );
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
                    sample_tex_nearest::<true>(opaque.as_raw(), 8, 8, u, v, sampler),
                    sample_tex_nearest::<false>(opaque.as_raw(), 8, 8, u, v, sampler),
                );
                assert_eq!(
                    sample_tex_linear::<true>(opaque.as_raw(), 8, 8, u, v, sampler),
                    sample_tex_linear::<false>(opaque.as_raw(), 8, 8, u, v, sampler),
                );

                let nearest = sample_tex_nearest::<false>(mixed.as_raw(), 8, 8, u, v, sampler)
                    .expect("nonempty image samples");
                assert_eq!(
                    sample_alpha_nearest(mixed.as_raw(), 8, 8, u, v, sampler),
                    Some(nearest[3]),
                );
                let alpha = sample_alpha_linear(mixed.as_raw(), 8, 8, u, v, sampler)
                    .expect("nonempty image samples");
                match sample_tex_linear::<false>(mixed.as_raw(), 8, 8, u, v, sampler) {
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
                image,
                sampler,
                opaque,
                WIDTH,
                HEIGHT,
                0,
                HEIGHT,
                &mut pixels,
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
                    sample_tex_linear::<false>(image.as_raw(), 2, 2, u, v, sampler),
                    None,
                );
                assert_eq!(
                    sample_alpha_linear(image.as_raw(), 2, 2, u, v, sampler),
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
            &textures.texture.image,
            textures.texture.sampler,
            textures.texture.opaque,
            0,
            HEIGHT,
            &mut retained,
            WIDTH,
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
                &textures.texture.image,
                textures.texture.sampler,
                textures.texture.opaque,
                WIDTH,
                HEIGHT,
                0,
                HEIGHT,
                &mut direct,
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
        prepare_objects(
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
        );
        prepare_objects(
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
    ) -> u32 {
        let fixed_vertices = prepared.iter().fold(0u32, |sum, object| {
            sum.saturating_add(object.fixed_vertices())
        });
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
                    stripe,
                    fixed_vertices,
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
    ) -> u32 {
        let fixed_vertices = prepared.iter().fold(0u32, |sum, object| {
            sum.saturating_add(object.fixed_vertices())
        });
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
                    stripe,
                    fixed_vertices,
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
            cameras: vec![ortho_for_window(WIDTH as u32, HEIGHT as u32)],
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
                    geometry: 0,
                    instance_start: 0,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: TEXTURE_HANDLE,
                    camera: 0,
                    depth_test: false,
                }),
                DrawOp::TexturedMesh(TexturedMeshRun {
                    geometry: 0,
                    instance_start: 1,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: MISSING_TEXTURE_HANDLE,
                    camera: 0,
                    depth_test: false,
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
            },
            lookups: AtomicUsize::new(0),
        }
    }
}
