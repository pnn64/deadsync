use crate::engine::gfx::{
    BlendMode, DrawStats, MeshMode, ObjectType, RenderList, RenderObject, SamplerDesc,
    SamplerFilter, SamplerWrap, Texture as RendererTexture, TextureHandleMap,
};
use crate::engine::space::ortho_for_window;
use glam::{Mat4 as Matrix4, Vec4 as Vector4};
use image::RgbaImage;
use log::info;
use std::{
    error::Error,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};
use winit::{dpi::PhysicalSize, window::Window};

const SOFTWARE_ROW_CHUNK: usize = 32;
const U8_TO_F32: f32 = 1.0 / 255.0;

pub struct Texture {
    pub image: RgbaImage,
    sampler: SamplerDesc,
}

pub struct State {
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    window_size: PhysicalSize<u32>,
    projection: Matrix4,
    thread_hint: Option<usize>,
}

pub fn init(window: Arc<Window>, _vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing software renderer backend (softbuffer)...");

    let window_size = window.inner_size();
    let projection = ortho_for_window(window_size.width, window_size.height);

    let context = softbuffer::Context::new(window.clone())?;
    let surface = softbuffer::Surface::new(&context, window)?;

    Ok(State {
        _context: context,
        surface,
        window_size,
        projection,
        thread_hint: None,
    })
}

pub const fn set_thread_hint(state: &mut State, threads: Option<usize>) {
    state.thread_hint = threads;
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
    render_list: &RenderList,
    textures: &TextureHandleMap<RendererTexture>,
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

    let resize_w = NonZeroU32::new(width).unwrap();
    let resize_h = NonZeroU32::new(height).unwrap();

    state.surface.resize(resize_w, resize_h)?;

    let mut buffer = state.surface.buffer_mut()?;
    let clear = pack_rgba(render_list.clear_color);
    for pixel in buffer.iter_mut() {
        *pixel = clear;
    }

    let default_proj = state.projection;
    let cameras = render_list.cameras.as_slice();
    let objects = render_list.objects.as_slice();
    let vertex_counter = AtomicU32::new(0);

    let threads_auto = thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .max(1);

    let threads = match state.thread_hint {
        Some(t) if t >= 1 => t.min(threads_auto),
        _ => threads_auto,
    };

    let use_parallel = threads > 1 && h >= SOFTWARE_ROW_CHUNK * 2 && !objects.is_empty();

    if use_parallel {
        let next_row = AtomicUsize::new(0);
        let buffer_addr = buffer.as_mut_ptr() as usize;
        let buffer_len = buffer.len();

        thread::scope(|scope| {
            for _worker in 0..threads {
                let width = w;
                let height = h;
                let counter = &vertex_counter;
                let next_row = &next_row;

                scope.spawn(move || {
                    let mut local_vertices: u32 = 0;

                    loop {
                        let y_start = next_row.fetch_add(SOFTWARE_ROW_CHUNK, Ordering::Relaxed);
                        if y_start >= height {
                            break;
                        }
                        let y_end = (y_start + SOFTWARE_ROW_CHUNK).min(height);
                        let offset = y_start * width;
                        let len = (y_end - y_start) * width;
                        debug_assert!(offset + len <= buffer_len);
                        // SAFETY: each worker claims a unique row range from `next_row`,
                        // so these slices never overlap. `buffer` lives until the scoped
                        // threads join, and `offset + len` stays within bounds.
                        let stripe = unsafe {
                            std::slice::from_raw_parts_mut(
                                (buffer_addr as *mut u32).add(offset),
                                len,
                            )
                        };
                        local_vertices = local_vertices.saturating_add(draw_rows(
                            objects,
                            cameras,
                            default_proj,
                            textures,
                            width,
                            height,
                            y_start,
                            y_end,
                            stripe,
                        ));
                    }

                    counter.fetch_add(local_vertices, Ordering::Relaxed);
                });
            }
        });
    } else {
        vertex_counter.store(
            draw_rows(
                objects,
                cameras,
                default_proj,
                textures,
                w,
                h,
                0,
                h,
                &mut buffer,
            ),
            Ordering::Relaxed,
        );
    }

    let present_started = Instant::now();
    buffer.present()?;

    Ok(DrawStats {
        vertices: vertex_counter.load(Ordering::Relaxed),
        present_us: elapsed_us_since(present_started),
        ..DrawStats::default()
    })
}

fn draw_rows(
    objects: &[RenderObject],
    cameras: &[Matrix4],
    default_proj: Matrix4,
    textures: &TextureHandleMap<RendererTexture>,
    width: usize,
    height: usize,
    stripe_y_start: usize,
    stripe_y_end: usize,
    buffer: &mut [u32],
) -> u32 {
    let mut vertices_drawn = 0u32;

    for obj in objects {
        let proj = cameras
            .get(obj.camera as usize)
            .copied()
            .unwrap_or(default_proj);
        let drawn = match &obj.object_type {
            ObjectType::Sprite {
                tint,
                uv_scale,
                uv_offset,
                local_offset,
                local_offset_rot_sin_cos,
                edge_fade: _,
                ..
            } => {
                let Some(RendererTexture::Software(tex)) = textures.get(&obj.texture_handle) else {
                    continue;
                };
                rasterize_sprite(
                    &proj,
                    &obj.transform,
                    *tint,
                    *uv_scale,
                    *uv_offset,
                    *local_offset,
                    *local_offset_rot_sin_cos,
                    obj.blend,
                    &tex.image,
                    tex.sampler,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                )
            }
            ObjectType::Mesh {
                tint,
                vertices,
                mode,
            } => match mode {
                MeshMode::Triangles => rasterize_mesh_triangles(
                    &proj,
                    &obj.transform,
                    *tint,
                    vertices.as_ref(),
                    obj.blend,
                    width,
                    height,
                    stripe_y_start,
                    stripe_y_end,
                    buffer,
                ),
            },
            ObjectType::TexturedMesh {
                tint,
                vertices,
                mode,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                ..
            } => match mode {
                MeshMode::Triangles => {
                    let Some(RendererTexture::Software(tex)) = textures.get(&obj.texture_handle)
                    else {
                        continue;
                    };
                    rasterize_textured_mesh_triangles(
                        &proj,
                        &obj.transform,
                        vertices.as_ref(),
                        *tint,
                        *uv_scale,
                        *uv_offset,
                        *uv_tex_shift,
                        obj.blend,
                        &tex.image,
                        tex.sampler,
                        width,
                        height,
                        stripe_y_start,
                        stripe_y_end,
                        buffer,
                    )
                }
            },
        };
        vertices_drawn = vertices_drawn.saturating_add(drawn);
    }

    vertices_drawn
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    state.window_size = PhysicalSize::new(width, height);
    if width == 0 || height == 0 {
        return;
    }
    state.projection = ortho_for_window(width, height);
}

pub fn cleanup(_state: &mut State) {
    info!("Software renderer backend cleanup.");
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

#[inline(always)]
fn rasterize_sprite(
    proj: &Matrix4,
    transform: &Matrix4,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    local_offset: [f32; 2],
    local_offset_rot_sin_cos: [f32; 2],
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

    let mut adjusted = *transform;
    if local_offset[0] != 0.0 || local_offset[1] != 0.0 {
        let s = local_offset_rot_sin_cos[0];
        let c = local_offset_rot_sin_cos[1];
        let ox = c.mul_add(local_offset[0], -(s * local_offset[1]));
        let oy = s.mul_add(local_offset[0], c * local_offset[1]);
        adjusted.w_axis.x += ox;
        adjusted.w_axis.y += oy;
    }
    let mvp = *proj * adjusted;

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
        let local = Vector4::new(lx, ly, 0.0, 1.0);
        let clip = mvp * local;
        if clip.w == 0.0 {
            return 0;
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

    rasterize_triangle(
        &v[0],
        &v[1],
        &v[2],
        tint,
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
        &v[0],
        &v[2],
        &v[3],
        tint,
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

fn rasterize_mesh_triangles(
    proj: &Matrix4,
    transform: &Matrix4,
    tint: [f32; 4],
    vertices: &[crate::engine::gfx::MeshVertex],
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

    let mvp = *proj * *transform;
    let mut tri: [ScreenVertexColor; 3] = [ScreenVertexColor {
        x: 0.0,
        y: 0.0,
        color: [0.0; 4],
    }; 3];

    let mut verts_drawn = 0u32;
    'tri: for chunk in vertices.chunks_exact(3) {
        for i in 0..3 {
            let p = chunk[i].pos;
            let clip = mvp * Vector4::new(p[0], p[1], 0.0, 1.0);
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
    proj: &Matrix4,
    transform: &Matrix4,
    vertices: &[crate::engine::gfx::TexturedMeshVertex],
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
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

    let mvp = *proj * *transform;
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
            let clip = mvp * Vector4::new(p[0], p[1], p[2], 1.0);
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

            let sr = clamp01(sampled[0] * tint[0]);
            let sg = clamp01(sampled[1] * tint[1]);
            let sb = clamp01(sampled[2] * tint[2]);
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

            let sr = clamp01(sampled[0] * cr);
            let sg = clamp01(sampled[1] * cg);
            let sb = clamp01(sampled[2] * cb);
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
