//! Frozen production reference from b2494f569.
use super::*;

pub(super) fn covered(weights: [f32; 3], edges: [bool; 3]) -> bool {
    weights
        .into_iter()
        .zip(edges)
        .all(|(w, edge)| w > 0.0 || (w == 0.0 && edge))
}

#[expect(clippy::too_many_arguments, reason = "Frozen production reference")]
pub(super) fn rasterize_triangle_tex_color_impl<
    const LINEAR: bool,
    const MASK: bool,
    const OPAQUE: bool,
>(
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
                    sample_tex_linear::<OPAQUE, 4>(tex_data, tex_w, tex_h, u, v, sampler)
                } else {
                    sample_tex_nearest::<OPAQUE, 4>(tex_data, tex_w, tex_h, u, v, sampler)
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

#[expect(clippy::too_many_arguments, reason = "Frozen production reference")]
pub(super) fn project_tmesh_polygon(
    mvp: &Matrix4,
    tint: [f32; 4],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    vertices: &[deadlib_render_core::TexturedMeshVertex],
    width: usize,
    height: usize,
    cull_mode: f32,
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
    if cull_mode > 0.5 && polygon.len() >= 3 {
        let [a, b, c] = [projected[0], projected[1], projected[2]];
        // Screen Y points down, so a CCW clip-space face has negative area.
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if (cull_mode < 1.5 && area >= 0.0) || (cull_mode > 1.5 && area <= 0.0) {
            return None;
        }
    }
    Some((projected, polygon.len()))
}

pub(super) fn clip_tmesh_near(
    triangle: [ClipVertexTexColor; 3],
) -> ([ClipVertexTexColor; 4], usize) {
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
