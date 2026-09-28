// Frozen from faef9065f; keep independent of production preparation.
use super::super::source_from_plan;
use super::*;
use deadlib_assets as assets;
use deadsync_noteskin::sprite::SpriteSourcePlan;

pub(super) fn animate(
    groups: &mut [Arc<[SpriteSlot]>],
    quantizations: usize,
    animation: NotePartAnimation,
    translate: NotePartTextureTranslate,
    beat_based: bool,
) {
    for group in groups.chunks_exact_mut(quantizations) {
        // Atlas skins share one layer set across quants; Var("Color") skins
        // have distinct sets. Animate each set without replacing its neighbors.
        for shared in group.chunk_by_mut(Arc::ptr_eq) {
            let mut layers = shared[0].as_ref().to_vec();
            for slot in &mut layers {
                apply(slot, animation, translate, beat_based);
            }
            let layers = Arc::<[SpriteSlot]>::from(layers);
            for entry in shared {
                *entry = Arc::clone(&layers);
            }
        }
    }
}

pub(super) fn apply(
    slot: &mut SpriteSlot,
    animation: NotePartAnimation,
    translate: NotePartTextureTranslate,
    beat_based: bool,
) {
    if slot.model.is_some() || matches!(slot.source.as_ref(), SpriteSource::Animated { .. }) {
        return;
    }
    let key = slot.texture_key();
    let (grid_x, grid_y) = assets::sprite_sheet_dims(key);
    let (grid_x, grid_y) = (grid_x.max(1) as usize, grid_y.max(1) as usize);
    if grid_x.saturating_mul(grid_y) <= 1 {
        return;
    }

    let color_x = translate.note_color_spacing[0].abs() > f32::EPSILON;
    let color_y = translate.note_color_spacing[1].abs() > f32::EPSILON;
    if color_x && color_y {
        return;
    }
    let frame_w = slot.def.size[0].abs().max(1);
    let frame_h = slot.def.size[1].abs().max(1);
    let base_col = (slot.def.src[0].max(0) / frame_w) as usize % grid_x;
    let base_row = (slot.def.src[1].max(0) / frame_h) as usize % grid_y;
    let frame_indices = match (color_x, color_y) {
        (false, false) => (0..grid_x * grid_y).collect::<Vec<_>>(),
        (true, false) => (0..grid_y).map(|row| row * grid_x + base_col).collect(),
        (false, true) => (0..grid_x).map(|col| base_row * grid_x + col).collect(),
        (true, true) => unreachable!(),
    };
    if frame_indices.len() <= 1 {
        return;
    }

    let tex_dims = match slot.source.as_ref() {
        SpriteSource::Atlas { tex_dims, .. } => *tex_dims,
        SpriteSource::Animated { .. } => return,
    };
    let frames_per_cycle = frame_indices.len() as f32 / animation.length.max(1e-6);
    slot.source = source_from_plan(
        SpriteSourcePlan::Animated {
            texture_key: key.to_string(),
            tex_dims,
            frame_size: [frame_w, frame_h],
            grid: (grid_x, grid_y),
            frame_count: frame_indices.len(),
            frame_indices: Some(frame_indices),
            rate: if beat_based {
                AnimationRate::FramesPerBeat(frames_per_cycle)
            } else {
                AnimationRate::FramesPerSecond(frames_per_cycle)
            },
            frame_durations: None,
        },
        &slot.def,
    );
}
