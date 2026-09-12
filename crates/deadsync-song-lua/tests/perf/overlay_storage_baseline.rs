// Frozen from 91d50c0d3 (0.5.1171); visibility changes only.
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
    >,
    beat: f32,
    next_beat: f32,
    next_seconds: f64,
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    scratch: &mut OverlaySampleScratch,
) -> Result<(), String> {
    merge_completed_scheduled_overlay_samples(
        tracks,
        track_indices,
        baseline,
        update_states,
        to_states,
        scheduled_samples,
        next_beat,
    );
    scratch.reset_indices.clear();
    scratch.captured_tracks.clear();
    scratch.captured_tracks.resize(tracks.len(), false);
    scratch.message_targets.clear();
    let OverlaySampleScratch {
        reset_indices,
        captured_tracks,
        message_targets,
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
            let message_seconds = next_seconds;
            let mut scheduled_values = std::collections::HashMap::new();
            for update in scheduled {
                let from = scheduled_values
                    .get(&update.target)
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
                scheduled_values.insert(update.target, update.value.clone());
            }
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

pub(super) struct SongLuaPerframeMessageReplay<'a> {
    messages: &'a [SongLuaMessageEvent],
    order: Vec<usize>,
    next: usize,
    active: Vec<Option<SongLuaPerframeActiveMessage>>,
}

impl<'a> SongLuaPerframeMessageReplay<'a> {
    pub(super) fn new(messages: &'a [SongLuaMessageEvent], overlay_count: usize) -> Self {
        let mut order = (0..messages.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| messages[a].beat.total_cmp(&messages[b].beat));
        Self {
            messages,
            order,
            next: 0,
            active: vec![None; overlay_count],
        }
    }

    pub(super) fn advance<Kind>(
        &mut self,
        lua: &Lua,
        context: &SongLuaCompileContext,
        overlays: &mut [SongLuaOverlayCompileActor<Kind>],
        states: &mut [SongLuaOverlayState],
        beat: f32,
    ) -> Result<Vec<usize>, String> {
        let mut started = Vec::new();
        while let Some(&event_index) = self.order.get(self.next) {
            let event = &self.messages[event_index];
            let event_beat = event.beat;
            if event_beat > beat + f32::EPSILON {
                break;
            }
            for (overlay_index, overlay) in overlays.iter_mut().enumerate() {
                let was_active = self.active[overlay_index].is_some();
                apply_perframe_active_message(
                    lua,
                    context,
                    overlay,
                    &mut self.active[overlay_index],
                    event_beat,
                )?;
                if was_active && let Some(state) = states.get_mut(overlay_index) {
                    *state = actor_overlay_initial_state(&overlay.table)?;
                }
                let Some(command_index) = overlay
                    .actor
                    .message_commands
                    .iter()
                    .position(|command| command.message == event.message)
                else {
                    continue;
                };
                if let Some(aux) = overlay.actor.message_commands[command_index].aux {
                    overlay
                        .table
                        .set("__songlua_aux", aux)
                        .map_err(|err| err.to_string())?;
                }
                let base = match states.get(overlay_index) {
                    Some(state) => *state,
                    None => actor_overlay_initial_state(&overlay.table)?,
                };
                self.active[overlay_index] = Some(SongLuaPerframeActiveMessage {
                    command_index,
                    start_beat: event_beat,
                    base,
                });
                started.push(overlay_index);
                apply_perframe_active_message(
                    lua,
                    context,
                    overlay,
                    &mut self.active[overlay_index],
                    event_beat,
                )?;
                if let Some(state) = states.get_mut(overlay_index) {
                    *state = actor_overlay_initial_state(&overlay.table)?;
                }
            }
            self.next += 1;
        }
        for (overlay_index, overlay) in overlays.iter_mut().enumerate() {
            let was_active = self.active[overlay_index].is_some();
            apply_perframe_active_message(
                lua,
                context,
                overlay,
                &mut self.active[overlay_index],
                beat,
            )?;
            if was_active && let Some(state) = states.get_mut(overlay_index) {
                *state = actor_overlay_initial_state(&overlay.table)?;
            }
        }
        started.sort_unstable();
        started.dedup();
        Ok(started)
    }
}

pub(super) fn append_scheduled_overlay_updates(
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    context: &SongLuaCompileContext,
    update_states: &[SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[crate::lua_util::SongLuaScheduledOverlayUpdate],
    message_seconds: f64,
) {
    let mut scheduled_values = std::collections::HashMap::new();
    for update in scheduled {
        let from = scheduled_values
            .get(&update.target)
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
        scheduled_values.insert(update.target, update.value.clone());
    }
}
