use super::{AnimationRate, SpriteSlot, SpriteSource};
use crate::assets;
use deadsync_noteskin::script::sprite_state_properties_command_plans;
#[cfg(test)]
use deadsync_noteskin::script::sprite_state_properties_plans;
use deadsync_noteskin::sprite_state_properties_animation;
use std::sync::{Arc, atomic::AtomicU64};

fn slot_is_beat_based(slot: &SpriteSlot) -> bool {
    matches!(
        slot.source.as_ref(),
        SpriteSource::Animated {
            rate: AnimationRate::FramesPerBeat(_),
            ..
        }
    )
}

fn itg_apply_slot_state_properties(
    slot: &mut SpriteSlot,
    frame_count: usize,
    frame_delays: &[f32],
    beat_based: bool,
) {
    if slot.model.is_some() {
        return;
    }
    let (texture_key, tex_dims) = match slot.source.as_ref() {
        SpriteSource::Atlas {
            texture_key,
            tex_dims,
            ..
        }
        | SpriteSource::Animated {
            texture_key,
            tex_dims,
            ..
        } => (texture_key.clone(), *tex_dims),
    };
    let (grid_x, grid_y) = assets::sprite_sheet_dims(&texture_key);
    let Some(animation) = sprite_state_properties_animation(
        [tex_dims.0, tex_dims.1],
        [grid_x as usize, grid_y as usize],
        slot.def.src,
        frame_count,
        frame_delays,
        beat_based,
    ) else {
        return;
    };

    slot.source = Arc::new(SpriteSource::Animated {
        texture_key: texture_key.clone(),
        tex_dims,
        frame_size: animation.frame_size,
        grid: (grid_x.max(1) as usize, grid_y.max(1) as usize),
        frame_count: animation.frame_count,
        frame_indices: None,
        rate: animation.rate,
        frame_durations: Some(Arc::<[f32]>::from(animation.frame_durations)),
        cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
        cached_generation: AtomicU64::new(u64::MAX),
    });
    slot.def.src = animation.start_src;
    slot.def.size = animation.frame_size;
    let source_frame =
        assets::texture_source_frame_dims_from_real(&texture_key, tex_dims.0, tex_dims.1);
    slot.source_size = [source_frame.0 as i32, source_frame.1 as i32];
}

pub(super) fn itg_apply_state_properties_from_commands(
    slot: &mut SpriteSlot,
    commands: &std::collections::HashMap<String, String>,
) {
    let (beat_based, plans) =
        sprite_state_properties_command_plans(commands, slot_is_beat_based(slot));
    for plan in plans {
        itg_apply_slot_state_properties(slot, plan.frame_count, &plan.frame_delays, beat_based);
    }
}

#[cfg(test)]
pub(super) fn itg_apply_state_properties_from_script(
    slot: &mut SpriteSlot,
    script: &str,
    beat_based: bool,
) {
    for plan in sprite_state_properties_plans(script) {
        itg_apply_slot_state_properties(slot, plan.frame_count, &plan.frame_delays, beat_based);
    }
}
