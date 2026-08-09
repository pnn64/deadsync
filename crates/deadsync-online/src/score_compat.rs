use deadsync_profile::compat as profile;

pub use crate::arrowcloud::{
    retry_manual_submit_from_app_runtime as retry_arrowcloud_submit,
    submit_gameplay_from_app_runtime as submit_arrowcloud_payloads_from_gameplay,
};
pub use crate::groovestats::{
    eval_state_from_app_runtime as groovestats_eval_state_from_gameplay,
    retry_manual_submit_from_app_runtime as retry_groovestats_submit,
    submit_gameplay_from_app_runtime as submit_groovestats_payloads_from_gameplay,
};
pub use crate::player_leaderboards::{
    ItlWheelSideContext,
    cached_itl_tournament_overall_ranks_for_profile_from_app_runtime as get_cached_itl_tournament_overall_ranks_for_profile,
    get_or_fetch_player_leaderboards_for_profile_from_app_runtime as get_or_fetch_player_leaderboards_for_profile,
    get_or_fetch_player_leaderboards_for_side_from_app_runtime as get_or_fetch_player_leaderboards_for_side,
    invalidate_player_leaderboards_for_side_from_app_runtime as invalidate_player_leaderboards_for_side,
    refresh_player_leaderboards_for_side_from_app_runtime as refresh_player_leaderboards_for_side,
};
pub use crate::score_import::{
    fetch_and_store_grade_from_app_runtime as fetch_and_store_grade,
    import_scores_for_profile_from_app_runtime as import_scores_for_profile,
};
pub use deadsync_profile_gameplay::{
    itl_eval_state_from_app_runtime as itl_eval_state_from_gameplay,
    save_itl_data_from_app_runtime as save_itl_data_from_gameplay,
    save_local_scores_from_app_runtime as save_local_scores_from_gameplay,
    should_warn_itl_cmod_from_app_runtime as should_warn_cmod_for_itl_chart,
};
pub use deadsync_score::{
    Grade, gameplay_run_failed, gameplay_run_passed, is_itl_unlocks_pack, itl_points_for_chart,
    runtime_lock_score_caches as lock_score_caches,
};
pub use profile::{
    cached_ac_scores_for_side as get_cached_ac_scores_for_side,
    cached_best_itg_score_for_side as get_cached_score_for_side,
    cached_best_itg_score_with_profile as get_cached_score_with_profile,
    cached_gs_score_for_side as get_cached_gs_score_for_side,
    cached_itl_score_for_id as get_cached_itl_score_for_profile,
    cached_itl_score_for_side as get_cached_itl_score_for_side,
    cached_itl_score_for_song as get_cached_itl_score_for_song,
    cached_local_ex_score_for_id as get_cached_local_ex_score_for_profile,
    cached_local_ex_score_for_side as get_cached_local_ex_score_for_side,
    cached_local_hard_ex_score_for_id as get_cached_local_hard_ex_score_for_profile,
    cached_local_hard_ex_score_for_side as get_cached_local_hard_ex_score_for_side,
    cached_local_itg_score_for_id as get_cached_local_score_for_profile,
    cached_local_pass_rate_with_profile as get_cached_local_pass_rate_with_profile,
    cached_local_score_for_side as get_cached_local_score_for_side,
    ensure_itl_wheel_caches_loaded_for_id as ensure_itl_wheel_caches_loaded,
    ensure_score_caches_loaded_for_id as ensure_score_caches_loaded,
    groovestats_score_service_allowed as is_gs_get_scores_service_allowed, import_itl_json,
    import_local_scores_for_id as import_local_scores,
    itl_song_folder_unlocked_for_side as is_itl_song_folder_unlocked_for_side,
    itl_song_folder_unlocked_with_profile as is_itl_song_folder_unlocked_with_profile,
    machine_leaderboard_local_with_names as get_machine_leaderboard_local_with_names,
    machine_leaderboard_local_without_names as get_machine_leaderboard_local,
    machine_record_local as get_machine_record_local,
    machine_replays_local as get_machine_replays_local,
    machine_scalar_record_local as get_machine_scalar_record_local,
    personal_leaderboard_local_for_side as get_personal_leaderboard_local_for_side,
    played_chart_counts_for_id as played_chart_counts_for_profile, played_chart_counts_for_machine,
    prewarm_select_music_score_caches,
    recent_played_chart_hashes_for_id as recent_played_chart_hashes_for_profile,
    recent_played_chart_hashes_for_machine, save_local_summary_score_for_side,
    scorebox_profile_snapshot, seed_session_gs_score_for_id as seed_session_gs_score,
    seed_session_itl_unlock_folders,
    seed_session_local_itg_score_for_id as seed_session_local_itg_score,
    seed_session_online_itl_self_rank, seed_session_online_itl_self_score,
    total_songs_played_for_id as total_songs_played_for_profile, total_songs_played_for_side,
};

#[derive(Clone, Debug, Default)]
pub struct EvaluationSubmissionSnapshot {
    pub groovestats_status: Option<deadsync_score::GrooveStatsSubmitUiStatus>,
    pub arrowcloud_status: Option<deadsync_score::ArrowCloudSubmitUiStatus>,
    pub event_progress: Vec<deadsync_score::EventProgress>,
    pub record_banner: Option<deadsync_score::GrooveStatsSubmitRecordBanner>,
    pub groovestats_next_retry_secs: Option<u32>,
    pub arrowcloud_next_retry_secs: Option<u32>,
    pub groovestats_next_retry_is_auto: bool,
    pub arrowcloud_next_retry_is_auto: bool,
}

pub fn tick_evaluation_auto_retries(
    groovestats_enabled: bool,
    boogiestats_enabled: bool,
    arrowcloud_enabled: bool,
) -> bool {
    let service = crate::groovestats::active_service(groovestats_enabled, boogiestats_enabled);
    let groovestats =
        crate::groovestats::tick_auto_submit_retries_from_app_frame(groovestats_enabled, service);
    let arrowcloud = crate::arrowcloud::tick_auto_submit_retries_from_app_frame(arrowcloud_enabled);
    groovestats || arrowcloud
}

pub fn evaluation_submission_snapshots<const N: usize>(
    queries: &[Option<(deadsync_profile::PlayerSide, &str)>; N],
) -> [EvaluationSubmissionSnapshot; N] {
    let mut groovestats = crate::groovestats::evaluation_submission_snapshots(queries);
    let arrowcloud = crate::arrowcloud::evaluation_submission_snapshots(queries);
    std::array::from_fn(|idx| {
        let groovestats = std::mem::take(&mut groovestats[idx]);
        EvaluationSubmissionSnapshot {
            groovestats_status: groovestats.status,
            arrowcloud_status: arrowcloud[idx].status,
            event_progress: groovestats.event_progress,
            record_banner: groovestats.record_banner,
            groovestats_next_retry_secs: groovestats.next_retry_secs,
            arrowcloud_next_retry_secs: arrowcloud[idx].next_retry_secs,
            groovestats_next_retry_is_auto: groovestats.next_retry_is_auto,
            arrowcloud_next_retry_is_auto: arrowcloud[idx].next_retry_is_auto,
        }
    })
}
