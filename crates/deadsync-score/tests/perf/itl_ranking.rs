// Reference algorithms frozen from db7a654b3 / 0.5.1138.
use super::*;
use std::hint::black_box;

#[inline(always)]
fn legacy_rank_for_points(sorted_points: &[u32], points: u32) -> Option<u32> {
    sorted_points
        .iter()
        .position(|value| *value == points)
        .map(|idx| idx.saturating_add(1) as u32)
}

fn legacy_rebuild(data: &mut ItlFileData, lookup: impl Fn(&[u32], u32) -> Option<u32>) {
    let mut points: Vec<u32> = data.hash_map.values().map(|entry| entry.points).collect();
    points.sort_unstable_by(|a, b| b.cmp(a));

    let mut points_single = Vec::with_capacity(points.len());
    let mut points_double = Vec::with_capacity(points.len());
    let mut unknown_points = Vec::new();
    let mut plays_single = 0usize;
    let mut plays_double = 0usize;

    for entry in data.hash_map.values_mut() {
        entry.rank = lookup(points.as_slice(), entry.points);
        if entry.steps_type.eq_ignore_ascii_case("single") {
            points_single.push(entry.points);
            plays_single = plays_single.saturating_add(1);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            points_double.push(entry.points);
            plays_double = plays_double.saturating_add(1);
        } else {
            unknown_points.push(entry.points);
        }
    }

    if plays_single > plays_double {
        points_single.extend(unknown_points);
    } else {
        points_double.extend(unknown_points);
    }

    points_single.sort_unstable_by(|a, b| b.cmp(a));
    points_double.sort_unstable_by(|a, b| b.cmp(a));

    for entry in data.hash_map.values_mut() {
        if entry.steps_type.eq_ignore_ascii_case("single") {
            entry.rank = lookup(points_single.as_slice(), entry.points);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            entry.rank = lookup(points_double.as_slice(), entry.points);
        }
    }

    data.points = points;
    data.points_single = points_single;
    data.points_double = points_double;
}

fn scores_fixture(count: usize, mode: &str) -> ItlFileData {
    let mut data = ItlFileData::default();
    data.hash_map.reserve(count);
    for i in 0..count {
        let steps_type = match mode {
            "single" => "single",
            "unknown" => "",
            "balanced" => {
                if i.is_multiple_of(2) {
                    "single"
                } else {
                    "double"
                }
            }
            _ => [
                "single",
                "SINGLE",
                "double",
                "Double",
                "single",
                "",
                " single ",
                "dance-single",
            ][i % 8],
        };
        let points = if mode == "ties" {
            1000
        } else {
            ((i * 7919 + 53) % count.max(1)) as u32 * 100
        };
        data.hash_map.insert(
            format!("chart-{i:05}"),
            ItlHashEntry {
                steps_type: steps_type.into(),
                points,
                rank: Some(u32::MAX),
                ex: (i % 10000) as u32,
                date: "2026-09-12".into(),
                passing_points: (i % 50) as u32,
                ..ItlHashEntry::default()
            },
        );
    }
    data.path_map
        .insert("retained/path".into(), "chart-00000".into());
    data.unlock_folders.insert("retained/unlock".into(), true);
    data.points = vec![u32::MAX; 3];
    data.points_single = vec![13; 7];
    data.points_double = vec![29; 5];
    data
}

#[test]
fn binary_ranks_keep_first_ties_missing_values_and_boundaries() {
    for len in [0, 1, 2, 31, 32, 65, 257, 2048] {
        let mut points: Vec<_> = (0..len).map(|i| ((i * 7919) % 107) as u32).collect();
        points.extend([0, u32::MAX, u32::MAX]);
        points.sort_unstable_by(|a, b| b.cmp(a));
        for value in (0..110).chain([u32::MAX - 1, u32::MAX]) {
            assert_eq!(
                rank_for_points(&points, value),
                legacy_rank_for_points(&points, value)
            );
        }
    }
    assert_eq!(rank_for_points(&[], 0), None);
    assert_eq!(rank_for_points(&[100, 100, 90, 80, 80], 80), Some(4));
    crate::perf::assert_no_churn(|| {
        black_box(rank_for_points(&[100, 100, 90], 90));
    });
}

fn assert_same_rebuild(input: &ItlFileData) {
    let mut expected = input.clone();
    let mut actual = input.clone();
    legacy_rebuild(&mut expected, legacy_rank_for_points);
    itl_rebuild_song_ranks(&mut actual);
    assert_eq!(
        serde_json::to_value(&actual).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    legacy_rebuild(&mut actual, rank_for_points);
    assert_eq!(
        serde_json::to_value(&actual).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
}

#[test]
fn reused_rank_buffers_preserve_all_fields_and_unknown_style_policy() {
    for count in [0, 1, 2, 3, 31, 64, 257, 2048] {
        for mode in ["mixed", "single", "balanced", "unknown", "ties"] {
            assert_same_rebuild(&scores_fixture(count, mode));
        }
    }
    let mut data = scores_fixture(32, "mixed");
    data.hash_map.get_mut("chart-00000").unwrap().points = u32::MAX;
    assert_same_rebuild(&data);
    itl_rebuild_song_ranks(&mut data);
    data.hash_map.retain(|key, _| key.as_str() < "chart-00011");
    for entry in data.hash_map.values_mut() {
        entry.steps_type = "double".into();
    }
    assert_same_rebuild(&data);
    itl_rebuild_song_ranks(&mut data);
    data.hash_map.clear();
    assert_same_rebuild(&data);
}

#[test]
fn rebuilding_reuses_capacity_and_cold_buffers_do_not_grow() {
    let mut data = scores_fixture(2048, "mixed");
    itl_rebuild_song_ranks(&mut data);
    for _ in 0..3 {
        data.hash_map.get_mut("chart-00000").unwrap().points ^= 17;
        crate::perf::assert_no_churn(|| {
            itl_rebuild_song_ranks(black_box(&mut data));
        });
    }
    data.points = Vec::new();
    data.points_single = Vec::new();
    data.points_double = Vec::new();
    crate::perf::assert_churn_budget(3, 2048 * 8, || {
        itl_rebuild_song_ranks(black_box(&mut data));
    });
    assert_eq!(data.points.capacity(), data.points.len());
    assert_eq!(data.points_single.capacity(), data.points_single.len());
    assert_eq!(data.points_double.capacity(), data.points_double.len());
    data.hash_map.clear();
    crate::perf::assert_no_churn(|| {
        itl_rebuild_song_ranks(&mut data);
    });
    assert!(
        data.points.is_empty() && data.points_single.is_empty() && data.points_double.is_empty()
    );
}

fn rebuild_old(data: &mut ItlFileData) {
    legacy_rebuild(data, legacy_rank_for_points);
}
fn rebuild_search_only(data: &mut ItlFileData) {
    legacy_rebuild(data, rank_for_points);
}

type Rebuild = fn(&mut ItlFileData);

fn cold_rebuild(data: &mut ItlFileData, rebuild: Rebuild) {
    // Move the prebuilt input map rather than cloning scores inside the timer.
    let mut cold = ItlFileData {
        hash_map: std::mem::take(&mut data.hash_map),
        ..ItlFileData::default()
    };
    rebuild(black_box(&mut cold));
    black_box(&cold);
    data.hash_map = std::mem::take(&mut cold.hash_map);
}

#[test]
#[ignore = "manual old/search-only/new release benchmark"]
fn score_ranking_bench() {
    for (label, count, mode, cold) in [
        ("small", 32, "mixed", false),
        ("medium", 2048, "mixed", false),
        ("large", 8192, "mixed", false),
        ("ties", 2048, "ties", false),
        ("unknown", 2048, "unknown", false),
        ("cold", 2048, "mixed", true),
    ] {
        let fixture = scores_fixture(count, mode);
        let mut variants: [(&str, Rebuild); 3] = [
            ("old", rebuild_old),
            ("search", rebuild_search_only),
            ("new", itl_rebuild_song_ranks),
        ];
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            variants.reverse();
        }
        for (variant, rebuild) in variants {
            let mut data = fixture.clone();
            let name = format!("itl_{label}_{variant}");
            crate::perf::measure_sampled(
                &name,
                if count < 100 {
                    1000
                } else if count < 4096 {
                    32
                } else {
                    4
                },
                count,
                || {
                    if cold {
                        cold_rebuild(&mut data, rebuild);
                    } else {
                        rebuild(black_box(&mut data));
                        black_box(&data);
                    }
                },
            );
        }
    }
}
