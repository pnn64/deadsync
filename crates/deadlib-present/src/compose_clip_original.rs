// Frozen mesh clipping and its callers from 5de01cdf4.
// Bounds, unchanged transforms and frame finalization use production routines.
#![allow(clippy::ptr_arg, reason = "keep frozen clipping signatures unchanged")]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn clip_textured_mesh_to_world_rect(
    tint: [f32; 4],
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> Option<ClippedSpriteObject> {
    if is_affine_world_transform(&transform) {
        clip_textured_mesh_to_world_rect_with(
            tint,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            clip,
            texture_mask,
            recycled_vertices,
            |pos| affine_world_xy_3d(&transform, pos),
        )
    } else {
        clip_textured_mesh_to_world_rect_with(
            tint,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            clip,
            texture_mask,
            recycled_vertices,
            |pos| world_xy_3d(&transform, pos),
        )
    }
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn clip_textured_mesh_to_world_rect_with(
    tint: [f32; 4],
    vertices: &[renderer::TexturedMeshVertex],
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    clip: WorldRect,
    texture_mask: bool,
    mut recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    mut world_xy: impl FnMut([f32; 3]) -> [f32; 2],
) -> Option<ClippedSpriteObject> {
    if vertices.len() < 3 {
        return None;
    }

    let mut out: Option<Vec<renderer::TexturedMeshVertex>> = None;
    for tri in vertices.as_chunks::<3>().0 {
        let p0 = world_xy(tri[0].pos);
        let p1 = world_xy(tri[1].pos);
        let p2 = world_xy(tri[2].pos);
        let left = p0[0].min(p1[0]).min(p2[0]);
        let right = p0[0].max(p1[0]).max(p2[0]);
        let bottom = p0[1].min(p1[1]).min(p2[1]);
        let top = p0[1].max(p1[1]).max(p2[1]);
        if right < clip.left || left > clip.right || top < clip.bottom || bottom > clip.top {
            continue;
        }

        let uv0 = baked_tmesh_uv(&tri[0], uv_scale, uv_offset, uv_tex_shift);
        let uv1 = baked_tmesh_uv(&tri[1], uv_scale, uv_offset, uv_tex_shift);
        let uv2 = baked_tmesh_uv(&tri[2], uv_scale, uv_offset, uv_tex_shift);
        if left >= clip.left && right <= clip.right && bottom >= clip.bottom && top <= clip.top {
            let out = clipped_text_mesh_out(&mut out, &mut recycled_vertices, vertices.len());
            out.extend(
                [
                    (p0, uv0, tri[0].color),
                    (p1, uv1, tri[1].color),
                    (p2, uv2, tri[2].color),
                ]
                .map(|(pos, uv, color)| renderer::TexturedMeshVertex {
                    normal: [0.0; 4],
                    pos: [pos[0], pos[1], 0.0],
                    uv,
                    tex_matrix_scale: [1.0, 1.0],
                    color,
                }),
            );
            continue;
        }

        let poly = [
            ClipVertex {
                pos: p0,
                uv: uv0,
                color: tri[0].color,
            },
            ClipVertex {
                pos: p1,
                uv: uv1,
                color: tri[1].color,
            },
            ClipVertex {
                pos: p2,
                uv: uv2,
                color: tri[2].color,
            },
        ];
        let clipped = clip_polygon_to_world_rect(&poly, clip);
        if clipped.len() < 3 {
            continue;
        }
        let out = clipped_text_mesh_out(&mut out, &mut recycled_vertices, vertices.len());

        let base = clipped[0];
        let mut i = 1usize;
        while i + 1 < clipped.len() {
            out.extend([base, clipped[i], clipped[i + 1]].map(|vertex| {
                renderer::TexturedMeshVertex {
                    normal: [0.0; 4],
                    pos: [vertex.pos[0], vertex.pos[1], 0.0],
                    uv: vertex.uv,
                    tex_matrix_scale: [1.0, 1.0],
                    color: vertex.color,
                }
            }));
            i += 1;
        }
    }

    let out = out?;
    if out.is_empty() {
        return None;
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
fn baked_tmesh_uv(
    vertex: &renderer::TexturedMeshVertex,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
) -> [f32; 2] {
    [
        uv_tex_shift[0].mul_add(
            vertex.tex_matrix_scale[0] - 1.0,
            vertex.uv[0].mul_add(uv_scale[0], uv_offset[0]),
        ),
        uv_tex_shift[1].mul_add(
            vertex.tex_matrix_scale[1] - 1.0,
            vertex.uv[1].mul_add(uv_scale[1], uv_offset[1]),
        ),
    ]
}

#[inline(always)]
fn clipped_text_mesh_out<'a>(
    out: &'a mut Option<Vec<renderer::TexturedMeshVertex>>,
    recycled_vertices: &mut Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    source_len: usize,
) -> &'a mut Vec<renderer::TexturedMeshVertex> {
    out.get_or_insert_with(|| {
        let mut vertices = recycled_vertices
            .take()
            .map(take_recycled_text_mesh_vertices)
            .unwrap_or_default();
        vertices.reserve(source_len.min(48));
        vertices
    })
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

fn clip_poly_edge_into(
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

pub(super) fn clip_objects_range_to_world_masks(
    objects: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    start: usize,
    sprite_start: usize,
    masks: &[WorldRect],
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    if start >= objects.len() {
        return;
    }
    if masks.is_empty() {
        for index in start..objects.len() {
            let object = objects.take_object(index);
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
        objects.truncate(start);
        sprite_instances.truncate(sprite_start);
        return;
    }
    if let [mask] = masks {
        clip_objects_range_to_world_rect(
            objects,
            sprite_instances,
            start,
            sprite_start,
            *mask,
            recycled_vertices,
        );
        return;
    }
    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let mut object = objects.take_object(read);
        let keep =
            clip_object_to_world_masks(&mut object, sprite_instances, masks, recycled_vertices);
        if keep {
            objects.replace_object(read, object);
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        } else {
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
    }
    objects.truncate(write);
    compact_sprite_instances_for_range(objects, start, sprite_instances, sprite_start);
}

fn clip_object_to_world_masks(
    obj: &mut EditableDraw,
    sprite_instances: &mut [renderer::SpriteInstanceRaw],
    masks: &[WorldRect],
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) -> bool {
    // Every candidate clips the same source in the same world transform.
    // Bounds cost one vertex traversal regardless of the number of masks.
    let textured_mesh_bounds = match &obj.object_type {
        EditablePayload::TexturedMesh {
            instance, vertices, ..
        } => {
            let Some(bounds) = textured_mesh_world_bounds(vertices, instance.transform()) else {
                return false;
            };
            Some(bounds)
        }
        _ => None,
    };
    let mut best_obj: Option<ClippedSpriteObject> = None;
    let mut best_area = -1.0_f32;
    let mut best_is_source = false;
    let mut source_area = None;
    for &mask in masks {
        if mask.left >= mask.right || mask.bottom >= mask.top {
            continue;
        }
        let unchanged = if let Some(bounds) = textured_mesh_bounds {
            if bounds.right < mask.left
                || bounds.left > mask.right
                || bounds.top < mask.bottom
                || bounds.bottom > mask.top
            {
                continue;
            }
            bounds.left >= mask.left
                && bounds.right <= mask.right
                && bounds.bottom >= mask.bottom
                && bounds.top <= mask.top
        } else {
            matches!(obj.object_type, EditablePayload::Mesh { .. })
        };
        if unchanged {
            // A contained candidate borrows the source. Preserve strict area
            // comparisons and mask order: rounding can still favor a later clip.
            let area = *source_area
                .get_or_insert_with(|| object_world_area(&obj.object_type, sprite_instances));
            if area > best_area {
                best_area = area;
                best_is_source = true;
                if let Some(previous) = best_obj.take() {
                    recycle_transient_object_vertices(previous.object_type, recycled_vertices);
                }
            }
            continue;
        }
        let Some(candidate) = clipped_sprite_object_to_world_rect(
            obj,
            sprite_instances,
            mask,
            Some(&mut *recycled_vertices),
            textured_mesh_bounds,
        ) else {
            continue;
        };
        let area = candidate.sprite.map_or_else(
            || object_world_area(&candidate.object_type, sprite_instances),
            |sprite| (sprite.size[0] * sprite.size[1]).abs(),
        );
        if area > best_area {
            best_area = area;
            best_is_source = false;
            if let Some(previous) = best_obj.replace(candidate) {
                recycle_transient_object_vertices(previous.object_type, recycled_vertices);
            }
        } else {
            recycle_transient_object_vertices(candidate.object_type, recycled_vertices);
        }
    }
    if best_is_source {
        true
    } else if let Some(chosen) = best_obj {
        if let Some(sprite) = chosen.sprite
            && let EditablePayload::Sprite(index) = &chosen.object_type
        {
            sprite_instances[*index as usize] = sprite;
        }
        let source = std::mem::replace(&mut obj.object_type, chosen.object_type);
        recycle_transient_object_vertices(source, recycled_vertices);
        true
    } else {
        false
    }
}

fn clip_objects_range_to_world_rect(
    objects: &mut FrameBuilder,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    start: usize,
    sprite_start: usize,
    clip: WorldRect,
    recycled_vertices: &mut Vec<Vec<renderer::TexturedMeshVertex>>,
) {
    if start >= objects.len() {
        return;
    }
    if clip.left >= clip.right || clip.bottom >= clip.top {
        for index in start..objects.len() {
            let object = objects.take_object(index);
            recycle_transient_object_vertices(object.object_type, recycled_vertices);
        }
        objects.truncate(start);
        sprite_instances.truncate(sprite_start);
        return;
    }

    let len = objects.len();
    let mut write = start;
    for read in start..len {
        let item = objects.items[read];
        let keep = match item.kind {
            DrawKind::Sprite => {
                // Ordinary clipping changes only the instance. Detach a draw
                // only when rotation promotes the sprite to a textured mesh.
                if let Some(clipped) = clipped_sprite_instance_to_world_rect(
                    item.payload_index,
                    sprite_instances[item.payload_index as usize],
                    clip,
                    Some(&mut *recycled_vertices),
                ) {
                    if let EditablePayload::Sprite(_) = clipped.object_type {
                        if let Some(sprite) = clipped.sprite {
                            sprite_instances[item.payload_index as usize] = sprite;
                        }
                    } else {
                        objects.replace_object(
                            read,
                            EditableDraw {
                                texture_handle: item.texture_handle,
                                order: item.order,
                                z: item.z,
                                blend: item.blend,
                                camera: item.camera,
                                object_type: clipped.object_type,
                            },
                        );
                    }
                    true
                } else {
                    false
                }
            }
            DrawKind::Mesh => true,
            DrawKind::TexturedMesh => {
                let mut object = objects.take_object(read);
                let keep = clip_sprite_object_to_world_rect_with_recycled(
                    &mut object,
                    sprite_instances,
                    clip,
                    Some(&mut *recycled_vertices),
                );
                if keep {
                    objects.replace_object(read, object);
                } else {
                    recycle_transient_object_vertices(object.object_type, recycled_vertices);
                }
                keep
            }
        };
        if keep {
            if write != read {
                objects.swap(write, read);
            }
            write += 1;
        }
    }
    objects.truncate(write);
    compact_sprite_instances_for_range(objects, start, sprite_instances, sprite_start);
}

fn clip_sprite_object_to_world_rect_with_recycled(
    obj: &mut EditableDraw,
    sprite_instances: &mut Vec<renderer::SpriteInstanceRaw>,
    clip: WorldRect,
    mut recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
) -> bool {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        return false;
    }
    let mut textured_mesh_bounds = None;
    match &obj.object_type {
        EditablePayload::Mesh { .. } => return true,
        EditablePayload::TexturedMesh {
            instance, vertices, ..
        } => {
            let transform = instance.transform();
            let Some(bounds) = textured_mesh_world_bounds(vertices.as_ref(), transform) else {
                return false;
            };
            if bounds.right < clip.left
                || bounds.left > clip.right
                || bounds.top < clip.bottom
                || bounds.bottom > clip.top
            {
                return false;
            }
            if bounds.left >= clip.left
                && bounds.right <= clip.right
                && bounds.bottom >= clip.bottom
                && bounds.top <= clip.top
            {
                return true;
            }
            textured_mesh_bounds = Some(bounds);
        }
        EditablePayload::Sprite(_) => {}
    }

    let Some(clipped) = clipped_sprite_object_to_world_rect(
        obj,
        sprite_instances,
        clip,
        recycled_vertices.as_deref_mut(),
        textured_mesh_bounds,
    ) else {
        return false;
    };
    if let Some(sprite) = clipped.sprite
        && let EditablePayload::Sprite(index) = &clipped.object_type
    {
        sprite_instances[*index as usize] = sprite;
    }
    let source = std::mem::replace(&mut obj.object_type, clipped.object_type);
    if let Some(pool) = recycled_vertices {
        // Transient text is rebuilt from this pool on the next frame. Keep
        // both the source and clipped output alive instead of losing a buffer.
        recycle_transient_object_vertices(source, pool);
    }
    true
}

fn clipped_sprite_object_to_world_rect(
    obj: &EditableDraw,
    sprite_instances: &[renderer::SpriteInstanceRaw],
    clip: WorldRect,
    recycled_vertices: Option<&mut Vec<Vec<renderer::TexturedMeshVertex>>>,
    textured_mesh_bounds: Option<WorldRect>,
) -> Option<ClippedSpriteObject> {
    if clip.left >= clip.right || clip.bottom >= clip.top {
        return None;
    }
    match &obj.object_type {
        EditablePayload::Sprite(index) => clipped_sprite_instance_to_world_rect(
            *index,
            sprite_instances[*index as usize],
            clip,
            recycled_vertices,
        ),
        EditablePayload::TexturedMesh {
            instance,
            sampler,
            vertices: mesh_vertices,
            ..
        } => {
            let vertices = mesh_vertices.as_ref();
            let transform = instance.transform();
            let bounds = match textured_mesh_bounds {
                Some(bounds) => bounds,
                None => textured_mesh_world_bounds(vertices, transform)?,
            };
            if bounds.right < clip.left
                || bounds.left > clip.right
                || bounds.top < clip.bottom
                || bounds.bottom > clip.top
            {
                return None;
            }
            let mut clipped = clip_textured_mesh_to_world_rect(
                instance.tint,
                vertices,
                transform,
                instance.uv_scale,
                instance.uv_offset,
                instance.uv_tex_shift,
                clip,
                instance.texture_mask != 0.0,
                recycled_vertices,
            )?;
            if let EditablePayload::TexturedMesh {
                sampler: clipped_sampler,
                ..
            } = &mut clipped.object_type
            {
                *clipped_sampler = *sampler;
            }
            Some(clipped)
        }
        EditablePayload::Mesh { .. } => unreachable!("callers keep colored meshes unchanged"),
    }
}
