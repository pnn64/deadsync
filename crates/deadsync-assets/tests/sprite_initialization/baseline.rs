// Frozen from 3e6ac31e2 (0.5.1657); unchanged noteskin planners and metadata helpers are shared.
use super::*;

pub(super) fn source_from_plan(
    plan: SpriteSourcePlan,
    def: &SpriteDefinition,
) -> Arc<SpriteSource> {
    match plan {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        } => Arc::new(atlas_source(texture_key.into(), tex_dims, def)),
        SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
        } => {
            let texel_scale = texture_texel_scale(tex_dims);
            let frame_timing = frame_durations
                .as_deref()
                .map(|durations| SpriteFrameTiming::new(frame_count, durations));
            let uv_cache = SpriteAnimatedUvCache::new(
                texel_scale,
                def,
                frame_size,
                [grid.0, grid.1],
                frame_count,
                frame_indices.is_some(),
            );
            Arc::new(SpriteSource::Animated {
                texture_key: texture_key.into(),
                tex_dims,
                texel_scale,
                frame_size,
                grid,
                frame_count,
                frame_indices: frame_indices.map(|indices| {
                    if indices.is_empty() {
                        Arc::clone(&SEQUENTIAL_FRAME_INDICES)
                    } else {
                        Arc::from(indices)
                    }
                }),
                rate,
                frame_durations: frame_durations.map(Arc::<[f32]>::from),
                frame_timing,
                uv_cache,
                cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
                cached_generation: AtomicU64::new(u64::MAX),
                cached_actor_texture: AtomicU64::new(0),
            })
        }
    }
}

pub(super) fn slot_from_plan(plan: SpriteSlotPlan) -> SpriteSlot {
    let def = plan.def;
    let source = source_from_plan(plan.source, &def);
    SpriteSlot {
        sphere_mapped: false,
        model_animation_length: 1.0,
        model_additive: None,
        stable_id: next_slot_id(),
        def,
        base_rot_sin_cos: [0.0, 1.0],
        source_size: plan.source_size,
        source,
        animation_start_frame: 0,
        animation_start_time: 0.0,
        actor_frame_child: false,
        model_fallback: false,
        uv_velocity: [0.0, 0.0],
        custom_uv: None,
        sprite_mesh: false,
        uv_offset: [0.0, 0.0],
        uv_cycle_seconds: None,
        beat_receptor_start: None,
        note_color_translate: plan.note_color_translate,
        model: None,
        model_draw: ModelDrawState::default(),
        model_timeline: Arc::from(Vec::<ModelTweenSegment>::new()),
        model_effect: ModelEffectState::default(),
        model_auto_rot_total_frames: 0.0,
        model_auto_rot_z_keys: Arc::from(Vec::<ModelAutoRotKey>::new()),
    }
}

pub(super) fn plan_from_slot(slot: &SpriteSlot) -> SpriteSlotPlan {
    SpriteSlotPlan {
        def: slot.def.clone(),
        source_size: slot.source_size,
        source: source_plan_from_slot(slot),
        note_color_translate: slot.note_color_translate,
    }
}

pub(super) fn apply_slot_plan(slot: &mut SpriteSlot, plan: SpriteSlotPlan) {
    slot.def = plan.def;
    slot.source_size = plan.source_size;
    slot.source = source_from_plan(plan.source, &slot.def);
    slot.note_color_translate = plan.note_color_translate;
}

pub(super) fn itg_apply_sprite_animation_plan(
    slot: &mut SpriteSlot,
    plan: deadsync_noteskin::script::SpriteAnimationCommandPlan,
    beat_based: bool,
) {
    if slot.model.is_some() {
        return;
    }
    if let Some(plan) = itg_sprite_animation_slot_plan(
        plan_from_slot(slot),
        plan,
        beat_based,
        assets::sprite_sheet_dims,
        assets::texture_source_frame_dims_from_real,
    ) {
        apply_slot_plan(slot, plan);
    }
}
