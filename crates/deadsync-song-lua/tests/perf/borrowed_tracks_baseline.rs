// Frozen from 8b3c45af9 (0.5.1626); visibility and formatting only.
use super::*;

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
        |overlay_index, values, scheduled, final_values, _| {
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
                let current = from_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, *target))
                    .unwrap_or_else(|| overlay_state_update_value(baseline, *target));
                let track_index = push_update_overlay_value(
                    tracks,
                    track_indices,
                    overlay_index,
                    *target,
                    beat,
                    current,
                    next_beat,
                    next.clone(),
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
                let current = from_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, target))?;
                let message = to_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, target))?;
                let tracked = tracks
                    .get(track_index)
                    .and_then(|track| track.samples.last())
                    .map(|sample| &sample.value)?;
                (*tracked != message).then_some((overlay_index, target, current, message))
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
