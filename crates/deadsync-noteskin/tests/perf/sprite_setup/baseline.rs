// Frozen from 7754dc3f7 (0.5.1163); only imports/visibility/formatting differ.
use super::super::*;

pub(super) fn itg_animation_sprite_slot_plan_from_path(
    path: &Path,
    frame0: usize,
    frame_count: usize,
    frame_indices: Option<&[usize]>,
    frame_delays: Option<&[f32]>,
    beat_based: bool,
    mut texture_key: impl FnMut(&Path) -> Option<String>,
    mut texture_dimensions: impl FnMut(&str) -> Option<(u32, u32)>,
    mut sprite_sheet_dims: impl FnMut(&str) -> (u32, u32),
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let key = texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (grid_x, grid_y) = sprite_sheet_dims(&key);
    let grid = (grid_x as usize, grid_y as usize);
    let source_frame = source_frame_dims(&key, dims.0, dims.1);
    animation_sprite_slot_plan(
        key.clone(),
        dims,
        grid,
        frame0,
        frame_count,
        frame_indices,
        frame_delays,
        beat_based,
        source_frame,
        true,
    )
    .or_else(|| {
        Some(frame_sprite_slot_plan(
            key,
            dims,
            grid,
            frame0,
            source_frame,
            true,
        ))
    })
}

pub(super) fn itg_all_frames_sprite_slot_plan_from_path(
    path: &Path,
    frame_delay: Option<f32>,
    beat_based: bool,
    mut texture_key: impl FnMut(&Path) -> Option<String>,
    mut texture_dimensions: impl FnMut(&str) -> Option<(u32, u32)>,
    mut sprite_sheet_dims: impl FnMut(&str) -> (u32, u32),
    mut source_frame_dims: impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let key = texture_key(path)?;
    let dims = texture_dimensions(&key)?;
    let (cols, rows) = sprite_sheet_dims(&key);
    let grid = (cols as usize, rows as usize);
    let source_frame = source_frame_dims(&key, dims.0, dims.1);
    all_frames_sprite_slot_plan(
        key.clone(),
        dims,
        grid,
        frame_delay,
        beat_based,
        source_frame,
        true,
    )
    .or_else(|| Some(atlas_sprite_slot_plan(key, dims, source_frame, true)))
}

pub(super) fn sprite_all_frames_animation_plan(
    tex_dims: [u32; 2],
    sheet_grid: [usize; 2],
    frame_delay: Option<f32>,
    beat_based: bool,
) -> Option<SpriteAnimationPlan> {
    let cols = sheet_grid[0].max(1);
    let rows = sheet_grid[1].max(1);
    let frame_count = cols.saturating_mul(rows).max(1);
    if frame_count <= 1 {
        return None;
    }
    let delays = frame_delay.map(|delay| vec![delay.max(1e-6); frame_count]);
    sprite_animation_plan(
        tex_dims,
        [cols, rows],
        0,
        frame_count,
        None,
        delays.as_deref(),
        beat_based,
    )
}

pub(super) fn all_frames_sprite_slot_plan(
    texture_key: String,
    tex_dims: (u32, u32),
    sheet_grid: (usize, usize),
    frame_delay: Option<f32>,
    beat_based: bool,
    source_frame: (u32, u32),
    note_color_translate: bool,
) -> Option<SpriteSlotPlan> {
    let plan = sprite_all_frames_animation_plan(
        [tex_dims.0, tex_dims.1],
        [sheet_grid.0.max(1), sheet_grid.1.max(1)],
        frame_delay,
        beat_based,
    )?;
    Some(animation_plan_to_slot_plan(
        texture_key,
        tex_dims,
        source_frame,
        plan,
        note_color_translate,
    ))
}
