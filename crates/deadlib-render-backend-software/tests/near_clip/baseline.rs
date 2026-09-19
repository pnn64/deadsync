// Frozen from adfd8870b for projection parity and CPU comparisons.
use super::*;

pub(super) fn project_tmesh_polygon(
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
