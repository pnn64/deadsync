//! Existing triangulation and line-strip geometry, independent of Lua values.
use crate::{SongLuaActorMultiVertexPoint, SongLuaOverlayMeshVertex};
use std::sync::Arc;

pub(super) fn actor_multi_vertex_mesh_arc(
    vertices: &[SongLuaActorMultiVertexPoint],
    mode: &str,
    width: f32,
) -> Option<Arc<[SongLuaOverlayMeshVertex]>> {
    if mode.eq_ignore_ascii_case("DrawMode_Quads") {
        let groups = vertices.as_chunks::<4>().0;
        if groups.is_empty() {
            return None;
        }
        return Some(
            (0..groups.len() * 6)
                .map(|i| {
                    actor_multi_vertex_mesh_vertex(vertices[i / 6 * 4 + [0, 1, 2, 2, 3, 0][i % 6]])
                })
                .collect(),
        );
    }
    if mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
        let count = vertices.len().saturating_sub(2) / 2;
        if count == 0 {
            return None;
        }
        return Some(
            (0..count * 6)
                .map(|i| {
                    actor_multi_vertex_mesh_vertex(vertices[i / 6 * 2 + [0, 1, 3, 3, 2, 0][i % 6]])
                })
                .collect(),
        );
    }
    if mode.eq_ignore_ascii_case("DrawMode_LineStrip") {
        let mesh = actor_multi_vertex_line_strip(vertices, width);
        return (!mesh.is_empty()).then(|| mesh.into());
    }
    if mode.eq_ignore_ascii_case("DrawMode_Fan") {
        if vertices.len() < 3 {
            return None;
        }
        return Some(
            (0..(vertices.len() - 2) * 3)
                .map(|i| {
                    actor_multi_vertex_mesh_vertex(match i % 3 {
                        0 => vertices[0],
                        1 => vertices[i / 3 + 1],
                        _ => vertices[i / 3 + 2],
                    })
                })
                .collect(),
        );
    }
    let count = vertices.len() / 3 * 3;
    if count == 0 {
        return None;
    }
    Some(
        vertices[..count]
            .iter()
            .copied()
            .map(actor_multi_vertex_mesh_vertex)
            .collect(),
    )
}

fn actor_multi_vertex_line_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 2 || line_width <= f32::EPSILON {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(1) * 6);
    let half_width = 0.5 * line_width;
    let (mut len, mut normal) = actor_multi_vertex_line_segment(vertices[0], vertices[1]);
    let mut offset = actor_multi_vertex_line_endpoint(len, normal, half_width);
    // Only adjacent segment normals and join offsets are needed. Calculate
    // each segment once and carry the shared endpoint into the next quad.
    for (index, pair) in vertices.windows(2).enumerate() {
        let next = vertices
            .get(index + 2)
            .map(|&next| actor_multi_vertex_line_segment(pair[1], next));
        let next_offset = if let Some((_, next_normal)) = next {
            actor_multi_vertex_line_join_offset(normal, next_normal, half_width)
        } else {
            actor_multi_vertex_line_endpoint(len, normal, half_width)
        };
        let current_offset = offset;
        let skip_segment = len <= f32::EPSILON;
        offset = next_offset;
        if let Some((next_len, next_normal)) = next {
            len = next_len;
            normal = next_normal;
        }
        // Carry the join forward even when a degenerate segment is skipped.
        if skip_segment {
            continue;
        }
        let a0 = actor_multi_vertex_offset_point(pair[0], current_offset, 1.0);
        let a1 = actor_multi_vertex_offset_point(pair[0], current_offset, -1.0);
        let b0 = actor_multi_vertex_offset_point(pair[1], next_offset, 1.0);
        let b1 = actor_multi_vertex_offset_point(pair[1], next_offset, -1.0);
        push_actor_multi_vertex_triangle(&mut out, a0, b0, b1);
        push_actor_multi_vertex_triangle(&mut out, a0, b1, a1);
    }
    out
}

fn actor_multi_vertex_line_segment(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
) -> (f32, [f32; 2]) {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let len = dx.hypot(dy);
    let normal = if len <= f32::EPSILON {
        [0.0, 0.0]
    } else {
        [-dy / len, dx / len]
    };
    (len, normal)
}

fn actor_multi_vertex_line_endpoint(len: f32, normal: [f32; 2], half_width: f32) -> [f32; 2] {
    if len <= f32::EPSILON {
        [0.0, 0.0]
    } else {
        [normal[0] * half_width, normal[1] * half_width]
    }
}

fn actor_multi_vertex_line_join_offset(
    prev_normal: [f32; 2],
    next_normal: [f32; 2],
    half_width: f32,
) -> [f32; 2] {
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half_width, prev_normal[1] * half_width];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[1].mul_add(prev_normal[1], miter[0] * prev_normal[0]);
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half_width, next_normal[1] * half_width];
    }
    let scale = (half_width / denom).clamp(-half_width * 4.0, half_width * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

fn actor_multi_vertex_offset_point(
    vertex: SongLuaActorMultiVertexPoint,
    offset: [f32; 2],
    sign: f32,
) -> SongLuaActorMultiVertexPoint {
    SongLuaActorMultiVertexPoint {
        pos: [
            offset[0].mul_add(sign, vertex.pos[0]),
            offset[1].mul_add(sign, vertex.pos[1]),
        ],
        z: vertex.z,
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn push_actor_multi_vertex_triangle(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
) {
    out.push(actor_multi_vertex_mesh_vertex(a));
    out.push(actor_multi_vertex_mesh_vertex(b));
    out.push(actor_multi_vertex_mesh_vertex(c));
}

#[inline(always)]
const fn actor_multi_vertex_mesh_vertex(
    vertex: SongLuaActorMultiVertexPoint,
) -> SongLuaOverlayMeshVertex {
    SongLuaOverlayMeshVertex {
        pos: vertex.pos,
        z: vertex.z,
        color: vertex.color,
        uv: vertex.uv,
    }
}
