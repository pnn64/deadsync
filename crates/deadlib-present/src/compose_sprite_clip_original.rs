// Frozen sprite clipping geometry from aa242cf0f.
// Payload types and the unchanged buffer-pool pop use production definitions.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn clipped_sprite_instance_to_world_rect(
    index: u32,
    sprite: renderer::SpriteInstanceRaw,
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    let eps = 1e-6;
    let offset_world = [
        sprite.local_offset_rot_sin_cos[1].mul_add(
            sprite.local_offset[0],
            -(sprite.local_offset_rot_sin_cos[0] * sprite.local_offset[1]),
        ),
        sprite.local_offset_rot_sin_cos[0].mul_add(
            sprite.local_offset[0],
            sprite.local_offset_rot_sin_cos[1] * sprite.local_offset[1],
        ),
    ];
    let world_center = [
        sprite.center[0] + offset_world[0],
        sprite.center[1] + offset_world[1],
    ];
    if sprite.rot_sin_cos[0].abs() > eps || sprite.rot_sin_cos[1] < 1.0 - eps {
        return clip_rotated_sprite_to_world_rect(
            sprite.tint,
            sprite.center,
            sprite.size,
            sprite.rot_sin_cos,
            sprite.uv_scale,
            sprite.uv_offset,
            offset_world,
            clip,
            sprite.texture_mask != 0.0,
            recycled_vertices,
        );
    }

    let w = sprite.size[0];
    let h = sprite.size[1];
    if w <= eps || h <= eps {
        return None;
    }

    let half_w = w * 0.5;
    let half_h = h * 0.5;

    let left = world_center[0] - half_w;
    let right = world_center[0] + half_w;
    let bottom = world_center[1] - half_h;
    let top = world_center[1] + half_h;

    if left >= clip.left && right <= clip.right && bottom >= clip.bottom && top <= clip.top {
        return Some(ClippedSpriteObject {
            object_type: EditablePayload::Sprite(index),
            sprite: None,
        });
    }

    let inter_left = left.max(clip.left);
    let inter_right = right.min(clip.right);
    let inter_bottom = bottom.max(clip.bottom);
    let inter_top = top.min(clip.top);
    if inter_left >= inter_right || inter_bottom >= inter_top {
        return None;
    }

    let inv_w = 1.0 / w;
    let inv_h = 1.0 / h;

    let cl = ((inter_left - left) * inv_w).clamp(0.0, 1.0);
    let cr = ((right - inter_right) * inv_w).clamp(0.0, 1.0);
    let cb = ((inter_bottom - bottom) * inv_h).clamp(0.0, 1.0);
    let ct = ((top - inter_top) * inv_h).clamp(0.0, 1.0);

    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= eps || sy_crop <= eps {
        return None;
    }

    let uv_offset = [
        sprite.uv_scale[0].mul_add(cl, sprite.uv_offset[0]),
        sprite.uv_scale[1].mul_add(ct, sprite.uv_offset[1]),
    ];
    let uv_scale = [sprite.uv_scale[0] * sx_crop, sprite.uv_scale[1] * sy_crop];

    let center_x = ((cl - cr) * w).mul_add(0.5, world_center[0]) - offset_world[0];
    let center_y = ((cb - ct) * h).mul_add(0.5, world_center[1]) - offset_world[1];
    let new_w = w * sx_crop;
    let new_h = h * sy_crop;

    Some(ClippedSpriteObject {
        object_type: EditablePayload::Sprite(index),
        sprite: Some(renderer::SpriteInstanceRaw {
            center: [center_x, center_y, sprite.center[2], sprite.center[3]],
            size: [new_w, new_h],
            rot_sin_cos: sprite.rot_sin_cos,
            tint: sprite.tint,
            uv_scale,
            uv_offset,
            local_offset: sprite.local_offset,
            local_offset_rot_sin_cos: sprite.local_offset_rot_sin_cos,
            edge_fade: sprite.edge_fade,
            texture_mask: sprite.texture_mask,
        }),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn clip_rotated_sprite_to_world_rect(
    tint: [f32; 4],
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    offset_world: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    let poly = [
        ClipVertex {
            pos: sprite_world_xy(
                center,
                size,
                rot_sin_cos,
                offset_world,
                [-0.5_f32, -0.5_f32],
            ),
            uv: [uv_offset[0], uv_offset[1] + uv_scale[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [0.5_f32, -0.5_f32]),
            uv: [uv_offset[0] + uv_scale[0], uv_offset[1] + uv_scale[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [0.5_f32, 0.5_f32]),
            uv: [uv_offset[0] + uv_scale[0], uv_offset[1]],
            color: [1.0; 4],
        },
        ClipVertex {
            pos: sprite_world_xy(center, size, rot_sin_cos, offset_world, [-0.5_f32, 0.5_f32]),
            uv: [uv_offset[0], uv_offset[1]],
            color: [1.0; 4],
        },
    ];
    let clipped = clip_polygon_to_world_rect(&poly, clip);
    if clipped.len() < 3 {
        return None;
    }

    // A clipped quad emits at most 18 vertices. Borrow the existing bounded
    // composition pool only after clipping succeeds, then recycle with the frame.
    let mut out = recycled_vertices
        .map(take_recycled_text_mesh_vertices)
        .unwrap_or_default();
    out.reserve((clipped.len() - 2) * 3);
    let base = clipped[0];
    let mut i = 1usize;
    while i + 1 < clipped.len() {
        for v in [base, clipped[i], clipped[i + 1]] {
            out.push(renderer::TexturedMeshVertex {
                normal: [0.0; 4],
                pos: [v.pos[0], v.pos[1], 0.0],
                uv: v.uv,
                tex_matrix_scale: [1.0, 1.0],
                color: v.color,
            });
        }
        i += 1;
    }

    Some(ClippedSpriteObject {
        object_type: EditablePayload::TexturedMesh {
            instance: renderer::TexturedMeshInstanceRaw::new(
                Matrix4::IDENTITY,
                tint,
                [1.0, 1.0],
                [0.0, 0.0],
                [0.0, 0.0],
                texture_mask,
            ),
            vertices: renderer::TexturedMeshVertices::Transient(out),
            geom_cache_key: renderer::INVALID_TMESH_CACHE_KEY,
            depth_test: false,
            depth_write: false,
            depth_test_on_fail: false,
            clear_depth: false,
            clear_depth_after: false,
            sampler: None,
        },
        sprite: None,
    })
}

#[inline(always)]
fn sprite_world_xy(
    center: [f32; 4],
    size: [f32; 2],
    rot_sin_cos: [f32; 2],
    offset_world: [f32; 2],
    p: [f32; 2],
) -> [f32; 2] {
    let local_x = p[0] * size[0];
    let local_y = p[1] * size[1];
    [
        rot_sin_cos[1].mul_add(
            local_x,
            (-rot_sin_cos[0]).mul_add(local_y, center[0]) + offset_world[0],
        ),
        rot_sin_cos[0].mul_add(
            local_x,
            rot_sin_cos[1].mul_add(local_y, center[1]) + offset_world[1],
        ),
    ]
}

#[inline(always)]
fn lerp_clip(a: ClipVertex, b: ClipVertex, t: f32) -> ClipVertex {
    let t = t.clamp(0.0, 1.0);
    ClipVertex {
        pos: [
            (b.pos[0] - a.pos[0]).mul_add(t, a.pos[0]),
            (b.pos[1] - a.pos[1]).mul_add(t, a.pos[1]),
        ],
        uv: [
            (b.uv[0] - a.uv[0]).mul_add(t, a.uv[0]),
            (b.uv[1] - a.uv[1]).mul_add(t, a.uv[1]),
        ],
        color: [
            (b.color[0] - a.color[0]).mul_add(t, a.color[0]),
            (b.color[1] - a.color[1]).mul_add(t, a.color[1]),
            (b.color[2] - a.color[2]).mul_add(t, a.color[2]),
            (b.color[3] - a.color[3]).mul_add(t, a.color[3]),
        ],
    }
}

#[inline(never)]
fn clip_poly_edge_into(
    poly: &[ClipVertex],
    axis: usize,
    bound: f32,
    keep_greater: bool,
    out: &mut ClipPolygon,
) {
    clip_poly_edge_into_inner(poly, axis, bound, keep_greater, out);
}

#[inline(always)]
fn clip_poly_edge_into_inner(
    poly: &[ClipVertex],
    axis: usize,
    bound: f32,
    keep_greater: bool,
    out: &mut ClipPolygon,
) {
    out.clear();
    if poly.is_empty() {
        return;
    }
    let mut prev = poly[poly.len() - 1];
    let mut prev_in = if keep_greater {
        prev.pos[axis] >= bound
    } else {
        prev.pos[axis] <= bound
    };

    for &curr in poly {
        let curr_in = if keep_greater {
            curr.pos[axis] >= bound
        } else {
            curr.pos[axis] <= bound
        };
        if prev_in != curr_in {
            let denom = curr.pos[axis] - prev.pos[axis];
            if denom.abs() > 1e-6 {
                let t = (bound - prev.pos[axis]) / denom;
                out.push(lerp_clip(prev, curr, t));
            }
        }
        if curr_in {
            out.push(curr);
        }
        prev = curr;
        prev_in = curr_in;
    }
}

fn clip_polygon_to_world_rect(poly: &[ClipVertex], clip: WorldRect) -> ClipPolygon {
    let mut a = ClipPolygon::new();
    let mut b = ClipPolygon::new();
    clip_poly_edge_into(poly, 0, clip.left, true, &mut a);
    clip_poly_edge_into(&a, 0, clip.right, false, &mut b);
    clip_poly_edge_into(&b, 1, clip.bottom, true, &mut a);
    clip_poly_edge_into(&a, 1, clip.top, false, &mut b);
    b
}
