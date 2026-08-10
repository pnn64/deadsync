use deadlib_render::{
    BlendMode, DrawOp, DrawStats, RenderFrame, SOFTWARE_MESH_STORAGE_SLOT,
    SOFTWARE_OBJECTS_STORAGE_SLOT, SOFTWARE_TMESH_STORAGE_SLOT, SamplerDesc, SamplerFilter,
    SamplerWrap, TextureHandle, draw_storage_stats,
};
use glam::{Mat4 as Matrix4, Vec4 as Vector4};
use image::RgbaImage;
use log::info;
use rayon::prelude::*;
use std::{error::Error, num::NonZeroU32, sync::Arc, time::Instant};
use winit::{dpi::PhysicalSize, window::Window};

const SOFTWARE_ROW_CHUNK: usize = 32;
// Staging wins once enough row workers would otherwise repeat every transform.
// The paired projection benchmark puts the crossover between 8 and 15 stripes.
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
    prepared_mesh_vertices: Vec<ScreenVertexColor>,
    prepared_tmesh_vertices: Vec<ScreenVertexTexColor>,
}

struct WorkerPool {
    threads: usize,
    pool: rayon::ThreadPool,
}

enum PreparedObject {
    Sprite {
        vertices: [ScreenVertex; 4],
        tint: [f32; 4],
        texture_mask: bool,
        blend: BlendMode,
        texture_handle: TextureHandle,
    },
    Mesh {
        vertex_start: u32,
        projected_count: u32,
        blend: BlendMode,
    },
    DirectMesh {
        vertex_start: u32,
        vertex_count: u32,
        projection: Matrix4,
        blend: BlendMode,
    },
    TexturedMesh {
        vertex_start: u32,
        projected_count: u32,
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
        prepared_mesh_vertices: Vec::with_capacity(MESH_STAGE_VERTEX_CAP),
        prepared_tmesh_vertices: Vec::with_capacity(MESH_STAGE_VERTEX_CAP),
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
    })
}

pub fn update_texture(texture: &mut Texture, image: &RgbaImage) -> Result<(), Box<dyn Error>> {
    texture.image.clone_from(image);
    Ok(())
}

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
    let threads = match state.thread_hint {
        Some(threads) if threads >= 1 => threads.min(state.available_threads),
        _ => state.available_threads,
    };
    let use_parallel = threads > 1 && h >= SOFTWARE_ROW_CHUNK * 2 && !frame.ops.is_empty();
    let stage_meshes = use_parallel && h.div_ceil(SOFTWARE_ROW_CHUNK) >= MIN_STAGE_MESH_STRIPES;
    let backend_prepare_started = Instant::now();
    ensure_worker_pool(state, threads)?;
    prepare_objects(
        frame,
        default_proj,
        textures,
        w,
        h,
        &mut state.prepared_objects,
        &mut state.prepared_mesh_vertices,
        &mut state.prepared_tmesh_vertices,
        stage_meshes,
    );
    let backend_prepare_us = elapsed_us_since(backend_prepare_started);

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
    for pixel in buffer.iter_mut() {
        *pixel = clear;
    }

    let prepared_objects = state.prepared_objects.as_slice();
    let prepared_mesh_vertices = state.prepared_mesh_vertices.as_slice();
    let prepared_tmesh_vertices = state.prepared_tmesh_vertices.as_slice();
    let vertices = if let Some(worker_pool) = worker_pool {
        let pixels: &mut [u32] = &mut buffer;
        worker_pool.install(|| {
            pixels
                .par_chunks_mut(w * SOFTWARE_ROW_CHUNK)
                .enumerate()
                .map(|(chunk_index, stripe)| {
                    let y_start = chunk_index * SOFTWARE_ROW_CHUNK;
                    let y_end = y_start + stripe.len() / w;
                    draw_rows(
                        frame,
                        prepared_objects,
                        prepared_mesh_vertices,
                        prepared_tmesh_vertices,
                        textures,
                        w,
                        h,
                        y_start,
                        y_end,
                        stripe,
                    )
                })
                .reduce(|| 0, u32::saturating_add)
        })
    } else {
        draw_rows(
            frame,
            prepared_objects,
            prepared_mesh_vertices,
            prepared_tmesh_vertices,
            textures,
            w,
            h,
            0,
            h,
            &mut buffer,
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
        .prepared_mesh_vertices
        .capacity()
        .min(u32::MAX as usize) as u32;
    storage.capacities[SOFTWARE_TMESH_STORAGE_SLOT] = state
        .prepared_tmesh_vertices
        .capacity()
        .min(u32::MAX as usize) as u32;
    Ok(DrawStats {
        vertices,
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
    frame: &RenderFrame,
    default_proj: Matrix4,
    textures: &(impl TextureLookup + Sync),
    width: usize,
    height: usize,
    prepared: &mut Vec<PreparedObject>,
    mesh_vertices: &mut Vec<ScreenVertexColor>,
    tmesh_vertices: &mut Vec<ScreenVertexTexColor>,
    stage_meshes: bool,
) {
    prepared.clear();
    prepared.reserve(
        frame
            .sprite_instances
            .len()
            .saturating_add(frame.tmesh_instances.len()),
    );
    mesh_vertices.clear();
    tmesh_vertices.clear();

    for op in &frame.ops {
        match *op {
            DrawOp::Sprite(run) => {
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
                    || vertices.len() > mesh_vertices.capacity().saturating_sub(mesh_vertices.len())
                {
                    prepared.push(PreparedObject::DirectMesh {
                        vertex_start: run.vertex_start,
                        vertex_count: run.vertex_count,
                        projection,
                        blend: run.blend,
                    });
                    continue;
                }
                let Some((vertex_start, projected_count)) = prepare_mesh_vertices(
                    mesh_vertices,
                    &projection,
                    [1.0; 4],
                    vertices,
                    width,
                    height,
                ) else {
                    continue;
                };
                prepared.push(PreparedObject::Mesh {
                    vertex_start,
                    projected_count,
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
                    if !stage_meshes
                        || geometry.vertices.len()
                            > tmesh_vertices
                                .capacity()
                                .saturating_sub(tmesh_vertices.len())
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
                    let Some((vertex_start, projected_count)) = prepare_tmesh_vertices(
                        tmesh_vertices,
                        &mvp,
                        instance.tint,
                        instance.uv_scale,
                        instance.uv_offset,
                        instance.uv_tex_shift,
                        geometry.vertices.as_ref(),
                        width,
                        height,
                    ) else {
                        continue;
                    };
                    prepared.push(PreparedObject::TexturedMesh {
                        vertex_start,
                        projected_count,
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
    frame: &RenderFrame,
    prepared_objects: &[PreparedObject],
    mesh_vertices: &[ScreenVertexColor],
    tmesh_vertices: &[ScreenVertexTexColor],
    textures: &(impl TextureLookup + Sync),
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) -> u32 {
    let mut vertices_drawn = 0u32;

    for prepared in prepared_objects {
        let drawn = match prepared {
            PreparedObject::Sprite {
                vertices,
                tint,
                texture_mask,
                blend,
                texture_handle,
            } => {
                let Some(tex) = textures.software_texture(*texture_handle) else {
                    continue;
                };
                rasterize_prepared_sprite(
                    vertices,
                    *tint,
                    *texture_mask,
                    *blend,
                    &tex.image,
                    tex.sampler,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                );
                4
            }
            PreparedObject::Mesh {
                vertex_start,
                projected_count,
                blend,
            } => {
                let start = *vertex_start as usize;
                let end = start + *projected_count as usize;
                rasterize_prepared_mesh(
                    &mesh_vertices[start..end],
                    *blend,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                );
                *projected_count
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
                    continue;
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
                vertex_start,
                projected_count,
                texture_mask,
                blend,
                texture_handle,
            } => {
                let Some(tex) = textures.software_texture(*texture_handle) else {
                    continue;
                };
                let start = *vertex_start as usize;
                let end = start + *projected_count as usize;
                rasterize_prepared_tmesh(
                    &tmesh_vertices[start..end],
                    *texture_mask,
                    *blend,
                    &tex.image,
                    tex.sampler,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                );
                *projected_count
            }
            PreparedObject::DirectTexturedMesh {
                geometry,
                instance,
                mvp,
                blend,
                texture_handle,
            } => {
                let Some(geometry) = frame.tmesh_geometries.get(*geometry as usize) else {
                    continue;
                };
                let Some(instance) = frame.tmesh_instances.get(*instance as usize) else {
                    continue;
                };
                let Some(tex) = textures.software_texture(*texture_handle) else {
                    continue;
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
                    tex.sampler,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                )
            }
        };
        vertices_drawn = vertices_drawn.saturating_add(drawn);
    }

    vertices_drawn
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
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[inline(always)]
fn pack_rgba(c: [f32; 4]) -> u32 {
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
            rot_sin_cos[0].mul_add(local_x, rot_sin_cos[1] * local_y + adjusted_center[1]),
            adjusted_center[2],
            1.0,
        );
        let clip = *proj * world;
        if clip.w == 0.0 {
            return None;
        }
        let ndc_x = clip.x / clip.w;
        let ndc_y = clip.y / clip.w;

        let sx = ((ndc_x + 1.0) * 0.5) * (width as f32);
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

fn prepare_mesh_vertices(
    out: &mut Vec<ScreenVertexColor>,
    mvp: &Matrix4,
    tint: [f32; 4],
    vertices: &[deadlib_render::MeshVertex],
    width: usize,
    height: usize,
) -> Option<(u32, u32)> {
    if vertices.is_empty() || width == 0 || height == 0 {
        return None;
    }
    let start = out.len();
    out.reserve(vertices.len());
    'tri: for chunk in vertices.chunks_exact(3) {
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
                x: ((ndc_x + 1.0) * 0.5) * width as f32,
                y: ((1.0 - ndc_y) * 0.5) * height as f32,
                color: [
                    chunk[i].color[0] * tint[0],
                    chunk[i].color[1] * tint[1],
                    chunk[i].color[2] * tint[2],
                    chunk[i].color[3] * tint[3],
                ],
            };
        }
        out.extend_from_slice(&tri);
    }
    let count = out.len() - start;
    Some((start as u32, count as u32))
}

#[allow(clippy::too_many_arguments)]
fn prepare_tmesh_vertices(
    out: &mut Vec<ScreenVertexTexColor>,
    mvp: &Matrix4,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    vertices: &[deadlib_render::TexturedMeshVertex],
    width: usize,
    height: usize,
) -> Option<(u32, u32)> {
    if vertices.is_empty() || width == 0 || height == 0 {
        return None;
    }
    let start = out.len();
    out.reserve(vertices.len());
    'tri: for chunk in vertices.chunks_exact(3) {
        let mut tri = [ScreenVertexTexColor {
            x: 0.0,
            y: 0.0,
            u: 0.0,
            v: 0.0,
            color: [0.0; 4],
        }; 3];
        for i in 0..3 {
            let p = chunk[i].pos;
            let clip = *mvp * Vector4::new(p[0], p[1], p[2], 1.0);
            if clip.w == 0.0 {
                continue 'tri;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            if !ndc_x.is_finite() || !ndc_y.is_finite() {
                continue 'tri;
            }
            tri[i] = ScreenVertexTexColor {
                x: ((ndc_x + 1.0) * 0.5) * width as f32,
                y: ((1.0 - ndc_y) * 0.5) * height as f32,
                u: chunk[i].uv[0].mul_add(uv_scale[0], uv_offset[0])
                    + uv_tex_shift[0] * (chunk[i].tex_matrix_scale[0] - 1.0),
                v: chunk[i].uv[1].mul_add(uv_scale[1], uv_offset[1])
                    + uv_tex_shift[1] * (chunk[i].tex_matrix_scale[1] - 1.0),
                color: [
                    chunk[i].color[0] * tint[0],
                    chunk[i].color[1] * tint[1],
                    chunk[i].color[2] * tint[2],
                    chunk[i].color[3] * tint[3],
                ],
            };
        }
        out.extend_from_slice(&tri);
    }
    let count = out.len() - start;
    Some((start as u32, count as u32))
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn rasterize_prepared_sprite(
    vertices: &[ScreenVertex; 4],
    tint: [f32; 4],
    texture_mask: bool,
    blend: BlendMode,
    image: &RgbaImage,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) -> u32 {
    if tint[3] <= 0.0 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
    }

    rasterize_triangle(
        &vertices[0],
        &vertices[1],
        &vertices[2],
        tint,
        texture_mask,
        blend,
        image,
        sampler,
        width,
        height,
        stripe_y_start,
        stripe_y_end,
        buffer,
    );
    rasterize_triangle(
        &vertices[0],
        &vertices[2],
        &vertices[3],
        tint,
        texture_mask,
        blend,
        image,
        sampler,
        width,
        height,
        stripe_y_start,
        stripe_y_end,
        buffer,
    );

    4
}

fn rasterize_prepared_mesh(
    vertices: &[ScreenVertexColor],
    blend: BlendMode,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) {
    for tri in vertices.chunks_exact(3) {
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
    }
}

#[allow(clippy::too_many_arguments)]
fn rasterize_prepared_tmesh(
    vertices: &[ScreenVertexTexColor],
    texture_mask: bool,
    blend: BlendMode,
    image: &RgbaImage,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) {
    let sampler = SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..sampler
    };
    for tri in vertices.chunks_exact(3) {
        rasterize_triangle_tex_color(
            &tri[0],
            &tri[1],
            &tri[2],
            blend,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
    }
}

fn rasterize_mesh_triangles(
    mvp: &Matrix4,
    tint: [f32; 4],
    vertices: &[deadlib_render::MeshVertex],
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
    'tri: for chunk in vertices.chunks_exact(3) {
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

            let sx = ((ndc_x + 1.0) * 0.5) * (width as f32);
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
    vertices: &[deadlib_render::TexturedMeshVertex],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    texture_mask: bool,
    blend: BlendMode,
    image: &RgbaImage,
    sampler: SamplerDesc,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) -> u32 {
    if vertices.len() < 3 || width == 0 || height == 0 || stripe_y_start >= stripe_y_end {
        return 0;
    }

    let mut tri: [ScreenVertexTexColor; 3] = [ScreenVertexTexColor {
        x: 0.0,
        y: 0.0,
        u: 0.0,
        v: 0.0,
        color: [0.0; 4],
    }; 3];
    let sampler = SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..sampler
    };

    let mut verts_drawn = 0u32;
    'tri: for chunk in vertices.chunks_exact(3) {
        for i in 0..3 {
            let p = chunk[i].pos;
            let clip = *mvp * Vector4::new(p[0], p[1], p[2], 1.0);
            if clip.w == 0.0 {
                continue 'tri;
            }
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;
            if !ndc_x.is_finite() || !ndc_y.is_finite() {
                continue 'tri;
            }

            tri[i] = ScreenVertexTexColor {
                x: ((ndc_x + 1.0) * 0.5) * width as f32,
                y: ((1.0 - ndc_y) * 0.5) * height as f32,
                u: chunk[i].uv[0].mul_add(uv_scale[0], uv_offset[0])
                    + uv_tex_shift[0] * (chunk[i].tex_matrix_scale[0] - 1.0),
                v: chunk[i].uv[1].mul_add(uv_scale[1], uv_offset[1])
                    + uv_tex_shift[1] * (chunk[i].tex_matrix_scale[1] - 1.0),
                color: [
                    chunk[i].color[0] * tint[0],
                    chunk[i].color[1] * tint[1],
                    chunk[i].color[2] * tint[2],
                    chunk[i].color[3] * tint[3],
                ],
            };
        }

        rasterize_triangle_tex_color(
            &tri[0],
            &tri[1],
            &tri[2],
            blend,
            texture_mask,
            image,
            sampler,
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

#[inline(always)]
fn rasterize_triangle(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    tint: [f32; 4],
    texture_mask: bool,
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
        (SamplerFilter::Nearest, true) => rasterize_triangle_impl::<false, true>(
            v0,
            v1,
            v2,
            tint,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (SamplerFilter::Nearest, false) => rasterize_triangle_impl::<false, false>(
            v0,
            v1,
            v2,
            tint,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (SamplerFilter::Linear, true) => rasterize_triangle_impl::<true, true>(
            v0,
            v1,
            v2,
            tint,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (SamplerFilter::Linear, false) => rasterize_triangle_impl::<true, false>(
            v0,
            v1,
            v2,
            tint,
            texture_mask,
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
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) {
    match (sampler.filter, matches!(blend, BlendMode::Add)) {
        (SamplerFilter::Nearest, true) => rasterize_triangle_tex_color_impl::<false, true>(
            v0,
            v1,
            v2,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (SamplerFilter::Nearest, false) => rasterize_triangle_tex_color_impl::<false, false>(
            v0,
            v1,
            v2,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (SamplerFilter::Linear, true) => rasterize_triangle_tex_color_impl::<true, true>(
            v0,
            v1,
            v2,
            texture_mask,
            image,
            sampler,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        ),
        (SamplerFilter::Linear, false) => rasterize_triangle_tex_color_impl::<true, false>(
            v0,
            v1,
            v2,
            texture_mask,
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
    if matches!(blend, BlendMode::Add) {
        rasterize_triangle_color_impl::<true>(
            v0,
            v1,
            v2,
            width,
            height,
            stripe_y_start,
            stripe_y_end,
            buffer,
        );
    } else {
        rasterize_triangle_color_impl::<false>(
            v0,
            v1,
            v2,
            width,
            height,
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
fn sample_tex_nearest(
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
        f32::from(tex_data[idx + 3]) * U8_TO_F32,
    ])
}

#[inline(always)]
fn sample_tex_linear(
    tex_data: &[u8],
    tex_w: usize,
    tex_h: usize,
    u: f32,
    v: f32,
    sampler: SamplerDesc,
) -> Option<[f32; 4]> {
    let x = wrap_uv(u, sampler.wrap) * tex_w as f32 - 0.5;
    let y = wrap_uv(v, sampler.wrap) * tex_h as f32 - 0.5;
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

    let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
    let c00 = [
        f32::from(tex_data[idx00]) * U8_TO_F32,
        f32::from(tex_data[idx00 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx00 + 2]) * U8_TO_F32,
        f32::from(tex_data[idx00 + 3]) * U8_TO_F32,
    ];
    let c10 = [
        f32::from(tex_data[idx10]) * U8_TO_F32,
        f32::from(tex_data[idx10 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx10 + 2]) * U8_TO_F32,
        f32::from(tex_data[idx10 + 3]) * U8_TO_F32,
    ];
    let c01 = [
        f32::from(tex_data[idx01]) * U8_TO_F32,
        f32::from(tex_data[idx01 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx01 + 2]) * U8_TO_F32,
        f32::from(tex_data[idx01 + 3]) * U8_TO_F32,
    ];
    let c11 = [
        f32::from(tex_data[idx11]) * U8_TO_F32,
        f32::from(tex_data[idx11 + 1]) * U8_TO_F32,
        f32::from(tex_data[idx11 + 2]) * U8_TO_F32,
        f32::from(tex_data[idx11 + 3]) * U8_TO_F32,
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
fn blend_src_over(dst: u32, sr: f32, sg: f32, sb: f32, sa: f32) -> u32 {
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
fn rasterize_triangle_impl<const LINEAR: bool, const ADD: bool>(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    tint: [f32; 4],
    texture_mask: bool,
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

    let denom = edge_function(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if denom == 0.0 {
        return;
    }
    let inv_denom = 1.0 / denom;
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

            let u = v0.u.mul_add(w0, v1.u * w1) + v2.u * w2;
            let v = v0.v.mul_add(w0, v1.v * w1) + v2.v * w2;
            let sampled = if LINEAR {
                sample_tex_linear(tex_data, tex_w, tex_h, u, v, sampler)
            } else {
                sample_tex_nearest(tex_data, tex_w, tex_h, u, v, sampler)
            };
            let Some(sampled) = sampled else {
                continue;
            };
            if sampled[3] <= 0.0 {
                continue;
            }

            let sr = clamp01(if texture_mask {
                tint[0]
            } else {
                sampled[0] * tint[0]
            });
            let sg = clamp01(if texture_mask {
                tint[1]
            } else {
                sampled[1] * tint[1]
            });
            let sb = clamp01(if texture_mask {
                tint[2]
            } else {
                sampled[2] * tint[2]
            });
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
fn rasterize_triangle_tex_color_impl<const LINEAR: bool, const ADD: bool>(
    v0: &ScreenVertexTexColor,
    v1: &ScreenVertexTexColor,
    v2: &ScreenVertexTexColor,
    texture_mask: bool,
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

    let denom = edge_function(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if denom == 0.0 {
        return;
    }
    let inv_denom = 1.0 / denom;
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

            let u = v0.u.mul_add(w0, v1.u * w1) + v2.u * w2;
            let v = v0.v.mul_add(w0, v1.v * w1) + v2.v * w2;
            let sampled = if LINEAR {
                sample_tex_linear(tex_data, tex_w, tex_h, u, v, sampler)
            } else {
                sample_tex_nearest(tex_data, tex_w, tex_h, u, v, sampler)
            };
            let Some(sampled) = sampled else {
                continue;
            };
            if sampled[3] <= 0.0 {
                continue;
            }

            let cr = clamp01(v0.color[0].mul_add(w0, v1.color[0] * w1) + v2.color[0] * w2);
            let cg = clamp01(v0.color[1].mul_add(w0, v1.color[1] * w1) + v2.color[1] * w2);
            let cb = clamp01(v0.color[2].mul_add(w0, v1.color[2] * w1) + v2.color[2] * w2);
            let ca = clamp01(v0.color[3].mul_add(w0, v1.color[3] * w1) + v2.color[3] * w2);

            let sr = clamp01(if texture_mask { cr } else { sampled[0] * cr });
            let sg = clamp01(if texture_mask { cg } else { sampled[1] * cg });
            let sb = clamp01(if texture_mask { cb } else { sampled[2] * cb });
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

    let denom = edge_function(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if denom == 0.0 {
        return;
    }
    let inv_denom = 1.0 / denom;

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

            let sr = clamp01(v0.color[0].mul_add(w0, v1.color[0] * w1) + v2.color[0] * w2);
            let sg = clamp01(v0.color[1].mul_add(w0, v1.color[1] * w1) + v2.color[1] * w2);
            let sb = clamp01(v0.color[2].mul_add(w0, v1.color[2] * w1) + v2.color[2] * w2);
            let sa = clamp01(v0.color[3].mul_add(w0, v1.color[3] * w1) + v2.color[3] * w2);
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_render::{
        INVALID_TMESH_CACHE_KEY, MeshRun, MeshVertex, SpriteInstanceRaw, SpriteRun,
        TexturedMeshGeometry, TexturedMeshInstanceRaw, TexturedMeshRun, TexturedMeshVertex,
        TexturedMeshVertices,
    };
    use glam::Vec3;
    use image::Rgba;

    const WIDTH: usize = 96;
    const HEIGHT: usize = 80;
    const TEXTURE_HANDLE: TextureHandle = 7;
    const MISSING_TEXTURE_HANDLE: TextureHandle = 99;

    struct TestTextures {
        texture: Texture,
    }

    impl TextureLookup for TestTextures {
        fn software_texture(&self, handle: TextureHandle) -> Option<&Texture> {
            (handle == TEXTURE_HANDLE).then_some(&self.texture)
        }
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
            &frame,
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
        assert_eq!(prepared_tmesh.len(), 3);
        let staged_vertices = render_prepared_stripes(
            &frame,
            &prepared,
            &prepared_mesh,
            &prepared_tmesh,
            &textures,
            &mut staged_pixels,
        );
        prepare_objects(
            &frame,
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
    fn mesh_staging_saturates_without_growing_buffers() {
        let textures = test_textures();
        let frame = mixed_frame();
        let mut prepared = Vec::new();
        let mut prepared_mesh = Vec::with_capacity(2);
        let mut prepared_tmesh = Vec::with_capacity(2);
        let mesh_capacity = prepared_mesh.capacity();
        let tmesh_capacity = prepared_tmesh.capacity();

        prepare_objects(
            &frame,
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
        mesh_vertices: &[ScreenVertexColor],
        tmesh_vertices: &[ScreenVertexTexColor],
        textures: &TestTextures,
        pixels: &mut [u32],
    ) -> u32 {
        pixels
            .chunks_mut(WIDTH * SOFTWARE_ROW_CHUNK)
            .enumerate()
            .map(|(chunk_index, stripe)| {
                let y_start = chunk_index * SOFTWARE_ROW_CHUNK;
                let y_end = y_start + stripe.len() / WIDTH;
                draw_rows(
                    frame,
                    prepared,
                    mesh_vertices,
                    tmesh_vertices,
                    textures,
                    WIDTH,
                    HEIGHT,
                    y_start,
                    y_end,
                    stripe,
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
            },
        }
    }
}
