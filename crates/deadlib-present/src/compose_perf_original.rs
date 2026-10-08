// Starting-main implementations for paired benchmarks and differential tests.

use super::*;

fn clone_retained_object(objects: &FrameBuilder, index: usize) -> Option<EditableDraw> {
    let item = objects.items[index];
    let object_type = match item.kind {
        DrawKind::Sprite => EditablePayload::Sprite(item.payload_index),
        DrawKind::Mesh => {
            let payload = objects.meshes.get(item.payload_index as usize)?.as_ref()?;
            EditablePayload::Mesh {
                transform: payload.transform,
                tint: payload.tint,
                vertices: payload.vertices.clone(),
            }
        }
        DrawKind::TexturedMesh => {
            let payload = objects
                .textured_meshes
                .get(item.payload_index as usize)?
                .as_ref()?;
            if matches!(
                payload.vertices,
                renderer::TexturedMeshVertices::Transient(_)
            ) {
                return None;
            }
            EditablePayload::TexturedMesh {
                instance: payload.instance,
                vertices: payload.vertices.clone(),
                geom_cache_key: payload.geom_cache_key,
                depth_test: payload.depth_test,
                clear_depth: payload.clear_depth,
                clear_depth_after: payload.clear_depth_after,
            }
        }
    };
    Some(EditableDraw {
        texture_handle: item.texture_handle,
        order: item.order,
        z: item.z,
        blend: item.blend,
        camera: item.camera,
        object_type,
    })
}

pub(super) fn capture_retained_frame(
    objects: &FrameBuilder,
    sprite_instances: &[renderer::SpriteInstanceRaw],
    object_start: usize,
    sprite_start: usize,
) -> Option<CachedRetainedFrame> {
    let sprite_start_u32 = u32::try_from(sprite_start).ok()?;
    if object_start > objects.len() {
        return None;
    }
    let mut cached_builder = FrameBuilder::default();
    cached_builder.reserve(objects.len().saturating_sub(object_start));
    for index in object_start..objects.len() {
        let mut object = clone_retained_object(objects, index)?;
        match &mut object.object_type {
            EditablePayload::Sprite(index) => {
                *index = index.checked_sub(sprite_start_u32)?;
            }
            EditablePayload::TexturedMesh {
                vertices: renderer::TexturedMeshVertices::Transient(_),
                ..
            } => return None,
            _ => {}
        }
        object.order = 0;
        cached_builder.push(object);
    }
    Some(CachedRetainedFrame {
        builder: cached_builder,
        sprite_instances: sprite_instances.get(sprite_start..)?.to_vec(),
    })
}

#[derive(Default)]
pub(super) struct TextAttrScratch {
    start_order: SmallVec<[usize; 8]>,
    end_order: SmallVec<[usize; 8]>,
    active: SmallVec<[usize; 8]>,
}

pub(super) struct TextAttrCursor<'a> {
    attributes: &'a [actors::TextAttribute],
    scratch: &'a mut TextAttrScratch,
    active_max: Option<usize>,
    next_start: usize,
    next_end: usize,
}

impl<'a> TextAttrCursor<'a> {
    pub(super) fn new(
        attributes: &'a [actors::TextAttribute],
        scratch: &'a mut TextAttrScratch,
    ) -> Option<Self> {
        if attributes.is_empty() {
            return None;
        }

        let TextAttrScratch {
            start_order,
            end_order,
            active,
        } = scratch;
        start_order.clear();
        end_order.clear();
        active.clear();
        // Moving ranges can increase overlap without increasing their count.
        active.reserve(attributes.len());
        start_order.extend(0..attributes.len());
        end_order.extend(0..attributes.len());

        // Equal-boundary events are consumed together; active_max preserves
        // original attribute precedence independently of their event order.
        start_order.sort_unstable_by_key(|&index| attributes[index].start);
        end_order.sort_unstable_by_key(|&index| attr_end(&attributes[index]));

        Some(Self {
            attributes,
            scratch,
            active_max: None,
            next_start: 0,
            next_end: 0,
        })
    }

    #[inline(always)]
    fn push_active(&mut self, attr_index: usize) {
        self.scratch.active.push(attr_index);
        self.active_max = Some(
            self.active_max
                .map_or(attr_index, |max| max.max(attr_index)),
        );
    }

    #[inline(always)]
    fn remove_active(&mut self, attr_index: usize) {
        let Some(index) = self
            .scratch
            .active
            .iter()
            .position(|&index| index == attr_index)
        else {
            return;
        };
        self.scratch.active.swap_remove(index);
    }

    #[inline(always)]
    pub(super) fn colors_for(&mut self, char_index: usize) -> [[f32; 4]; 4] {
        if self.next_end < self.scratch.end_order.len()
            && attr_end(&self.attributes[self.scratch.end_order[self.next_end]]) <= char_index
        {
            loop {
                let attr_index = self.scratch.end_order[self.next_end];
                self.remove_active(attr_index);
                self.next_end += 1;
                if self.next_end == self.scratch.end_order.len()
                    || attr_end(&self.attributes[self.scratch.end_order[self.next_end]])
                        > char_index
                {
                    break;
                }
            }
            // Intermediate winners are never observed while expiring a group.
            if self
                .active_max
                .is_some_and(|index| attr_end(&self.attributes[index]) <= char_index)
            {
                self.active_max = self.scratch.active.iter().copied().max();
            }
        }

        while self.next_start < self.scratch.start_order.len()
            && self.attributes[self.scratch.start_order[self.next_start]].start <= char_index
        {
            let attr_index = self.scratch.start_order[self.next_start];
            let attr = &self.attributes[attr_index];
            if char_index < attr_end(attr) {
                self.push_active(attr_index);
            }
            self.next_start += 1;
        }

        self.active_max
            .map(|index| self.attributes[index].colors())
            .unwrap_or([[1.0; 4]; 4])
    }
}

pub(super) fn clip_objects_range_to_world_rect(
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
        let mut object = objects.take_object(read);
        let keep = clip_sprite_object_to_world_rect_with_recycled(
            &mut object,
            sprite_instances,
            clip,
            Some(&mut *recycled_vertices),
        );
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
        EditablePayload::Sprite(index) => {
            let sprite = sprite_instances[*index as usize];
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

            if left >= clip.left && right <= clip.right && bottom >= clip.bottom && top <= clip.top
            {
                return Some(ClippedSpriteObject {
                    object_type: EditablePayload::Sprite(*index),
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
                object_type: EditablePayload::Sprite(*index),
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
        EditablePayload::TexturedMesh {
            instance,
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
            clip_textured_mesh_to_world_rect(
                instance.tint,
                vertices,
                transform,
                instance.uv_scale,
                instance.uv_offset,
                instance.uv_tex_shift,
                clip,
                instance.texture_mask != 0.0,
                recycled_vertices,
            )
        }
        EditablePayload::Mesh { .. } => unreachable!("callers keep colored meshes unchanged"),
    }
}

impl TextAttrScratch {
    pub(super) fn heap_bytes(&self) -> usize {
        [&self.start_order, &self.end_order, &self.active]
            .into_iter()
            .filter(|indices| indices.spilled())
            .map(|indices| indices.capacity() * std::mem::size_of::<usize>())
            .sum()
    }
}
