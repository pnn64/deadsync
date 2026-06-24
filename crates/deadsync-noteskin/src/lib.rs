//! DeadSync noteskin parsing and resolution.
//!
//! This crate owns renderer-agnostic noteskin data loading. Root gameplay
//! presentation still owns texture registration and actor construction during
//! the migration.

pub mod actor;
pub mod compiled;
pub mod compiler;
pub mod draw;
pub mod explosion;
pub mod itg;
pub mod lua;
pub mod mine;
pub mod model;
pub mod parts;
pub mod receptor;
pub mod runtime;
pub mod script;
pub mod sprite;

pub use draw::{
    ModelAutoRotKey, ModelDrawState, ModelEffectClock, ModelEffectMode, ModelEffectState,
    ModelMesh, ModelTweenSegment, ModelVertex, TweenType, glowshift_mix, model_auto_rot_z_at,
    model_draw_at, model_effect_clock_units, model_effect_mix, model_glow_at, model_glow_with_draw,
};
pub use explosion::{
    ExplosionAnimation, ExplosionSegment, ExplosionState, ExplosionVisualState, GlowEffect,
    itg_explosion_source, itg_explosion_wrapper, parse_explosion_animation,
};
pub use parts::{
    ITG_DANCE_COL_SPACING, NOTE_ANIM_PART_COUNT, NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType,
    NoteDisplayMetrics, NotePartAnimation, NotePartTextureTranslate, Quantization, Style,
    itg_column_xs,
};
pub use receptor::{
    ItgReceptorVisuals, ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior,
    ReceptorReverseState, ReceptorStepBehavior, ReceptorStepBehaviors, itg_receptor_pulse_command,
    itg_receptor_reverse_behaviors, itg_receptor_visuals,
};
pub use runtime::{
    HoldVisualParts, HoldVisuals, ItgTapNoteColumn, NoteskinRuntime, TapExplosion,
    TapExplosionLayer, bright_tap_explosion_key, default_hold_visuals, default_tap_explosions,
    itg_hit_mine_explosion_from_slot, itg_hold_visuals_from_parts,
    itg_is_common_fallback_hold_explosion_key, itg_is_common_noteskin_key, itg_lift_layers_for_col,
    itg_mine_explosion_from_commands, itg_mine_visuals_from_layers, itg_roll_explosion_commands,
    itg_roll_explosion_should_use_hold, itg_roll_visuals_from_parts,
    itg_tap_explosion_map_from_sources, itg_tap_note_base_layer, itg_tap_note_column,
    itg_tap_note_layer_priority,
};
pub use script::{ItgCommandEffect, model_draw_program};
pub use sprite::{
    AnimationRate, SpriteAnimationPlan, SpriteDefinition, SpriteFramePlan,
    SpriteStatePropertiesAnimation, duration_frame_index, frame_duration_total, neg_rot_sin_cos,
    sprite_all_frames_animation_plan, sprite_animated_uv, sprite_animation_plan, sprite_atlas_uv,
    sprite_frame_index, sprite_frame_index_from_phase, sprite_scrolled_uv, sprite_sheet_frame,
    sprite_state_properties_animation, sprite_uv_scroll_clock,
};
