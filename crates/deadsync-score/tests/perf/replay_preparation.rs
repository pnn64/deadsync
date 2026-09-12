use super::*;
use crate::perf;
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use std::hint::black_box;

pub(crate) mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/replay_preparation/baseline.rs"
    ));
}

pub(crate) fn pair<A, B>(
    name: &str,
    iterations: usize,
    units: usize,
    mut old: impl FnMut() -> A,
    mut new: impl FnMut() -> B,
) {
    let mut before =
        || perf::measure_sampled(&format!("{name}_old"), iterations, units.max(1), &mut old);
    let mut after =
        || perf::measure_sampled(&format!("{name}_new"), iterations, units.max(1), &mut new);
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        after();
        before();
    } else {
        before();
        after();
    }
}

pub(crate) fn edges(count: usize, invalid_every: usize) -> Vec<LocalReplayEdge> {
    (0..count)
        .map(|i| LocalReplayEdge {
            event_music_time_ns: if invalid_every > 0 && i % invalid_every == 0 {
                i64::MIN
            } else {
                (i as i64 - 100) * 1_000_000
            },
            lane: (i % 256) as u8,
            pressed: i % 2 == 0,
            source: (i % 4) as u8,
        })
        .collect()
}

pub(crate) fn assert_replay_equal(old: &MachineReplayEntry, new: &MachineReplayEntry) {
    assert_eq!(old.rank, new.rank);
    assert_eq!(old.name, new.name);
    assert_eq!(old.score.to_bits(), new.score.to_bits());
    assert_eq!(old.date, new.date);
    assert_eq!(old.is_fail, new.is_fail);
    assert_eq!(old.replay_beat0_time_ns, new.replay_beat0_time_ns);
    assert_edges_equal(&old.replay, &new.replay);
}

fn assert_edges_equal(old: &[ReplayEdge], new: &[ReplayEdge]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(old.event_music_time_ns, new.event_music_time_ns);
        assert_eq!(old.lane_index, new.lane_index);
        assert_eq!(old.pressed, new.pressed);
        assert_eq!(old.source, new.source);
    }
}

#[test]
fn dates_preserve_local_time_offsets_extended_years_and_boundaries() {
    for millis in [
        i64::MIN,
        i64::MAX,
        -2_208_988_800_000,
        -1,
        0,
        1,
        1_772_326_799_999,
        1_792_890_000_000,
        4_102_444_800_000,
    ] {
        assert_eq!(
            baseline::local_score_date_string(millis),
            local_score_date_string(millis)
        );
    }
    let mut seed = 79u64;
    for _ in 0..512 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let millis = (seed % 6_000_000_000_000) as i64 - 2_000_000_000_000;
        assert_eq!(
            baseline::local_score_date_string(millis),
            local_score_date_string(millis)
        );
    }
    let mut dates = vec![
        NaiveDate::MIN.and_hms_opt(0, 0, 0).unwrap(),
        NaiveDate::MAX.and_hms_opt(23, 59, 59).unwrap(),
    ];
    for year in [
        -100_000, -1, 0, 1, 999, 1000, 9999, 10_000, 100_000, 2024, 2026,
    ] {
        dates.push(
            NaiveDate::from_ymd_opt(year, 2, 28)
                .unwrap()
                .and_hms_opt(23, 59, 59)
                .unwrap(),
        );
    }
    for naive in dates {
        for seconds in [
            -86_399, -43_200, -18_000, 0, 3_600, 7_200, 19_800, 43_200, 86_399,
        ] {
            let dt = naive
                .and_utc()
                .with_timezone(&FixedOffset::east_opt(seconds).unwrap());
            assert_eq!(
                dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                format_local_score_datetime(dt)
            );
        }
    }
}

#[test]
fn replay_conversion_preserves_order_invalid_times_and_unknown_sources() {
    for count in [0, 1, 31, 4096] {
        for invalid_every in [0, 1, 2, 7] {
            let input = edges(count, invalid_every);
            assert_edges_equal(
                &baseline::replay_edges_from_local(input.clone()),
                &replay_edges_from_local(input),
            );
        }
    }
    let input = [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX]
        .into_iter()
        .flat_map(|time| {
            (0..=255).map(move |source| LocalReplayEdge {
                event_music_time_ns: time,
                lane: source,
                pressed: source % 2 == 0,
                source,
            })
        })
        .collect::<Vec<_>>();
    assert_edges_equal(
        &baseline::replay_edges_from_local(input.clone()),
        &replay_edges_from_local(input),
    );
    for score_percent in [
        0.0,
        -0.0,
        0.123456789,
        -1.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(0x7ff8000000000042),
    ] {
        for rank in [0, 1, u32::MAX] {
            let play = || MachineReplayPlay {
                initials: "雪 ABC".into(),
                score_percent,
                played_at_ms: 1_800_000_000_000,
                is_fail: true,
                replay_beat0_time_ns: i64::MIN + 1,
                replay: edges(31, 3),
            };
            assert_replay_equal(
                &baseline::machine_replay_entry(rank, play()),
                &machine_replay_entry(rank, play()),
            );
            let play = || MachineLeaderboardPlay {
                name: "雪 ABC".into(),
                machine_tag: Some("CAB1".into()),
                score_percent,
                played_at_ms: 1_800_000_000_000,
                is_fail: true,
            };
            let old = baseline::machine_leaderboard_entry(rank, play());
            let new = machine_leaderboard_entry(rank, play());
            assert_eq!(old.score.to_bits(), new.score.to_bits());
            assert_eq!(format!("{old:?}"), format!("{new:?}"));
        }
    }
}

#[test]
fn replay_conversion_reuses_owned_storage_and_dates_need_one_buffer() {
    for invalid_every in [0, 3] {
        let mut input = Vec::with_capacity(512);
        input.extend(edges(257, invalid_every));
        let pointer = input.as_ptr().cast::<u8>();
        let capacity = input.capacity();
        let mut converted = Vec::new();
        perf::assert_no_churn(|| converted = replay_edges_from_local(input));
        assert_eq!(converted.as_ptr().cast::<u8>(), pointer);
        assert_eq!(converted.capacity(), capacity);
    }
    let dt = DateTime::<Utc>::from_timestamp_millis(1_800_000_000_000)
        .unwrap()
        .fixed_offset();
    let mut output = String::new();
    perf::assert_churn_budget(1, 22, || output = format_local_score_datetime(dt));
    assert_eq!(output, dt.format("%Y-%m-%d %H:%M:%S").to_string());
}

#[test]
#[ignore = "manual release benchmark; seven batches and separate allocation accounting"]
fn replay_preparation_bench_conversion() {
    let leaderboard_play = || MachineLeaderboardPlay {
        name: "雪 ABC".into(),
        machine_tag: Some("CAB1".into()),
        score_percent: black_box(0.987654321),
        played_at_ms: black_box(1_800_000_000_000),
        is_fail: false,
    };
    pair(
        "leaderboard_entry",
        8192,
        1,
        || baseline::machine_leaderboard_entry(black_box(1), leaderboard_play()),
        || machine_leaderboard_entry(black_box(1), leaderboard_play()),
    );
    for (label, millis) in [
        ("epoch", 0),
        ("modern", 1_800_000_000_000),
        ("past", -2_208_988_800_000),
        ("invalid", i64::MAX),
    ] {
        pair(
            &format!("date_{label}"),
            8192,
            1,
            || baseline::local_score_date_string(black_box(millis)),
            || local_score_date_string(black_box(millis)),
        );
    }
    for (label, year, offset) in [
        ("utc", 2026, 0),
        ("offset", 2026, 19_800),
        ("negative_year", -100_000, -18_000),
        ("future_year", 100_000, 7_200),
    ] {
        let dt = NaiveDate::from_ymd_opt(year, 9, 12)
            .unwrap()
            .and_hms_opt(14, 30, 45)
            .unwrap()
            .and_utc()
            .with_timezone(&FixedOffset::east_opt(offset).unwrap());
        pair(
            &format!("date_fixed_{label}"),
            8192,
            1,
            || black_box(dt).format("%Y-%m-%d %H:%M:%S").to_string(),
            || format_local_score_datetime(black_box(dt)),
        );
    }
    for count in [0, 64, 4096, 65536] {
        for invalid_every in [0, 3] {
            if count == 0 && invalid_every > 0 {
                continue;
            }
            let input = edges(count, invalid_every);
            pair(
                &format!(
                    "edges_{count}_{}",
                    if invalid_every == 0 { "valid" } else { "mixed" }
                ),
                if count > 4096 { 32 } else { 1024 },
                count,
                || baseline::replay_edges_from_local(black_box(&input).clone()),
                || replay_edges_from_local(black_box(&input).clone()),
            );
        }
    }
    let input = edges(4096, 1);
    pair(
        "edges_all_invalid",
        1024,
        input.len(),
        || baseline::replay_edges_from_local(black_box(&input).clone()),
        || replay_edges_from_local(black_box(&input).clone()),
    );
    for count in [0, 4096, 65536] {
        let input = edges(count, 7);
        let play = || MachineReplayPlay {
            initials: "雪 ABC".into(),
            score_percent: black_box(0.987654321),
            played_at_ms: black_box(1_800_000_000_000),
            is_fail: false,
            replay_beat0_time_ns: -123456789,
            replay: black_box(&input).clone(),
        };
        pair(
            &format!("entry_{count}"),
            if count > 4096 { 32 } else { 1024 },
            count,
            || baseline::machine_replay_entry(black_box(3), play()),
            || machine_replay_entry(black_box(3), play()),
        );
    }
}
