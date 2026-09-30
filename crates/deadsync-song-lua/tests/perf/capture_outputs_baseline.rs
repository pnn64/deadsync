// Frozen from a082c2239 (0.5.1627); visibility and formatting only.
use super::*;

pub(super) fn push_update_overlay_value(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    overlay_index: usize,
    target: crate::SongLuaOverlayUpdateTarget,
    beat: f32,
    current: crate::SongLuaOverlayUpdateValue,
    next_beat: f32,
    next: crate::SongLuaOverlayUpdateValue,
) -> usize {
    let key = (overlay_index, target);
    let track_index = match track_indices.get(&key).copied() {
        Some(index) => index,
        None => {
            let index = tracks.len();
            track_indices.insert(key, index);
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index,
                target,
                // A target enters capture only when UpdateCommand writes it.
                // Do not invent a ramp from the actor default before that first
                // write; the runtime applies the first sampled value as a step.
                samples: vec![SongLuaOverlayUpdateSample {
                    beat: next_beat,
                    value: next,
                }],
            });
            return index;
        }
    };
    if current == next {
        return track_index;
    }
    let track = &mut tracks[track_index];
    if track
        .samples
        .last()
        .is_some_and(|sample| sample.beat < beat - f32::EPSILON)
    {
        track.samples.push(SongLuaOverlayUpdateSample {
            beat,
            value: current,
        });
    }
    track.samples.push(SongLuaOverlayUpdateSample {
        beat: next_beat,
        value: next,
    });
    track_index
}

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
        next_beat,
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

pub(super) fn append_scheduled_overlay_updates(
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    context: &SongLuaCompileContext,
    update_states: &[SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[crate::lua_util::SongLuaScheduledOverlayUpdate],
    message_seconds: f64,
) {
    if scheduled.is_empty() {
        return;
    }
    // Targets are contiguous enum discriminants through StretchRect.
    // Borrow prior writes; only emitted samples need owned values.
    let mut scheduled_values: [Option<&SongLuaOverlayUpdateValue>;
        SongLuaOverlayUpdateTarget::StretchRect as usize + 1] =
        [None; SongLuaOverlayUpdateTarget::StretchRect as usize + 1];
    for update in scheduled {
        let from = scheduled_values[update.target as usize]
            .cloned()
            .or_else(|| {
                update_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, update.target))
            })
            .unwrap_or(SongLuaOverlayUpdateValue::None);
        scheduled_samples.push(SongLuaScheduledOverlaySample {
            overlay_index,
            target: update.target,
            start_seconds: message_seconds + f64::from(update.delay_seconds),
            end_seconds: message_seconds
                + f64::from(update.delay_seconds)
                + f64::from(update.duration_seconds),
            start_beat: song_beat_at_elapsed_seconds(
                (message_seconds + f64::from(update.delay_seconds)) as f32,
                context,
            ),
            end_beat: song_beat_at_elapsed_seconds(
                (message_seconds
                    + f64::from(update.delay_seconds)
                    + f64::from(update.duration_seconds)) as f32,
                context,
            ),
            easing: update.easing.clone(),
            opt1: update.opt1,
            from,
            value: update.value.clone(),
        });
        scheduled_values[update.target as usize] = Some(&update.value);
    }
}
