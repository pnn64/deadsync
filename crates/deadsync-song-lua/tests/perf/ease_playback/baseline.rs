// Frozen from 0.5.1698; preserves the original state copies and ease order.
use super::*;

pub(super) fn apply_song_lua_overlay_runtime_eases_for(
    now: f32,
    overlay_index: usize,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    mut current: SongLuaOverlayState,
) -> SongLuaOverlayState {
    let Some(ease_range) = overlay_ease_ranges.get(overlay_index) else {
        return current;
    };
    for ease in &overlay_eases[ease_range.clone()] {
        debug_assert_eq!(ease.overlay_index, overlay_index);
        // Grouped ease ranges are sorted by start time.
        if now < ease.start_second {
            break;
        }
        if let Some(cutoff_second) = ease.cutoff_second
            && now >= cutoff_second
        {
            continue;
        }
        if now >= ease.sustain_end_second {
            apply_overlay_delta(&mut current, &ease.to.delta);
            if ease.to.delta.sprite_state_index.is_some() {
                current.sprite_animation_epoch = Some(ease.end_second);
            }
            continue;
        }
        if ease.end_second <= ease.start_second || now >= ease.end_second {
            apply_overlay_delta(&mut current, &ease.to.delta);
            if ease.to.delta.sprite_state_index.is_some() {
                current.sprite_animation_epoch = Some(ease.end_second);
            }
            continue;
        }
        let t = ease.easing.factor(
            ((now - ease.start_second) / (ease.end_second - ease.start_second)).clamp(0.0, 1.0),
            ease.opt1,
            ease.opt2,
        );
        apply_overlay_delta(&mut current, &ease.from.delta);
        overlay_state_lerp(&mut current, &ease.to.delta, t);
        if ease.from.delta.sprite_state_index.is_some() {
            current.sprite_animation_epoch = Some(ease.start_second);
        }
    }
    current
}
