// Frozen area and mask-selection control flow from 6b94c5be9.
// Projection, bounds, clipping and pool management use unchanged production helpers.
use super::*;

pub(super) fn object_world_area(
    object_type: &EditablePayload,
    sprite_instances: &[renderer::SpriteInstanceRaw],
) -> f32 {
    match object_type {
        EditablePayload::Sprite(index) => {
            let sprite = sprite_instances[*index as usize];
            (sprite.size[0] * sprite.size[1]).abs()
        }
        EditablePayload::TexturedMesh {
            instance, vertices, ..
        } => {
            if vertices.len() < 3 {
                return 0.0;
            }
            let transform = instance.transform();
            let mut area = 0.0_f32;
            let mut i = 0usize;
            while i + 2 < vertices.len() {
                let p0 = world_xy_3d(&transform, vertices[i].pos);
                let p1 = world_xy_3d(&transform, vertices[i + 1].pos);
                let p2 = world_xy_3d(&transform, vertices[i + 2].pos);
                let a =
                    (p1[1] - p0[1]).mul_add(-(p2[0] - p0[0]), (p1[0] - p0[0]) * (p2[1] - p0[1]));
                area = 0.5f32.mul_add(a.abs(), area);
                i += 3;
            }
            area
        }
        EditablePayload::Mesh { .. } => 0.0,
    }
}

pub(super) fn clip_object_to_world_masks(
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
