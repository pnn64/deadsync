//! Frozen from 56a0f78eb48cb5af19d2b33667585fcdbcb79ffb; only function visibility and formatting differ.

use super::*;

pub(super) fn actor_multi_vertex_line_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 2 || line_width <= f32::EPSILON {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(1) * 6);
    let half_width = 0.5 * line_width;
    let mut offsets = Vec::with_capacity(vertices.len());
    for index in 0..vertices.len() {
        let offset = if index == 0 {
            actor_multi_vertex_line_normal(vertices[0], vertices[1], half_width)
        } else if index + 1 == vertices.len() {
            actor_multi_vertex_line_normal(vertices[index - 1], vertices[index], half_width)
        } else {
            actor_multi_vertex_line_join_offset(
                vertices[index - 1],
                vertices[index],
                vertices[index + 1],
                half_width,
            )
        };
        offsets.push(offset);
    }
    for index in 0..vertices.len().saturating_sub(1) {
        let a = vertices[index];
        let b = vertices[index + 1];
        if actor_multi_vertex_segment_len(a, b) <= f32::EPSILON {
            continue;
        }
        let a0 = actor_multi_vertex_offset_point(a, offsets[index], 1.0);
        let a1 = actor_multi_vertex_offset_point(a, offsets[index], -1.0);
        let b0 = actor_multi_vertex_offset_point(b, offsets[index + 1], 1.0);
        let b1 = actor_multi_vertex_offset_point(b, offsets[index + 1], -1.0);
        push_actor_multi_vertex_triangle(&mut out, a0, b0, b1);
        push_actor_multi_vertex_triangle(&mut out, a0, b1, a1);
    }
    out
}

pub(super) fn actor_multi_vertex_segment_len(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
) -> f32 {
    (b.pos[0] - a.pos[0]).hypot(b.pos[1] - a.pos[1])
}

pub(super) fn actor_multi_vertex_line_normal(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let len = dx.hypot(dy);
    if len <= f32::EPSILON {
        return [0.0, 0.0];
    }
    [-dy / len * half_width, dx / len * half_width]
}

pub(super) fn actor_multi_vertex_line_join_offset(
    prev: SongLuaActorMultiVertexPoint,
    current: SongLuaActorMultiVertexPoint,
    next: SongLuaActorMultiVertexPoint,
    half_width: f32,
) -> [f32; 2] {
    let prev_normal = actor_multi_vertex_line_normal(prev, current, 1.0);
    let next_normal = actor_multi_vertex_line_normal(current, next, 1.0);
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
