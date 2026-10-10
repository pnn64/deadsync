use super::*;
use std::hint::black_box;

fn entry(i: usize) -> score_data::LeaderboardEntry {
    score_data::LeaderboardEntry {
        rank: (i + 1) as u32,
        name: format!("Player {i}: 日本語"),
        machine_tag: Some(format!("P{i:02}")),
        score: 9876.0 - i as f64,
        date: "2026-10-09 12:34:56".into(),
        is_rival: i % 7 == 4,
        is_self: i == 21,
        is_fail: i % 5 == 2,
    }
}

fn pane(name: &str, count: usize) -> score_data::LeaderboardPane {
    score_data::LeaderboardPane {
        name: name.into(),
        entries: (0..count).map(entry).collect(),
        is_ex: false,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    }
}

fn online(count: usize) -> score_data::CachedPlayerLeaderboardData {
    let mut hidden = pane("ArrowCloud", 0);
    hidden.arrowcloud_kind = Some(score_data::ArrowCloudPaneKind::Ex);
    hidden.personalized = false;
    let mut hard_ex = hidden.clone();
    hard_ex.arrowcloud_kind = Some(score_data::ArrowCloudPaneKind::HardEx);
    let mut ex = pane("GrooveStats", count);
    ex.is_ex = true;
    score_data::CachedPlayerLeaderboardData::ready(score_data::PlayerLeaderboardData {
        panes: vec![pane("GrooveStats", count), hidden, hard_ex, ex],
        srpg_self_score: None,
        itl_self_score: None,
        itl_self_rank: None,
    })
}

fn side(count: usize) -> LeaderboardSideState {
    LeaderboardSideState {
        joined: true,
        panes: vec![pane("GrooveStats", count), pane("ITL Online", count)],
        pane_index: 1,
        show_icons: true,
        machine_pane: Some(gs_machine_pane((0..count).map(entry).collect())),
        chart_hash: Some("current-chart".into()),
        ..Default::default()
    }
}

fn view(count: usize, kind: usize) -> SelectMusicLeaderboardSideView {
    SelectMusicLeaderboardSideView {
        chart_hash: Some(
            if kind == 5 {
                "stale-chart"
            } else {
                "current-chart"
            }
            .into(),
        ),
        machine_entries: (0..count).map(entry).collect(),
        leaderboards: match kind {
            0 | 5 => Some(online(count)),
            1 => Some(score_data::CachedPlayerLeaderboardData::loading()),
            2 | 3 => Some(score_data::CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: Some(Arc::from(if kind == 2 {
                    "TIMEOUT waiting"
                } else {
                    "failed"
                })),
            }),
            4 => None,
            _ => Some(score_data::CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: None,
            }),
        },
    }
}

#[test]
fn moved_leaderboard_snapshots_preserve_state_transitions() {
    for count in [0, 1, 13, 64] {
        for initial_index in [0, 1, usize::MAX] {
            for empty in [false, true] {
                let mut before = side(count);
                if empty {
                    before.panes.clear();
                    before.machine_pane = None;
                }
                before.pane_index = initial_index;
                let mut after = before.clone();
                for kind in [5, 1, 2, 0, 1, 3, 4, 6, 0] {
                    let input = view(count, kind);
                    borrowed_state_original::apply_leaderboard_side_view(
                        &mut before,
                        input.clone(),
                    );
                    apply_leaderboard_side_view(&mut after, input);
                    assert_eq!(format!("{before:?}"), format!("{after:?}"));
                }
            }
        }
    }
    // Matching pane identity includes all five original fields, and the first
    // duplicate still wins after filtering an empty ArrowCloud pane.
    for selected in 0..5 {
        let mut before = side(2);
        let snapshot = online(2);
        before.panes = snapshot.data.as_ref().unwrap().panes.clone();
        before.pane_index = selected;
        let mut after = before.clone();
        borrowed_state_original::apply_leaderboard_side_snapshot(&mut before, snapshot.clone());
        apply_leaderboard_side_snapshot(&mut after, snapshot);
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }
}

#[test]
fn moved_leaderboard_snapshots_remove_machine_copy_and_noop_pane_copy() {
    for kind in [0, 1, 2, 3, 6] {
        let mut before = side(13);
        let mut after = before.clone();
        let old_input = view(13, kind);
        let new_input = old_input.clone();
        let (_, old) = crate::perf::measure(|| {
            borrowed_state_original::apply_leaderboard_side_view(&mut before, old_input)
        });
        let (_, new) = crate::perf::measure(|| apply_leaderboard_side_view(&mut after, new_input));
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
        assert!(
            old.allocs >= new.allocs + 40,
            "kind={kind}: {old:?} -> {new:?}"
        );
        assert!(old.allocated_bytes > new.allocated_bytes);
    }
}

fn overlay(count: usize, kind: usize) -> LeaderboardOverlayStateData {
    let mut p1 = side(count);
    p1.pane_index = 0;
    p1.loading = kind == 1;
    p1.error_text = (kind == 2).then(|| "Timed Out".into());
    p1.panes[0].disabled = kind == 3;
    if kind == 4 {
        p1.scorebox = ScoreboxSideView {
            display_name: Arc::from("Player 21: 日本語"),
            player_initials: Arc::from(" ME "),
            local_itg: Some(crate::views::ScoreboxLocalView {
                score_10000: 9855.0,
                failed: true,
            }),
            ..Default::default()
        };
        if count > 21 {
            p1.panes[0].entries[21].is_self = false;
            p1.panes[0].entries[21].machine_tag = None;
        }
    }
    let mut p2 = side(count);
    p2.joined = kind == 5;
    p2.pane_index = 0;
    p2.panes[0].arrowcloud_kind = Some(score_data::ArrowCloudPaneKind::HardEx);
    LeaderboardOverlayStateData {
        elapsed: 0.35,
        p1,
        p2,
        presentation_revision: 0,
        presentation: RefCell::new(None),
    }
}

fn render<const INCLUDE_ICONS: bool>(
    overlay: &LeaderboardOverlayStateData,
    original: bool,
    font: MachineFont,
) -> Vec<Actor> {
    let joined_count = usize::from(overlay.p1.joined) + usize::from(overlay.p2.joined);
    let mut actors = Vec::with_capacity(if joined_count <= 1 { 73 } else { 118 });
    if original {
        borrowed_state_original::push_leaderboard_overlay_unreserved::<INCLUDE_ICONS>(
            &mut actors,
            overlay,
            font,
        );
    } else {
        push_leaderboard_overlay_unreserved::<INCLUDE_ICONS>(&mut actors, overlay, font);
    }
    actors
}

#[test]
fn borrowed_leaderboard_rows_preserve_complete_actor_trees() {
    for count in [0, 1, 13, 14, 64] {
        for kind in 0..6 {
            for font in [MachineFont::Wendy, MachineFont::Mega] {
                let mut overlay = overlay(count, kind);
                for selection in [0, 1, usize::MAX] {
                    overlay.p1.pane_index = selection;
                    let before = render::<true>(&overlay, true, font);
                    let after = render::<true>(&overlay, false, font);
                    assert_eq!(
                        format!("{before:?}"),
                        format!("{after:?}"),
                        "rows={count}, kind={kind}, selection={selection}, font={font:?}"
                    );
                    assert_eq!(
                        format!("{:?}", render::<false>(&overlay, true, font)),
                        format!("{:?}", render::<false>(&overlay, false, font)),
                        "cached panel rows={count}, kind={kind}, selection={selection}, font={font:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn borrowed_leaderboard_rows_remove_temporary_owned_entries() {
    for kind in 0..6 {
        let overlay = overlay(64, kind);
        let (before, old) =
            crate::perf::measure(|| render::<false>(&overlay, true, MachineFont::Wendy));
        let (after, new) =
            crate::perf::measure(|| render::<false>(&overlay, false, MachineFont::Wendy));
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
        assert!(
            old.allocs >= new.allocs + 39,
            "kind={kind}: {old:?} -> {new:?}"
        );
        assert!(old.allocated_bytes > new.allocated_bytes);
        assert!(
            new.peak_added_bytes <= old.peak_added_bytes,
            "kind={kind}: {old:?} -> {new:?}"
        );
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ui_borrowed_state_snapshots() {
    for (name, count, kind) in [
        ("ready-13", 13, 0),
        ("ready-64", 64, 0),
        ("loading-13", 13, 1),
        ("error-13", 13, 2),
        ("local-only-13", 13, 4),
        ("stale-13", 13, 5),
        ("empty-ready", 0, 0),
    ] {
        let input = view(count, kind);
        let mut before = side(count);
        let mut after = before.clone();
        crate::ui_borrowed_state_support::compare(
            &format!("snapshots/{name}"),
            || {
                borrowed_state_original::apply_leaderboard_side_view(
                    black_box(&mut before),
                    black_box(&input).clone(),
                );
                black_box(&before);
            },
            || {
                apply_leaderboard_side_view(black_box(&mut after), black_box(&input).clone());
                black_box(&after);
            },
        );
        assert_eq!(format!("{before:?}"), format!("{after:?}"));
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ui_borrowed_state_render() {
    for (name, count, kind) in [
        ("empty", 0, 0),
        ("single-13", 13, 0),
        ("single-64", 64, 0),
        ("double-13", 13, 5),
        ("loading-64", 64, 1),
        ("error-64", 64, 2),
        ("disabled-64", 64, 3),
        ("local-self-64", 64, 4),
    ] {
        let overlay = overlay(count, kind);
        crate::ui_borrowed_state_support::compare(
            &format!("render/{name}"),
            || {
                black_box(render::<false>(
                    black_box(&overlay),
                    true,
                    MachineFont::Wendy,
                ));
            },
            || {
                black_box(render::<false>(
                    black_box(&overlay),
                    false,
                    MachineFont::Wendy,
                ));
            },
        );
    }
}
