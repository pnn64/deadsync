// Frozen from 82e409572 (0.5.1169); keep old algorithms for differential tests and benchmarks.
use super::*;

pub(super) fn player_option_sample(table: &Table) -> Result<SongLuaUpdateModState, String> {
    let mut out = SongLuaUpdateModState::new();
    if let Some(state) = table
        .raw_get::<Option<Table>>("__songlua_player_option_state")
        .map_err(|err| err.to_string())?
    {
        for pair in state.pairs::<String, Value>() {
            let (key, value) = pair.map_err(|err| err.to_string())?;
            let value = match value {
                Value::Boolean(value) => f32::from(value),
                value => match read_f32(value) {
                    Some(value) => value,
                    None => continue,
                },
            };
            out.insert(key, value);
        }
    }
    for key in ["xmod", "cmod", "mmod"] {
        if let Some(value) = table
            .get::<Option<f32>>(format!("__songlua_speedmod_{key}"))
            .map_err(|err| err.to_string())?
        {
            out.insert(key.to_string(), value);
        }
    }
    Ok(out)
}

pub(super) fn push_update_mod_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    to_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    baseline_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    speeds: &[SongLuaUpdateModState; LUA_PLAYERS],
    last_windows: &mut BTreeMap<(usize, String), usize>,
) {
    for player in 0..LUA_PLAYERS {
        for (key, &from) in &from_players[player] {
            let key = key.as_str();
            let baseline = baseline_players[player].get(key).copied().unwrap_or(0.0);
            let to = to_players[player].get(key).copied().unwrap_or(baseline);
            let Some(target) = runtime_player_option_ease_target(key, key) else {
                continue;
            };
            if let Some(speed) = speeds[player].get(key).copied() {
                if !from.is_finite() || !speed.is_finite() {
                    continue;
                }
                let value = update_mod_runtime_value(key, from);
                let key = (player, key.to_string());
                if let Some(index) = last_windows.get(&key)
                    && let Some(window) = out.get_mut(*index)
                    && window.to == value
                    && window.approach_speed == Some(speed)
                {
                    window.limit = end - window.start;
                    continue;
                }
                last_windows.insert(key, out.len());
                // Song-level writes are step targets; Current approaches them
                // at the authored speed. Never tween toward a future write.
                out.push(SongLuaEaseWindow {
                    approach_speed: Some(speed),
                    unit: SongLuaTimeUnit::Beat,
                    start,
                    limit: end - start,
                    span_mode: SongLuaSpanMode::Len,
                    from: value,
                    to: value,
                    target,
                    easing: None,
                    player: Some(player as u8 + 1),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                });
                continue;
            }
            push_perframe_player_target(
                out,
                start,
                end,
                Some(update_mod_runtime_value(key, from)),
                Some(update_mod_runtime_value(key, to)),
                Some(update_mod_runtime_value(key, baseline)),
                0.0,
                target,
                player,
            );
        }
    }
}

pub(super) fn sort_overlay_update_samples(samples: &mut Vec<SongLuaOverlayUpdateSample>) {
    // Sample tracks are normally already ordered. Avoid the stable sort's
    // temporary allocation in that case; preserve stable ties when sorting.
    if !samples.is_sorted_by(|left, right| left.beat.total_cmp(&right.beat).is_le()) {
        samples.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    }
    samples.dedup_by(|next, previous| {
        if (previous.beat - next.beat).abs() <= f32::EPSILON {
            // Keep the last value AND timestamp so epsilon-connected runs
            // collapse exactly as they do when replacing the last output item.
            std::mem::swap(previous, next);
            true
        } else {
            false
        }
    });
}

pub(super) fn merge_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    baseline: &[SongLuaOverlayState],
    mut scheduled: Vec<SongLuaScheduledOverlaySample>,
) {
    for track in tracks.iter_mut() {
        sort_overlay_update_samples(&mut track.samples);
    }
    scheduled.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
    for sample in scheduled {
        let track_index = *track_indices
            .entry((sample.overlay_index, sample.target))
            .or_insert_with(|| {
                let index = tracks.len();
                tracks.push(SongLuaOverlayUpdateTrack {
                    overlay_index: sample.overlay_index,
                    target: sample.target,
                    samples: vec![SongLuaOverlayUpdateSample {
                        beat: 0.0,
                        value: overlay_state_update_value(
                            &baseline[sample.overlay_index],
                            sample.target,
                        ),
                    }],
                });
                index
            });
        let track = &mut tracks[track_index];
        let current = track
            .samples
            .iter()
            .rev()
            .find(|current| current.beat <= sample.start_beat + f32::EPSILON)
            .map(|current| current.value.clone())
            .unwrap_or_else(|| {
                overlay_state_update_value(&baseline[sample.overlay_index], sample.target)
            });
        if sample.end_beat > sample.start_beat + f32::EPSILON {
            track.samples.push(SongLuaOverlayUpdateSample {
                beat: sample.start_beat,
                value: current,
            });
        }
        track.samples.push(SongLuaOverlayUpdateSample {
            beat: sample.end_beat,
            value: sample.value,
        });
        sort_overlay_update_samples(&mut track.samples);
    }
    for track in tracks {
        sort_overlay_update_samples(&mut track.samples);
    }
}
