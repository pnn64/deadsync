// Frozen from 7db83fdcf (0.5.1661).
use super::*;

pub(super) fn itg_note_animation_source(
    slot: &SpriteSlot,
    animation: NotePartAnimation,
    translate: NotePartTextureTranslate,
    beat_based: bool,
) -> Option<Arc<SpriteSource>> {
    if slot.model.is_some() || matches!(slot.source.as_ref(), SpriteSource::Animated { .. }) {
        return None;
    }
    let key = slot.texture_key();
    let (grid_x, grid_y) = assets::sprite_sheet_dims(key);
    let (grid_x, grid_y) = (grid_x.max(1) as usize, grid_y.max(1) as usize);
    if grid_x.saturating_mul(grid_y) <= 1 {
        return None;
    }

    let color_x = translate.note_color_spacing[0].abs() > f32::EPSILON;
    let color_y = translate.note_color_spacing[1].abs() > f32::EPSILON;
    if color_x && color_y {
        return None;
    }
    let frame_w = slot.def.size[0].abs().max(1);
    let frame_h = slot.def.size[1].abs().max(1);
    let base_col = (slot.def.src[0].max(0) / frame_w) as usize % grid_x;
    let base_row = (slot.def.src[1].max(0) / frame_h) as usize % grid_y;
    let frame_count = match (color_x, color_y) {
        (false, false) => grid_x * grid_y,
        (true, false) => grid_y,
        (false, true) => grid_x,
        (true, true) => unreachable!(),
    };
    if frame_count <= 1 {
        return None;
    }
    // Explicit empty indices preserve the existing sheet-origin semantics.
    // Small color lanes need only their final Arc, without a temporary Vec.
    let frame_indices = if !color_x && !color_y {
        Arc::clone(&SEQUENTIAL_FRAME_INDICES)
    } else {
        let index = |frame| {
            if color_x {
                frame * grid_x + base_col
            } else {
                base_row * grid_x + frame
            }
        };
        if frame_count <= 64 {
            let mut indices = [0; 64];
            for (frame, value) in indices[..frame_count].iter_mut().enumerate() {
                *value = index(frame);
            }
            Arc::from(&indices[..frame_count])
        } else {
            Arc::from((0..frame_count).map(index).collect::<Vec<_>>())
        }
    };

    let tex_dims = match slot.source.as_ref() {
        SpriteSource::Atlas { tex_dims, .. } => *tex_dims,
        SpriteSource::Animated { .. } => return None,
    };
    let frames_per_cycle = frame_count as f32 / animation.length.max(1e-6);
    Some(Arc::new(animated_source(
        slot.texture_key_shared(),
        tex_dims,
        [frame_w, frame_h],
        (grid_x, grid_y),
        frame_count,
        Some(frame_indices),
        if beat_based {
            AnimationRate::FramesPerBeat(frames_per_cycle)
        } else {
            AnimationRate::FramesPerSecond(frames_per_cycle)
        },
        None,
        &slot.def,
    )))
}
