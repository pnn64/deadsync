use super::ui_dataflows_original as original;
use super::*;
use crate::perf::measure;
use std::hint::black_box;

fn grade_fixture(count: usize, mode: &str) -> (Vec<Arc<SongData>>, SelectMusicHistoryView) {
    use score_data::Grade::*;
    let grades = [
        Quint, Tier01, Tier02, Tier03, Tier04, Tier05, Tier06, Tier07, Tier08, Tier09, Tier10,
        Tier11, Tier12, Tier13, Tier14, Tier15, Tier16, Tier17, Failed,
    ];
    let mut history = SelectMusicHistoryView::default();
    let songs = (0..count)
        .map(|i| {
            let mut song = (*test_folder_stats_song(i)).clone();
            song.title = format!("Song {:04} \u{6771}\u{4eac}", count - i);
            if mode == "mixed" && i % 11 == 0 {
                song.charts[0].has_note_data = false;
                song.charts[1].chart_type = "dance-double".into();
            }
            if mode != "unplayed" && (mode != "mixed" || i % 3 != 0) {
                for (side, data) in history.sides.iter_mut().enumerate() {
                    let grade = if mode == "same" {
                        Tier05
                    } else {
                        grades[(i + side * 5) % grades.len()]
                    };
                    data.available = true;
                    data.cached_scores.push((
                        song.charts[side].short_hash.clone(),
                        score_data::CachedScore {
                            grade,
                            score_percent: if grade == Failed && i % 2 == 0 {
                                0.0
                            } else {
                                87.5
                            },
                            lamp_index: Some(1),
                            lamp_judge_count: None,
                        },
                    ));
                }
            }
            Arc::new(song)
        })
        .collect();
    for side in &mut history.sides {
        side.cached_scores.sort_by(|a, b| a.0.cmp(&b.0));
    }
    (songs, history)
}

fn group(
    ranking: &score_data::SongRankingIndex<'_>,
    workspace: &mut score_data::SongRankingWorkspace,
    history: &SelectMusicHistoryView,
    side: Option<usize>,
    chart_type: &str,
    current: bool,
) -> Vec<MusicWheelEntry> {
    match (current, side) {
        (false, None) => {
            original::build_top_grades_grouped_entries(ranking, workspace, chart_type, history)
        }
        (true, None) => build_top_grades_grouped_entries(ranking, workspace, chart_type, history),
        (false, Some(side)) => original::build_top_grades_grouped_entries_for_side(
            ranking,
            workspace,
            chart_type,
            &history.sides[side],
        ),
        (true, Some(side)) => build_top_grades_grouped_entries_for_side(
            ranking,
            workspace,
            chart_type,
            &history.sides[side],
        ),
    }
}

fn assert_entries(before: &[MusicWheelEntry], after: &[MusicWheelEntry]) {
    assert_eq!(after.len(), before.len());
    for (before, after) in before.iter().zip(after) {
        match (before, after) {
            (MusicWheelEntry::Song(before), MusicWheelEntry::Song(after)) => {
                // Identity proves that the complete song and chart data is preserved.
                assert!(Arc::ptr_eq(before, after));
            }
            (MusicWheelEntry::PackHeader { .. }, MusicWheelEntry::PackHeader { .. }) => {
                assert_eq!(format!("{after:?}"), format!("{before:?}"));
            }
            _ => panic!("entry variant or order changed"),
        }
    }
}

#[test]
fn grade_headings_preserve_full_entries_for_both_profiles_and_chart_types() {
    for count in [0, 1, 19, 128] {
        for mode in ["unplayed", "same", "mixed"] {
            let (songs, history) = grade_fixture(count, mode);
            let before_history = history.clone();
            let mut ranking = score_data::SongRankingIndex::new(&songs);
            for prepared in [false, true] {
                if prepared {
                    ranking.prepare_song_order(song_title_cmp);
                }
                for side in [None, Some(0), Some(1)] {
                    for chart_type in ["dance-single", "DANCE-DOUBLE", "missing"] {
                        let mut old = score_data::SongRankingWorkspace::default();
                        let mut new = score_data::SongRankingWorkspace::default();
                        assert_entries(
                            &group(&ranking, &mut old, &history, side, chart_type, false),
                            &group(&ranking, &mut new, &history, side, chart_type, true),
                        );
                        assert!(old.top_grades().is_empty() && new.top_grades().is_empty());
                    }
                }
            }
            assert_eq!(history, before_history);
        }
    }
}

#[test]
fn grade_headings_preserve_empty_titles_duplicate_songs_and_workspace_reuse() {
    let (mut songs, mut history) = grade_fixture(24, "mixed");
    Arc::make_mut(&mut songs[0]).title.clear();
    Arc::make_mut(&mut songs[1]).title = "Same".into();
    Arc::make_mut(&mut songs[2]).title = "same".into();
    songs.push(Arc::clone(&songs[0]));
    let mut ranking = score_data::SongRankingIndex::new(&songs);
    ranking.prepare_song_order(song_title_cmp);
    let mut old = score_data::SongRankingWorkspace::default();
    let mut new = score_data::SongRankingWorkspace::default();
    for side in [None, Some(0), Some(1), None] {
        assert_entries(
            &group(&ranking, &mut old, &history, side, "dance-single", false),
            &group(&ranking, &mut new, &history, side, "dance-single", true),
        );
    }
    history.sides[0].cached_scores.clear();
    history.sides[1].cached_scores.clear();
    assert_entries(
        &group(&ranking, &mut old, &history, None, "dance-single", false),
        &group(&ranking, &mut new, &history, None, "dance-single", true),
    );
}

#[test]
fn grade_headings_remove_one_string_per_song_and_per_header() {
    for count in [0, 1, 32, 2048] {
        for mode in ["unplayed", "same", "mixed"] {
            let (songs, history) = grade_fixture(count, mode);
            let mut ranking = score_data::SongRankingIndex::new(&songs);
            ranking.prepare_song_order(song_title_cmp);
            for side in [None, Some(0), Some(1)] {
                let mut old = score_data::SongRankingWorkspace::default();
                let mut new = score_data::SongRankingWorkspace::default();
                drop(group(
                    &ranking,
                    &mut old,
                    &history,
                    side,
                    "dance-single",
                    false,
                ));
                drop(group(
                    &ranking,
                    &mut new,
                    &history,
                    side,
                    "dance-single",
                    true,
                ));
                let (before, old_churn) =
                    measure(|| group(&ranking, &mut old, &history, side, "dance-single", false));
                let (after, new_churn) =
                    measure(|| group(&ranking, &mut new, &history, side, "dance-single", true));
                assert_entries(&before, &after);
                let headers = before
                    .iter()
                    .filter(|entry| matches!(entry, MusicWheelEntry::PackHeader { .. }))
                    .count();
                assert_eq!(old_churn.allocs - new_churn.allocs, count + headers);
                assert_eq!(old_churn.reallocs, new_churn.reallocs);
                assert!(new_churn.allocated_bytes <= old_churn.allocated_bytes);
                assert_eq!(before.capacity(), after.capacity());
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_grade_headings() {
    for (count, mode, side) in [
        (0, "unplayed", None),
        (1, "unplayed", None),
        (1, "same", Some(0)),
        (128, "unplayed", None),
        (128, "mixed", None),
        (128, "same", Some(0)),
        (2048, "unplayed", None),
        (2048, "mixed", None),
        (2048, "mixed", Some(0)),
        (8192, "unplayed", None),
        (8192, "same", Some(1)),
    ] {
        let (songs, history) = grade_fixture(count, mode);
        let mut ranking = score_data::SongRankingIndex::new(&songs);
        ranking.prepare_song_order(song_title_cmp);
        let mut old = score_data::SongRankingWorkspace::default();
        let mut new = score_data::SongRankingWorkspace::default();
        drop(group(
            &ranking,
            &mut old,
            &history,
            side,
            "dance-single",
            false,
        ));
        drop(group(
            &ranking,
            &mut new,
            &history,
            side,
            "dance-single",
            true,
        ));
        let (before, old_churn) =
            measure(|| group(&ranking, &mut old, &history, side, "dance-single", false));
        let (after, new_churn) =
            measure(|| group(&ranking, &mut new, &history, side, "dance-single", true));
        assert_entries(&before, &after);
        assert_eq!(before.capacity(), after.capacity());
        let label = format!(
            "grades-{count}-{mode}-{}",
            side.map_or("both", |side| if side == 0 { "p1" } else { "p2" })
        );
        println!("{label} churn: original {old_churn:?}, current {new_churn:?}");
        crate::paired_bench::compare(&label, 10, |current| {
            black_box(group(
                black_box(&ranking),
                if current { &mut new } else { &mut old },
                black_box(&history),
                side,
                "dance-single",
                current,
            ));
        });
    }
}

fn lobby_fixture(screen: &str) -> State {
    let mut state = init_placeholder();
    state.lobby_view.snapshot = Arc::new(lobby_data::Snapshot {
        connection: lobby_data::ConnectionState::Connected,
        joined_lobby: Some(lobby_data::JoinedLobby {
            code: "ABCD".into(),
            players: (0..3)
                .map(|i| lobby_data::LobbyPlayer {
                    label: format!("Player {i}"),
                    ready: false,
                    screen_name: screen.into(),
                    judgments: None,
                    score: None,
                    ex_score: None,
                })
                .collect(),
            song_info: None,
        }),
        ..lobby_data::Snapshot::default()
    });
    state
}

fn countdown(state: &State) -> Option<i32> {
    lobby_disconnect_hold_elapsed(state)
        .map(|elapsed| ((state.lobby_view.disconnect_hold_seconds - elapsed).ceil() as i32).max(0))
}

fn assert_status(state: &State) -> Option<String> {
    for _ in 0..10 {
        let seconds = countdown(state);
        let before = original::select_music_lobby_status_text(state);
        let after = select_music_lobby_status_text(state);
        if countdown(state) == seconds {
            assert_eq!(after, before);
            return after;
        }
    }
    panic!("could not capture both messages in one countdown second");
}

#[test]
fn lobby_status_preserves_notice_and_all_lock_message_branches() {
    let mut state = init_placeholder();
    assert_eq!(assert_status(&state), None);
    for screen in [
        "ScreenSelectMusic",
        "ScreenGameplay",
        "ScreenEvaluationStage",
        "Other",
        "screengameplay",
    ] {
        state = lobby_fixture(screen);
        assert_status(&state);
        state.lobby_view.reconnect_status_text =
            Some("Reconnecting \u{6771}\u{4eac}\n{remaining}".into());
        assert_status(&state);
        state.lobby_notice_text = Some("Notice\n".into());
        assert_eq!(assert_status(&state).as_deref(), Some("Notice\n"));
        state.lobby_notice_text = Some(String::new());
        assert_eq!(assert_status(&state).as_deref(), Some(""));
        state.lobby_notice_text = None;
        let joined = Arc::make_mut(&mut state.lobby_view.snapshot)
            .joined_lobby
            .as_mut()
            .unwrap();
        joined.players.clear();
        assert_eq!(assert_status(&state), None);
    }
}

#[test]
fn lobby_status_preserves_countdown_pluralization_and_nonfinite_limits() {
    let mut state = lobby_fixture("ScreenGameplay");
    state.lobby_view.reconnect_status_text =
        Some("Reconnect {remaining} / {s} / \u{6771}\u{4eac}".into());
    for holds in [
        [None, None],
        [Some(1.25), None],
        [None, Some(3.25)],
        [Some(1.25), Some(3.25)],
        [Some(60.0), None],
    ] {
        let now = Instant::now();
        state.lobby_disconnect_hold_p1 = holds[0].map(|v| now - Duration::from_secs_f32(v));
        state.lobby_disconnect_hold_p2 = holds[1].map(|v| now - Duration::from_secs_f32(v));
        for limit in [
            5.0,
            4.0,
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            state.lobby_view.disconnect_hold_seconds = limit;
            assert_status(&state);
        }
    }
    state.lobby_disconnect_hold_p1 = Some(Instant::now() - Duration::from_secs(60));
    state.lobby_disconnect_hold_p2 = None;
    state.lobby_view.disconnect_hold_seconds = 5.0;
    assert!(assert_status(&state).unwrap().contains("0"));
}

#[test]
fn lobby_status_removes_temporary_prompt_allocations_without_retaining_more_text() {
    for holding in [false, true] {
        let mut state = lobby_fixture("ScreenGameplay");
        if holding {
            state.lobby_disconnect_hold_p1 = Some(Instant::now() - Duration::from_secs(60));
        }
        assert_status(&state);
        let (before, old) = measure(|| original::select_music_lobby_status_text(&state));
        let (after, new) = measure(|| select_music_lobby_status_text(&state));
        assert_eq!(after, before);
        assert_eq!(old.allocs - new.allocs, if holding { 2 } else { 1 });
        assert!(new.allocated_bytes < old.allocated_bytes);
        assert!(after.unwrap().capacity() <= before.unwrap().capacity());
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_lobby_status() {
    for name in [
        "no-lobby",
        "unlocked",
        "notice",
        "basic",
        "evaluation",
        "holding",
        "reconnecting",
        "long-reconnecting",
    ] {
        let mut state = lobby_fixture(if name == "evaluation" {
            "ScreenEvaluationStage"
        } else {
            "ScreenGameplay"
        });
        match name {
            "no-lobby" => state.lobby_view.snapshot = Arc::default(),
            "unlocked" => {
                for player in &mut Arc::make_mut(&mut state.lobby_view.snapshot)
                    .joined_lobby
                    .as_mut()
                    .unwrap()
                    .players
                {
                    player.screen_name = "ScreenSelectMusic".into();
                }
            }
            "notice" => state.lobby_notice_text = Some("Notice".into()),
            "holding" => {
                state.lobby_disconnect_hold_p1 = Some(Instant::now() - Duration::from_secs(60))
            }
            "reconnecting" => {
                state.lobby_view.reconnect_status_text = Some("Reconnecting...".into())
            }
            "long-reconnecting" => {
                state.lobby_view.reconnect_status_text =
                    Some("Reconnect \u{6771}\u{4eac} ".repeat(100))
            }
            _ => {}
        }
        assert_status(&state);
        let (before, old) = measure(|| original::select_music_lobby_status_text(&state));
        let (after, new) = measure(|| select_music_lobby_status_text(&state));
        assert_eq!(after, before);
        let label = format!("lobby-{name}");
        println!("{label} churn: original {old:?}, current {new:?}");
        println!(
            "{label} retained text bytes: original {}, current {}",
            before.as_ref().map_or(0, String::capacity),
            after.as_ref().map_or(0, String::capacity)
        );
        crate::paired_bench::compare(&label, 100, |current| {
            if current {
                black_box(select_music_lobby_status_text(black_box(&state)));
            } else {
                black_box(original::select_music_lobby_status_text(black_box(&state)));
            }
        });
    }
}
