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
    parse_explosion_animation,
};
pub use parts::{
    NOTE_ANIM_PART_COUNT, NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType, NoteDisplayMetrics,
    NotePartAnimation, NotePartTextureTranslate, Quantization, Style,
};
pub use receptor::{
    ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior, ReceptorReverseState,
    ReceptorStepBehavior, ReceptorStepBehaviors,
};
pub use runtime::{
    HoldVisuals, NoteskinRuntime, TapExplosion, TapExplosionLayer, bright_tap_explosion_key,
};
pub use script::{ItgCommandEffect, model_draw_program};
pub use sprite::{
    AnimationRate, SpriteDefinition, duration_frame_index, frame_duration_total, neg_rot_sin_cos,
};
