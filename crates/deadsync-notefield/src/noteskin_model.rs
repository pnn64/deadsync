use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_render::{BlendMode, TMeshCacheKey, TexturedMeshVertex};
use deadsync_noteskin::{
    ModelDrawState, ModelMesh, ModelTweenCursor, NoteskinSlot, model_vertex_for_sprite,
};
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4};
use std::sync::Arc;

const MIN_SLOT_LOOKUP_SIZE: usize = 2;

#[derive(Clone, Copy, Default)]
struct SlotLookup {
    stable_id: u64,
    slot_index: usize,
}

struct SlotFrameState {
    geometry: Option<(TMeshCacheKey, Arc<[TexturedMeshVertex]>)>,
    draw: ModelDrawState,
    draw_time: u32,
    draw_beat: u32,
    draw_frame: u64,
    tween_cursor: ModelTweenCursor,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelMeshCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub saturated_misses: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoteskinFrameCacheStats {
    pub hits: u64,
    pub evaluations: u64,
    pub unregistered_misses: u64,
    pub saturated_misses: u64,
}

/// Per-player notefield model geometry cache.
///
/// Owner: gameplay screen logic, one cache per active player notefield.
/// Threading: single game/render frame path; callers hold it behind `RefCell`.
/// Lifetime: gameplay screen/session, cleared or rebuilt at song transitions.
/// Capacity: caller-sized dense slot states and a lookup table kept at or below
/// 50% load. Before sealing, both grow when the initial estimate is too small.
/// Warmup: the gameplay adapter registers every noteskin slot and then seals
/// growth before live play. It supplies the exact unique slot count so normal
/// prewarming does not reallocate or rehash.
/// Miss: before sealing, registers the slot and prebuilds model geometry. A
/// sealed draw miss evaluates directly and is instrumented without retaining
/// state; the existing actor fallback may rebuild geometry to preserve output.
/// Eviction: none during gameplay. After sealing, unknown slots count a
/// `saturated_misses` event instead of growing or pruning.
/// Destruction: entries drop with the gameplay screen or explicit cache clear.
/// Instrumentation: misses and saturation are always collected to diagnose
/// failed prewarming. Geometry hits, frame hits, and draw evaluations are
/// opt-in so ordinary frames do not write diagnostic metadata.
/// Worst-case registered-slot frame cost: one draw evaluation per unique
/// `(slot, time, beat)` tuple; no allocation, pruning, or geometry conversion.
/// A sealed unregistered model is a counted prewarm failure and retains the
/// legacy conversion fallback for visual correctness.
/// Identity invariant: stable IDs are unique within the noteskin runtime.
pub struct ModelMeshCache {
    slots: Vec<SlotFrameState>,
    lookup: Box<[SlotLookup]>,
    frame: u64,
    sealed: bool,
    collect_hit_stats: bool,
    stats: ModelMeshCacheStats,
    frame_stats: NoteskinFrameCacheStats,
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
            slots: Vec::with_capacity(capacity),
            lookup: vec![SlotLookup::default(); lookup_size_for_slots(capacity)].into_boxed_slice(),
            frame: 1,
            sealed: false,
            collect_hit_stats: false,
            stats: ModelMeshCacheStats::default(),
            frame_stats: NoteskinFrameCacheStats::default(),
        }
    }

    #[inline(always)]
    pub const fn stats(&self) -> ModelMeshCacheStats {
        self.stats
    }

    #[inline(always)]
    pub fn reset_stats(&mut self) {
        self.stats = ModelMeshCacheStats::default();
        self.frame_stats = NoteskinFrameCacheStats::default();
    }

    /// Start or stop optional hit/evaluation collection and reset those counters.
    #[inline(always)]
    pub fn begin_hit_stats(&mut self, enabled: bool) {
        self.collect_hit_stats = enabled;
        self.stats.hits = 0;
        self.frame_stats.hits = 0;
        self.frame_stats.evaluations = 0;
    }

    #[inline(always)]
    pub const fn frame_stats(&self) -> NoteskinFrameCacheStats {
        self.frame_stats
    }

    #[inline(always)]
    pub fn begin_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1).max(1);
    }

    #[inline(always)]
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    #[inline(always)]
    pub fn prewarm_slot<S: NoteskinSlot>(&mut self, slot: &S) -> bool {
        if slot.model().is_some() {
            let _ = self.get_or_insert_slot(slot);
        } else {
            let _ = self.ensure_slot(slot);
        }
        let retained = self.find_slot(slot.stable_id()).is_some();
        if !retained {
            self.frame_stats.saturated_misses = self.frame_stats.saturated_misses.saturating_add(1);
        }
        retained
    }

    #[inline(always)]
    pub fn draw_at<S: NoteskinSlot>(&mut self, slot: &S, time: f32, beat: f32) -> ModelDrawState {
        let Some(index) = self
            .find_slot(slot.stable_id())
            .or_else(|| self.ensure_slot(slot))
        else {
            self.frame_stats.unregistered_misses =
                self.frame_stats.unregistered_misses.saturating_add(1);
            self.frame_stats.saturated_misses = self.frame_stats.saturated_misses.saturating_add(1);
            return slot.model_draw_at(time, beat);
        };
        let state = &mut self.slots[index];
        if state.draw_frame == self.frame
            && state.draw_time == time.to_bits()
            && state.draw_beat == beat.to_bits()
        {
            if self.collect_hit_stats {
                self.frame_stats.hits = self.frame_stats.hits.saturating_add(1);
            }
            return state.draw;
        }
        let draw = slot.model_draw_at_cursor(time, beat, &mut state.tween_cursor);
        state.draw = draw;
        state.draw_time = time.to_bits();
        state.draw_beat = beat.to_bits();
        state.draw_frame = self.frame;
        if self.collect_hit_stats {
            self.frame_stats.evaluations = self.frame_stats.evaluations.saturating_add(1);
        }
        draw
    }

    #[inline(always)]
    fn get_or_insert_slot<S: NoteskinSlot>(
        &mut self,
        slot: &S,
    ) -> Option<(TMeshCacheKey, Arc<[TexturedMeshVertex]>)> {
        slot.model()?;
        if let Some(index) = self.find_slot(slot.stable_id())
            && let Some((key, vertices)) = &self.slots[index].geometry
        {
            if self.collect_hit_stats {
                self.stats.hits = self.stats.hits.saturating_add(1);
            }
            return Some((*key, vertices.clone()));
        }
        Some(self.get_or_insert_with(slot, || build_model_geometry(slot)))
    }

    #[inline(always)]
    fn get_or_insert_with<S, F>(
        &mut self,
        slot: &S,
        build: F,
    ) -> (TMeshCacheKey, Arc<[TexturedMeshVertex]>)
    where
        S: NoteskinSlot,
        F: FnOnce() -> Arc<[TexturedMeshVertex]>,
    {
        let stable_id = slot.stable_id();
        let geom_cache_key = hashed_model_cache_key(stable_id);
        if let Some(index) = self.find_slot(stable_id)
            && let Some((_, vertices)) = &self.slots[index].geometry
        {
            if self.collect_hit_stats {
                self.stats.hits = self.stats.hits.saturating_add(1);
            }
            return (geom_cache_key, vertices.clone());
        }
        self.stats.misses = self.stats.misses.saturating_add(1);
        let vertices = build();
        if let Some(index) = self.register_slot(slot, Some((geom_cache_key, vertices.clone()))) {
            if self.slots[index].geometry.is_none() {
                self.slots[index].geometry = Some((geom_cache_key, vertices.clone()));
            }
        } else {
            self.stats.saturated_misses = self.stats.saturated_misses.saturating_add(1);
        }
        (geom_cache_key, vertices)
    }

    #[inline(always)]
    fn ensure_slot<S: NoteskinSlot>(&mut self, slot: &S) -> Option<usize> {
        if let Some(index) = self.find_slot(slot.stable_id()) {
            return Some(index);
        }
        if self.sealed {
            return None;
        }
        let geometry = slot.model().map(|_| {
            (
                hashed_model_cache_key(slot.stable_id()),
                build_model_geometry(slot),
            )
        });
        if geometry.is_some() {
            self.stats.misses = self.stats.misses.saturating_add(1);
        }
        self.register_slot(slot, geometry)
    }

    fn register_slot<S: NoteskinSlot>(
        &mut self,
        slot: &S,
        geometry: Option<(TMeshCacheKey, Arc<[TexturedMeshVertex]>)>,
    ) -> Option<usize> {
        let stable_id = slot.stable_id();
        if let Some(index) = self.find_slot(stable_id) {
            return Some(index);
        }
        if self.sealed {
            return None;
        }
        self.ensure_lookup_capacity(self.slots.len() + 1);
        let index = self.slots.len();
        self.slots.push(SlotFrameState {
            geometry,
            draw: ModelDrawState::default(),
            draw_time: 0,
            draw_beat: 0,
            draw_frame: 0,
            tween_cursor: ModelTweenCursor::default(),
        });
        insert_lookup_entry(
            &mut self.lookup,
            SlotLookup {
                stable_id,
                slot_index: index,
            },
        );
        Some(index)
    }

    fn ensure_lookup_capacity(&mut self, slot_count: usize) {
        let lookup_size = lookup_size_for_slots(slot_count);
        if lookup_size <= self.lookup.len() {
            return;
        }

        let mut lookup = vec![SlotLookup::default(); lookup_size].into_boxed_slice();
        for entry in self.lookup.iter().copied() {
            if entry.stable_id != 0 {
                insert_lookup_entry(&mut lookup, entry);
            }
        }
        self.lookup = lookup;
    }

    #[inline(always)]
    fn find_slot(&self, stable_id: u64) -> Option<usize> {
        let lookup_mask = self.lookup.len() - 1;
        let mut cell = slot_lookup_start(stable_id, lookup_mask);
        loop {
            let entry = self.lookup[cell];
            if entry.stable_id == stable_id {
                return Some(entry.slot_index);
            }
            if entry.stable_id == 0 {
                return None;
            }
            cell = (cell + 1) & lookup_mask;
        }
    }
}

fn lookup_size_for_slots(slot_count: usize) -> usize {
    slot_count
        .checked_mul(2)
        .expect("noteskin slot cache capacity overflow")
        .max(MIN_SLOT_LOOKUP_SIZE)
        .checked_next_power_of_two()
        .expect("noteskin slot lookup capacity overflow")
}

fn insert_lookup_entry(lookup: &mut [SlotLookup], entry: SlotLookup) {
    let lookup_mask = lookup.len() - 1;
    let mut cell = slot_lookup_start(entry.stable_id, lookup_mask);
    loop {
        if lookup[cell].stable_id == 0 {
            lookup[cell] = entry;
            return;
        }
        cell = (cell + 1) & lookup_mask;
    }
}

#[inline(always)]
fn slot_lookup_start(stable_id: u64, lookup_mask: usize) -> usize {
    let mixed = stable_id.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    ((mixed ^ (mixed >> 32)) as usize) & lookup_mask
}

#[inline(always)]
fn hashed_model_cache_key(stable_id: u64) -> TMeshCacheKey {
    let mixed = stable_id.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    (mixed ^ (mixed >> 32)).max(1)
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

#[cfg(feature = "bench-support")]
mod bench_support {
    use super::*;
    use deadsync_noteskin::{
        ModelEffectState, ModelTweenSegment, ModelVertex, SpriteDefinition, TweenType,
        model_draw_at, model_draw_at_cursor,
    };
    use std::collections::HashMap;
    use std::hash::BuildHasherDefault;
    use twox_hash::XxHash64;

    const UNIQUE_SLOTS: usize = 8;
    const VISIBLE_NOTES: usize = 512;
    const TIMELINE_SEGMENTS: usize = 64;

    struct BenchSlot {
        id: u64,
        def: SpriteDefinition,
        model: ModelMesh,
        timeline: Arc<[ModelTweenSegment]>,
        texture: Arc<str>,
    }

    impl NoteskinSlot for BenchSlot {
        fn sprite_def(&self) -> &SpriteDefinition {
            &self.def
        }

        fn source_size(&self) -> [i32; 2] {
            [64, 64]
        }

        fn texture_key_shared(&self) -> Arc<str> {
            self.texture.clone()
        }

        fn model(&self) -> Option<&ModelMesh> {
            Some(&self.model)
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

        fn model_draw_at(&self, time: f32, beat: f32) -> ModelDrawState {
            model_draw_at(
                ModelDrawState::default(),
                &self.timeline,
                ModelEffectState::default(),
                0.0,
                &[],
                time,
                beat,
            )
        }

        fn stable_id(&self) -> u64 {
            self.id
        }

        fn model_draw_at_cursor(
            &self,
            time: f32,
            beat: f32,
            cursor: &mut ModelTweenCursor,
        ) -> ModelDrawState {
            model_draw_at_cursor(
                ModelDrawState::default(),
                &self.timeline,
                ModelEffectState::default(),
                0.0,
                &[],
                [time, beat],
                cursor,
            )
        }

        fn model_glow_with_draw(
            &self,
            _draw: ModelDrawState,
            _time: f32,
            _beat: f32,
            _diffuse_alpha: f32,
        ) -> Option<[f32; 4]> {
            None
        }

        fn model_uv_params(&self, _uv_rect: [f32; 4]) -> ([f32; 2], [f32; 2], [f32; 2]) {
            ([1.0; 2], [0.0; 2], [0.0; 2])
        }
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SlotFrameBenchOutput {
        pub checksum: u64,
        pub draws: u64,
        pub geometry_clones: u64,
    }

    pub struct SlotFrameBench {
        slots: Vec<BenchSlot>,
        old_geometry: HashMap<u64, Arc<[TexturedMeshVertex]>, BuildHasherDefault<XxHash64>>,
        cache: ModelMeshCache,
    }

    impl Default for SlotFrameBench {
        fn default() -> Self {
            let slots: Vec<_> = (0..UNIQUE_SLOTS)
                .map(|index| bench_slot(index as u64 + 1))
                .collect();
            let old_geometry = slots
                .iter()
                .map(|slot| (slot.id, build_model_geometry(slot)))
                .collect();
            let mut cache = ModelMeshCache::with_capacity(UNIQUE_SLOTS);
            for slot in &slots {
                cache.prewarm_slot(slot);
            }
            cache.seal();
            cache.reset_stats();
            Self {
                slots,
                old_geometry,
                cache,
            }
        }
    }

    impl SlotFrameBench {
        pub fn old_frame(&mut self, frame: usize) -> SlotFrameBenchOutput {
            let time = frame as f32 / 120.0;
            let beat = time * 4.0;
            let mut output = SlotFrameBenchOutput::default();
            for note in 0..VISIBLE_NOTES {
                let slot = &self.slots[note % self.slots.len()];
                let draw = slot.model_draw_at(time, beat);
                let geometry = self.old_geometry.get(&slot.id).expect("prewarmed").clone();
                mix_output(&mut output, draw, geometry.len());
            }
            output
        }

        pub fn new_frame(&mut self, frame: usize) -> SlotFrameBenchOutput {
            let time = frame as f32 / 120.0;
            let beat = time * 4.0;
            self.cache.begin_frame();
            let mut output = SlotFrameBenchOutput::default();
            for note in 0..VISIBLE_NOTES {
                let slot = &self.slots[note % self.slots.len()];
                let draw = self.cache.draw_at(slot, time, beat);
                let (_, geometry) = self.cache.get_or_insert_slot(slot).expect("prewarmed");
                mix_output(&mut output, draw, geometry.len());
            }
            output
        }

        pub fn fixed_bytes(&self) -> usize {
            self.cache.slots.capacity() * std::mem::size_of::<SlotFrameState>()
                + self.cache.lookup.len() * std::mem::size_of::<SlotLookup>()
        }

        pub const fn visible_notes() -> usize {
            VISIBLE_NOTES
        }

        pub const fn unique_slots() -> usize {
            UNIQUE_SLOTS
        }
    }

    fn bench_slot(id: u64) -> BenchSlot {
        let mut timeline = Vec::with_capacity(TIMELINE_SEGMENTS);
        let mut from = ModelDrawState::default();
        for segment in 0..TIMELINE_SEGMENTS {
            let to = ModelDrawState {
                pos: [segment as f32 * 0.25, id as f32, 0.0],
                rot: [0.0, 0.0, segment as f32 * 3.0],
                zoom: [1.0 + segment as f32 * 0.001, 1.0, 1.0],
                ..from
            };
            timeline.push(ModelTweenSegment {
                start: segment as f32 * 0.05,
                duration: 0.05,
                tween: TweenType::Linear,
                from,
                to,
            });
            from = to;
        }
        BenchSlot {
            id,
            def: SpriteDefinition::default(),
            model: ModelMesh {
                vertices: Arc::from([ModelVertex {
                    pos: [0.0; 3],
                    uv: [0.0; 2],
                    tex_matrix_scale: [1.0; 2],
                }]),
                bounds: [0.0, 0.0, 0.0, 64.0, 64.0, 0.0],
            },
            timeline: Arc::from(timeline),
            texture: Arc::from("slot-frame-bench"),
        }
    }

    #[inline(always)]
    fn mix_output(output: &mut SlotFrameBenchOutput, draw: ModelDrawState, geometry_len: usize) {
        output.checksum = output.checksum.rotate_left(7)
            ^ draw.pos[0].to_bits() as u64
            ^ ((draw.rot[2].to_bits() as u64) << 32)
            ^ geometry_len as u64;
        output.draws += 1;
        output.geometry_clones += 1;
    }
}

#[cfg(feature = "bench-support")]
pub use bench_support::{SlotFrameBench, SlotFrameBenchOutput};

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
    geom_cache_key: TMeshCacheKey,
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
    let (geom_cache_key, vertices) = cache.get_or_insert_slot(slot)?;
    let (uv_scale, uv_offset, uv_tex_shift) = slot.model_uv_params(uv_rect);
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
    geom_cache_key: TMeshCacheKey,
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
        geom_cache_key,
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
    fn cached_geometry_reuses_stable_slot_key() {
        let slot = TestSlot::model();
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
        assert_eq!(cache.stats().hits, 0);
        cache.begin_hit_stats(true);
        let (key_c, verts_c) =
            cache.get_or_insert_with(&slot, || panic!("retained geometry must remain cached"));

        assert_eq!(builds, 1);
        assert_eq!(key_a, hashed_model_cache_key(slot.stable_id()));
        assert_eq!(key_a, key_b);
        assert_eq!(key_a, key_c);
        assert!(Arc::ptr_eq(&verts_a, &verts_b));
        assert!(Arc::ptr_eq(&verts_a, &verts_c));
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
    fn cache_grows_beyond_its_initial_capacity_without_rebuilding_geometry() {
        const SLOT_COUNT: usize = 700;
        let slots: Vec<Box<TestSlot>> = (0..SLOT_COUNT)
            .map(|_| Box::new(TestSlot::model()))
            .collect();
        let mut cache = ModelMeshCache::with_capacity(1);
        cache.begin_hit_stats(true);
        let mut builds = 0usize;

        for slot in &slots {
            cache.get_or_insert_with(slot.as_ref(), || {
                builds += 1;
                Arc::from([TexturedMeshVertex::default()])
            });
        }
        assert_eq!(builds, SLOT_COUNT);
        assert_eq!(cache.slots.len(), SLOT_COUNT);
        assert!(cache.lookup.len() >= SLOT_COUNT * 2);
        assert!(cache.lookup.len().is_power_of_two());
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 0,
                misses: SLOT_COUNT as u64,
                saturated_misses: 0,
            }
        );

        cache.get_or_insert_with(slots[0].as_ref(), || {
            panic!("retained geometry must remain cached after growth")
        });
        cache.get_or_insert_with(slots[SLOT_COUNT - 1].as_ref(), || {
            panic!("the last geometry must remain cached after growth")
        });
        assert_eq!(builds, SLOT_COUNT);
        assert_eq!(cache.slots.len(), SLOT_COUNT);
        assert_eq!(
            cache.stats(),
            ModelMeshCacheStats {
                hits: 2,
                misses: SLOT_COUNT as u64,
                saturated_misses: 0,
            }
        );

        cache.reset_stats();
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
        assert_eq!(cache.slots.len(), SLOT_COUNT);
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
    fn prewarm_registers_non_model_slots_without_geometry() {
        let mut slot = TestSlot::model();
        slot.model = None;
        let mut cache = ModelMeshCache::with_capacity(1);

        cache.prewarm_slot(&slot);

        assert_eq!(cache.slots.len(), 1);
        assert!(cache.slots[0].geometry.is_none());
        assert_eq!(cache.stats(), ModelMeshCacheStats::default());
    }

    #[test]
    fn frame_cache_evaluates_each_slot_time_once_per_frame() {
        let slot = TestSlot::model();
        let mut cache = ModelMeshCache::with_capacity(1);
        cache.prewarm_slot(&slot);
        cache.seal();
        cache.reset_stats();
        cache.begin_frame();

        let first = cache.draw_at(&slot, 1.25, 4.0);
        let second = cache.draw_at(&slot, 1.25, 4.0);

        assert_eq!(first.pos, second.pos);
        assert_eq!(cache.frame_stats(), NoteskinFrameCacheStats::default());
        cache.begin_hit_stats(true);
        cache.begin_frame();
        let first = cache.draw_at(&slot, 1.25, 4.0);
        let second = cache.draw_at(&slot, 1.25, 4.0);
        assert_eq!(first.pos, second.pos);
        assert_eq!(
            cache.frame_stats(),
            NoteskinFrameCacheStats {
                hits: 1,
                evaluations: 1,
                unregistered_misses: 0,
                saturated_misses: 0,
            }
        );
        cache.begin_frame();
        let _ = cache.draw_at(&slot, 1.25, 4.0);
        assert_eq!(cache.frame_stats().evaluations, 2);
    }

    #[test]
    fn sealed_frame_cache_saturates_without_allocating_or_retaining() {
        let registered = TestSlot::model();
        let missed = TestSlot::model();
        let mut cache = ModelMeshCache::with_capacity(1);
        cache.prewarm_slot(&registered);
        cache.seal();
        cache.reset_stats();
        cache.begin_frame();

        let expected = missed.model_draw_at(2.0, 8.0);
        let actual = cache.draw_at(&missed, 2.0, 8.0);

        assert_eq!(actual.pos, expected.pos);
        assert_eq!(cache.slots.len(), 1);
        assert_eq!(cache.frame_stats().unregistered_misses, 1);
        assert_eq!(cache.frame_stats().saturated_misses, 1);
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
            geom_cache_key,
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
        assert_eq!(geom_cache_key, deadlib_render::INVALID_TMESH_CACHE_KEY);
        assert_eq!(uv_scale, [0.7, 0.7]);
        assert_eq!(uv_offset, [0.1, 0.2]);
        assert_eq!(uv_tex_shift, [0.125, 0.25]);
        assert!(!depth_test);
        assert!(visible);
        assert_eq!(blend, BlendMode::Add);
        assert_eq!(z, 47);
    }

    #[test]
    fn cached_actor_reuses_geometry_and_nonzero_key() {
        let slot = TestSlot::model();
        let mut cache = ModelMeshCache::default();
        cache.begin_hit_stats(true);
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
                geom_cache_key: key_a,
                ..
            },
            Actor::TexturedMesh {
                vertices: vertices_b,
                geom_cache_key: key_b,
                ..
            },
        ) = (actor_a, actor_b)
        else {
            panic!("expected cached textured mesh actors");
        };
        assert_ne!(key_a, deadlib_render::INVALID_TMESH_CACHE_KEY);
        assert_eq!(key_a, key_b);
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
            77,
        )
        .expect("depth-sorted actor should build");

        let Actor::TexturedMesh {
            local_transform,
            vertices: actual_vertices,
            geom_cache_key,
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
        assert_eq!(geom_cache_key, 77);
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
                1,
            )
            .is_none()
        );
    }
}
