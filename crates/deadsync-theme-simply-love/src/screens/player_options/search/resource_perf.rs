// Frozen originals from 3b0f61b73348db057f8d2a189ac86d46da9b6513; paired benchmarks exercise the production functions.
use super::*;
use crate::{perf::measure, resource_perf_support::compare};
use deadsync_chart::{ChartData, SongData};
use std::{hint::black_box, path::PathBuf};
fn original_new(row_id: RowId, pane: OptionsPane, label: Arc<str>, score: i32) -> SettingMatch {
    SettingMatch {
        choice_index: None,
        thumb: None,
        row_id,
        pane,
        score,
        row_text: super::search_ranking::SearchRowText::default(),
        pane_text: pane_label(pane),
        retained_text: super::search_text::SearchResultText::default(),
        label,
    }
}
fn original_component_matches(
    state: &State,
    row: RowId,
    player: usize,
    query: &str,
) -> Vec<SettingMatch> {
    let q = fuzzy::prepare_query(query);
    let mut matches = Vec::new();
    for (index, label) in state.pane().row_map.row(row).choices.iter().enumerate() {
        let score = if q.is_empty() {
            0
        } else {
            let Some(score) =
                fuzzy::best_match_score(&q, &fuzzy::fold_diacritics(label.as_str()), &[])
            else {
                continue;
            };
            score
        };
        let mut item = original_new(row, OptionsPane::Display, Arc::from(label.as_str()), score);
        item.choice_index = Some(index);
        item.thumb = state.pack_menu.choice_thumb(state, player, row, index);
        item.pane_text = Arc::from("");
        matches.push(item);
    }
    if !q.is_empty() {
        matches.sort_by(|a, b| b.score.cmp(&a.score));
    }
    matches
}
fn original_rebuild_matches(state: &State, query: &str) -> Vec<SettingMatch> {
    let q = fuzzy::prepare_query(query);
    let active = state.active;
    let mut seen = [false; RowId::COUNT];
    let mut matches: Vec<SettingMatch> = Vec::new();

    for pane in SEARCH_PANE_ORDER {
        let row_map = &state.panes[pane.index()].row_map;
        let visibility =
            visibility::row_visibility(row_map, active, state.option_masks, state.policy);
        for (display_idx, &id) in row_map.display_order().iter().enumerate() {
            if id == RowId::Exit || seen[id.index()] {
                continue;
            }
            let Some(row) = row_map.get(id) else {
                continue;
            };
            if !visibility::is_row_visible(row_map, display_idx, visibility) {
                continue;
            }
            seen[id.index()] = true;

            if let Some((label, score)) =
                super::search_ranking::matched_setting_label(&q, &row.name.get(), row_aliases(id))
            {
                matches.push(original_new(id, pane, label, score));
            }
        }
    }

    if !q.is_empty() {
        matches.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.label.len().cmp(&b.label.len()))
                .then_with(|| a.label.cmp(&b.label))
        });
    }

    matches
}
fn test_song() -> Arc<SongData> {
    Arc::new(SongData {
        simfile_path: PathBuf::from("tests/player-options/test.ssc"),
        title: "Test Song".to_string(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Test Artist".to_string(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: "120".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
        normalized_bpms: "120".to_string(),
        song_timing: None,
        music_length_seconds: 120.0,
        first_second: 0.0,
        total_length_seconds: 120,
        precise_last_second_seconds: 120.0,
        last_second_hint: 0.0,
        charts: vec![test_chart()],
    })
}
fn test_chart() -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "Hard".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 9,
        step_artist: String::new(),
        music_path: None,
        short_hash: "player-options-test".to_string(),
        stats: deadsync_chart::ArrowStats::default(),
        tech_counts: deadsync_chart::TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: deadsync_chart::StaminaCounts::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        matrix_profile: Box::default(),
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: false,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
    }
}

fn state(choices: usize) -> State {
    crate::tests::init_paths();
    crate::i18n::init_for_tests();
    let mut state = super::super::init(
        test_song(),
        [0; 2],
        [0; 2],
        1,
        Screen::SelectMusic,
        None,
        NoteskinCatalogView {
            names: vec!["default".to_owned()],
        },
        SmxGifCatalogView::default(),
        HeartRateDevicesView::default(),
        crate::views::PlayerOptionsInitView::default(),
    );
    state
        .pane_mut()
        .row_map
        .get_mut(RowId::NoteSkin)
        .unwrap()
        .choices = (0..choices)
        .map(|i| {
            let name = match i % 4 {
                0 => "Caf\u{e9}",
                1 => "Cafe\u{301}",
                2 => "Cyber",
                _ => "\u{30ab}\u{30bf}\u{30ab}\u{30ca}",
            };
            deadlib_present::actors::TextContent::from(format!("{name} skin {}", i % 7))
        })
        .collect();
    state
}

#[test]
fn resource_search_matches_original_results_and_stable_ties() {
    for count in [0, 1, 8, 256] {
        let state = state(count);
        for player in [0, 1] {
            for query in ["", " ", "skin", "cafe", "CYBR", "\u{30ab}", "zzzzzzzzz"] {
                let old = original_component_matches(&state, RowId::NoteSkin, player, query);
                let new = component_matches(&state, RowId::NoteSkin, player, query);
                assert_eq!(
                    format!("{old:?}"),
                    format!("{new:?}"),
                    "{count}/{player}/{query}"
                );
                assert!(new.iter().all(|m| m.pane_text.is_empty()));
            }
        }
        for query in ["", "speed", "music", "zzzzzzzzz"] {
            assert_eq!(
                format!("{:?}", original_rebuild_matches(&state, query)),
                format!("{:?}", rebuild_matches(&state, query))
            );
        }
    }
}

#[test]
fn resource_search_shares_empty_labels_without_per_match_allocations() {
    let state = state(256);
    drop(component_matches(&state, RowId::NoteSkin, 0, "")); // warm static
    drop(original_component_matches(&state, RowId::NoteSkin, 0, ""));
    let (before, old) = measure(|| original_component_matches(&state, RowId::NoteSkin, 0, ""));
    let (after, new) = measure(|| component_matches(&state, RowId::NoteSkin, 0, ""));
    assert_eq!(before.len(), 256);
    assert_eq!(old.allocs - new.allocs, after.len());
    assert_eq!(old.allocated_bytes - new.allocated_bytes, after.len() * 16);
    assert!(
        after
            .windows(2)
            .all(|pair| Arc::ptr_eq(&pair[0].pane_text, &pair[1].pane_text))
    );
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_resources_search() {
    for (count, query, label) in [
        (0, "", "empty"),
        (1, "", "one"),
        (8, "", "eight"),
        (256, "", "256-all"),
        (256, "skin", "256-query"),
        (256, "zzzzzzzzz", "256-miss"),
    ] {
        let state = state(count);
        drop(component_matches(&state, RowId::NoteSkin, 0, query));
        compare(
            &format!("search/{label}"),
            || {
                black_box(original_component_matches(
                    black_box(&state),
                    RowId::NoteSkin,
                    0,
                    black_box(query),
                ));
            },
            || {
                black_box(component_matches(
                    black_box(&state),
                    RowId::NoteSkin,
                    0,
                    black_box(query),
                ));
            },
        );
    }

    let state = state(256);
    for (query, label) in [("", "all"), ("music", "query")] {
        drop(original_rebuild_matches(&state, query));
        drop(rebuild_matches(&state, query));
        compare(
            &format!("settings/{label}"),
            || {
                black_box(original_rebuild_matches(
                    black_box(&state),
                    black_box(query),
                ));
            },
            || {
                black_box(rebuild_matches(black_box(&state), black_box(query)));
            },
        );
    }
}
