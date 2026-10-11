use super::*;
use crate::perf::measure;
use crate::select_data_support::compare;
use std::hint::black_box;

fn fixture(count: usize) -> (ScoreboxSideView, score_data::LeaderboardPane) {
    let view = ScoreboxSideView {
        display_name: "Local Player".into(),
        groovestats_username: "local".into(),
        player_initials: " P1 ".into(),
        local_itg: Some(crate::views::ScoreboxLocalView {
            score_10000: 9876.0,
            failed: true,
        }),
        ..Default::default()
    };
    let pane = score_data::LeaderboardPane {
        name: "GrooveStats".into(),
        entries: (0..count)
            .map(|index| score_data::LeaderboardEntry {
                rank: index as u32 + 1,
                name: if index == count / 2 {
                    "Local Player".into()
                } else {
                    format!("Player {index:03}")
                },
                machine_tag: Some("TAG".into()),
                score: 9876.0,
                date: "2026-10-10".into(),
                is_rival: index % 3 == 0,
                is_self: index == count / 2,
                is_fail: true,
            })
            .collect(),
        is_ex: false,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    };
    (view, pane)
}

fn assert_original(view: &ScoreboxSideView, pane: &score_data::LeaderboardPane) {
    let before = format!("{pane:?}");
    let original = select_data_original::entries_with_local_self_state(view, pane);
    let current = entries_with_local_self_state(view, pane);
    assert_eq!(format!("{:?}", &*current), format!("{:?}", &*original));
    assert_eq!(format!("{pane:?}"), before);
}

#[test]
fn local_self_reconciliation_matches_original_edge_cases() {
    for (name, is_ex, arrowcloud_kind) in [
        ("GrooveStats", false, None),
        ("GrooveStats", true, None),
        (
            "ArrowCloud",
            true,
            Some(score_data::ArrowCloudPaneKind::HardEx),
        ),
        ("ITL", false, None),
        ("SRPG", false, None),
        ("Other", false, None),
    ] {
        for initials in ["", " \t\n", " P1 ", "Å猫"] {
            for tag in [None, Some(""), Some("other")] {
                for failed in [false, true] {
                    for score in [9876.0, 9876.00001, 9999.0, f64::NAN, f64::INFINITY] {
                        let (mut view, mut pane) = fixture(13);
                        view.player_initials = initials.into();
                        let local = Some(crate::views::ScoreboxLocalView {
                            score_10000: score,
                            failed,
                        });
                        view.local_itg = local;
                        view.local_ex = local;
                        view.local_hard_ex = local;
                        view.local_itl = local;
                        pane.name = name.into();
                        pane.is_ex = is_ex;
                        pane.arrowcloud_kind = arrowcloud_kind;
                        pane.entries[6].machine_tag = tag.map(str::to_owned);
                        for already_failed in [false, true] {
                            pane.entries[6].is_fail = already_failed;
                            assert_original(&view, &pane);
                        }
                        // Matching a name must still mark the row, regardless of its fail flag.
                        pane.entries[6].is_self = false;
                        assert_original(&view, &pane);
                        pane.entries[6].name = "unrelated".into();
                        assert_original(&view, &pane);
                    }
                }
            }
        }
    }
    let (view, mut pane) = fixture(13);
    pane.entries[0].is_self = true;
    pane.entries[0].is_fail = false;
    pane.entries[0].score = 100.0;
    // The first self row takes precedence over a later matching score or name.
    assert_original(&view, &pane);
    let (view, pane) = fixture(0);
    assert_original(&view, &pane);
}

#[test]
fn reconciled_self_rows_borrow_but_real_edits_still_copy() {
    let (mut view, mut pane) = fixture(64);
    for tag in [Some("TAG"), Some(""), None] {
        pane.entries[32].machine_tag = tag.map(str::to_owned);
        view.player_initials = if tag.is_none() { " \t" } else { " P1 " }.into();
        let (entries, churn) = measure(|| entries_with_local_self_state(&view, &pane));
        assert!(matches!(entries, Cow::Borrowed(_)));
        assert!(std::ptr::eq(entries.as_ptr(), pane.entries.as_ptr()));
        assert_eq!(churn.allocs + churn.reallocs + churn.frees, 0);
        assert_original(&view, &pane);
    }
    view.player_initials = " P1 ".into();
    let entries = entries_with_local_self_state(&view, &pane);
    assert!(matches!(entries, Cow::Owned(_)));
    assert_eq!(entries[32].machine_tag.as_deref(), Some("P1"));
    assert_original(&view, &pane);
    pane.entries[32].machine_tag = Some("TAG".into());
    pane.entries[32].is_fail = false;
    let entries = entries_with_local_self_state(&view, &pane);
    assert!(matches!(entries, Cow::Owned(_)));
    assert!(entries[32].is_fail);
    assert!(!pane.entries[32].is_fail);
    assert_original(&view, &pane);
}

#[test]
#[ignore = "paired release benchmark; run serially with --nocapture"]
fn benchmark_select_data_scorebox() {
    for (label, count, mode) in [
        ("scorebox/noop-13", 13, 0),
        ("scorebox/noop-64", 64, 0),
        ("scorebox/noop-empty-tag-64", 64, 1),
        ("scorebox/set-fail-64", 64, 2),
        ("scorebox/set-tag-64", 64, 3),
        ("scorebox/match-name-64", 64, 4),
        ("scorebox/passing-13", 13, 5),
        ("scorebox/empty", 0, 0),
    ] {
        let (mut view, mut pane) = fixture(count);
        if let Some(entry) = pane.entries.get_mut(count / 2) {
            match mode {
                1 => {
                    entry.machine_tag = None;
                    view.player_initials = " \t".into();
                }
                2 => entry.is_fail = false,
                3 => entry.machine_tag = None,
                4 => entry.is_self = false,
                5 => view.local_itg.as_mut().unwrap().failed = false,
                _ => {}
            }
        }
        assert_original(&view, &pane);
        compare(
            label,
            || {
                black_box(select_data_original::entries_with_local_self_state(
                    black_box(&view),
                    black_box(&pane),
                ));
            },
            || {
                black_box(entries_with_local_self_state(
                    black_box(&view),
                    black_box(&pane),
                ));
            },
        );
    }
}
