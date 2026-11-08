// Frozen bounds and clipping control flow from 54f8191ca.
// Geometry generation and frame finishing use the unchanged production routines.
#![allow(
    clippy::ptr_arg,
    reason = "keep the frozen clipping signatures unchanged"
)]
use super::*;

pub(super) fn textured_mesh_world_bounds(
    vertices: &[renderer::TexturedMeshVertex],
    transform: Matrix4,
) -> Option<WorldRect> {
    if is_affine_world_transform(&transform) {
        textured_mesh_world_bounds_with(vertices, |pos| affine_world_xy_3d(&transform, pos))
    } else {
        textured_mesh_world_bounds_with(vertices, |pos| world_xy_3d(&transform, pos))
    }
}

#[inline(always)]
fn textured_mesh_world_bounds_with(
    vertices: &[renderer::TexturedMeshVertex],
    mut world_xy: impl FnMut([f32; 3]) -> [f32; 2],
) -> Option<WorldRect> {
    let first = vertices.first()?;
    let first = world_xy(first.pos);
    let mut bounds = WorldRect {
        left: first[0],
        right: first[0],
        bottom: first[1],
        top: first[1],
    };
    for vertex in &vertices[1..] {
        let p = world_xy(vertex.pos);
        bounds.left = bounds.left.min(p[0]);
        bounds.right = bounds.right.max(p[0]);
        bounds.bottom = bounds.bottom.min(p[1]);
        bounds.top = bounds.top.max(p[1]);
    }
    Some(bounds)
}

#[inline(always)]
fn affine_world_xy_3d(t: &Matrix4, p: [f32; 3]) -> [f32; 2] {
    [
        t.z_axis
            .x
            .mul_add(p[2], t.y_axis.x.mul_add(p[1], t.x_axis.x * p[0]))
            + t.w_axis.x,
        t.z_axis
            .y
            .mul_add(p[2], t.y_axis.y.mul_add(p[1], t.x_axis.y * p[0]))
            + t.w_axis.y,
    ]
}

#[inline(always)]
fn world_xy_3d(t: &Matrix4, p: [f32; 3]) -> [f32; 2] {
    let clip = *t * Vector4::new(p[0], p[1], p[2], 1.0);
    let inv_w = if clip.w.abs() > f32::EPSILON {
        clip.w.recip()
    } else {
        1.0
    };
    [clip.x * inv_w, clip.y * inv_w]
}

#[inline(always)]
fn is_affine_world_transform(t: &Matrix4) -> bool {
    t.x_axis.w == 0.0 && t.y_axis.w == 0.0 && t.z_axis.w == 0.0 && t.w_axis.w == 1.0
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
