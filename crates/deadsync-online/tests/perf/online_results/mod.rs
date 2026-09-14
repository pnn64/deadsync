use crate::{arrowcloud::*, groovestats::*, perf};
use chrono::{DateTime, Utc};
use deadsync_score::*;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    hint::black_box,
};
#[allow(dead_code)]
mod baseline;

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn page(count: usize, offset: usize, mode: &str) -> ArrowCloudLeaderboardPane {
    ArrowCloudLeaderboardPane {
        r#type: "HardEX".into(),
        page: offset as u32 + 1,
        has_next: true,
        total_pages: 8,
        scores: (0..count)
            .map(|i| {
                let id = (i + offset) % 37;
                ArrowCloudLeaderboardEntry {
                    rank: (i + offset + 1) as u32,
                    score: 98.0 + i as f64 / 1000.0,
                    alias: format!("Player {i}"),
                    date: "2026-05-03T19:10:17.504Z".into(),
                    user_id: match mode {
                        "empty" => String::new(),
                        "spaces" => format!("\u{2003}user-{id}\u{a0}"),
                        _ => format!("user-{id}"),
                    },
                    is_self: mode == "all" || (mode != "none" && i % 19 == 0),
                    is_rival: mode == "all" || (mode != "none" && i % 13 == 0),
                }
            })
            .collect(),
    }
}
fn clone_page(page: &ArrowCloudLeaderboardPane) -> ArrowCloudLeaderboardPane {
    ArrowCloudLeaderboardPane {
        r#type: page.r#type.clone(),
        page: page.page,
        has_next: page.has_next,
        total_pages: page.total_pages,
        scores: page
            .scores
            .iter()
            .map(|e| ArrowCloudLeaderboardEntry {
                rank: e.rank,
                score: e.score,
                alias: e.alias.clone(),
                date: e.date.clone(),
                user_id: e.user_id.clone(),
                is_rival: e.is_rival,
                is_self: e.is_self,
            })
            .collect(),
    }
}
fn context() -> ArrowCloudUserContext {
    ArrowCloudUserContext {
        self_user_id: Some("user-1".into()),
        rival_user_ids: ["user-2", "user-7", "user-20"].map(str::to_owned).into(),
    }
}
fn assert_pane(a: &LeaderboardPane, b: &LeaderboardPane) {
    assert_eq!(
        (
            &a.name,
            a.is_ex,
            a.disabled,
            a.personalized,
            a.arrowcloud_kind
        ),
        (
            &b.name,
            b.is_ex,
            b.disabled,
            b.personalized,
            b.arrowcloud_kind
        )
    );
    assert_eq!(a.entries.len(), b.entries.len());
    for (a, b) in a.entries.iter().zip(&b.entries) {
        assert_eq!(
            (
                a.rank,
                &a.name,
                &a.date,
                &a.machine_tag,
                a.is_rival,
                a.is_self,
                a.is_fail,
                a.score.to_bits()
            ),
            (
                b.rank,
                &b.name,
                &b.date,
                &b.machine_tag,
                b.is_rival,
                b.is_self,
                b.is_fail,
                b.score.to_bits()
            )
        );
    }
}
#[test]
fn page_merge_preserves_order_duplicates_identity_flags_and_empty_ids() {
    let context = context();
    let mut rng = 37;
    for count in [0, 1, 5, 32, 128] {
        for mode in ["none", "all", "mixed", "empty", "spaces"] {
            let first = page(count, 0, mode);
            let extra: Vec<_> = (0..3).map(|i| page(count, i * 7, mode)).collect();
            for ctx in [None, Some(&context)] {
                assert_pane(
                    &baseline::hard_ex_pane_from_pages(
                        clone_page(&first),
                        extra.iter().map(clone_page).collect(),
                        ctx,
                    ),
                    &hard_ex_pane_from_pages(
                        clone_page(&first),
                        extra.iter().map(clone_page).collect(),
                        ctx,
                    ),
                );
            }
        }
    }
    for _ in 0..128 {
        let mut first = page(32, 0, "mixed");
        let mut extra = page(128, 0, "spaces");
        for e in first.scores.iter_mut().chain(&mut extra.scores) {
            let r = random(&mut rng);
            e.user_id = [
                "",
                " ",
                "\u{2003}user-1\u{a0}",
                "user-1",
                "USER-1",
                "\u{65e5}",
                "user-2",
            ][r as usize % 7]
                .into();
            e.is_self = r & 8 != 0;
            e.is_rival = r & 16 != 0;
            e.score = [0.0, -0.0, f64::NAN, f64::INFINITY, -20.0, 110.0][(r >> 8) as usize % 6];
        }
        assert_pane(
            &baseline::hard_ex_pane_from_pages(
                clone_page(&first),
                vec![clone_page(&extra)],
                Some(&context),
            ),
            &hard_ex_pane_from_pages(first, vec![extra], Some(&context)),
        );
    }
}
fn player(itl: bool) -> GrooveStatsSubmitPlayerJob {
    GrooveStatsSubmitPlayerJob {
        side: deadsync_profile::PlayerSide::P1,
        slot: 1,
        chart_hash: "hash".into(),
        username: "user".into(),
        profile_name: "profile".into(),
        profile_id: None,
        token: 0,
        itl_score_hundredths: itl.then_some(9750),
        show_ex_score: true,
        score_10000: 9876,
        rate_hundredths: 110,
        comment: String::new(),
    }
}
fn event(rows: usize, quests: usize, comments: usize) -> serde_json::Value {
    let leaders:Vec<_>=(0..rows).map(|i|serde_json::json!({"rank":i+1,"name":format!("user-{i}"),"machineTag":if i%2==0 {Some("TAG")} else {None},"score":9500.0+i as f64,"date":"2026-05-03","isRival":i%3==0,"isSelf":i==7,"isFail":i%17==0,"comments":"x".repeat(comments)})).collect();
    serde_json::json!({"name":"  ITL Event \u{65e5} ","scoreDelta":50,"rateDelta":10,"topScorePoints":1280,"prevTopScorePoints":1300,"totalPasses":42,
        "currentRankingPointTotal":1000,"previousRankingPointTotal":900,"currentSongPointTotal":2000,"previousSongPointTotal":1800,
        "currentExPointTotal":3000,"previousExPointTotal":2800,"currentPointTotal":4000,"previousPointTotal":3800,
        "itlLeaderboard":leaders,"rpgLeaderboard":leaders,"isDoubles":true,
        "progress":{"statImprovements":[{"name":"clearType","gained":2,"current":5},{"name":"rank","gained":4,"current":12},{"name":"ignored","gained":0,"current":2}],
            "skillImprovements":[" Stream ","\u{65e5}"],
            "questsCompleted":(0..quests).map(|i|serde_json::json!({"title":format!("Quest {i}"),"rewards":[{"type":"XP","description":" 100 XP "},{"type":" xp ","description":" 20 XP "},{"type":"ad-hoc","description":"hello"}]})).collect::<Vec<_>>(),
            "achievementsCompleted":(0..quests).map(|i|serde_json::json!({"title":format!("Achievement {i}"),"rewards":[{"tier":"1","requirements":["  FC ",""],"titleUnlocked":" Winner "}]})).collect::<Vec<_>>()}})
}
fn response(
    rows: usize,
    quests: usize,
    comments: usize,
    bits: usize,
) -> GrooveStatsSubmitApiPlayer {
    serde_json::from_value(serde_json::json!({"result":"score-added", "itl":(bits&1!=0).then(||event(rows,quests,comments)),"rpg":(bits&2!=0).then(||event(rows,quests,comments))})).unwrap()
}
fn assert_progress(a: &[ItlEventProgress], b: &[ItlEventProgress]) {
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
}
#[test]
fn event_preparation_preserves_every_output_field_and_eligibility() {
    for bits in 0..4 {
        for enabled in [false, true] {
            for rows in [0, 1, 32] {
                for quests in [0, 3] {
                    let player = player(enabled);
                    let mut response = response(rows, quests, 128, bits);
                    for variant in 0..4 {
                        response.result =
                            ["score-added", "SCORE-ADDED", "improved", "other"][variant].into();
                        for e in response.itl.iter_mut().chain(response.srpg.iter_mut()) {
                            if variant == 1 {
                                e.progress = None;
                            }
                            if variant == 2 {
                                e.name = " \u{2003} ".into();
                                e.score_delta = i32::MIN;
                                e.current_point_total = u32::MAX;
                            }
                            if variant == 3 {
                                e.name = "\u{65e5} ".into();
                                e.rate_delta = i32::MAX;
                                e.previous_point_total = u32::MAX;
                            }
                        }
                        assert_progress(
                            &baseline::event_progress_from_submit_response(&player, &response),
                            &event_progress_from_submit_response(&player, &response),
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn owned_event_api_reuses_leaderboard_buffers_and_preserves_borrowed_api() {
    let response = response(32, 2, 0, 3);
    let player = player(true);
    let input = SubmitEventProgressInput {
        result: response.result.clone(),
        score_10000: player.score_10000,
        rate_hundredths: player.rate_hundredths,
        itl_score_hundredths: player.itl_score_hundredths,
        itl: response
            .itl
            .as_ref()
            .map(|e| submit_event_progress_from_api(e, e.itl_leaderboard.clone())),
        srpg: response
            .srpg
            .as_ref()
            .map(|e| submit_event_progress_from_api(e, e.srpg_leaderboard.clone())),
    };
    let expected = baseline::event_progress_from_submit(&input);
    assert_progress(&expected, &event_progress_from_submit(&input));
    let pointers = [
        input.srpg.as_ref().unwrap().leaderboard.as_ptr(),
        input.itl.as_ref().unwrap().leaderboard.as_ptr(),
    ];
    let owned = event_progress_from_submit_owned(input);
    assert_progress(&expected, &owned);
    for (event, pointer) in owned.iter().zip(pointers) {
        let Some(ItlOverlayPage::Leaderboard(rows)) = event.overlay_pages.last() else {
            panic!("missing leaderboard")
        };
        assert_eq!(rows.as_ptr(), pointer);
    }
}
fn score(date: &str, old: bool) -> ArrowCloudScore {
    let f = if old {
        baseline::arrowcloud_score_from_retrieve_fields
    } else {
        arrowcloud_score_from_retrieve_fields
    };
    f(Some(97.5), Some("Tristar"), Some(date), Some(42), false).unwrap()
}
#[test]
fn timestamp_fast_path_matches_chrono_calendar_fraction_offset_and_invalid_inputs() {
    for year in [0, 1, 4, 100, 400, 1900, 2000, 2024, 2026, 9999] {
        for month in 0..=13 {
            for day in 0..=32 {
                for (hour, minute, second) in [
                    (0, 0, 0),
                    (23, 59, 59),
                    (23, 59, 60),
                    (24, 0, 0),
                    (12, 60, 30),
                ] {
                    for fraction in ["", ".000", ".123", ".999"] {
                        let date = format!(
                            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{fraction}Z"
                        );
                        assert_eq!(score(&date, true), score(&date, false), "{date}");
                    }
                }
            }
        }
    }
    for date in [
        "2026-05-03T19:10:17.504Z",
        "2026-05-03T19:10:17Z",
        "2026-05-03t19:10:17.504z",
        "2026-05-03 19:10:17Z",
        "2026-05-03T19:10:17.1Z",
        "2026-05-03T19:10:17.123456789Z",
        "2026-05-03T19:10:17.123456789123Z",
        "2016-12-31T23:59:60.500Z",
        "2026-05-03T19:10:17+02:30",
        "2026-05-03T19:10:17-00:00",
        " 2026-05-03T19:10:17Z",
        "2026-05-03T19:10:17Z ",
        "+10000-05-03T19:10:17Z",
        "-0001-05-03T19:10:17Z",
        "\u{65e5}",
        "",
        "2026-05-03T19:10:17.\u{65e5}Z",
    ] {
        assert_eq!(score(date, true), score(date, false), "{date}");
    }
    let template = "2026-05-03T19:10:17.504Z";
    for index in 0..template.len() {
        for byte in 0u8..=127 {
            let mut text = template.as_bytes().to_vec();
            text[index] = byte;
            let text = String::from_utf8(text).unwrap();
            assert_eq!(score(&text, true), score(&text, false), "{text:?}");
        }
    }
    let mut random_state = 132;
    for _ in 0..4096 {
        let text: String = (0..(random(&mut random_state) % 40))
            .map(|_| {
                ['0', '1', '9', '-', 'T', ':', 'Z', '.', '\u{65e5}']
                    [random(&mut random_state) as usize % 9]
            })
            .collect();
        assert_eq!(score(&text, true), score(&text, false), "{text:?}");
    }
}
#[test]
fn result_preparation_reduces_churn_and_timestamp_parsing_stays_allocation_free() {
    for bits in [0, 1] {
        let player = player(false);
        let response = response(128, 4, 4096, bits);
        perf::assert_no_churn(|| {
            black_box(event_progress_from_submit_response(&player, &response));
        });
    }

    for date in [
        "2026-05-03T19:10:17Z",
        "2026-05-03T19:10:17.504Z",
        "2016-12-31T23:59:60Z",
    ] {
        perf::assert_no_churn(|| {
            black_box(score(black_box(date), false));
        });
    }
    let p = player(true);
    let response = response(128, 2, 128, 3);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::event_progress_from_submit_response(&p, &response));
        },
        || {
            black_box(event_progress_from_submit_response(&p, &response));
        },
    );
    let first = page(128, 0, "none");
    let extra = page(128, 128, "none");
    let old = (clone_page(&first), vec![clone_page(&extra)]);
    let new = (clone_page(&first), vec![clone_page(&extra)]);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::hard_ex_pane_from_pages(old.0, old.1, None));
        },
        || {
            black_box(hard_ex_pane_from_pages(new.0, new.1, None));
        },
    );
}
fn variants() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    }
}
#[test]
#[ignore = "manual release comparison against 0.5.1223"]
fn benchmark_online_results() {
    let context = context();
    for (label, count, extra_count, mode, ctx) in [
        ("empty", 0, 0, "none", None),
        ("one", 1, 0, "none", None),
        ("page", 128, 0, "none", None),
        ("paged", 128, 4, "mixed", Some(&context)),
        ("duplicates", 128, 4, "all", None),
        ("spaces", 128, 4, "spaces", Some(&context)),
    ] {
        let first = page(count, 0, mode);
        let extra: Vec<_> = (0..extra_count)
            .map(|i| page(count, (i + 1) * 7, mode))
            .collect();
        for new in variants() {
            let f = black_box(if new {
                hard_ex_pane_from_pages
            } else {
                baseline::hard_ex_pane_from_pages
            }
                as fn(
                    ArrowCloudLeaderboardPane,
                    Vec<ArrowCloudLeaderboardPane>,
                    Option<&ArrowCloudUserContext>,
                ) -> LeaderboardPane);
            perf::measure_sampled_with_setup(
                &format!("merge/{label}/{}", if new { "new" } else { "old" }),
                128,
                (count * (extra_count + 1)).max(1),
                || Some((clone_page(&first), extra.iter().map(clone_page).collect())),
                |state| {
                    let (first, extra) = state.take().unwrap();
                    black_box(f(first, extra, black_box(ctx)));
                },
            );
        }
    }
    for (label, rows, quests, comments, bits, enabled) in [
        ("empty", 0, 0, 0, 0, true),
        ("small", 5, 0, 0, 3, true),
        ("events", 128, 4, 128, 3, true),
        ("comments", 128, 0, 4096, 3, true),
        ("ineligible", 128, 4, 128, 1, false),
    ] {
        let p = player(enabled);
        let response = response(rows, quests, comments, bits);
        for new in variants() {
            let f = black_box(if new {
                event_progress_from_submit_response
            } else {
                baseline::event_progress_from_submit_response
            }
                as fn(
                    &GrooveStatsSubmitPlayerJob,
                    &GrooveStatsSubmitApiPlayer,
                ) -> Vec<ItlEventProgress>);
            perf::measure_sampled(
                &format!("events/{label}/{}", if new { "new" } else { "old" }),
                256,
                1,
                || f(black_box(&p), black_box(&response)),
            );
        }
    }
    for (label, date) in [
        ("milliseconds", "2026-05-03T19:10:17.504Z"),
        ("seconds", "2026-05-03T19:10:17Z"),
        ("offset", "2026-05-03T19:10:17+02:00"),
        ("nanoseconds", "2026-05-03T19:10:17.123456789Z"),
        ("leap", "2016-12-31T23:59:60.500Z"),
        ("invalid", "2026-99-03T19:10:17.504Z"),
    ] {
        for new in variants() {
            let f = black_box(if new {
                arrowcloud_score_from_retrieve_fields
            } else {
                baseline::arrowcloud_score_from_retrieve_fields
            }
                as fn(
                    Option<f64>,
                    Option<&str>,
                    Option<&str>,
                    Option<i64>,
                    bool,
                ) -> Option<ArrowCloudScore>);
            perf::measure_sampled(
                &format!("timestamp/{label}/{}", if new { "new" } else { "old" }),
                20_000,
                1,
                || {
                    f(
                        black_box(Some(97.5)),
                        black_box(Some("Tristar")),
                        black_box(Some(date)),
                        black_box(Some(42)),
                        black_box(false),
                    )
                },
            );
        }
    }
    let maps: Vec<_> = (0..512)
        .map(|i| {
            ["2", "3", "4"]
                .map(|id| {
                    (
                        id.to_string(),
                        ArrowCloudRetrieveScoreEntry {
                            score: Some(97.5),
                            grade: Some("Tristar".into()),
                            date: Some(format!("2026-05-{:02}T19:10:17.504Z", i % 28 + 1)),
                            play_id: Some(i),
                            is_fail: false,
                        },
                    )
                })
                .into()
        })
        .collect();
    for new in variants() {
        let f = black_box(if new {
            scores_from_retrieve_entry_map
        } else {
            baseline::scores_from_retrieve_entry_map
        }
            as fn(&HashMap<String, ArrowCloudRetrieveScoreEntry>) -> ArrowCloudScores);
        perf::measure_sampled(
            &format!("bulk/512/{}", if new { "new" } else { "old" }),
            128,
            1536,
            || {
                for map in &maps {
                    black_box(f(black_box(map)));
                }
            },
        );
    }
}
