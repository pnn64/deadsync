use super::*;
use crate::perf;
use std::hint::black_box;

#[allow(dead_code)]
mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/groovestats_preparation/baseline.rs"
    ));
}

fn payload(size: usize, escaped: bool) -> GrooveStatsSubmitPlayerPayload {
    GrooveStatsSubmitPlayerPayload {
        rate: u32::MAX,
        score: 9975,
        judgment_counts: GrooveStatsJudgmentCounts {
            fantastic_plus: u32::MAX,
            decent: Some(0),
            way_off: None,
            total_steps: 8192,
            ..Default::default()
        },
        rescore_counts: GrooveStatsRescoreCounts {
            great: 42,
            ..Default::default()
        },
        used_cmod: true,
        comment: if escaped {
            "\"\\\n\r\t\0雪".repeat(size)
        } else {
            "score 雪 ".repeat(size)
        },
        player_options: "{\"SpeedModType\":2,\"SpeedMod\":650}".repeat(size.max(1)),
    }
}

fn draft(slot: u8, size: usize, escaped: bool) -> GrooveStatsSubmitPlayerDraft {
    GrooveStatsSubmitPlayerDraft::new(
        if slot == 1 {
            profile_data::PlayerSide::P1
        } else {
            profile_data::PlayerSide::P2
        },
        slot,
        format!("hash-{slot}"),
        format!("user-{slot}"),
        format!("profile-{slot}"),
        Some(format!("profile-id-{slot}")),
        Some(9950),
        true,
        format!("test-api-key-{slot}"),
        payload(size, escaped),
    )
}

fn requests(slots: &[u8], size: usize, escaped: bool) -> Vec<GrooveStatsSubmitPlayerRequest> {
    slots
        .iter()
        .enumerate()
        .map(|(index, &slot)| {
            let mut player = draft(slot, size, escaped).player_request();
            player.payload.score = index as u32;
            player.payload.judgment_counts.decent = (index % 2 == 0).then_some(index as u32);
            player.payload.judgment_counts.way_off = (index % 3 == 0).then_some(u32::MAX);
            player
        })
        .collect()
}

fn assert_parts(
    old: &baseline::GrooveStatsSubmitRequestParts,
    new: &GrooveStatsSubmitRequestParts,
) {
    assert_eq!(old.headers, new.headers);
    assert_eq!(old.query, new.query);
    let value: serde_json::Value = serde_json::from_slice(&new.body).unwrap();
    assert_eq!(old.body, value);
    assert_eq!(new.body.first(), Some(&b'{'));
    assert_eq!(new.body.last(), Some(&b'}'));
}

#[test]
fn encoded_requests_preserve_values_headers_duplicates_and_slots() {
    let slots: Vec<_> = (0..=u8::MAX).rev().chain([1, 10, 1, 255, 0]).collect();
    for slots in [
        &[][..],
        &[1][..],
        &[2, 1][..],
        &[2, 2, 1, 1][..],
        &slots[..],
    ] {
        for size in [0, 1, 128] {
            for escaped in [false, true] {
                let players = requests(slots, size, escaped);
                assert_parts(
                    &baseline::submit_request_parts(&players),
                    &submit_request_parts(&players),
                );
            }
        }
    }
}

#[test]
fn consuming_drafts_preserves_jobs_and_retry_requests() {
    for slots in [&[][..], &[1][..], &[2, 1][..], &[2, 2, 1, 255][..]] {
        let players: Vec<_> = slots
            .iter()
            .enumerate()
            .map(|(i, &slot)| {
                let mut player = draft(slot, 16, true);
                if i % 2 == 0 {
                    player.profile_id = None;
                    player.itl_score_hundredths = None;
                    player.show_ex_score = false;
                }
                (player, i as u64 + 123)
            })
            .collect();
        let old = baseline::submit_request_from_drafts(players.clone());
        let new = submit_request_from_drafts(players.clone());
        assert_eq!(format!("{:?}", old.players), format!("{:?}", new.players));
        assert_parts(&old.parts, &new.parts);
        for (player, token) in players {
            let entry = player.retry_entry();
            let before = format!("{entry:?}");
            let old = baseline::retry_submit_request(&entry, token);
            let new = retry_submit_request(&entry, token);
            assert_eq!(format!("{:?}", old.players), format!("{:?}", new.players));
            assert_parts(&old.parts, &new.parts);
            assert_eq!(before, format!("{entry:?}"));
        }
    }
}

#[test]
fn draft_ownership_reuses_string_buffers() {
    let player = draft(1, 128, true);
    let expected = [
        player.chart_hash.as_ptr(),
        player.username.as_ptr(),
        player.profile_name.as_ptr(),
        player.profile_id.as_ref().unwrap().as_ptr(),
        player.payload.comment.as_ptr(),
        player.api_key.as_ptr(),
    ];
    let new = submit_request_from_drafts(vec![(player, 17)]);
    let job = &new.players[0];
    assert_eq!(
        expected,
        [
            job.chart_hash.as_ptr(),
            job.username.as_ptr(),
            job.profile_name.as_ptr(),
            job.profile_id.as_ref().unwrap().as_ptr(),
            job.comment.as_ptr(),
            new.parts.headers[0].1.as_ptr()
        ]
    );
    let player = draft(1, 8, false);
    let bytes = player.chart_hash.len()
        + player.username.len()
        + player.profile_name.len()
        + player.profile_id.as_ref().unwrap().len()
        + player.api_key.len()
        + player.payload.comment.len()
        + player.payload.player_options.len();
    // Seven input-string clones are the entire budget: consuming the draft must
    // add no allocation. Dropping unused fields and the returned job is included.
    perf::assert_churn_budget(7, bytes, || {
        black_box(player.clone().into_player_job(17));
    });
}

fn event(quests: usize, valid_urls: bool, whitespace: bool) -> GrooveStatsSubmitApiEvent {
    let quests: Vec<_> = (0..quests).map(|i| serde_json::json!({
        "title": if whitespace { " \u{2003}\t".to_string() } else { format!(" Quest 雪 {i} \u{2003}") },
        "songDownloadUrl": if valid_urls && i % 3 != 1 { format!(" \u{2003}https://example.invalid/{i}.zip\t") } else { " \u{2003}\r\n".to_string() },
        "songDownloadFolders": if i % 3 == 0 { vec!["Pack/A", "Pack/A", "雪"] } else if i % 3 == 1 { vec![] } else { vec!["Pack/B"] },
        "rewards": []
    })).collect();
    serde_json::from_value(serde_json::json!({"name": if whitespace { " \u{2003}" } else { " Test Event 雪 " }, "progress": { "questsCompleted": quests }})).unwrap()
}

fn response(
    count: usize,
    valid: bool,
    whitespace: bool,
    itl: bool,
    srpg: bool,
) -> GrooveStatsSubmitApiPlayer {
    let mut response: GrooveStatsSubmitApiPlayer = serde_json::from_str("{}").unwrap();
    response.itl = itl.then(|| event(count, valid, whitespace));
    response.srpg = srpg.then(|| event(count, valid, whitespace));
    response
}

#[test]
fn streamed_unlocks_preserve_trimming_order_gates_and_empty_folder_groups() {
    for count in [0, 1, 17, 128] {
        for valid in [false, true] {
            for whitespace in [false, true] {
                let event = event(count, valid, whitespace);
                for profile in ["", " \u{2003}", "  Profile 雪 \u{2003}"] {
                    for separate in [false, true] {
                        assert_eq!(
                            format!(
                                "{:?}",
                                baseline::unlock_downloads_from_submit_event(
                                    &event, profile, separate
                                )
                            ),
                            format!(
                                "{:?}",
                                unlock_downloads_from_submit_event(&event, profile, separate)
                            )
                        );
                    }
                }
            }
        }
    }
    for (itl, srpg) in [(false, false), (true, false), (false, true), (true, true)] {
        for eligible in [false, true] {
            for auto in [false, true] {
                for separate in [false, true] {
                    let mut player = draft(1, 1, false).into_player_job(7);
                    player.itl_score_hundredths = eligible.then_some(9950);
                    let mut response = response(17, true, false, itl, srpg);
                    for remove_progress in [false, true] {
                        if remove_progress {
                            if let Some(event) = response.itl.as_mut() {
                                event.progress = None;
                            }
                            if let Some(event) = response.srpg.as_mut() {
                                event.progress = None;
                            }
                        }
                        assert_eq!(
                            format!(
                                "{:?}",
                                baseline::submit_unlock_plan_from_response(
                                    &player, &response, auto, separate
                                )
                            ),
                            format!(
                                "{:?}",
                                submit_unlock_plan_from_response(
                                    &player, &response, auto, separate
                                )
                            )
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn unlock_outputs_have_bounded_churn_and_empty_plans_allocate_nothing() {
    let player = draft(1, 1, false).into_player_job(0);
    let empty = response(0, false, false, false, false);
    perf::assert_no_churn(|| {
        black_box(submit_unlock_plan_from_response(
            &player, &empty, true, true,
        ));
    });
    let invalid_event = event(128, false, false);
    perf::assert_no_churn(|| {
        black_box(unlock_downloads_from_submit_event(
            &invalid_event,
            "profile",
            false,
        ));
    });
    let event = event(128, true, false);
    let expected = baseline::unlock_downloads_from_submit_event(&event, "profile", false);
    let bytes = expected.len() * size_of::<GrooveStatsUnlockDownload>()
        + expected
            .iter()
            .map(|d| d.url.len() + d.download_name.len() + d.pack_name.len())
            .sum::<usize>();
    // Exactly the final output vector plus three strings per download; no
    // temporary name copies or collection/string growth.
    perf::assert_churn_budget(1 + 3 * expected.len(), bytes, || {
        black_box(unlock_downloads_from_submit_event(&event, "profile", false));
    });
}

#[test]
fn encoded_body_is_sent_as_json_without_double_encoding() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/submit", listener.local_addr().unwrap());
    let parts = submit_request_parts(&requests(&[1, 2], 1, true));
    let expected = parts.body.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("local HTTP accept: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let header_end = loop {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
        let length: usize = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < header_end + length {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        assert!(headers.starts_with("POST /submit?"));
        assert!(headers.contains("maxLeaderboardResults=10"));
        assert!(headers.contains("chartHashP1=hash-1"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("content-type: application/json")
        );
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("x-api-key-player-1: test-api-key-1")
        );
        assert_eq!(&bytes[header_end..header_end + length], expected);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
    });
    let result = submit_score_request_to_url(&url, &parts.headers, &parts.query, &parts.body);
    server.join().unwrap();
    assert!(result.is_ok(), "local submit failed: {result:?}");
}

fn old_parts_ready(
    players: &[GrooveStatsSubmitPlayerRequest],
) -> (baseline::GrooveStatsSubmitRequestParts, Vec<u8>) {
    let parts = baseline::submit_request_parts(players);
    let bytes = serde_json::to_vec_pretty(&parts.body).unwrap();
    (parts, bytes)
}

fn old_drafts_ready(
    players: Vec<(GrooveStatsSubmitPlayerDraft, u64)>,
) -> (baseline::GrooveStatsSubmitRequest, Vec<u8>) {
    let request = baseline::submit_request_from_drafts(players);
    let bytes = serde_json::to_vec_pretty(&request.parts.body).unwrap();
    (request, bytes)
}

fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before = || perf::measure_sampled(&format!("{name}_old"), iterations, units, &mut old);
    let mut after = || perf::measure_sampled(&format!("{name}_new"), iterations, units, &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn groovestats_preparation_bench() {
    for (name, slots, size, escaped) in [
        ("empty", &[][..], 1, false),
        ("single", &[1][..], 1, false),
        ("double", &[1, 2][..], 1, false),
        ("duplicate", &[2, 1, 2][..], 1, false),
        ("long", &[1, 2][..], 256, false),
        ("escaped", &[1, 2][..], 256, true),
    ] {
        let players = requests(slots, size, escaped);
        pair(
            &format!("parts_{name}"),
            512,
            slots.len().max(1),
            || {
                black_box(old_parts_ready as fn(&[GrooveStatsSubmitPlayerRequest]) -> _)(black_box(
                    &players,
                ))
            },
            || {
                black_box(submit_request_parts as fn(&[GrooveStatsSubmitPlayerRequest]) -> _)(
                    black_box(&players),
                )
            },
        );
        let drafts: Vec<_> = slots
            .iter()
            .map(|&slot| (draft(slot, size, escaped), 17))
            .collect();
        pair(
            &format!("drafts_{name}"),
            512,
            slots.len().max(1),
            || old_drafts_ready(black_box(&drafts).clone()),
            || submit_request_from_drafts(black_box(&drafts).clone()),
        );
    }
    for (name, size, escaped) in [
        ("single", 1, false),
        ("long", 256, false),
        ("escaped", 256, true),
    ] {
        let draft = draft(1, size, escaped);
        pair(
            &format!("job_{name}"),
            4000,
            1,
            || black_box(&draft).clone().player_job(17),
            || black_box(&draft).clone().into_player_job(17),
        );
        let retry = draft.retry_entry();
        pair(
            &format!("retry_{name}"),
            512,
            1,
            || {
                let request = baseline::retry_submit_request(black_box(&retry), 17);
                let bytes = serde_json::to_vec_pretty(&request.parts.body).unwrap();
                (request, bytes)
            },
            || retry_submit_request(black_box(&retry), 17),
        );
    }
    for (name, quests, valid, whitespace) in [
        ("empty", 0, false, false),
        ("one", 1, true, false),
        ("many", 128, true, false),
        ("invalid", 128, false, false),
        ("blank_titles", 128, true, true),
    ] {
        let event = event(quests, valid, whitespace);
        pair(
            &format!("downloads_{name}"),
            512,
            quests.max(1),
            || baseline::unlock_downloads_from_submit_event(black_box(&event), " Profile ", false),
            || unlock_downloads_from_submit_event(black_box(&event), " Profile ", false),
        );
        let response = response(quests, valid, whitespace, quests != 0, quests != 0);
        let mut player = draft(1, 1, false).into_player_job(7);
        for (mode, eligible, auto) in [
            ("enabled", true, true),
            ("no_itl", false, true),
            ("no_downloads", true, false),
        ] {
            player.itl_score_hundredths = eligible.then_some(9950);
            pair(
                &format!("plan_{name}_{mode}"),
                256,
                (quests * 2).max(1),
                || {
                    baseline::submit_unlock_plan_from_response(
                        black_box(&player),
                        black_box(&response),
                        auto,
                        true,
                    )
                },
                || {
                    submit_unlock_plan_from_response(
                        black_box(&player),
                        black_box(&response),
                        auto,
                        true,
                    )
                },
            );
        }
    }
}
