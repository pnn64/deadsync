use crate::game::parsing::noteskin::{SpriteSlot, build_model_geometry};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, TMeshCacheKey, TexturedMeshVertex};
use deadsync_noteskin::{ModelDrawState, ModelMesh};
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::Arc;
use twox_hash::XxHash64;

const MODEL_MESH_CACHE_LIMIT: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ModelMeshCacheKey {
    slot: *const SpriteSlot,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelMeshCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub saturated_misses: u64,
}

/// Per-player notefield model geometry cache.
///
/// Owner: gameplay screen logic, one cache per active player notefield.
/// Threading: single game/render frame path; callers hold it behind `RefCell`.
/// Lifetime: gameplay screen/session, cleared or rebuilt at song transitions.
/// Capacity: fixed entry cap; default starts empty, gameplay prewarms with 96.
/// Warmup: `notefield_model_cache_from_assets` prewarms visible model slots.
/// Miss: builds CPU vertex data from an already-loaded `SpriteSlot`, with no
/// disk I/O, parsing, GPU upload, or asset registration.
/// Eviction: none during gameplay; once full, insertions saturate and count a
/// `saturated_misses` event instead of pruning.
/// Destruction: entries drop with the gameplay screen or explicit cache clear.
/// Instrumentation: hit, miss, and saturated miss counters.
/// Worst-case frame cost: one bounded model vertex conversion per miss.
pub(crate) struct ModelMeshCache {
    entries: HashMap<ModelMeshCacheKey, Arc<[TexturedMeshVertex]>, BuildHasherDefault<XxHash64>>,
    stats: ModelMeshCacheStats,
}

impl Default for ModelMeshCache {
    fn default() -> Self {
        Self::with_capacity(0)
    }
}

impl ModelMeshCache {
    #[inline(always)]
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default()),
            stats: ModelMeshCacheStats::default(),
        }
    }

    #[inline(always)]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    #[inline(always)]
    pub(crate) const fn stats(&self) -> ModelMeshCacheStats {
        self.stats
    }

    #[inline(always)]
    pub(crate) fn reset_stats(&mut self) {
        self.stats = ModelMeshCacheStats::default();
    }

    #[inline(always)]
    pub(crate) fn prewarm_slot(&mut self, slot: &SpriteSlot) {
        if slot.model.is_none() {
            return;
        }
        let _ = self.get_or_insert_with(slot, || build_model_geometry(slot));
    }

    #[inline(always)]
    pub(crate) fn get_or_insert_slot(
        &mut self,
        slot: &SpriteSlot,
    ) -> Option<(TMeshCacheKey, Arc<[TexturedMeshVertex]>)> {
        slot.model
            .as_ref()
            .map(|_| self.get_or_insert_with(slot, || build_model_geometry(slot)))
    }

    #[inline(always)]
    pub(crate) fn get_or_insert_with<F>(
        &mut self,
        slot: &SpriteSlot,
        build: F,
    ) -> (TMeshCacheKey, Arc<[TexturedMeshVertex]>)
    where
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        let key = model_cache_key(slot);
        let geom_cache_key = hashed_model_cache_key(&key);
        if let Some(vertices) = self.entries.get(&key) {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return (geom_cache_key, vertices.clone());
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let vertices = build();
        if self.entries.len() < MODEL_MESH_CACHE_LIMIT {
            self.entries.insert(key, vertices.clone());
        } else {
            self.stats.saturated_misses = self.stats.saturated_misses.saturating_add(1);
        }
        (geom_cache_key, vertices)
    }
}

#[inline(always)]
fn model_cache_key(slot: &SpriteSlot) -> ModelMeshCacheKey {
    ModelMeshCacheKey {
        slot: slot as *const SpriteSlot,
    }
}

#[inline(always)]
fn hashed_model_cache_key(key: &ModelMeshCacheKey) -> TMeshCacheKey {
    let mut hasher = XxHash64::default();
    key.hash(&mut hasher);
    hasher.finish().max(1)
}

#[inline(always)]
fn model_uv_params(slot: &SpriteSlot, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let uv_scale = [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]];
    let uv_offset = [uv_rect[0], uv_rect[1]];
    let uv_tex_shift = match slot.source.as_ref() {
        crate::game::parsing::noteskin::SpriteSource::Atlas { tex_dims, .. } => {
            let tw = tex_dims.0.max(1) as f32;
            let th = tex_dims.1.max(1) as f32;
            let base_u0 = slot.def.src[0] as f32 / tw;
            let base_v0 = slot.def.src[1] as f32 / th;
            [uv_offset[0] - base_u0, uv_offset[1] - base_v0]
        }
        crate::game::parsing::noteskin::SpriteSource::Animated { .. } => [0.0, 0.0],
    };
    (uv_scale, uv_offset, uv_tex_shift)
}

#[inline(always)]
fn model_tint(color: [f32; 4], draw: ModelDrawState) -> [f32; 4] {
    [
        color[0] * draw.tint[0],
        color[1] * draw.tint[1],
        color[2] * draw.tint[2],
        color[3] * draw.tint[3],
    ]
}

#[inline(always)]
const fn model_blend(draw: ModelDrawState, blend: BlendMode) -> BlendMode {
    if draw.blend_add {
        BlendMode::Add
    } else {
        blend
    }
}

#[inline(always)]
fn model_draw_transform(model_size: [f32; 2], affine: Matrix4) -> Matrix4 {
    let focal = model_size[0]
        .max(model_size[1])
        .mul_add(6.0, 0.0)
        .max(180.0);
    let inv_focal = focal.recip();
    Matrix4::from_cols(
        Vec4::new(
            affine.x_axis.x,
            -affine.x_axis.y,
            0.0,
            -affine.x_axis.z * inv_focal,
        ),
        Vec4::new(
            affine.y_axis.x,
            -affine.y_axis.y,
            0.0,
            -affine.y_axis.z * inv_focal,
        ),
        Vec4::new(
            affine.z_axis.x,
            -affine.z_axis.y,
            0.0,
            -affine.z_axis.z * inv_focal,
        ),
        Vec4::new(
            affine.w_axis.x,
            -affine.w_axis.y,
            0.0,
            1.0 - affine.w_axis.z * inv_focal,
        ),
    )
}

#[inline(always)]
fn model_affine_transform(
    model: &ModelMesh,
    size: [f32; 2],
    rotation_deg: f32,
    draw: ModelDrawState,
) -> Matrix4 {
    let model_size = model.size();
    let model_h = model_size[1];
    let scale = if model_h > f32::EPSILON && size[1] > f32::EPSILON {
        size[1] / model_h
    } else {
        1.0
    };
    let local_scale = Vector3::new(
        scale * draw.zoom[0].max(0.0),
        scale * draw.zoom[1].max(0.0),
        scale * draw.zoom[2].max(0.0),
    );
    let align_y = (0.5 - draw.vert_align) * size[1];
    Matrix4::from_translation(Vector3::new(draw.pos[0], draw.pos[1], draw.pos[2]))
        * sm_rotation_xyz(draw.rot[0], draw.rot[1], draw.rot[2] + rotation_deg)
        * Matrix4::from_translation(Vector3::new(0.0, align_y, 0.0))
        * Matrix4::from_scale(local_scale)
}

#[inline(always)]
fn sm_rotation_xyz(rot_x_deg: f32, rot_y_deg: f32, rot_z_deg: f32) -> Matrix4 {
    let (sin_x, cos_x) = rot_x_deg.to_radians().sin_cos();
    let (sin_y, cos_y) = rot_y_deg.to_radians().sin_cos();
    let (sin_z, cos_z) = rot_z_deg.to_radians().sin_cos();
    Matrix4::from_cols(
        Vec4::new(
            cos_z * cos_y,
            cos_z * sin_y * sin_x + sin_z * cos_x,
            cos_z * sin_y * cos_x - sin_z * sin_x,
            0.0,
        ),
        Vec4::new(
            -sin_z * cos_y,
            -sin_z * sin_y * sin_x + cos_z * cos_x,
            -sin_z * sin_y * cos_x - cos_z * sin_x,
            0.0,
        ),
        Vec4::new(-sin_y, cos_y * sin_x, cos_y * cos_x, 0.0),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

#[inline(always)]
fn actor_from_vertices(
    slot: &SpriteSlot,
    xy: [f32; 2],
    tint: [f32; 4],
    vertices: Arc<[TexturedMeshVertex]>,
    geom_cache_key: deadlib_render::TMeshCacheKey,
    local_transform: Matrix4,
    uv_scale: [f32; 2],
    uv_offset: [f32; 2],
    uv_tex_shift: [f32; 2],
    depth_test: bool,
    blend: BlendMode,
    z: i16,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: xy,
        world_z: 0.0,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform,
        texture: slot.texture_key_shared(),
        tint,
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices,
        geom_cache_key,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        depth_test,
        visible: true,
        blend,
        z,
    }
}

#[inline(always)]
fn actor_from_draw(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let blend = model_blend(draw, blend);
    let vertices = build_model_geometry(slot);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = model_draw_transform(model.size(), affine);
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        deadlib_render::INVALID_TMESH_CACHE_KEY,
        local_transform,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        false,
        blend,
        z,
    ))
}

#[inline(always)]
pub(crate) fn noteskin_model_actor_from_draw(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    actor_from_draw(slot, draw, xy, size, uv_rect, rotation_deg, color, blend, z)
}

#[inline(always)]
pub(crate) fn noteskin_model_actor_from_draw_cached(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
    cache: &mut ModelMeshCache,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = model_draw_transform(model.size(), affine);
    let (geom_cache_key, vertices) = cache.get_or_insert_slot(slot)?;
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        geom_cache_key,
        local_transform,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        false,
        model_blend(draw, blend),
        z,
    ))
}

pub(crate) fn noteskin_model_actor_from_draw_depth_sorted_affine(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let model = slot.model.as_ref()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let blend = model_blend(draw, blend);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = affine * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));
    let (uv_scale, uv_offset, uv_tex_shift) = model_uv_params(slot, uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        build_model_geometry(slot),
        deadlib_render::INVALID_TMESH_CACHE_KEY,
        local_transform,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        true,
        blend,
        z,
    ))
}

#[inline(always)]
pub(crate) fn noteskin_model_actor(
    slot: &SpriteSlot,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    elapsed: f32,
    beat: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let draw = slot.model_draw_at(elapsed, beat);
    actor_from_draw(slot, draw, xy, size, uv_rect, rotation_deg, color, blend, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::parsing::noteskin::test_model_slot;

    #[test]
    fn cached_geometry_reuses_key_across_tints() {
        let slot = test_model_slot();
        let mut cache = ModelMeshCache::default();
        let mut builds = 0usize;

        let (key_a, verts_a) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });
        let (key_b, verts_b) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });

        assert_eq!(builds, 1);
        assert_eq!(key_a, key_b);
        assert!(Arc::ptr_eq(&verts_a, &verts_b));
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 1,
                saturated_misses: 0,
            }
        );
    }

    #[test]
    fn cached_geometry_ignores_draw_state_changes() {
        let slot = test_model_slot();
        let mut cache = ModelMeshCache::default();
        let mut builds = 0usize;
        let draw = ModelDrawState {
            pos: [12.0, -4.0, 1.0],
            rot: [10.0, 20.0, 30.0],
            zoom: [1.5, 0.75, 2.0],
            vert_align: 0.1,
            ..ModelDrawState::default()
        };

        let (_, verts_a) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });
        let (_, verts_b) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex {
                pos: [draw.pos[0], draw.pos[1], draw.pos[2]],
                ..TexturedMeshVertex::default()
            }])
        });

        assert_eq!(builds, 1);
        assert!(Arc::ptr_eq(&verts_a, &verts_b));
    }
}
