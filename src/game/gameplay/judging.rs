use crate::game::judgment::{
    JudgeGrade, Judgment, TimingWindow, judgment_time_error_ms_from_music_ns,
};
use crate::game::profile;
use crate::game::timing::{
    TimingProfile, TimingProfileNs, classify_offset_ns_with_disabled_windows,
    largest_enabled_tap_window_ns,
};

use super::{SongTimeNs, State, live_autoplay_judgment_offset_music_ns};

#[derive(Clone, Copy, Debug)]
pub(super) struct PlayerJudgmentTiming {
    pub(super) profile_music_ns: TimingProfileNs,
    pub(super) disabled_windows: [bool; 5],
    pub(super) largest_tap_window_music_ns: SongTimeNs,
}

impl Default for PlayerJudgmentTiming {
    fn default() -> Self {
        Self {
            profile_music_ns: TimingProfileNs::default(),
            disabled_windows: [false; 5],
            largest_tap_window_music_ns: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NoteHitEval {
    pub(super) note_time_ns: SongTimeNs,
    pub(super) measured_offset_music_ns: SongTimeNs,
    pub(super) grade: JudgeGrade,
    pub(super) window: TimingWindow,
}

#[inline(always)]
fn default_fa_plus_window_s(state: &State) -> f32 {
    state
        .timing_profile
        .fa_plus_window_s
        .unwrap_or(state.timing_profile.windows_s[0])
}

#[inline(always)]
fn profile_custom_window_ms(profile: &profile::Profile) -> f32 {
    let ms = profile.custom_fantastic_window_ms;
    f32::from(crate::game::profile::clamp_custom_fantastic_window_ms(ms))
}

#[inline(always)]
pub fn player_fa_plus_window_s(state: &State, player_idx: usize) -> f32 {
    let base = default_fa_plus_window_s(state);
    if player_idx >= state.num_players {
        return base;
    }
    let profile = &state.player_profiles[player_idx];
    if profile.custom_fantastic_window {
        profile_custom_window_ms(profile) / 1000.0
    } else {
        base
    }
}

#[inline(always)]
pub fn player_blue_window_ms(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return default_fa_plus_window_s(state) * 1000.0;
    }
    let profile = &state.player_profiles[player_idx];
    if profile.custom_fantastic_window {
        return profile_custom_window_ms(profile);
    }
    if profile.fa_plus_10ms_blue_window {
        return 10.0;
    }
    default_fa_plus_window_s(state) * 1000.0
}

#[inline(always)]
pub(super) fn build_player_judgment_timing(
    mut timing_profile: TimingProfile,
    player_profile: &profile::Profile,
    music_rate: f32,
) -> PlayerJudgmentTiming {
    let base_fa_plus_s = timing_profile
        .fa_plus_window_s
        .unwrap_or(timing_profile.windows_s[0]);
    timing_profile.fa_plus_window_s = Some(if player_profile.custom_fantastic_window {
        profile_custom_window_ms(player_profile) / 1000.0
    } else {
        base_fa_plus_s
    });
    let disabled_windows = player_profile.timing_windows.disabled_windows();
    let profile_music_ns = TimingProfileNs::from_profile_scaled(&timing_profile, music_rate);
    let largest_tap_window_music_ns =
        largest_enabled_tap_window_ns(&profile_music_ns, &disabled_windows)
            .unwrap_or(profile_music_ns.windows_ns[2]);

    PlayerJudgmentTiming {
        profile_music_ns,
        disabled_windows,
        largest_tap_window_music_ns,
    }
}

#[inline(always)]
pub(super) fn player_largest_tap_window_ns(state: &State, player_idx: usize) -> SongTimeNs {
    if player_idx >= state.num_players {
        return 0;
    }
    state.player_judgment_timing[player_idx].largest_tap_window_music_ns
}

#[inline(always)]
fn classify_player_tap_offset_ns(
    state: &State,
    player_idx: usize,
    offset_music_ns: SongTimeNs,
) -> Option<(JudgeGrade, TimingWindow)> {
    if player_idx >= state.num_players {
        return None;
    }
    let timing = state.player_judgment_timing[player_idx];
    classify_offset_ns_with_disabled_windows(
        offset_music_ns,
        &timing.profile_music_ns,
        &timing.disabled_windows,
    )
}

#[inline(always)]
pub(super) fn note_hit_eval(
    state: &State,
    player_idx: usize,
    note_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> Option<NoteHitEval> {
    if player_idx >= state.num_players {
        return None;
    }
    let timing = state.player_judgment_timing[player_idx];
    let measured_offset_music_ns = current_time_ns.saturating_sub(note_time_ns);
    if i128::from(measured_offset_music_ns).abs() > i128::from(timing.largest_tap_window_music_ns) {
        return None;
    }
    let (grade, window) =
        classify_player_tap_offset_ns(state, player_idx, measured_offset_music_ns)?;
    Some(NoteHitEval {
        note_time_ns,
        measured_offset_music_ns,
        grade,
        window,
    })
}

#[inline(always)]
pub(super) fn build_final_note_hit_judgment(
    state: &mut State,
    player_idx: usize,
    hit: NoteHitEval,
    rate: f32,
) -> (Judgment, SongTimeNs) {
    let judgment_offset_music_ns = live_autoplay_judgment_offset_music_ns(
        state,
        player_idx,
        hit.window,
        hit.measured_offset_music_ns,
    );
    let judgment_event_time_ns = hit.note_time_ns.saturating_add(judgment_offset_music_ns);
    (
        Judgment {
            time_error_ms: judgment_time_error_ms_from_music_ns(judgment_offset_music_ns, rate),
            time_error_music_ns: judgment_offset_music_ns,
            grade: hit.grade,
            window: Some(hit.window),
            miss_because_held: false,
        },
        judgment_event_time_ns,
    )
}

#[inline(always)]
pub(super) fn effective_player_global_offset_seconds(state: &State, player_idx: usize) -> f32 {
    let shift = state
        .player_global_offset_shift_seconds
        .get(player_idx)
        .copied()
        .unwrap_or(0.0);
    state.global_offset_seconds + shift
}
