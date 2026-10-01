// Frozen from dc21ff490 (0.5.1629); helper contains its original final-values loop.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_update_overlay_samples<Kind>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    baseline: &[SongLuaOverlayState],
    from_states: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    restored_indices: &[usize],
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    beat: f32,
    next_beat: f32,
    next_seconds: f64,
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    scratch: &mut OverlaySampleScratch,
) -> Result<(), String> {
    merge_completed_scheduled_overlay_samples_into(
        tracks,
        track_indices,
        baseline,
        update_states,
        to_states,
        scheduled_samples,
        next_seconds,
        &mut scratch.completed,
    );
    scratch.reset_indices.clear();
    scratch.captured_tracks.clear();
    scratch.captured_tracks.resize(tracks.len(), false);
    scratch.message_targets.clear();
    let OverlaySampleScratch {
        reset_indices,
        captured_tracks,
        message_targets,
        ..
    } = scratch;
    crate::lua_util::drain_overlay_update_capture(
        lua,
        |overlay_index, values, scheduled, final_values| {
            let Some(baseline) = baseline.get(overlay_index) else {
                return Ok(());
            };
            debug_assert!(overlay_index < overlays.len());
            reset_indices.push(overlay_index);
            for (target, value) in final_values {
                if scheduled.iter().any(|update| update.target == *target) {
                    continue;
                }
                if let Some(state) = update_states.get_mut(overlay_index) {
                    set_overlay_state_update_value(state, *target, value);
                }
                if restored_indices.binary_search(&overlay_index).is_err()
                    && let Some(state) = to_states.get_mut(overlay_index)
                {
                    set_overlay_state_update_value(state, *target, value);
                }
            }
            for (target, next) in values {
                let current = from_states.get(overlay_index).unwrap_or(baseline);
                let track_index = push_captured_overlay_value(
                    tracks,
                    track_indices,
                    overlay_index,
                    *target,
                    beat,
                    current,
                    next_beat,
                    next,
                );
                // Tracks only append. Mark unchanged writes too: they still
                // take precedence over a restored message on this tick.
                if track_index >= captured_tracks.len() {
                    captured_tracks.resize(track_index + 1, false);
                }
                captured_tracks[track_index] = true;
            }
            append_scheduled_overlay_updates(
                scheduled_samples,
                context,
                update_states,
                overlay_index,
                scheduled,
                next_seconds,
            );
            Ok(())
        },
    )?;
    message_targets.extend(
        track_indices
            .iter()
            .filter(|(_, index)| !captured_tracks[**index])
            .filter_map(|(&(overlay_index, target), &track_index)| {
                let current_state = from_states.get(overlay_index)?;
                let message_state = to_states.get(overlay_index)?;
                let tracked = tracks
                    .get(track_index)
                    .and_then(|track| track.samples.last())
                    .map(|sample| &sample.value)?;
                if overlay_state_matches_update_value(message_state, target, tracked) {
                    return None;
                }
                Some((
                    overlay_index,
                    target,
                    overlay_state_update_value(current_state, target),
                    overlay_state_update_value(message_state, target),
                ))
            }),
    );
    // Runtime update tracks are applied after message commands. Keep an existing
    // track synchronized when a message changes its target, otherwise a stale
    // sampled value can overwrite a later persistent action (notably Player
    // ActorProxy visibility in Step Your Game Up).
    for (overlay_index, target, current, message) in message_targets.drain(..) {
        push_update_overlay_value(
            tracks,
            track_indices,
            overlay_index,
            target,
            beat,
            current,
            next_beat,
            message,
        );
    }
    for &overlay_index in reset_indices.iter() {
        reset_actor_capture(lua, &overlays[overlay_index].table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub(super) fn apply_scheduled_overlay_states<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
) -> Result<(), String> {
    for sample in scheduled {
        if seconds + f64::EPSILON < sample.start_seconds {
            continue;
        }
        let linear_factor = if sample.end_seconds <= sample.start_seconds + f64::EPSILON {
            1.0
        } else {
            ((seconds - sample.start_seconds) / (sample.end_seconds - sample.start_seconds))
                .clamp(0.0, 1.0) as f32
        };
        let factor = crate::overlay_command_ease_factor(
            sample.easing.as_deref(),
            linear_factor,
            sample.opt1,
        );
        let Some(state) = states.get_mut(sample.overlay_index) else {
            continue;
        };
        let value = lerp_scheduled_value(&sample.from, &sample.value, factor);
        set_overlay_state_update_value(state, sample.target, &value);
        if let Some(overlay) = overlays.get(sample.overlay_index) {
            crate::lua_util::set_actor_overlay_update_getter_value(
                lua,
                &overlay.table,
                sample.target,
                &value,
            )?;
        }
    }
    Ok(())
}

pub(super) fn apply_captured_final_values(
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[crate::lua_util::SongLuaScheduledOverlayUpdate],
    final_values: &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
    restored_indices: &[usize],
) {
    for (target, value) in final_values {
        if scheduled.iter().any(|update| update.target == *target) {
            continue;
        }
        if let Some(state) = update_states.get_mut(overlay_index) {
            set_overlay_state_update_value(state, *target, value);
        }
        if restored_indices.binary_search(&overlay_index).is_err()
            && let Some(state) = to_states.get_mut(overlay_index)
        {
            set_overlay_state_update_value(state, *target, value);
        }
    }
}
