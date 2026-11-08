// Frozen from 7becb497857a0296ea923f4b835b1eea2687d4ad.
use super::*;
pub(super) fn append_hold_indicators(
    draws: &mut Vec<FlatDraw>,
    request: &JudgmentFeedbackRequest<'_>,
) {
    if request.held_miss_sprite.is_none() && request.hold_sprite.is_none() {
        return;
    }
    let num_cols = request
        .num_cols
        .min(MAX_COLS)
        .min(request.column_reverse_percent.len());
    let mut fallback_offsets;
    let mut fallback_invert;
    let mut fallback_tornado;
    let (col_offsets, invert, tornado, beat_push) =
        if let Some(geometry) = request.prepared_geometry.as_ref() {
            (
                geometry.col_offsets,
                geometry.invert,
                geometry.tornado,
                geometry.beat_factor,
            )
        } else {
            fallback_offsets = [0.0; MAX_COLS];
            fallback_invert = [0.0; MAX_COLS];
            fallback_tornado = [TornadoBounds::default(); MAX_COLS];
            fill_lane_col_offsets(
                &mut fallback_offsets,
                request.noteskin_column_xs,
                num_cols,
                request.spacing_multiplier,
                request.field_zoom,
            );
            compute_active_note_geometry(
                &request.visual,
                &fallback_offsets[..num_cols],
                &mut fallback_invert[..num_cols],
                &mut fallback_tornado[..num_cols],
            );
            // Flip mirrors against the last active lane, not the backing buffer's end.
            (
                &fallback_offsets[..num_cols],
                &fallback_invert[..num_cols],
                &fallback_tornado[..num_cols],
                beat_factor(
                    request.current_beat,
                    request.visual.beat_offset,
                    request.visual.beat_mult,
                ),
            )
        };

    if let Some(sprite) = request.held_miss_sprite {
        for (i, feedback) in request.held_misses.iter().take(num_cols).enumerate() {
            let Some(feedback) = feedback else { continue };
            let elapsed = (request.elapsed_screen - feedback.started_at_screen_s).max(0.0);
            if elapsed >= HELD_MISS_TOTAL_DURATION {
                continue;
            }
            let (zoom_x, zoom_y) = held_miss_zoom(elapsed, request.mini);
            let zoom = [zoom_x, zoom_y];
            if zoom[0] <= f32::EPSILON || zoom[1] <= f32::EPSILON {
                continue;
            }
            let xy = [
                indicator_x(request, i, beat_push, col_offsets, invert, tornado),
                player_metric_y(
                    request.screen_center_y,
                    request.field_center_y,
                    request.column_reverse_percent[i],
                    request.style.held_miss_normal_y,
                    request.style.held_miss_reverse_y,
                ),
            ];
            append_indicator_sprite(draws, sprite, xy, request.style.held_miss_z, 0, zoom);
        }
    }

    if let Some(sprite) = request.hold_sprite {
        for (i, feedback) in request.hold_judgments.iter().take(num_cols).enumerate() {
            let Some(feedback) = feedback else { continue };
            let elapsed = (request.elapsed_screen - feedback.started_at_screen_s).max(0.0);
            if elapsed >= HOLD_JUDGMENT_TOTAL_DURATION {
                continue;
            }
            let progress = (elapsed / 0.3).clamp(0.0, 1.0);
            let zoom = (request.style.hold_initial_zoom
                + progress * (request.style.hold_final_zoom - request.style.hold_initial_zoom))
                * request.judgment_zoom;
            let frame_index = match feedback.result {
                HoldResult::Held => 0,
                HoldResult::LetGo | HoldResult::Missed => 1,
            };
            let xy = [
                indicator_x(request, i, beat_push, col_offsets, invert, tornado),
                player_metric_y(
                    request.screen_center_y,
                    request.field_center_y,
                    request.column_reverse_percent[i],
                    request.style.hold_normal_y,
                    request.style.hold_reverse_y,
                ),
            ];
            append_indicator_sprite(
                draws,
                sprite,
                xy,
                request.style.hold_z,
                frame_index,
                [zoom, zoom],
            );
        }
    }
}
