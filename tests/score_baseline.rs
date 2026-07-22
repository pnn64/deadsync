use deadsync_core::input::InputSource;
use deadsync_core::song_time::SongTimeNs;
use deadsync_score::{
    ArrowCloudPaneKind, Grade, LeaderboardEntry, LeaderboardPane, MachineReplayEntry, ReplayEdge,
    gameplay_run_failed, gameplay_run_passed, leaderboard_rank_for_score, lua_chart_submit_allowed,
    lua_submit_allowed, promote_quint_grade, score_to_grade,
};

fn entry(score: f64) -> LeaderboardEntry {
    LeaderboardEntry {
        rank: 0,
        name: String::new(),
        machine_tag: None,
        score,
        date: String::new(),
        is_rival: false,
        is_self: false,
        is_fail: false,
    }
}

#[test]
fn score_grade_thresholds_stay_stable() {
    assert_eq!(score_to_grade(10000.0), Grade::Tier01);
    assert_eq!(score_to_grade(9900.0), Grade::Tier02);
    assert_eq!(score_to_grade(9200.0), Grade::Tier06);
    assert_eq!(score_to_grade(5499.0), Grade::Tier17);
    assert_eq!(promote_quint_grade(Grade::Tier01, 100.0), Grade::Quint);
    assert_eq!(promote_quint_grade(Grade::Failed, 100.0), Grade::Failed);
}

#[test]
fn leaderboard_rank_treats_equal_scores_as_current_run_ahead() {
    let entries = [entry(9800.0), entry(9750.0), entry(9700.0)];
    assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
    assert_eq!(leaderboard_rank_for_score(&entries, f64::NAN), None);
}

#[test]
fn score_submit_helpers_keep_lua_and_fail_semantics() {
    assert!(!lua_chart_submit_allowed(" D5BD4DD7224F68FF "));
    assert!(!lua_chart_submit_allowed("deadbeefcafebabe"));
    assert!(lua_submit_allowed(false, "deadbeefcafebabe"));
    assert!(!lua_submit_allowed(true, "deadbeefcafebabe"));

    assert!(gameplay_run_passed(true, false, 1.0, false));
    assert!(!gameplay_run_passed(true, true, 1.0, false));
    assert!(!gameplay_run_passed(true, false, 0.0, false));
    assert!(!gameplay_run_failed(false, false));
    assert!(gameplay_run_failed(false, true));
}

#[test]
fn replay_and_leaderboard_dtos_stay_available_from_score_crate() {
    let event_music_time_ns: SongTimeNs = 1_250_000_000;
    let replay = ReplayEdge {
        event_music_time_ns,
        lane_index: 2,
        pressed: true,
        source: InputSource::Keyboard,
    };
    let machine = MachineReplayEntry {
        rank: 1,
        name: "ABCD".to_string(),
        score: 9750.0,
        date: "2026-05-28".to_string(),
        is_fail: false,
        replay_beat0_time_ns: 0,
        replay: vec![replay],
    };
    assert_eq!(machine.replay[0].event_music_time_ns, event_music_time_ns);

    let pane = LeaderboardPane {
        name: "Hard EX".to_string(),
        entries: vec![entry(9750.0)],
        is_ex: true,
        disabled: false,
        personalized: true,
        arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
    };
    assert!(pane.is_arrowcloud());
    assert!(pane.is_hard_ex());
}
