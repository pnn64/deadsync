pub(crate) use crate::frame_capture::*;

use std::collections::BTreeMap;

use crate::LUA_PLAYERS;
use crate::SongLuaCompileContext;
use crate::SongLuaEaseTarget;
use crate::SongLuaEaseWindow;

use crate::SongLuaOverlayEase;
use crate::SongLuaOverlayState;
use crate::SongLuaSpanMode;
use crate::SongLuaTimeUnit;

use crate::overlay_delta_pair_from_states;

use crate::runtime_player_option_ease_target;

use crate::song_beat_at_elapsed_seconds;
use crate::song_display_bps;
use crate::song_elapsed_seconds_at;
use crate::song_elapsed_seconds_for_beat;

use crate::song_music_rate;

pub const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 8192;
pub(crate) const SONG_LUA_UPDATE_REFERENCE_FPS: f32 = 60.0;

pub(crate) fn first_update_seconds(_context: &SongLuaCompileContext) -> f32 {
    #[cfg(feature = "test-support")]
    if let Some(frame) = _context.compile_clock_inputs.get(1) {
        let origin = _context
            .song_timing
            .as_ref()
            .map_or(0.0, |timing| f64::from(timing.get_time_for_beat_exact(0.0)));
        return ((frame[0] - origin) / f64::from(song_music_rate(_context))) as f32;
    }
    1.0 / SONG_LUA_UPDATE_REFERENCE_FPS
}

pub(crate) fn startup_command(
    startup: &crate::lua_util::SongLuaStartupState,
    ready: SongLuaOverlayState,
) -> Option<crate::SongLuaOverlayMessageCommand> {
    let mut blocks = startup.blocks.clone();
    if blocks.is_empty() {
        let (_, delta) = overlay_delta_pair_from_states(startup.initial, ready, ready)?;
        blocks.push(crate::SongLuaOverlayCommandBlock {
            progress: None,
            queued: false,
            start: 0.0,
            duration: 0.0,
            easing: None,
            opt1: None,
            opt2: None,
            delta,
        });
    }
    // Native zero-time queues consume the first positive frame's delta.
    for block in &mut blocks {
        if block.queued
            && block.delta.effect_mode.is_some_and(|mode| {
                matches!(
                    mode,
                    deadlib_present::anim::EffectMode::Bob
                        | deadlib_present::anim::EffectMode::Bounce
                        | deadlib_present::anim::EffectMode::Wag
                )
            })
        {
            // Immediate motion macros run after the timer advance on their
            // first positive dispatch frame, including an exact sleep boundary.
            block.start = (block.start * SONG_LUA_UPDATE_REFERENCE_FPS + 0.000_01).floor()
                / SONG_LUA_UPDATE_REFERENCE_FPS;
        } else {
            block.start -= 1.0 / SONG_LUA_UPDATE_REFERENCE_FPS;
        }
    }
    Some(crate::SongLuaOverlayMessageCommand {
        frame_advance: 0.0,
        message: "__songlua_queued_startup".to_string(),
        blocks,
    })
}

pub(crate) const PLAYER_TRANSFORM_CAPTURE_KEYS: [&str; 11] = [
    "x",
    "y",
    "z",
    "rot_x_deg",
    "rot_z_deg",
    "rot_y_deg",
    "zoom_x",
    "zoom_y",
    "zoom_z",
    "skew_x",
    "skew_y",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaPerframeSample {
    pub beat: f32,
    pub eval_beat: f32,
    pub delta_beats: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaPerframePlayerState {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub rotation_x: Option<f32>,
    pub rotation_z: Option<f32>,
    pub rotation_y: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
}

pub type SongLuaUpdateModState = BTreeMap<String, f32>;

pub(crate) trait ModState {
    fn entries(&self) -> impl Iterator<Item = (&str, &f32)>;
    fn get(&self, key: &str) -> Option<&f32>;
    fn is_empty(&self) -> bool;
}

impl ModState for SongLuaUpdateModState {
    fn entries(&self) -> impl Iterator<Item = (&str, &f32)> {
        self.iter().map(|(key, value)| (key.as_str(), value))
    }

    fn get(&self, key: &str) -> Option<&f32> {
        BTreeMap::get(self, key)
    }

    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
}

pub(crate) fn sample_player_pose(
    output: &mut SongLuaPerframePlayerState,
    state: &SongLuaOverlayState,
    mask: Option<u16>,
) {
    macro_rules! sample {
        ($bit:literal, $out:ident, $field:ident) => {
            if mask.map_or(output.$out.is_some(), |mask| mask & (1 << $bit) != 0) {
                output.$out = Some(state.$field);
            }
        };
    }
    sample!(0, x, x);
    sample!(1, y, y);
    sample!(2, z, z);
    sample!(3, rotation_x, rot_x_deg);
    sample!(4, rotation_z, rot_z_deg);
    sample!(5, rotation_y, rot_y_deg);
    sample!(6, zoom_x, zoom_x);
    sample!(7, zoom_y, zoom_y);
    sample!(8, zoom_z, zoom_z);
    sample!(9, skew_x, skew_x);
    sample!(10, skew_y, skew_y);
}

#[inline(always)]
#[must_use]
pub fn perframe_segment_step(len: f32) -> f32 {
    (len / 96.0).clamp(1.0 / 192.0, 0.125)
}

#[inline(always)]
#[must_use]
pub fn perframe_delta_seconds(context: &SongLuaCompileContext, delta_beats: f32) -> f32 {
    song_elapsed_seconds_for_beat(
        delta_beats,
        song_display_bps(context),
        song_music_rate(context),
    )
}

#[inline(always)]
#[must_use]
pub fn relative_player_target(value: Option<f32>, baseline: Option<f32>) -> Option<f32> {
    value.map(|value| value - baseline.unwrap_or(0.0))
}

#[must_use]
pub fn update_function_end_beat(context: &SongLuaCompileContext) -> f32 {
    let seconds = context.music_length_seconds.max(0.0);
    let seconds = if context.song_timing.is_some() {
        seconds / song_music_rate(context)
    } else {
        seconds
    };
    song_beat_at_elapsed_seconds(seconds, context).max(0.0)
}

#[must_use]
pub fn update_function_sample_step(len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    (len / SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES as f32).max(1.0 / 192.0)
}

pub(crate) fn update_function_replay_beats(
    context: &SongLuaCompileContext,
    start: f32,
    end: f32,
) -> Vec<(f64, f64)> {
    #[cfg(feature = "test-support")]
    if !context.compile_clock_inputs.is_empty() {
        let origin = context
            .song_timing
            .as_ref()
            .map_or(0.0, |timing| f64::from(timing.get_time_for_beat_exact(0.0)));
        let rate = f64::from(song_music_rate(context));
        let mut previous = 0.0;
        return context
            .compile_clock_inputs
            .iter()
            .map(|&[music, beat, _]| {
                let seconds = (music - origin) / rate;
                let delta = seconds - previous;
                previous = seconds;
                (beat, delta)
            })
            .collect();
    }
    let start_seconds = f64::from(song_elapsed_seconds_at(start, context));
    let end_seconds = f64::from(if end == update_function_end_beat(context) {
        // Keep the supplied horizon. Converting its rounded beat back to
        // seconds can truncate the final frame and skip boundary callbacks.
        context.music_length_seconds / song_music_rate(context)
    } else {
        song_elapsed_seconds_at(end, context)
    });
    let frame_count = ((end_seconds - start_seconds) * f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
        .ceil()
        .max(0.0) as usize;
    // Empty timing maps have no prefix to reuse. Keep their original build
    // path without pre-sizing the temporary argument storage.
    if context.song_timing_bpms.is_empty() || context.song_timing.is_some() {
        let mut out = vec![(f64::from(start), 0.0)];
        let mut previous_seconds = start_seconds;
        for frame in 1..=frame_count {
            let seconds = (start_seconds + frame as f64 / f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
                .min(end_seconds);
            out.push((
                crate::song_beat_at_seconds64(seconds, context),
                seconds - previous_seconds,
            ));
            previous_seconds = seconds;
        }
        return out;
    }
    let mut out = Vec::with_capacity(frame_count.saturating_add(1));
    out.push((f64::from(start), 0.0));
    if frame_count == 0 {
        return out;
    }
    let mut previous_seconds = start_seconds;
    let rate = f64::from(song_music_rate(context));
    let mut cursor_beat = 0.0;
    let mut cursor_seconds = 0.0;
    let mut bpm = context
        .song_timing_bpms
        .first()
        .filter(|(beat, bpm)| *beat <= 0.0 && *bpm > 0.0)
        .map_or_else(
            || {
                f64::from(context.song_display_bpms[0].max(context.song_display_bpms[1]))
                    .max(f64::from(f32::EPSILON) * 60.0)
            },
            |segment| f64::from(segment.1),
        );
    let mut segments = context.song_timing_bpms.as_slice();
    let ordered = rate.is_finite()
        && rate > 0.0
        && bpm.is_finite()
        && segments
            .iter()
            .all(|(beat, bpm)| beat.is_finite() && bpm.is_finite() && *bpm > 0.0)
        && segments.is_sorted_by(|left, right| left.0 <= right.0);
    // Each timestamp uses the original frame formula. Only the immutable BPM
    // prefix is retained; accumulated delta rounding cannot move a boundary.
    for index in 0..frame_count {
        let frame = index + 1;
        let seconds = (start_seconds + frame as f64 / f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
            .min(end_seconds);
        let beat = if ordered {
            let target = seconds * rate;
            while let Some((&(segment_beat, segment_bpm), rest)) = segments.split_first() {
                let segment_beat = f64::from(segment_beat);
                let next_seconds = cursor_seconds + (segment_beat - cursor_beat) * 60.0 / bpm;
                if next_seconds > target {
                    break;
                }
                cursor_beat = segment_beat;
                cursor_seconds = next_seconds;
                bpm = f64::from(segment_bpm).max(f64::EPSILON);
                segments = rest;
            }
            cursor_beat + (target - cursor_seconds) * bpm / 60.0
        } else {
            // Public compile contexts can contain unordered or invalid timing.
            // Preserve their existing conversion semantics.
            crate::song_beat_at_seconds64(seconds, context)
        };
        let delta = seconds - previous_seconds;
        previous_seconds = seconds;
        out.push((beat, delta));
    }
    out
}

pub(crate) fn reference_actor_frames(_context: &SongLuaCompileContext) -> Option<Vec<[f32; 2]>> {
    #[cfg(feature = "test-support")]
    if !_context.compile_clock_inputs.is_empty() {
        let rate = f64::from(song_music_rate(_context));
        return Some(
            _context
                .compile_clock_inputs
                .iter()
                .enumerate()
                .map(|(index, &[music, _, delta])| {
                    let span = if index == 0 {
                        0.0
                    } else {
                        (music - _context.compile_clock_inputs[index - 1][0]) / rate
                    };
                    [delta as f32, span as f32]
                })
                .collect(),
        );
    }
    None
}

#[must_use]
pub fn update_function_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    let step = update_function_sample_step(end - start);
    let mut out = Vec::new();
    let mut beat = (start + step).min(end);
    let mut prev_eval = Some(start);

    loop {
        let eval_beat = beat;
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
    }
    out
}

#[must_use]
pub fn perframe_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    perframe_sample_iter(start, end).collect()
}

pub(crate) fn perframe_sample_iter(
    start: f32,
    end: f32,
) -> impl Iterator<Item = SongLuaPerframeSample> {
    let step = perframe_segment_step(end - start);
    let eps = (0.5 * step).min(0.25 * (end - start)).max(1.0e-4_f32);
    let mut beat = start;
    let mut prev_eval = None::<f32>;
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
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
        let sample = SongLuaPerframeSample {
            beat,
            eval_beat,
            delta_beats,
        };
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            done = true;
        } else {
            beat = (beat + step).min(end);
            if beat > end {
                beat = end;
            }
        }
        Some(sample)
    })
}

pub fn push_perframe_overlay_targets(
    out: &mut Vec<SongLuaOverlayEase>,
    start: f32,
    end: f32,
    from_overlays: &[SongLuaOverlayState],
    to_overlays: &[SongLuaOverlayState],
    baseline_overlays: &[SongLuaOverlayState],
    skip_unchanged: bool,
) {
    for overlay_index in 0..from_overlays.len().min(to_overlays.len()) {
        if skip_unchanged && from_overlays[overlay_index] == to_overlays[overlay_index] {
            continue;
        }
        let Some((from, to)) = overlay_delta_pair_from_states(
            baseline_overlays[overlay_index],
            from_overlays[overlay_index],
            to_overlays[overlay_index],
        ) else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit: SongLuaTimeUnit::Beat,
            start,
            limit: end - start,
            span_mode: SongLuaSpanMode::Len,
            from,
            to,
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        });
    }
}

pub fn push_perframe_player_target(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from: Option<f32>,
    to: Option<f32>,
    baseline: Option<f32>,
    neutral: f32,
    target: SongLuaEaseTarget,
    player: usize,
) {
    if end <= start {
        return;
    }
    let baseline = baseline.unwrap_or(neutral);
    let from = from.unwrap_or(baseline);
    let to = to.unwrap_or(baseline);
    if !from.is_finite() || !to.is_finite() {
        return;
    }
    if (from - baseline).abs() <= f32::EPSILON && (to - baseline).abs() <= f32::EPSILON {
        return;
    }
    out.push(SongLuaEaseWindow {
        approach_speed: None,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: end - start,
        span_mode: SongLuaSpanMode::Len,
        from,
        to,
        target,
        easing: Some("linear".to_string()),
        player: Some((player + 1) as u8),
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

pub fn push_perframe_player_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    to_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
) {
    for player in 0..LUA_PLAYERS {
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].x,
            to_players[player].x,
            baseline_players[player].x,
            0.0,
            SongLuaEaseTarget::PlayerX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].y,
            to_players[player].y,
            baseline_players[player].y,
            0.0,
            SongLuaEaseTarget::PlayerY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            relative_player_target(from_players[player].z, baseline_players[player].z),
            relative_player_target(to_players[player].z, baseline_players[player].z),
            Some(0.0),
            0.0,
            SongLuaEaseTarget::PlayerZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_x,
            to_players[player].rotation_x,
            baseline_players[player].rotation_x,
            0.0,
            SongLuaEaseTarget::PlayerRotationX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_z,
            to_players[player].rotation_z,
            baseline_players[player].rotation_z,
            0.0,
            SongLuaEaseTarget::PlayerRotationZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_y,
            to_players[player].rotation_y,
            baseline_players[player].rotation_y,
            0.0,
            SongLuaEaseTarget::PlayerRotationY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_x,
            to_players[player].zoom_x,
            baseline_players[player].zoom_x,
            1.0,
            SongLuaEaseTarget::PlayerZoomX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_y,
            to_players[player].zoom_y,
            baseline_players[player].zoom_y,
            1.0,
            SongLuaEaseTarget::PlayerZoomY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_z,
            to_players[player].zoom_z,
            baseline_players[player].zoom_z,
            1.0,
            SongLuaEaseTarget::PlayerZoomZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].skew_x,
            to_players[player].skew_x,
            baseline_players[player].skew_x,
            0.0,
            SongLuaEaseTarget::PlayerSkewX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].skew_y,
            to_players[player].skew_y,
            baseline_players[player].skew_y,
            0.0,
            SongLuaEaseTarget::PlayerSkewY,
            player,
        );
    }
}

#[inline(always)]
fn update_mod_runtime_value(key: &str, value: f32) -> f32 {
    if matches!(key, "xmod" | "cmod" | "mmod") {
        value
    } else {
        value * 100.0
    }
}

pub fn push_update_mod_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    to_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    baseline_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    speeds: &[SongLuaUpdateModState; LUA_PLAYERS],
    last_windows: &mut BTreeMap<(usize, String), usize>,
) {
    push_update_mod_targets_with_key(
        out,
        start,
        end,
        from_players,
        to_players,
        baseline_players,
        speeds,
        None,
        last_windows,
        &mut (0, String::new()),
        SongLuaTimeUnit::Beat,
    );
}

pub(crate) fn push_update_mod_targets_with_key<S: ModState>(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[S; LUA_PLAYERS],
    to_players: &[S; LUA_PLAYERS],
    baseline_players: &[S; LUA_PLAYERS],
    speeds: &[S; LUA_PLAYERS],
    next_speeds: Option<&[S; LUA_PLAYERS]>,
    last_windows: &mut BTreeMap<(usize, String), usize>,
    lookup_key: &mut (usize, String),
    unit: SongLuaTimeUnit,
) {
    if from_players.iter().all(ModState::is_empty) {
        return;
    }
    for player in 0..LUA_PLAYERS {
        for (key, &from) in from_players[player].entries() {
            // Classify without allocating the name of a coalesced speed target.
            let Some(mut target) = runtime_player_option_ease_target(key, "") else {
                continue;
            };
            let speed = speeds[player].get(key).copied();
            // The first native setter supplies its approach speed at the next
            // sample. Until then this is still the baseline, not a ramp toward
            // a future write (which would move Mini before the paired XMod).
            if speed.is_none() && next_speeds.is_some_and(|next| next[player].get(key).is_some()) {
                continue;
            }
            // These switches have no native approach speed, but initial values and
            // later writes are still step targets, never interpolated samples.
            if speed.is_some()
                || matches!(
                    key,
                    "cosecant"
                        | "dizzyholds"
                        | "stealthtype"
                        | "zbuffer"
                        | "modtimersetting"
                        | "failsetting"
                )
            {
                if !from.is_finite() || speed.is_some_and(|speed| !speed.is_finite()) {
                    continue;
                }
                let value = update_mod_runtime_value(key, from);
                // Reuse this lookup buffer across keys and compiler samples.
                // Only newly indexed windows need an owned cache key.
                lookup_key.0 = player;
                lookup_key.1.clear();
                lookup_key.1.push_str(key);
                if let Some(index) = last_windows.get_mut(lookup_key) {
                    if let Some(window) = out.get_mut(*index)
                        && window.to == value
                        && window.approach_speed == speed
                    {
                        window.limit = end - window.start;
                        continue;
                    }
                    *index = out.len();
                } else {
                    last_windows.insert(lookup_key.clone(), out.len());
                }
                if let SongLuaEaseTarget::Mod(name) = &mut target {
                    *name = key.to_owned();
                }
                // Current approaches float targets at the authored speed;
                // switches change immediately. Never tween toward a future write.
                out.push(SongLuaEaseWindow {
                    approach_speed: speed,
                    unit,
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
            let baseline = baseline_players[player].get(key).copied().unwrap_or(0.0);
            let to = to_players[player].get(key).copied().unwrap_or(baseline);
            if let SongLuaEaseTarget::Mod(name) = &mut target {
                *name = key.to_owned();
            }
            let first = out.len();
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
            for window in &mut out[first..] {
                window.unit = unit;
            }
        }
    }
}

pub fn push_perframe_static_targets(
    out_eases: &mut Vec<SongLuaEaseWindow>,
    out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
    start: f32,
    end: f32,
    current_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    current_overlays: &[SongLuaOverlayState],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_overlays: &[SongLuaOverlayState],
) {
    push_perframe_player_targets(
        out_eases,
        start,
        end,
        current_players,
        current_players,
        baseline_players,
    );
    push_perframe_overlay_targets(
        out_overlay_eases,
        start,
        end,
        current_overlays,
        current_overlays,
        baseline_overlays,
        false,
    );
}

pub fn push_sampled_perframe_targets(
    out_eases: &mut Vec<SongLuaEaseWindow>,
    out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
    end: f32,
    sample_beats: &[f32],
    player_samples: &[[SongLuaPerframePlayerState; LUA_PLAYERS]],
    overlay_samples: &[Vec<SongLuaOverlayState>],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_overlays: &[SongLuaOverlayState],
) {
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        if seg_end <= seg_start {
            continue;
        }
        let from_players = player_samples[index];
        let to_players = player_samples
            .get(index + 1)
            .copied()
            .unwrap_or(from_players);
        push_perframe_player_targets(
            out_eases,
            seg_start,
            seg_end,
            &from_players,
            &to_players,
            baseline_players,
        );
        let from_overlays = &overlay_samples[index];
        let to_overlays = overlay_samples.get(index + 1).unwrap_or(from_overlays);
        push_perframe_overlay_targets(
            out_overlay_eases,
            seg_start,
            seg_end,
            from_overlays,
            to_overlays,
            baseline_overlays,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue};

    #[test]
    fn scheduled_native_lerp() {
        use SongLuaOverlayUpdateValue as Value;
        // Native near-camera Actor capture at 101/60 seconds of a 3s tween.
        let expected = 85.5555419921875_f32;
        let factor = (101.0_f32 / 60.0) / 3.0;
        let actual = lerp_scheduled_value(&Value::F32(-700.0), &Value::F32(700.0), factor);
        let Value::F32(actual) = actual else {
            panic!("scalar tween")
        };
        assert_eq!(actual.to_bits(), expected.to_bits());
        let actual = lerp_scheduled_value(
            &Value::Vec3([516.0, 31.0, -700.0]),
            &Value::Vec3([516.0, 31.0, 700.0]),
            factor,
        );
        let Value::Vec3(actual) = actual else {
            panic!("vector tween")
        };
        assert_eq!(actual, [516.0, 31.0, expected]);
    }

    #[test]
    fn completed_update_tweens_persist_into_following_state() {
        let baseline = vec![SongLuaOverlayState::default()];
        let mut update_states = baseline.clone();
        let mut next_states = baseline.clone();
        let mut tracks = Vec::new();
        let mut track_indices = std::collections::HashMap::new();
        let mut scheduled = vec![SongLuaScheduledOverlaySample {
            progress: None,
            duration: 1.0,
            dispatch_seconds: None,
            frame_advance: 0.0,
            overlay_index: 0,
            target: SongLuaOverlayUpdateTarget::X,
            start_seconds: 1.0,
            end_seconds: 2.0,
            start_beat: 1.0,
            end_beat: 2.0,
            easing: Some("accelerate".to_owned()),
            opt1: None,
            from: SongLuaOverlayUpdateValue::F32(0.0),
            value: SongLuaOverlayUpdateValue::F32(427.0),
        }];

        merge_completed_scheduled_overlay_samples(
            &mut tracks,
            &mut track_indices,
            &baseline,
            &mut update_states,
            &mut next_states,
            &mut scheduled,
            2.0,
        );

        assert!(scheduled.is_empty());
        assert_eq!(update_states[0].x, 427.0);
        assert_eq!(next_states[0].x, 427.0);
        assert!(tracks[0].samples.last().is_some_and(|sample| {
            sample.time == 2.0 && sample.value == SongLuaOverlayUpdateValue::F32(427.0)
        }));
    }
}
