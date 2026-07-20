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
    ModelMesh, ModelTweenCursor, ModelTweenSegment, ModelVertex, TweenType, glowshift_mix,
    model_auto_rot_z_at, model_draw_at, model_draw_at_cursor, model_effect_clock_units,
    model_effect_mix, model_glow_at, model_glow_with_draw, model_texture_uv_params,
};
pub use explosion::{
    ExplosionAnimation, ExplosionSegment, ExplosionState, ExplosionVisualState, GlowEffect,
    itg_direct_tap_explosion_layers, itg_explosion_source, itg_explosion_wrapper,
    parse_explosion_animation,
};
pub use model::{ItgModelSlotPlan, itg_load_model_slots_from_path};
pub use parts::{
    ITG_DANCE_COL_SPACING, NOTE_ANIM_PART_COUNT, NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType,
    NoteDisplayMetrics, NotePartAnimation, NotePartTextureTranslate, Quantization, Style,
    clamped_hold_let_go_gray_percent, itg_column_xs,
};
pub use receptor::{
    ItgReceptorVisuals, ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior,
    ReceptorReverseState, ReceptorStepBehavior, ReceptorStepBehaviors, itg_receptor_pulse_command,
    itg_receptor_reverse_behaviors, itg_receptor_visuals,
};
pub use runtime::{
    HoldVisualParts, HoldVisuals, ItgCompiledSpriteOps, ItgHoldKind, ItgReceptorColumn,
    ItgResolvedSprite, ItgRuntimeColumns, ItgTapNoteColumn, NoteskinRuntime, TapExplosion,
    TapExplosionLayer, bright_tap_explosion_key, default_hold_visuals, default_tap_explosions,
    itg_apply_child_actor_commands, itg_apply_hold_explosions_by_col, itg_apply_loader_command,
    itg_direct_tap_explosion_resolved_layers, itg_first_actor_sprite_slot,
    itg_first_actor_sprite_slot_with_ops, itg_first_resolved_slot_or_fallback,
    itg_hit_mine_explosion_from_layers, itg_hit_mine_explosion_from_slot,
    itg_hold_explosion_from_resolved_layers, itg_hold_head_layers, itg_hold_visual_parts,
    itg_hold_visuals_from_parts, itg_is_common_fallback_hold_explosion_key,
    itg_is_common_noteskin_key, itg_lift_layers_for_col, itg_load_sprite_decl_slot,
    itg_mine_explosion_from_commands, itg_mine_visuals_from_layers, itg_noteskin_runtime_compiled,
    itg_noteskin_runtime_with_ops_compiled, itg_receptor_column,
    itg_receptor_glow_behavior_from_layers, itg_receptor_pulse_from_command,
    itg_resolve_actor_file_compiled, itg_resolve_actor_sprites_compiled,
    itg_resolve_actor_sprites_inner_compiled, itg_resolve_actor_sprites_with_ops_compiled,
    itg_resolve_hold_explosion_slot_compiled, itg_resolve_model_decl, itg_resolve_path_ref_decl,
    itg_resolve_ref_decl, itg_resolve_sprite_decl, itg_resolved_slots_with_model_draw,
    itg_roll_explosion_commands, itg_roll_explosion_from_resolved,
    itg_roll_explosion_from_resolved_layers, itg_roll_explosion_should_use_hold,
    itg_roll_visuals_from_parts, itg_runtime_columns_compiled, itg_slot_with_active_model_draw,
    itg_tap_explosion_map_from_layers, itg_tap_explosion_map_from_resolved_layers,
    itg_tap_explosion_map_from_sources, itg_tap_explosions_by_col_compiled, itg_tap_note_column,
    itg_tap_note_layers,
};
pub use script::{ItgCommandEffect, model_draw_program};
pub use sprite::{
    AnimationRate, NoteskinSlot, SpriteAnimationPlan, SpriteDefinition, SpriteFramePlan,
    SpriteSlotPlan, SpriteSourcePlan, SpriteStatePropertiesAnimation, all_frames_sprite_slot_plan,
    all_state_delays_source_plan, animation_plan_to_slot_plan, animation_sprite_slot_plan,
    atlas_sprite_slot_plan, duration_frame_index, frame_duration_total, frame_sprite_slot_plan,
    generated_animation_sprite_slot_plan, itg_all_frames_sprite_slot_plan_from_path,
    itg_animation_sprite_slot_plan_from_path, itg_frame_sprite_slot_plan_from_path,
    itg_sprite_animation_slot_plan, itg_sprite_slot_plan_from_path, model_vertex_for_sprite,
    neg_rot_sin_cos, sprite_all_frames_animation_plan, sprite_animated_uv, sprite_animation_plan,
    sprite_atlas_uv, sprite_frame_index, sprite_frame_index_from_phase, sprite_scrolled_uv,
    sprite_sheet_frame, sprite_state_properties_animation, sprite_uv_scroll_clock,
    state_properties_source_plan,
};
