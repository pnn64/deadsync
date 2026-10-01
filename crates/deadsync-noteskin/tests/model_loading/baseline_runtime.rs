// Frozen from 1a2129e19 (0.5.1652).
// Share the production compiler to isolate borrowed command selection.
use super::baseline_program::itg_active_model_commands;
use super::*;
use crate::script::model_draw_program;

pub fn itg_slot_with_active_model_draw<T: Clone>(
    slot: &T,
    commands: &HashMap<String, String>,
    active_key: &str,
    mut apply: impl FnMut(&mut T, ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState),
) -> T {
    let mut out = slot.clone();
    let scripted = itg_active_model_commands(commands, active_key);
    let (draw, timeline, effect) = model_draw_program(&scripted);
    apply(&mut out, draw, timeline, effect);
    out
}
