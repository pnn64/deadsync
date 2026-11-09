// Frozen colored-triangle drawing from ca3942150.
// Coverage, projection setup and pixel writers use unchanged production helpers.
#![allow(
    clippy::too_many_arguments,
    reason = "preserve frozen raster signatures"
)]
use super::*;

pub(super) fn rasterize_mesh_triangles(
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

#[inline(always)]
pub(super) fn rasterize_triangle_color(
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
pub(super) fn rasterize_triangle_color_prepared(
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
pub(super) fn rasterize_triangle_color_impl(
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
