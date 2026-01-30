use crate::core::gfx::{
    BlendMode, MeshMode, ObjectType, RenderList, SamplerDesc, SamplerFilter, SamplerWrap,
    Texture as RendererTexture,
};
use crate::core::space::ortho_for_window;
use cgmath::{Matrix4, Vector4};
use image::RgbaImage;
use log::info;
use std::{
    collections::HashMap,
    error::Error,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
};
use winit::{dpi::PhysicalSize, window::Window};

pub struct Texture {
    pub image: RgbaImage,
    sampler: SamplerDesc,
}

pub struct State {
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    window_size: PhysicalSize<u32>,
    projection: Matrix4<f32>,
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

pub fn create_texture(image: &RgbaImage, sampler: SamplerDesc) -> Result<Texture, Box<dyn Error>> {
    Ok(Texture {
        image: image.clone(),
        sampler,
    })
}

pub fn draw(
    state: &mut State,
    render_list: &RenderList<'_>,
    textures: &HashMap<String, RendererTexture>,
) -> Result<u32, Box<dyn Error>> {
    let PhysicalSize { width, height } = state.window_size;
    if width == 0 || height == 0 {
        return Ok(0);
    }

    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return Ok(0);
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
    let vertex_counter = AtomicU32::new(0);

    let threads_auto = thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .max(1);

    let threads = match state.thread_hint {
        Some(t) if t >= 1 => t.min(threads_auto),
        _ => threads_auto,
    };

    let use_parallel = threads > 1 && h >= 64 && render_list.objects.len() > 1;

    if use_parallel {
        let rows_per = h.div_ceil(threads);

        thread::scope(|scope| {
            let mut remainder: &mut [u32] = &mut buffer;

            for worker in 0..threads {
                let y_start = worker * rows_per;
                if y_start >= h {
                    break;
                }
                let y_end = ((worker + 1) * rows_per).min(h);
                let rows = y_end - y_start;
                let len = rows * w;

                let (stripe, rest) = remainder.split_at_mut(len);
                remainder = rest;

                let objects = &render_list.objects;
                let textures = textures;
                let default_proj = default_proj;
                let cameras = cameras;
                let width = w;
                let height = h;
                let counter = &vertex_counter;

                scope.spawn(move || {
                    let mut local_vertices: u32 = 0;

                    for obj in objects {
                        let proj = cameras
                            .get(obj.camera as usize)
                            .copied()
                            .unwrap_or(default_proj);
                        match &obj.object_type {
                            ObjectType::Sprite {
                                texture_id,
                                tint,
                                uv_scale,
                                uv_offset,
                                edge_fade: _,
                            } => {
                                let tex_key = texture_id.as_ref();
                                let Some(RendererTexture::Software(tex)) = textures.get(tex_key)
                                else {
                                    continue;
                                };
                                local_vertices += rasterize_sprite(
                                    &proj,
                                    &obj.transform,
                                    *tint,
                                    *uv_scale,
                                    *uv_offset,
                                    obj.blend,
                                    &tex.image,
                                    tex.sampler,
                                    width,
                                    height,
                                    y_start,
                                    y_end,
                                    stripe,
                                );
                            }
                            ObjectType::Mesh { vertices, mode } => match mode {
                                MeshMode::Triangles => {
                                    local_vertices += rasterize_mesh_triangles(
                                        &proj,
                                        &obj.transform,
                                        vertices.as_ref(),
                                        obj.blend,
                                        width,
                                        height,
                                        y_start,
                                        y_end,
                                        stripe,
                                    );
                                }
                            },
                        }
                    }

                    counter.fetch_add(local_vertices, Ordering::Relaxed);
                });
            }
        });
    } else {
        for obj in &render_list.objects {
            let proj = cameras
                .get(obj.camera as usize)
                .copied()
                .unwrap_or(default_proj);
            let v = match &obj.object_type {
                ObjectType::Sprite {
                    texture_id,
                    tint,
                    uv_scale,
                    uv_offset,
                    edge_fade: _,
                } => {
                    let tex_key = texture_id.as_ref();
                    let Some(RendererTexture::Software(tex)) = textures.get(tex_key) else {
                        continue;
                    };
                    rasterize_sprite(
                        &proj,
                        &obj.transform,
                        *tint,
                        *uv_scale,
                        *uv_offset,
                        obj.blend,
                        &tex.image,
                        tex.sampler,
                        w,
                        h,
                        0,
                        h,
                        &mut buffer,
                    )
                }
                ObjectType::Mesh { vertices, mode } => match mode {
                    MeshMode::Triangles => rasterize_mesh_triangles(
                        &proj,
                        &obj.transform,
                        vertices.as_ref(),
                        obj.blend,
                        w,
                        h,
                        0,
                        h,
                        &mut buffer,
                    ),
                },
            };
            vertex_counter.fetch_add(v, Ordering::Relaxed);
        }
    }

    buffer.present()?;

    Ok(vertex_counter.load(Ordering::Relaxed))
}

pub fn resize(state: &mut State, width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }
    state.window_size = PhysicalSize::new(width, height);
    state.projection = ortho_for_window(width, height);
}

pub fn cleanup(_state: &mut State) {
    info!("Software renderer backend cleanup.");
}

#[inline(always)]
fn pack_rgba(c: [f32; 4]) -> u32 {
    fn clamp01(x: f32) -> f32 {
        if x <= 0.0 {
            0.0
        } else if x >= 1.0 {
            1.0
        } else {
            x
        }
    }

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

#[inline(always)]
fn rasterize_sprite(
    proj: &Matrix4<f32>,
    transform: &Matrix4<f32>,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
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

    let mvp = *proj * *transform;

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
    proj: &Matrix4<f32>,
    transform: &Matrix4<f32>,
    vertices: &[crate::core::gfx::MeshVertex],
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
    let mut tri: [ScreenVertexColor; 3] = [
        ScreenVertexColor {
            x: 0.0,
            y: 0.0,
            color: [0.0; 4],
        };
        3
    ];

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
    let min_x = v0.x.min(v1.x).min(v2.x).floor().max(0.0) as i32;
    let max_x = v0.x.max(v1.x).max(v2.x).ceil().min((width - 1) as f32) as i32;
    let mut min_y = v0.y.min(v1.y).min(v2.y).floor().max(0.0) as i32;
    let mut max_y = v0.y.max(v1.y).max(v2.y).ceil().min((height - 1) as f32) as i32;

    if min_x > max_x || min_y > max_y {
        return;
    }

    let stripe_start = stripe_y_start as i32;
    let stripe_end = (stripe_y_end as i32) - 1;
    if stripe_start > stripe_end {
        return;
    }
    if max_y < stripe_start || min_y > stripe_end {
        return;
    }
    if min_y < stripe_start {
        min_y = stripe_start;
    }
    if max_y > stripe_end {
        max_y = stripe_end;
    }

    let denom = edge_function(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);
    if denom == 0.0 {
        return;
    }

    let inv_denom = 1.0 / denom;

    let tex_w = image.width().max(1) as usize;
    let tex_h = image.height().max(1) as usize;
    let tex_data = image.as_raw();

    #[inline(always)]
    fn wrap_uv(u: f32, wrap: SamplerWrap) -> f32 {
        match wrap {
            SamplerWrap::Clamp => u.clamp(0.0, 1.0),
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

            let u_norm = wrap_uv(u, sampler.wrap);
            let v_norm = wrap_uv(v, sampler.wrap);

            let (sr, sg, sb, sa) = if sampler.filter == SamplerFilter::Nearest {
                let tx = (u_norm * tex_w as f32).floor() as i32;
                let ty = (v_norm * tex_h as f32).floor() as i32;
                let tx = wrap_index(tx, tex_w, sampler.wrap);
                let ty = wrap_index(ty, tex_h, sampler.wrap);
                let idx = (ty * tex_w + tx) * 4;
                if idx + 3 >= tex_data.len() {
                    continue;
                }
                (
                    f32::from(tex_data[idx]) / 255.0,
                    f32::from(tex_data[idx + 1]) / 255.0,
                    f32::from(tex_data[idx + 2]) / 255.0,
                    f32::from(tex_data[idx + 3]) / 255.0,
                )
            } else {
                let x = u_norm * tex_w as f32 - 0.5;
                let y = v_norm * tex_h as f32 - 0.5;
                let x0 = x.floor() as i32;
                let y0 = y.floor() as i32;
                let x1 = x0 + 1;
                let y1 = y0 + 1;
                let fx = (x - x0 as f32).clamp(0.0, 1.0);
                let fy = (y - y0 as f32).clamp(0.0, 1.0);

                let ix0 = wrap_index(x0, tex_w, sampler.wrap);
                let ix1 = wrap_index(x1, tex_w, sampler.wrap);
                let iy0 = wrap_index(y0, tex_h, sampler.wrap);
                let iy1 = wrap_index(y1, tex_h, sampler.wrap);

                let idx00 = (iy0 * tex_w + ix0) * 4;
                let idx10 = (iy0 * tex_w + ix1) * 4;
                let idx01 = (iy1 * tex_w + ix0) * 4;
                let idx11 = (iy1 * tex_w + ix1) * 4;
                if idx11 + 3 >= tex_data.len() {
                    continue;
                }

                let c00 = [
                    f32::from(tex_data[idx00]) / 255.0,
                    f32::from(tex_data[idx00 + 1]) / 255.0,
                    f32::from(tex_data[idx00 + 2]) / 255.0,
                    f32::from(tex_data[idx00 + 3]) / 255.0,
                ];
                let c10 = [
                    f32::from(tex_data[idx10]) / 255.0,
                    f32::from(tex_data[idx10 + 1]) / 255.0,
                    f32::from(tex_data[idx10 + 2]) / 255.0,
                    f32::from(tex_data[idx10 + 3]) / 255.0,
                ];
                let c01 = [
                    f32::from(tex_data[idx01]) / 255.0,
                    f32::from(tex_data[idx01 + 1]) / 255.0,
                    f32::from(tex_data[idx01 + 2]) / 255.0,
                    f32::from(tex_data[idx01 + 3]) / 255.0,
                ];
                let c11 = [
                    f32::from(tex_data[idx11]) / 255.0,
                    f32::from(tex_data[idx11 + 1]) / 255.0,
                    f32::from(tex_data[idx11 + 2]) / 255.0,
                    f32::from(tex_data[idx11 + 3]) / 255.0,
                ];

                let lerp = |a: f32, b: f32, t: f32| (b - a).mul_add(t, a);
                let r0 = lerp(c00[0], c10[0], fx);
                let g0 = lerp(c00[1], c10[1], fx);
                let b0 = lerp(c00[2], c10[2], fx);
                let a0 = lerp(c00[3], c10[3], fx);
                let r1 = lerp(c01[0], c11[0], fx);
                let g1 = lerp(c01[1], c11[1], fx);
                let b1 = lerp(c01[2], c11[2], fx);
                let a1 = lerp(c01[3], c11[3], fx);
                (
                    lerp(r0, r1, fy),
                    lerp(g0, g1, fy),
                    lerp(b0, b1, fy),
                    lerp(a0, a1, fy),
                )
            };

            if sa <= 0.0 {
                continue;
            }

            let mut sr = sr * tint[0];
            let mut sg = sg * tint[1];
            let mut sb = sb * tint[2];
            let mut sa = sa * tint[3];

            sr = sr.clamp(0.0, 1.0);
            sg = sg.clamp(0.0, 1.0);
            sb = sb.clamp(0.0, 1.0);
            sa = sa.clamp(0.0, 1.0);

            let dst_idx = row * width + x as usize;
            let dst = buffer[dst_idx];

            let dr = ((dst >> 16) & 0xFF) as f32 / 255.0;
            let dg = ((dst >> 8) & 0xFF) as f32 / 255.0;
            let db = (dst & 0xFF) as f32 / 255.0;
            let da = ((dst >> 24) & 0xFF) as f32 / 255.0;

            let (out_r, out_g, out_b, out_a) = match blend {
                BlendMode::Add => {
                    let r = sr.mul_add(sa, dr).min(1.0);
                    let g = sg.mul_add(sa, dg).min(1.0);
                    let b = sb.mul_add(sa, db).min(1.0);
                    let a = (da + sa).min(1.0);
                    (r, g, b, a)
                }
                _ => {
                    let inv = 1.0 - sa;
                    let r = sr.mul_add(sa, dr * inv);
                    let g = sg.mul_add(sa, dg * inv);
                    let b = sb.mul_add(sa, db * inv);
                    let a = sa + da * inv;
                    (r, g, b, a)
                }
            };

            buffer[dst_idx] = pack_rgba([out_r, out_g, out_b, out_a]);
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
    let min_x = v0.x.min(v1.x).min(v2.x).floor().max(0.0) as i32;
    let max_x = v0.x.max(v1.x).max(v2.x).ceil().min((width - 1) as f32) as i32;
    let mut min_y = v0.y.min(v1.y).min(v2.y).floor().max(0.0) as i32;
    let mut max_y = v0.y.max(v1.y).max(v2.y).ceil().min((height - 1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }

    let stripe_start = stripe_y_start as i32;
    let stripe_end = (stripe_y_end as i32) - 1;
    if stripe_start > stripe_end || max_y < stripe_start || min_y > stripe_end {
        return;
    }
    min_y = min_y.max(stripe_start);
    max_y = max_y.min(stripe_end);

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

            let mut sr = v0.color[0].mul_add(w0, v1.color[0] * w1) + v2.color[0] * w2;
            let mut sg = v0.color[1].mul_add(w0, v1.color[1] * w1) + v2.color[1] * w2;
            let mut sb = v0.color[2].mul_add(w0, v1.color[2] * w1) + v2.color[2] * w2;
            let mut sa = v0.color[3].mul_add(w0, v1.color[3] * w1) + v2.color[3] * w2;

            sr = sr.clamp(0.0, 1.0);
            sg = sg.clamp(0.0, 1.0);
            sb = sb.clamp(0.0, 1.0);
            sa = sa.clamp(0.0, 1.0);
            if sa <= 0.0 {
                continue;
            }

            let dst_idx = row * width + x as usize;
            let dst = buffer[dst_idx];

            let dr = ((dst >> 16) & 0xFF) as f32 / 255.0;
            let dg = ((dst >> 8) & 0xFF) as f32 / 255.0;
            let db = (dst & 0xFF) as f32 / 255.0;
            let da = ((dst >> 24) & 0xFF) as f32 / 255.0;

            let (out_r, out_g, out_b, out_a) = match blend {
                BlendMode::Add => {
                    let r = sr.mul_add(sa, dr).min(1.0);
                    let g = sg.mul_add(sa, dg).min(1.0);
                    let b = sb.mul_add(sa, db).min(1.0);
                    let a = (da + sa).min(1.0);
                    (r, g, b, a)
                }
                _ => {
                    let inv = 1.0 - sa;
                    let r = sr.mul_add(sa, dr * inv);
                    let g = sg.mul_add(sa, dg * inv);
                    let b = sb.mul_add(sa, db * inv);
                    let a = sa + da * inv;
                    (r, g, b, a)
                }
            };

            buffer[dst_idx] = pack_rgba([out_r, out_g, out_b, out_a]);
        }
    }
}

#[inline(always)]
fn edge_function(x0: f32, y0: f32, x1: f32, y1: f32, px: f32, py: f32) -> f32 {
    (px - x0).mul_add(y1 - y0, -((py - y0) * (x1 - x0)))
}
