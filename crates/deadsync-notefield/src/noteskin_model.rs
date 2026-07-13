use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, TMeshGeometryId, TexturedMeshVertex};
use deadsync_noteskin::{
    ModelDrawState, ModelMesh, ModelVertex, NoteskinSlot, model_vertex_for_sprite,
};
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use twox_hash::XxHash64;

const MODEL_MESH_CACHE_LIMIT: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ModelMeshCacheKey {
    slot: *const (),
}

struct ModelMeshCacheEntry {
    geometry_id: TMeshGeometryId,
    vertices: Arc<[TexturedMeshVertex]>,
    source_vertices: Arc<[ModelVertex]>,
    mirror: [bool; 2],
}

impl ModelMeshCacheEntry {
    #[inline(always)]
    fn matches<S: NoteskinSlot>(&self, slot: &S) -> bool {
        let Some(model) = slot.model() else {
            return false;
        };
        Arc::ptr_eq(&self.source_vertices, &model.vertices)
            && self.mirror == [slot.sprite_def().mirror_h, slot.sprite_def().mirror_v]
    }
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
/// Warmup: the gameplay adapter prewarms visible model slots before live play.
/// Miss: builds CPU vertex data from an already-loaded noteskin slot, with no
/// disk I/O, parsing, GPU upload, or asset registration.
/// Eviction: none during gameplay; once full, insertions saturate and count a
/// `saturated_misses` event instead of pruning.
/// Destruction: entries drop with the gameplay screen or explicit cache clear.
/// Instrumentation: hit, miss, and saturated miss counters.
/// Worst-case frame cost: one bounded model vertex conversion per miss.
/// CPU lookup uses slot addresses only as a fast candidate index. Each hit also
/// validates the retained model storage and mirror flags, so allocator reuse
/// cannot return stale vertices. Backend geometry identity is content-validated
/// and never derives from an address.
pub struct ModelMeshCache {
    entries: HashMap<ModelMeshCacheKey, ModelMeshCacheEntry, BuildHasherDefault<XxHash64>>,
    stats: ModelMeshCacheStats,
}

impl Default for ModelMeshCache {
    fn default() -> Self {
        Self::with_capacity(0)
    }
}

impl ModelMeshCache {
    #[inline(always)]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default()),
            stats: ModelMeshCacheStats::default(),
        }
    }

    #[inline(always)]
    pub const fn stats(&self) -> ModelMeshCacheStats {
        self.stats
    }

    #[inline(always)]
    pub fn reset_stats(&mut self) {
        self.stats = ModelMeshCacheStats::default();
    }

    #[inline(always)]
    pub fn prewarm_slot<S: NoteskinSlot>(&mut self, slot: &S) {
        if slot.model().is_none_or(|model| model.vertices.is_empty()) {
            return;
        }
        let _ = self.get_or_insert_with(slot, || build_model_geometry(slot));
    }

    #[inline(always)]
    fn get_or_insert_slot<S: NoteskinSlot>(
        &mut self,
        slot: &S,
    ) -> Option<(Option<TMeshGeometryId>, Arc<[TexturedMeshVertex]>)> {
        slot.model()
            .map(|_| self.get_or_insert_with(slot, || build_model_geometry(slot)))
    }

    #[inline(always)]
    fn get_or_insert_with<S, F>(
        &mut self,
        slot: &S,
        build: F,
    ) -> (Option<TMeshGeometryId>, Arc<[TexturedMeshVertex]>)
    where
        S: NoteskinSlot,
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        let key = model_cache_key(slot);
        if let Some(entry) = self.entries.get(&key)
            && entry.matches(slot)
        {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return (Some(entry.geometry_id), Arc::clone(&entry.vertices));
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let vertices = build();
        let replaces_stale = self.entries.contains_key(&key);
        if !replaces_stale && self.entries.len() >= MODEL_MESH_CACHE_LIMIT {
            self.stats.saturated_misses = self.stats.saturated_misses.saturating_add(1);
            // Saturation deliberately bypasses backend retention too: without
            // a CPU entry there is nowhere to retain a hash-free identity.
            return (None, vertices);
        }
        if let Some(geometry_id) = TMeshGeometryId::from_content(vertices.as_ref()) {
            let model = slot
                .model()
                .expect("model geometry cache requested for non-model slot");
            self.entries.insert(
                key,
                ModelMeshCacheEntry {
                    geometry_id,
                    vertices: Arc::clone(&vertices),
                    source_vertices: Arc::clone(&model.vertices),
                    mirror: [slot.sprite_def().mirror_h, slot.sprite_def().mirror_v],
                },
            );
            return (Some(geometry_id), vertices);
        }
        (None, vertices)
    }
}

#[inline(always)]
fn model_cache_key<S: NoteskinSlot>(slot: &S) -> ModelMeshCacheKey {
    ModelMeshCacheKey {
        slot: slot as *const S as *const (),
    }
}

#[inline(always)]
fn build_model_geometry<S: NoteskinSlot>(slot: &S) -> Arc<[TexturedMeshVertex]> {
    let model = slot
        .model()
        .expect("model geometry requested for non-model noteskin slot");
    let mut vertices = Vec::with_capacity(model.vertices.len());
    for &vertex in model.vertices.iter() {
        let vertex = model_vertex_for_sprite(slot.sprite_def(), vertex);
        vertices.push(TexturedMeshVertex {
            pos: vertex.pos,
            uv: vertex.uv,
            color: [1.0; 4],
            tex_matrix_scale: vertex.tex_matrix_scale,
        });
    }
    Arc::from(vertices)
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
fn actor_from_vertices<S: NoteskinSlot>(
    slot: &S,
    xy: [f32; 2],
    tint: [f32; 4],
    vertices: Arc<[TexturedMeshVertex]>,
    geometry_id: Option<TMeshGeometryId>,
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
        geometry_id,
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
fn actor_from_draw<S: NoteskinSlot>(
    slot: &S,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    let model = slot.model()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let blend = model_blend(draw, blend);
    let vertices = build_model_geometry(slot);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = model_draw_transform(model.size(), affine);
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        None,
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
pub fn noteskin_model_actor_from_draw<S: NoteskinSlot>(
    slot: &S,
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
pub(crate) fn noteskin_model_actor_from_draw_cached<S: NoteskinSlot>(
    slot: &S,
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
    let model = slot.model()?;
    if !draw.visible || model.vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = model_draw_transform(model.size(), affine);
    let (geometry_id, vertices) = cache.get_or_insert_slot(slot)?;
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        geometry_id,
        local_transform,
        uv_scale,
        uv_offset,
        uv_tex_shift,
        false,
        model_blend(draw, blend),
        z,
    ))
}

pub fn noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry<S: NoteskinSlot>(
    slot: &S,
    draw: ModelDrawState,
    xy: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    rotation_deg: f32,
    color: [f32; 4],
    blend: BlendMode,
    z: i16,
    vertices: Arc<[TexturedMeshVertex]>,
    geometry_id: Option<TMeshGeometryId>,
) -> Option<Actor> {
    let model = slot.model()?;
    if !draw.visible || model.vertices.is_empty() || vertices.is_empty() {
        return None;
    }

    let tint = model_tint(color, draw);
    let blend = model_blend(draw, blend);
    let affine = model_affine_transform(model, size, rotation_deg, draw);
    let local_transform = affine * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0));
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
    Some(actor_from_vertices(
        slot,
        xy,
        tint,
        vertices,
        geometry_id,
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
pub fn noteskin_model_actor<S: NoteskinSlot>(
    slot: &S,
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
    use deadsync_noteskin::{ModelVertex, SpriteDefinition};

    struct TestSlot {
        def: SpriteDefinition,
        model: Option<ModelMesh>,
        draw: ModelDrawState,
        texture: Arc<str>,
    }

    impl TestSlot {
        fn model() -> Self {
            Self {
                def: SpriteDefinition::default(),
                model: Some(ModelMesh {
                    vertices: Arc::from([ModelVertex {
                        pos: [2.0, 3.0, 4.0],
                        uv: [0.2, 0.3],
                        tex_matrix_scale: [5.0, 6.0],
                    }]),
                    bounds: [0.0, 0.0, 0.0, 64.0, 64.0, 4.0],
                }),
                draw: ModelDrawState::default(),
                texture: Arc::from("test-model"),
            }
        }
    }

    impl NoteskinSlot for TestSlot {
        fn sprite_def(&self) -> &SpriteDefinition {
            &self.def
        }

        fn source_size(&self) -> [i32; 2] {
            [64, 64]
        }

        fn logical_size(&self) -> [f32; 2] {
            [64.0, 64.0]
        }

        fn texture_key_shared(&self) -> Arc<str> {
            self.texture.clone()
        }

        fn model(&self) -> Option<&ModelMesh> {
            self.model.as_ref()
        }

        fn base_rot_sin_cos(&self) -> [f32; 2] {
            [0.0, 1.0]
        }

        fn frame_index(&self, _time: f32, _beat: f32) -> usize {
            0
        }

        fn frame_index_from_phase(&self, _phase: f32) -> usize {
            0
        }

        fn uv_for_frame_at(&self, _frame_index: usize, _elapsed: f32) -> [f32; 4] {
            [0.0, 0.0, 1.0, 1.0]
        }

        fn model_draw_at(&self, _time: f32, _beat: f32) -> ModelDrawState {
            self.draw
        }

        fn model_glow_with_draw(
            &self,
            draw: ModelDrawState,
            _time: f32,
            _beat: f32,
            diffuse_alpha: f32,
        ) -> Option<[f32; 4]> {
            Some([
                draw.glow[0],
                draw.glow[1],
                draw.glow[2],
                draw.glow[3] * diffuse_alpha,
            ])
        }

        fn model_uv_params(&self, uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
            (
                [uv_rect[2] - uv_rect[0], uv_rect[3] - uv_rect[1]],
                [uv_rect[0], uv_rect[1]],
                [0.125, 0.25],
            )
        }
    }

    fn assert_matrix_eq(actual: Matrix4, expected: Matrix4) {
        for (actual, expected) in actual
            .to_cols_array()
            .into_iter()
            .zip(expected.to_cols_array())
        {
            assert!((actual - expected).abs() <= 1e-5, "{actual} != {expected}");
        }
    }

    fn expected_affine(
        model: &ModelMesh,
        size: [f32; 2],
        rotation_deg: f32,
        draw: ModelDrawState,
    ) -> Matrix4 {
        let model_h = model.size()[1];
        let scale = if model_h > f32::EPSILON && size[1] > f32::EPSILON {
            size[1] / model_h
        } else {
            1.0
        };
        let scale = Vector3::new(
            scale * draw.zoom[0].max(0.0),
            scale * draw.zoom[1].max(0.0),
            scale * draw.zoom[2].max(0.0),
        );
        let align_y = (0.5 - draw.vert_align) * size[1];
        Matrix4::from_translation(Vector3::from_array(draw.pos))
            * Matrix4::from_rotation_x(-draw.rot[0].to_radians())
            * Matrix4::from_rotation_y(-draw.rot[1].to_radians())
            * Matrix4::from_rotation_z((draw.rot[2] + rotation_deg).to_radians())
            * Matrix4::from_translation(Vector3::new(0.0, align_y, 0.0))
            * Matrix4::from_scale(scale)
    }

    #[test]
    fn cached_geometry_reuses_precomputed_identity() {
        let slot = TestSlot::model();
        let mut cache = ModelMeshCache::default();
        let mut builds = 0usize;

        let (id_a, verts_a) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });
        let (id_b, verts_b) = cache.get_or_insert_with(&slot, || {
            builds += 1;
            Arc::from(vec![TexturedMeshVertex::default()])
        });

        assert_eq!(builds, 1);
        assert!(id_a.is_some());
        assert_eq!(id_a, id_b);
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
    fn geometry_identity_does_not_depend_on_slot_address() {
        let slot_a = Box::new(TestSlot::model());
        let slot_b = Box::new(TestSlot::model());
        let mut cache_a = ModelMeshCache::default();
        let mut cache_b = ModelMeshCache::default();

        let (id_a, _) = cache_a
            .get_or_insert_slot(slot_a.as_ref())
            .expect("first model slot should cache");
        let (id_b, _) = cache_b
            .get_or_insert_slot(slot_b.as_ref())
            .expect("second model slot should cache");

        assert_ne!(
            slot_a.as_ref() as *const TestSlot,
            slot_b.as_ref() as *const TestSlot
        );
        assert_eq!(id_a, id_b);
    }

    #[test]
    fn cache_rebuilds_same_address_after_model_replacement() {
        let mut slot = TestSlot::model();
        let mut cache = ModelMeshCache::default();
        let (id_a, vertices_a) = cache
            .get_or_insert_slot(&slot)
            .expect("initial model slot should cache");

        slot.model = Some(ModelMesh {
            vertices: Arc::from([ModelVertex {
                pos: [9.0, 3.0, 4.0],
                uv: [0.2, 0.3],
                tex_matrix_scale: [5.0, 6.0],
            }]),
            bounds: [0.0, 0.0, 0.0, 64.0, 64.0, 4.0],
        });
        let (id_b, vertices_b) = cache
            .get_or_insert_slot(&slot)
            .expect("replacement model slot should cache");

        assert_ne!(id_a, id_b);
        assert_eq!(vertices_a.len(), vertices_b.len());
        assert_eq!(vertices_a[0].pos, [2.0, 3.0, 4.0]);
        assert_eq!(vertices_b[0].pos, [9.0, 3.0, 4.0]);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 0,
                misses: 2,
                saturated_misses: 0,
            }
        );
    }

    #[test]
    fn cached_geometry_ignores_draw_state_changes() {
        let slot = TestSlot::model();
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

    #[test]
    fn cache_saturates_without_evicting_retained_geometry() {
        let slots: Vec<Box<TestSlot>> = (0..=MODEL_MESH_CACHE_LIMIT)
            .map(|_| Box::new(TestSlot::model()))
            .collect();
        let mut cache = ModelMeshCache::with_capacity(MODEL_MESH_CACHE_LIMIT);
        let mut builds = 0usize;

        for slot in &slots {
            cache.get_or_insert_with(slot.as_ref(), || {
                builds += 1;
                Arc::from([TexturedMeshVertex::default()])
            });
        }
        assert_eq!(builds, MODEL_MESH_CACHE_LIMIT + 1);
        assert_eq!(cache.entries.len(), MODEL_MESH_CACHE_LIMIT);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 0,
                misses: 513,
                saturated_misses: 1,
            }
        );

        cache.get_or_insert_with(slots[0].as_ref(), || {
            panic!("retained geometry must remain cached after saturation")
        });
        let (saturated_id, _) =
            cache.get_or_insert_with(slots[MODEL_MESH_CACHE_LIMIT].as_ref(), || {
                builds += 1;
                Arc::from([TexturedMeshVertex::default()])
            });
        assert!(saturated_id.is_none());
        assert_eq!(builds, MODEL_MESH_CACHE_LIMIT + 2);
        assert_eq!(cache.entries.len(), MODEL_MESH_CACHE_LIMIT);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 514,
                saturated_misses: 2,
            }
        );

        cache.reset_stats();
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
        assert_eq!(cache.entries.len(), MODEL_MESH_CACHE_LIMIT);
        cache.get_or_insert_with(slots[0].as_ref(), || {
            panic!("resetting stats must not clear retained geometry")
        });
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 1,
                misses: 0,
                saturated_misses: 0,
            }
        );
    }

    #[test]
    fn prewarm_ignores_slots_without_models() {
        let mut slot = TestSlot::model();
        slot.model = None;
        let mut cache = ModelMeshCache::with_capacity(1);

        cache.prewarm_slot(&slot);

        assert!(cache.entries.is_empty());
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
    }

    #[test]
    fn prewarm_ignores_empty_model_geometry() {
        let mut slot = TestSlot::model();
        slot.model
            .as_mut()
            .expect("test slot should have a model")
            .vertices = Arc::from([]);
        let mut cache = ModelMeshCache::with_capacity(1);

        cache.prewarm_slot(&slot);

        assert!(cache.entries.is_empty());
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
    }

    #[test]
    fn model_geometry_preserves_mirror_and_vertex_fields() {
        let mut slot = TestSlot::model();
        slot.def.mirror_h = true;
        slot.def.mirror_v = true;

        let vertices = build_model_geometry(&slot);
        assert_eq!(vertices.len(), 1);
        assert_eq!(vertices[0].pos, [-2.0, -3.0, 4.0]);
        assert_eq!(vertices[0].uv, [0.8, 0.7]);
        assert_eq!(vertices[0].color, [1.0; 4]);
        assert_eq!(vertices[0].tex_matrix_scale, [5.0, 6.0]);
    }

    #[test]
    fn model_actor_preserves_draw_and_actor_fields() {
        let mut slot = TestSlot::model();
        let draw = ModelDrawState {
            pos: [4.0, 5.0, 6.0],
            rot: [10.0, 20.0, 30.0],
            zoom: [2.0, 0.5, 1.5],
            tint: [0.5, 0.25, 1.0, 0.75],
            vert_align: 0.25,
            blend_add: true,
            ..ModelDrawState::default()
        };
        slot.draw = draw;
        let xy = [120.0, 240.0];
        let size = [96.0, 128.0];
        let uv_rect = [0.1, 0.2, 0.8, 0.9];
        let rotation_deg = 15.0;
        let actor = noteskin_model_actor(
            &slot,
            xy,
            size,
            uv_rect,
            rotation_deg,
            3.0,
            4.0,
            [0.8, 0.4, 0.2, 0.5],
            BlendMode::Alpha,
            47,
        )
        .expect("model actor should build");

        let Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size: actor_size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geometry_id,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
            ..
        } = actor
        else {
            panic!("expected textured mesh actor");
        };
        let expected_transform = Matrix4::from_cols_array(&[
            2.6578522,
            -2.9534407,
            0.0,
            -0.0012019041,
            -0.66446304,
            -0.65436834,
            0.0,
            0.00093999587,
            -1.0260605,
            -0.48952773,
            0.0,
            -0.007229817,
            -17.262817,
            -25.939787,
            0.0,
            1.0144548,
        ]);
        assert_eq!(align, [0.0, 0.0]);
        assert_eq!(offset, xy);
        assert_eq!(world_z, 0.0);
        assert!(matches!(
            actor_size,
            [SizeSpec::Px(width), SizeSpec::Px(height)] if width == 0.0 && height == 0.0
        ));
        assert_matrix_eq(local_transform, expected_transform);
        assert_eq!(texture.as_ref(), "test-model");
        assert_eq!(tint, [0.4, 0.1, 0.2, 0.375]);
        assert_eq!(glow, [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(vertices.len(), 1);
        assert!(geometry_id.is_none());
        assert_eq!(uv_scale, [0.7, 0.7]);
        assert_eq!(uv_offset, [0.1, 0.2]);
        assert_eq!(uv_tex_shift, [0.125, 0.25]);
        assert!(!depth_test);
        assert!(visible);
        assert_eq!(blend, BlendMode::Add);
        assert_eq!(z, 47);
    }

    #[test]
    fn cached_actor_reuses_geometry_and_identity() {
        let slot = TestSlot::model();
        let mut cache = ModelMeshCache::default();
        let actor_a = noteskin_model_actor_from_draw_cached(
            &slot,
            ModelDrawState::default(),
            [10.0, 20.0],
            [64.0, 64.0],
            [0.0, 0.0, 1.0, 1.0],
            0.0,
            [1.0; 4],
            BlendMode::Alpha,
            3,
            &mut cache,
        )
        .expect("first cached actor should build");
        let actor_b = noteskin_model_actor_from_draw_cached(
            &slot,
            ModelDrawState::default(),
            [10.0, 20.0],
            [64.0, 64.0],
            [0.0, 0.0, 1.0, 1.0],
            0.0,
            [0.5; 4],
            BlendMode::Alpha,
            3,
            &mut cache,
        )
        .expect("second cached actor should build");
        let (
            Actor::TexturedMesh {
                vertices: vertices_a,
                geometry_id: id_a,
                ..
            },
            Actor::TexturedMesh {
                vertices: vertices_b,
                geometry_id: id_b,
                ..
            },
        ) = (actor_a, actor_b)
        else {
            panic!("expected cached textured mesh actors");
        };
        assert!(id_a.is_some());
        assert_eq!(id_a, id_b);
        assert!(Arc::ptr_eq(&vertices_a, &vertices_b));
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
    fn depth_sorted_actor_keeps_affine_geometry_contract() {
        let slot = TestSlot::model();
        let draw = ModelDrawState {
            rot: [5.0, 15.0, 25.0],
            ..ModelDrawState::default()
        };
        let vertices: Arc<[TexturedMeshVertex]> = Arc::from([TexturedMeshVertex::default()]);
        let geometry_id = TMeshGeometryId::new(77, vertices.as_ref());
        let actor = noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry(
            &slot,
            draw,
            [1.0, 2.0],
            [64.0, 96.0],
            [0.0, 0.0, 1.0, 1.0],
            35.0,
            [1.0; 4],
            BlendMode::Alpha,
            -9,
            vertices.clone(),
            geometry_id,
        )
        .expect("depth-sorted actor should build");

        let Actor::TexturedMesh {
            local_transform,
            vertices: actual_vertices,
            geometry_id: actual_geometry_id,
            depth_test,
            blend,
            z,
            ..
        } = actor
        else {
            panic!("expected textured mesh actor");
        };
        let affine = expected_affine(
            slot.model().expect("test slot should have a model"),
            [64.0, 96.0],
            35.0,
            draw,
        );
        assert_matrix_eq(
            local_transform,
            affine * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0)),
        );
        assert!(Arc::ptr_eq(&actual_vertices, &vertices));
        assert_eq!(actual_geometry_id, geometry_id);
        assert!(depth_test);
        assert_eq!(blend, BlendMode::Alpha);
        assert_eq!(z, -9);
    }

    #[test]
    fn model_actor_rejects_missing_hidden_and_empty_models() {
        let mut missing = TestSlot::model();
        missing.model = None;
        assert!(
            noteskin_model_actor(
                &missing,
                [0.0; 2],
                [64.0; 2],
                [0.0, 0.0, 1.0, 1.0],
                0.0,
                0.0,
                0.0,
                [1.0; 4],
                BlendMode::Alpha,
                0,
            )
            .is_none()
        );

        let mut hidden = TestSlot::model();
        hidden.draw.visible = false;
        assert!(
            noteskin_model_actor(
                &hidden,
                [0.0; 2],
                [64.0; 2],
                [0.0, 0.0, 1.0, 1.0],
                0.0,
                0.0,
                0.0,
                [1.0; 4],
                BlendMode::Alpha,
                0,
            )
            .is_none()
        );

        let mut empty = TestSlot::model();
        empty
            .model
            .as_mut()
            .expect("test slot should have model")
            .vertices = Arc::from([]);
        assert!(
            noteskin_model_actor(
                &empty,
                [0.0; 2],
                [64.0; 2],
                [0.0, 0.0, 1.0, 1.0],
                0.0,
                0.0,
                0.0,
                [1.0; 4],
                BlendMode::Alpha,
                0,
            )
            .is_none()
        );

        let depth_sorted = TestSlot::model();
        assert!(
            noteskin_model_actor_from_draw_depth_sorted_affine_cached_geometry(
                &depth_sorted,
                ModelDrawState::default(),
                [0.0; 2],
                [64.0; 2],
                [0.0, 0.0, 1.0, 1.0],
                0.0,
                [1.0; 4],
                BlendMode::Alpha,
                0,
                Arc::<[TexturedMeshVertex]>::from([]),
                None,
            )
            .is_none()
        );
    }
}
