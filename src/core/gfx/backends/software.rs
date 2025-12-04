use crate::core::gfx::{BlendMode, ObjectType, RenderList, Texture as RendererTexture};
use crate::core::space::ortho_for_window;
use cgmath::{Matrix4, Vector4};
use image::RgbaImage;
use log::info;
use std::{collections::HashMap, error::Error, num::NonZeroU32, sync::Arc};
use winit::{dpi::PhysicalSize, window::Window};

pub struct Texture {
    pub image: RgbaImage,
}

pub struct State {
    _context: softbuffer::Context<Arc<Window>>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    window_size: PhysicalSize<u32>,
    projection: Matrix4<f32>,
}

pub fn init(window: Arc<Window>, _vsync_enabled: bool) -> Result<State, Box<dyn Error>> {
    info!("Initializing software renderer backend (softbuffer)...");

    let window_size = window.inner_size();
    let projection = ortho_for_window(window_size.width, window_size.height);

    let context = softbuffer::Context::new(window.clone())?;
    let surface = softbuffer::Surface::new(&context, window.clone())?;

    Ok(State {
        _context: context,
        surface,
        window_size,
        projection,
    })
}

pub fn create_texture(image: &RgbaImage) -> Result<Texture, Box<dyn Error>> {
    Ok(Texture { image: image.clone() })
}

pub fn draw<'a>(
    state: &mut State,
    render_list: &RenderList<'a>,
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

    let proj = state.projection;
    let mut total_vertices: u32 = 0;

    for obj in &render_list.objects {
        let ObjectType::Sprite {
            texture_id,
            tint,
            uv_scale,
            uv_offset,
            edge_fade: _,
        } = &obj.object_type;

        let tex_key = texture_id.as_ref();
        let Some(RendererTexture::Software(tex)) = textures.get(tex_key) else {
            continue;
        };

        total_vertices += rasterize_sprite(
            &proj,
            &obj.transform,
            *tint,
            *uv_scale,
            *uv_offset,
            obj.blend,
            &tex.image,
            w,
            h,
            &mut buffer,
        );
    }

    buffer.present()?;

    Ok(total_vertices)
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

    let r = (clamp01(c[0]) * 255.0 + 0.5) as u32;
    let g = (clamp01(c[1]) * 255.0 + 0.5) as u32;
    let b = (clamp01(c[2]) * 255.0 + 0.5) as u32;
    let a = (clamp01(c[3]) * 255.0 + 0.5) as u32;

    (a << 24) | (r << 16) | (g << 8) | b
}

#[derive(Clone, Copy)]
struct ScreenVertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
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
    width: usize,
    height: usize,
    buffer: &mut [u32],
) -> u32 {
    if tint[3] <= 0.0 || width == 0 || height == 0 {
        return 0;
    }

    let mvp = *proj * *transform;

    const POS: [(f32, f32); 4] = [
        (-0.5, -0.5),
        (0.5, -0.5),
        (0.5, 0.5),
        (-0.5, 0.5),
    ];
    const UV_BASE: [(f32, f32); 4] = [
        (0.0, 1.0),
        (1.0, 1.0),
        (1.0, 0.0),
        (0.0, 0.0),
    ];

    let mut v = [ScreenVertex { x: 0.0, y: 0.0, u: 0.0, v: 0.0 }; 4];

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
        let u = u0 * uv_scale[0] + uv_offset[0];
        let vv = v0 * uv_scale[1] + uv_offset[1];

        v[i] = ScreenVertex { x: sx, y: sy, u, v: vv };
    }

    rasterize_triangle(&v[0], &v[1], &v[2], tint, blend, image, width, height, buffer);
    rasterize_triangle(&v[0], &v[2], &v[3], tint, blend, image, width, height, buffer);

    4
}

#[inline(always)]
fn rasterize_triangle(
    v0: &ScreenVertex,
    v1: &ScreenVertex,
    v2: &ScreenVertex,
    tint: [f32; 4],
    blend: BlendMode,
    image: &RgbaImage,
    width: usize,
    height: usize,
    buffer: &mut [u32],
) {
    let min_x = v0.x.min(v1.x).min(v2.x).floor().max(0.0) as i32;
    let max_x = v0.x.max(v1.x).max(v2.x).ceil().min((width - 1) as f32) as i32;
    let min_y = v0.y.min(v1.y).min(v2.y).floor().max(0.0) as i32;
    let max_y = v0.y.max(v1.y).max(v2.y).ceil().min((height - 1) as f32) as i32;

    if min_x > max_x || min_y > max_y {
        return;
    }

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
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;

            let w0 = edge_function(v1.x, v1.y, v2.x, v2.y, px, py) * inv_denom;
            let w1 = edge_function(v2.x, v2.y, v0.x, v0.y, px, py) * inv_denom;
            let w2 = 1.0 - w0 - w1;

            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let u = v0.u * w0 + v1.u * w1 + v2.u * w2;
            let v = v0.v * w0 + v1.v * w1 + v2.v * w2;

            let u_norm = u.fract().max(0.0);
            let v_norm = v.fract().max(0.0);

            let tx = (u_norm * tex_w as f32).floor() as usize % tex_w;
            let ty = (v_norm * tex_h as f32).floor() as usize % tex_h;

            let idx = (ty * tex_w + tx) * 4;
            if idx + 3 >= tex_data.len() {
                continue;
            }

            let sr = tex_data[idx] as f32 / 255.0;
            let sg = tex_data[idx + 1] as f32 / 255.0;
            let sb = tex_data[idx + 2] as f32 / 255.0;
            let sa = tex_data[idx + 3] as f32 / 255.0;

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

            let dst_idx = (y as usize * width + x as usize) as usize;
            let dst = buffer[dst_idx];

            let dr = ((dst >> 16) & 0xFF) as f32 / 255.0;
            let dg = ((dst >> 8) & 0xFF) as f32 / 255.0;
            let db = (dst & 0xFF) as f32 / 255.0;
            let da = ((dst >> 24) & 0xFF) as f32 / 255.0;

            let (out_r, out_g, out_b, out_a) = match blend {
                BlendMode::Add => {
                    let r = (dr + sr * sa).min(1.0);
                    let g = (dg + sg * sa).min(1.0);
                    let b = (db + sb * sa).min(1.0);
                    let a = (da + sa).min(1.0);
                    (r, g, b, a)
                }
                _ => {
                    let inv = 1.0 - sa;
                    let r = sr * sa + dr * inv;
                    let g = sg * sa + dg * inv;
                    let b = sb * sa + db * inv;
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
    (px - x0) * (y1 - y0) - (py - y0) * (x1 - x0)
}
