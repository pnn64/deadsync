//! Resolve score eligibility from saved profile modifiers and chart facts.

use crate::Profile;

#[must_use]
pub fn groovestats_eval_state_from_profile(
    chart: &deadsync_chart::ChartData,
    profile: &Profile,
    music_rate: f32,
    autoplay_used: bool,
    is_course_mode: bool,
    course_submit_allowed: bool,
    fail_type_ok: bool,
) -> deadsync_score::GrooveStatsEvalState {
    deadsync_score::groovestats_eval_state_from_parts(deadsync_score::GrooveStatsEvalInput {
        chart_type: chart.chart_type.as_str(),
        music_rate,
        remove_mask: profile.remove_active_mask.bits(),
        insert_mask: profile.insert_active_mask.bits(),
        holds_mask: profile.holds_active_mask.bits(),
        fail_type_ok,
        autoplay_used,
        is_course_mode,
        course_submit_allowed,
    })
}

#[must_use]
pub fn groovestats_submit_invalid_reason_from_profile(
    chart: &deadsync_chart::ChartData,
    song_has_lua: bool,
    lua_submit_allowed: bool,
    profile: &Profile,
    music_rate: f32,
    fail_type_ok: bool,
) -> Option<String> {
    if song_has_lua && !lua_submit_allowed {
        return Some("simfile relies on lua".to_string());
    }
    groovestats_eval_state_from_profile(
        chart,
        profile,
        music_rate,
        false,
        false,
        false,
        fail_type_ok,
    )
    .reason_lines
    .into_iter()
    .next()
}
