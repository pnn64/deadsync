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
pub mod parts;
pub mod receptor;

pub use draw::{
    ModelAutoRotKey, ModelDrawState, ModelMesh, ModelTweenSegment, ModelVertex, TweenType,
};
pub use explosion::{
    ExplosionAnimation, ExplosionSegment, ExplosionState, ExplosionVisualState, GlowEffect,
};
pub use parts::{
    NOTE_ANIM_PART_COUNT, NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType, NoteDisplayMetrics,
    NotePartAnimation, NotePartTextureTranslate, Quantization, Style,
};
pub use receptor::{
    ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior, ReceptorReverseState,
    ReceptorStepBehavior, ReceptorStepBehaviors,
};
