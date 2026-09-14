use super::*;
use crate::perf;
use deadsync_score::import::{ImportedHighScore, local_score_from_itg, parse_itg_datetime_ms};
use std::hint::black_box;
#[allow(dead_code)]
mod baseline;

fn rows(n: usize, mode: &str, comment_bytes: usize) -> Vec<LeaderboardApiEntry> {
    (0..n)
        .map(|i| LeaderboardApiEntry {
            rank: (i + 1) as u32,
            name: if mode == "name" && i + 1 == n {
                "pLaYeR".into()
            } else {
                format!("Player {i}")
            },
            machine_tag: (i % 3 == 0).then(|| "CAB".into()),
            score: 9800.0 + i as f64 / 10.0,
            date: "2026-09-14 15:30:59".into(),
            is_self: mode == "self" && i + 1 == n,
            is_rival: i % 7 == 0,
            is_fail: false,
            comments: (comment_bytes != 0).then(|| {
                let mut comment = "0w, 2e, 0g, 0d, 0wo, 0m, 99.123% EX ".to_owned();
                comment.push_str(&"x".repeat(comment_bytes.saturating_sub(comment.len())));
                comment
            }),
        })
        .collect()
}
fn response(n: usize, mode: &str, comment_bytes: usize) -> LeaderboardsApiResponse {
    LeaderboardsApiResponse {
        player1: Some(LeaderboardApiPlayer {
            is_ranked: true,
            gs_leaderboard: rows(n, mode, comment_bytes),
            ex_leaderboard: rows(n, mode, 0),
            srpg: Some(LeaderboardEventData {
                name: "rpg".into(),
                srpg_leaderboard: rows(n, mode, 0),
                itl_leaderboard: vec![],
            }),
            itl: Some(LeaderboardEventData {
                name: "ITL 2026".into(),
                itl_leaderboard: rows(n, mode, 0),
                srpg_leaderboard: vec![],
            }),
        }),
    }
}
fn clone_response(source: &LeaderboardsApiResponse) -> LeaderboardsApiResponse {
    fn event(e: &LeaderboardEventData) -> LeaderboardEventData {
        LeaderboardEventData {
            name: e.name.clone(),
            srpg_leaderboard: e.srpg_leaderboard.clone(),
            itl_leaderboard: e.itl_leaderboard.clone(),
        }
    }
    LeaderboardsApiResponse {
        player1: source.player1.as_ref().map(|p| LeaderboardApiPlayer {
            is_ranked: p.is_ranked,
            gs_leaderboard: p.gs_leaderboard.clone(),
            ex_leaderboard: p.ex_leaderboard.clone(),
            srpg: p.srpg.as_ref().map(event),
            itl: p.itl.as_ref().map(event),
        }),
    }
}
fn assert_rows(a: &[LeaderboardEntry], b: &[LeaderboardEntry]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(
            (
                a.rank,
                &a.name,
                &a.machine_tag,
                a.score.to_bits(),
                &a.date,
                a.is_rival,
                a.is_self,
                a.is_fail
            ),
            (
                b.rank,
                &b.name,
                &b.machine_tag,
                b.score.to_bits(),
                &b.date,
                b.is_rival,
                b.is_self,
                b.is_fail
            )
        );
    }
}
fn assert_score(a: Option<&ImportedPlayerScore>, b: Option<&ImportedPlayerScore>) {
    assert_eq!(a.is_some(), b.is_some());
    if let (Some(a), Some(b)) = (a, b) {
        assert_eq!(
            (
                a.score_10000.to_bits(),
                &a.comments,
                a.is_fail,
                a.ex_evidence.leaderboard_score_10000.map(f64::to_bits),
                a.ex_evidence.comment_percent.map(f64::to_bits)
            ),
            (
                b.score_10000.to_bits(),
                &b.comments,
                b.is_fail,
                b.ex_evidence.leaderboard_score_10000.map(f64::to_bits),
                b.ex_evidence.comment_percent.map(f64::to_bits)
            )
        );
    }
}
fn assert_fetched(a: FetchedPlayerLeaderboards, b: FetchedPlayerLeaderboards) {
    assert_eq!(
        (
            a.itl_self_found,
            a.data.srpg_self_score,
            a.data.itl_self_score,
            a.data.itl_self_rank
        ),
        (
            b.itl_self_found,
            b.data.srpg_self_score,
            b.data.itl_self_score,
            b.data.itl_self_rank
        )
    );
    assert_score(a.imported_score.as_ref(), b.imported_score.as_ref());
    assert_eq!(a.data.panes.len(), b.data.panes.len());
    for (a, b) in a.data.panes.iter().zip(&b.data.panes) {
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
        assert_rows(&a.entries, &b.entries);
    }
}
fn assert_import(a: PlayerScoreImportResult, b: PlayerScoreImportResult) {
    assert_eq!(
        (
            a.score_proves_nonquint_ex,
            a.itl_self_found,
            a.itl_self_score,
            a.itl_self_rank
        ),
        (
            b.score_proves_nonquint_ex,
            b.itl_self_found,
            b.itl_self_score,
            b.itl_self_rank
        )
    );
    assert_score(a.score.as_ref(), b.score.as_ref());
}
fn random(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}
fn old_summary(
    entries: &[LeaderboardApiEntry],
    username: &str,
) -> (bool, Option<u32>, Option<u32>) {
    (
        leaderboard_self_entry(entries, username).is_some(),
        leaderboard_self_score_10000(entries, username),
        leaderboard_self_rank(entries, username),
    )
}

#[test]
fn summary_matches_three_searches_for_duplicates_failures_and_missing_players() {
    let mut rng = 790;
    for n in [0, 1, 3, 16, 128] {
        for mode in ["self", "name", "missing"] {
            let mut input = rows(n, mode, 0);
            for _ in 0..100 {
                for row in &mut input {
                    row.name = ["Player", "pLaYeR", " Player ", "\u{130}", "", "\u{2003}"]
                        [random(&mut rng) as usize % 6]
                        .into();
                    row.score = [0.0, -0.0, -100.0, 10001.0, 9876.5, f64::INFINITY, f64::NAN]
                        [random(&mut rng) as usize % 7];
                    row.rank = (random(&mut rng) % 8) as u32;
                    row.is_fail = random(&mut rng) & 1 != 0;
                    row.is_self = random(&mut rng) % 7 == 0;
                }
                for username in ["Player", "missing", " Player ", "", "\u{2003}", "\u{130}"] {
                    assert_eq!(
                        old_summary(&input, username),
                        leaderboard_self_summary(&input, username)
                    );
                }
            }
        }
    }
    let input = rows(128, "name", 0);
    perf::assert_no_churn(|| {
        black_box(leaderboard_self_summary(
            black_box(&input),
            black_box("Player"),
        ));
    });
}
#[test]
fn complete_responses_match_baseline_across_endpoints_and_pane_orders() {
    let mut rng = 901;
    for n in [0, 1, 4, 32] {
        for mode in ["self", "name", "missing"] {
            for case in 0..30 {
                let mut input = response(n, mode, 32);
                if case == 0 {
                    input.player1 = None;
                }
                if let Some(p) = &mut input.player1 {
                    if case % 3 == 0 {
                        p.srpg = None;
                    }
                    if case % 4 == 0 {
                        p.itl = None;
                    }
                    for e in [&mut p.srpg, &mut p.itl].into_iter().flatten() {
                        e.name = ["", "  ", " rPg ", "\u{2003}Event\u{130} "][case % 4].into();
                    }
                    for row in p
                        .gs_leaderboard
                        .iter_mut()
                        .chain(&mut p.ex_leaderboard)
                        .chain(p.itl.iter_mut().flat_map(|e| &mut e.itl_leaderboard))
                    {
                        row.is_self = random(&mut rng) % 5 == 0;
                        row.is_fail = random(&mut rng) & 1 != 0;
                        row.score = [f64::NAN, f64::NEG_INFINITY, -0.0, -1.0, 10001.0, 9955.55]
                            [random(&mut rng) as usize % 6];
                        row.rank = (random(&mut rng) % 7) as u32;
                        row.name = ["Player", "PLAYER", " Player ", "\u{130}", "", "Other"]
                            [random(&mut rng) as usize % 6]
                            .into();
                        row.comments = [
                            None,
                            Some(""),
                            Some("[DS] EX: 99.123%"),
                            Some("\u{1f3b5} 100%"),
                        ][random(&mut rng) as usize % 4]
                            .map(str::to_owned);
                    }
                }
                for username in ["Player", "", " Player ", "\u{130}"] {
                    for show_ex in [false, true] {
                        assert_fetched(
                            baseline::fetched_player_leaderboards_from_api(
                                clone_response(&input),
                                username,
                                show_ex,
                            ),
                            fetched_player_leaderboards_from_api(
                                clone_response(&input),
                                username,
                                show_ex,
                            ),
                        );
                    }
                    for endpoint in [
                        ScoreImportEndpoint::GrooveStats,
                        ScoreImportEndpoint::BoogieStats,
                        ScoreImportEndpoint::ArrowCloud,
                    ] {
                        assert_import(
                            baseline::player_score_import_result_from_api(
                                clone_response(&input),
                                endpoint,
                                username,
                            ),
                            player_score_import_result_from_api(
                                clone_response(&input),
                                endpoint,
                                username,
                            ),
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn owned_comments_keep_their_allocation_and_matching_priority() {
    let mut input = rows(3, "self", 4096);
    input[0].name = "Player".into();
    let ptr = input[2].comments.as_ref().unwrap().as_ptr();
    let old = imported_player_score_from_leaderboard_entries(&input, &[], "Player");
    let mut new = None;
    perf::assert_no_churn(|| {
        new = take_imported_player_score(&mut input, &[], "Player");
    });
    assert_score(old.as_ref(), new.as_ref());
    assert_eq!(
        new.as_ref().unwrap().comments.as_ref().unwrap().as_ptr(),
        ptr
    );
    assert!(input[2].comments.is_none());
    assert!(input[0].comments.is_some());
    for endpoint in [
        ScoreImportEndpoint::GrooveStats,
        ScoreImportEndpoint::BoogieStats,
        ScoreImportEndpoint::ArrowCloud,
    ] {
        let mut input = response(3, "self", 4096);
        let p = input.player1.as_mut().unwrap();
        p.gs_leaderboard[0].name = "Player".into();
        let i = if endpoint.requires_username() { 0 } else { 2 };
        let ptr = p.gs_leaderboard[i].comments.as_ref().unwrap().as_ptr();
        let result = player_score_import_result_from_api(input, endpoint, "Player");
        assert_eq!(
            result
                .score
                .as_ref()
                .unwrap()
                .comments
                .as_ref()
                .unwrap()
                .as_ptr(),
            ptr
        );
    }
    let input = response(10, "self", 4096);
    let old = clone_response(&input);
    let new = clone_response(&input);
    perf::assert_reduced_churn(
        || {
            black_box(baseline::player_score_import_result_from_api(
                old,
                ScoreImportEndpoint::GrooveStats,
                "Player",
            ));
        },
        || {
            black_box(player_score_import_result_from_api(
                new,
                ScoreImportEndpoint::GrooveStats,
                "Player",
            ));
        },
    );
    let input = response(10, "self", 4096);
    let p = input.player1.as_ref().unwrap().gs_leaderboard[9]
        .comments
        .as_ref()
        .unwrap()
        .as_ptr();
    let result = fetched_player_leaderboards_from_api(input, "Player", false);
    assert_eq!(
        result
            .imported_score
            .as_ref()
            .unwrap()
            .comments
            .as_ref()
            .unwrap()
            .as_ptr(),
        p
    );
}

#[test]
#[ignore = "manual before/after CPU, throughput, and allocation benchmark"]
fn benchmark_player_responses() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    benchmark_itg(reverse);
    for (n, mode) in [
        (0, "missing"),
        (1, "self"),
        (10, "self"),
        (128, "self"),
        (128, "name"),
        (128, "missing"),
        (1024, "name"),
    ] {
        let input = rows(n, mode, 0);
        let mut variants = [
            ("old", old_summary as fn(&[LeaderboardApiEntry], &str) -> _),
            (
                "new",
                leaderboard_self_summary as fn(&[LeaderboardApiEntry], &str) -> _,
            ),
        ];
        if reverse {
            variants.reverse();
        }
        for (v, f) in variants {
            let f = black_box(f);
            perf::measure_sampled(&format!("summary/{n}-{mode}/{v}"), 10000, n, || {
                f(black_box(&input), black_box("Player"))
            });
        }
    }
    for bytes in [0, 128, 4096] {
        let input = rows(10, "self", bytes);
        let mut variants = ["old", "new"];
        if reverse {
            variants.reverse();
        }
        for v in variants {
            perf::measure_sampled_with_setup(
                &format!("comment/{bytes}/{v}"),
                1000,
                1,
                || input.clone(),
                |input| {
                    if v == "old" {
                        black_box(black_box(
                            imported_player_score_from_leaderboard_entries
                                as fn(&[LeaderboardApiEntry], &[LeaderboardApiEntry], &str) -> _,
                        )(
                            black_box(input), black_box(&[]), black_box("Player")
                        ));
                    } else {
                        black_box(black_box(
                            take_imported_player_score
                                as fn(
                                    &mut [LeaderboardApiEntry],
                                    &[LeaderboardApiEntry],
                                    &str,
                                ) -> _,
                        )(
                            black_box(input), black_box(&[]), black_box("Player")
                        ));
                    }
                },
            );
        }
    }
    for (n, mode, bytes) in [
        (0, "missing", 0),
        (10, "self", 128),
        (128, "self", 128),
        (128, "name", 128),
        (128, "missing", 0),
        (10, "self", 4096),
    ] {
        let input = response(n, mode, bytes);
        let mut variants = [
            (
                "old",
                baseline::fetched_player_leaderboards_from_api as fn(_, &str, bool) -> _,
            ),
            (
                "new",
                fetched_player_leaderboards_from_api as fn(_, &str, bool) -> _,
            ),
        ];
        if reverse {
            variants.reverse();
        }
        for (v, f) in variants {
            let f = black_box(f);
            perf::measure_sampled_with_setup(
                &format!("response/{n}-{mode}-{bytes}/{v}"),
                1000,
                n * 4,
                || Some(clone_response(&input)),
                |x| {
                    black_box(f(
                        black_box(x.take().unwrap()),
                        black_box("Player"),
                        black_box(true),
                    ));
                },
            );
        }
        let mut variants = [
            (
                "old",
                baseline::player_score_import_result_from_api as fn(_, _, &str) -> _,
            ),
            (
                "new",
                player_score_import_result_from_api as fn(_, _, &str) -> _,
            ),
        ];
        if reverse {
            variants.reverse();
        }
        for (v, f) in variants {
            let f = black_box(f);
            perf::measure_sampled_with_setup(
                &format!("import/{n}-{mode}-{bytes}/{v}"),
                1000,
                n * 4,
                || Some(clone_response(&input)),
                |x| {
                    black_box(f(
                        black_box(x.take().unwrap()),
                        black_box(ScoreImportEndpoint::GrooveStats),
                        black_box("Player"),
                    ));
                },
            );
        }
    }
}

#[test]
fn itg_dates_match_chrono_calendar_and_fallback_semantics() {
    // Include leap years, invalid dates, DST gaps/folds, and boundary years.
    for year in [
        0, 1, 1899, 1900, 1999, 2000, 2023, 2024, 2026, 2100, 2400, 9999,
    ] {
        for month in 0..=13 {
            for day in [0, 1, 28, 29, 30, 31, 32] {
                for (hour, minute, second) in [
                    (0, 0, 0),
                    (1, 30, 0),
                    (2, 30, 0),
                    (12, 34, 56),
                    (23, 59, 59),
                    (23, 59, 60),
                    (24, 0, 0),
                    (12, 60, 0),
                ] {
                    let text =
                        format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}");
                    assert_eq!(
                        baseline::parse_itg_datetime_ms(&text),
                        parse_itg_datetime_ms(&text),
                        "{text}"
                    );
                }
            }
        }
    }
    for text in [
        "",
        "not a date",
        "2026-9-4 1:2:3",
        "2026-09-14T15:30:59",
        "+10000-01-01 00:00:00",
        "-0001-01-01 00:00:00",
        "2026-09-14 15:30:59.123",
        "2026-09-14 15:30:59Z",
        "\u{2003}2026-09-14 15:30:59\u{a0}",
        "2026-03-29 02:30:00",
        "2026-10-25 02:30:00",
        "2016-12-31 23:59:60",
    ] {
        assert_eq!(
            baseline::parse_itg_datetime_ms(text),
            parse_itg_datetime_ms(text),
            "{text:?}"
        );
    }
    let canonical = b"2026-09-14 15:30:59";
    for i in 0..canonical.len() {
        for c in 0..=127 {
            let mut bytes = *canonical;
            bytes[i] = c;
            let s = std::str::from_utf8(&bytes).unwrap();
            assert_eq!(
                baseline::parse_itg_datetime_ms(s),
                parse_itg_datetime_ms(s),
                "{s:?}"
            );
        }
    }
    let mut rng = 377;
    for _ in 0..2048 {
        let mut s = String::new();
        for _ in 0..random(&mut rng) % 30 {
            if let Some(c) = char::from_u32((random(&mut rng) % 0x110000) as u32) {
                s.push(c);
            }
        }
        assert_eq!(
            baseline::parse_itg_datetime_ms(&s),
            parse_itg_datetime_ms(&s),
            "{s:?}"
        );
    }
    // Warm up Chrono's thread-local timezone cache before checking steady state.
    let _ = parse_itg_datetime_ms("2026-09-14 15:30:59");
    perf::assert_no_churn(|| {
        black_box(parse_itg_datetime_ms(black_box("2026-09-14 15:30:59")));
    });
}
fn high_scores(n: usize) -> Vec<ImportedHighScore> {
    (0..n)
        .map(|i| ImportedHighScore {
            grade: if i % 7 == 0 { "Failed" } else { "Tier03" }.into(),
            percent_dp: 0.9825,
            date_time: format!(
                "2026-{:02}-{:02} {:02}:{:02}:{:02}",
                i % 12 + 1,
                i % 28 + 1,
                i % 24,
                i % 60,
                (i * 7) % 60
            ),
            modifiers: "1.1xMusic, Overhead".into(),
            w1: 987,
            w2: 13,
            w3: 2,
            held: 30,
            let_go: 1,
            avoid_mine: 20,
            survive_seconds: 123.4,
            ..Default::default()
        })
        .collect()
}
#[test]
fn full_itg_score_conversion_matches_baseline() {
    for mut input in high_scores(1024) {
        for percent in [0.9825, 0.0, 1.0, -1.0, 2.0, f64::NAN] {
            input.percent_dp = percent;
            // Debug checks every field and preserves NaN equality for this fixture.
            assert_eq!(
                format!("{:?}", baseline::local_score_from_itg(&input)),
                format!("{:?}", local_score_from_itg(&input))
            );
        }
    }
}
fn benchmark_itg(reverse: bool) {
    for (name, input) in [
        ("canonical", "2026-09-14 15:30:59"),
        ("trimmed", "\u{2003}2026-09-14 15:30:59\u{a0}"),
        ("relaxed", "2026-9-4 1:2:3"),
        ("leap-second", "2016-12-31 23:59:60"),
        ("invalid", "not a date"),
        ("invalid-date", "2026-02-30 15:30:59"),
    ] {
        let mut variants = [
            ("old", baseline::parse_itg_datetime_ms as fn(&str) -> _),
            ("new", parse_itg_datetime_ms as fn(&str) -> _),
        ];
        if reverse {
            variants.reverse();
        }
        for (v, f) in variants {
            let f = black_box(f);
            perf::measure_sampled(&format!("datetime/{name}/{v}"), 4000, 1, || {
                f(black_box(input))
            });
        }
    }
    let input = high_scores(512);
    let mut variants = [
        (
            "old",
            baseline::local_score_from_itg as fn(&ImportedHighScore) -> _,
        ),
        ("new", local_score_from_itg as fn(&ImportedHighScore) -> _),
    ];
    if reverse {
        variants.reverse();
    }
    for (v, f) in variants {
        let f = black_box(f);
        perf::measure_sampled(&format!("itg-import/512/{v}"), 100, input.len(), || {
            for score in black_box(&input) {
                black_box(f(black_box(score)));
            }
        });
    }
}
