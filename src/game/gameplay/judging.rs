use deadsync_profile as profile_data;
use deadsync_rules::timing::TimingProfile;

use super::{
    FantasticWindowOptions, FinalNoteHitPlan, NoteHitEval, PlayerJudgmentTiming, SongTimeNs, State,
    blue_fantastic_window_ms, build_player_judgment_timing_for_options, fantastic_window_seconds,
    final_note_hit_plan,
    gameplay_effective_player_global_offset_seconds as effective_global_offset_seconds,
    live_autoplay_judgment_offset_music_ns, note_hit_eval_for_timing,
};

#[inline(always)]
fn default_fa_plus_window_s(state: &State) -> f32 {
    state
        .timing_profile
        .fa_plus_window_s
        .unwrap_or(state.timing_profile.windows_s[0])
}

#[inline(always)]
fn profile_custom_window_ms(profile: &profile_data::Profile) -> f32 {
    let ms = profile.custom_fantastic_window_ms;
    f32::from(profile_data::clamp_custom_fantastic_window_ms(ms))
}

#[inline(always)]
fn profile_custom_window_s(profile: &profile_data::Profile) -> f32 {
    profile_custom_window_ms(profile) / 1000.0
}

#[inline(always)]
fn fantastic_window_options(
    base_fa_plus_s: f32,
    profile: &profile_data::Profile,
) -> FantasticWindowOptions {
    FantasticWindowOptions {
        base_fa_plus_s,
        custom_fantastic_window_s: profile
            .custom_fantastic_window
            .then(|| profile_custom_window_s(profile)),
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
    }
}

#[inline(always)]
pub fn player_fa_plus_window_s(state: &State, player_idx: usize) -> f32 {
    let base = default_fa_plus_window_s(state);
    if player_idx >= state.num_players {
        return base;
    }
    fantastic_window_seconds(fantastic_window_options(
        base,
        &state.player_profiles[player_idx],
    ))
}

#[inline(always)]
pub fn player_blue_window_ms(state: &State, player_idx: usize) -> f32 {
    let base = default_fa_plus_window_s(state);
    if player_idx >= state.num_players {
        return base * 1000.0;
    }
    blue_fantastic_window_ms(fantastic_window_options(
        base,
        &state.player_profiles[player_idx],
    ))
}

#[inline(always)]
pub(super) fn build_player_judgment_timing(
    timing_profile: TimingProfile,
    player_profile: &profile_data::Profile,
    music_rate: f32,
) -> PlayerJudgmentTiming {
    let base_fa_plus_s = timing_profile
        .fa_plus_window_s
        .unwrap_or(timing_profile.windows_s[0]);
    let disabled_windows = player_profile.timing_windows.disabled_windows();
    build_player_judgment_timing_for_options(
        timing_profile,
        fantastic_window_options(base_fa_plus_s, player_profile),
        disabled_windows,
        music_rate,
    )
}

#[inline(always)]
pub(super) fn player_largest_tap_window_ns(state: &State, player_idx: usize) -> SongTimeNs {
    if player_idx >= state.num_players {
        return 0;
    }
    state.player_judgment_timing[player_idx].largest_tap_window_music_ns
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
    note_hit_eval_for_timing(timing, note_time_ns, current_time_ns)
}

#[inline(always)]
pub(super) fn build_final_note_hit_plan(
    state: &mut State,
    player_idx: usize,
    hit: NoteHitEval,
    rate: f32,
) -> FinalNoteHitPlan {
    let judgment_offset_music_ns = live_autoplay_judgment_offset_music_ns(
        state,
        player_idx,
        hit.window,
        hit.measured_offset_music_ns,
    );
    final_note_hit_plan(hit, judgment_offset_music_ns, rate)
}

#[inline(always)]
pub(super) fn effective_player_global_offset_seconds(state: &State, player_idx: usize) -> f32 {
    effective_global_offset_seconds(
        state.global_offset_seconds,
        &state.player_global_offset_shift_seconds,
        player_idx,
    )
}
