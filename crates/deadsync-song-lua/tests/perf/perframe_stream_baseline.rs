// Frozen from f2d8fea821cefb4aa7655302b8e4da2430ca16ff; only test visibility differs.

use super::*;

pub fn perframe_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    let step = perframe_segment_step(end - start);
    let eps = (0.5 * step).min(0.25 * (end - start)).max(1.0e-4_f32);
    let mut out = Vec::new();
    let mut beat = start;
    let mut prev_eval = None::<f32>;
    loop {
        let eval_beat = if beat <= start + f32::EPSILON {
            (start + eps).min(end - eps)
        } else if beat >= end - f32::EPSILON {
            (end - eps).max(start + eps)
        } else {
            beat
        };
        let delta_beats = prev_eval
            .map(|prev| (eval_beat - prev).abs())
            .unwrap_or(0.0);
        out.push(SongLuaPerframeSample {
            beat,
            eval_beat,
            delta_beats,
        });
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + step).min(end);
        if beat > end {
            beat = end;
        }
    }
    out
}

pub fn compile_perframes<Kind>(
    lua: &Lua,
    prefix_table: Option<Table>,
    global_table: Option<Table>,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    messages: &[SongLuaMessageEvent],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let mut entries = read_perframe_entries(prefix_table)?;
    entries.extend(read_perframe_entries(global_table)?);
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let boundaries = perframe_boundaries(&entries);
    if boundaries.len() < 2 {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let player_tables = tracked_player_tables(tracked_actors);
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let mut out_eases = Vec::new();
    let mut out_overlay_eases = Vec::new();
    let mut saw_recognized_side_effect = false;
    let mut message_replay = SongLuaPerframeMessageReplay::new(messages, overlays.len());
    let mut message_states = baseline_overlays.clone();

    for window in boundaries.windows(2) {
        let [start, end] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        let active = active_perframe_entries(&entries, start, end);
        if active.is_empty() {
            let current_players = current_perframe_player_states(&player_tables)?;
            let current_overlays = current_overlay_compile_actor_states(overlays)?;
            message_states.clone_from(&current_overlays);
            push_perframe_static_targets(
                &mut out_eases,
                &mut out_overlay_eases,
                start,
                end,
                &current_players,
                &current_overlays,
                &baseline_players,
                &baseline_overlays,
            );
            continue;
        }

        let mut sample_beats = Vec::new();
        let mut player_samples = Vec::new();
        let mut overlay_samples = Vec::new();
        for sample in perframe_samples(start, end) {
            let delta_seconds = beat_span_seconds(
                context,
                sample.eval_beat - sample.delta_beats,
                sample.eval_beat,
            );
            let _ = message_replay.advance(
                lua,
                context,
                overlays,
                &mut message_states,
                sample.eval_beat,
            )?;
            reset_overlay_compile_actor_capture_tables(lua, overlays)?;
            reset_tracked_capture_tables(lua, tracked_actors)?;
            for entry in &active {
                saw_recognized_side_effect |= call_perframe_entry(
                    lua,
                    entry,
                    sample.eval_beat,
                    sample.delta_beats,
                    delta_seconds,
                )?;
            }
            sample_beats.push(sample.beat);
            player_samples.push(current_perframe_player_states(&player_tables)?);
            let current_overlays = current_overlay_compile_actor_states(overlays)?;
            message_states.clone_from(&current_overlays);
            overlay_samples.push(current_overlays);
        }

        push_sampled_perframe_targets(
            &mut out_eases,
            &mut out_overlay_eases,
            end,
            &sample_beats,
            &player_samples,
            &overlay_samples,
            &baseline_players,
            &baseline_overlays,
        );
    }

    let mut info = SongLuaCompileInfo::default();
    if out_eases.is_empty() && out_overlay_eases.is_empty() && !saw_recognized_side_effect {
        info = unsupported_perframe_info(&entries);
    }
    Ok((out_eases, out_overlay_eases, info))
}
