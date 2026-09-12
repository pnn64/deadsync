// Reference algorithms frozen from 3494eee4b / 0.5.1139.
use super::*;
use std::hint::black_box;

impl OnlineItlSelfCacheState {
    fn legacy_set_value(
        &mut self,
        profile_id: Option<&str>,
        api_key: &str,
        chart_hash: &str,
        value: Option<u32>,
    ) -> OnlineItlSelfCacheUpdate {
        let api_key = api_key.trim();
        let chart_hash = chart_hash.trim();
        if api_key.is_empty() || chart_hash.is_empty() {
            return OnlineItlSelfCacheUpdate {
                changed: false,
                profile_snapshot: None,
            };
        }

        let key = OnlineItlSelfScoreKey {
            chart_hash: chart_hash.to_string(),
            api_key: api_key.to_string(),
        };
        let session_changed = if let Some(value) = value {
            self.session_by_key.insert(key.clone(), value) != Some(value)
        } else {
            self.session_by_key.remove(&key).is_some()
        };

        let Some(profile_id) = profile_id.map(str::trim).filter(|id| !id.is_empty()) else {
            return OnlineItlSelfCacheUpdate {
                changed: session_changed,
                profile_snapshot: None,
            };
        };
        let Some(profile_values) = self.loaded_profiles.get_mut(profile_id) else {
            return OnlineItlSelfCacheUpdate {
                changed: session_changed,
                profile_snapshot: None,
            };
        };
        let profile_changed = if let Some(value) = value {
            profile_values.insert(key, value) != Some(value)
        } else {
            profile_values.remove(&key).is_some()
        };

        OnlineItlSelfCacheUpdate {
            changed: session_changed || profile_changed,
            profile_snapshot: profile_changed
                .then(|| (profile_id.to_string(), profile_values.clone())),
        }
    }
}

fn legacy_apply_itl_overall_ranks(
    out: &mut OnlineItlOverallRanks,
    mut by_chart_points: Vec<(String, u32)>,
) {
    by_chart_points.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut prev_points = None;
    let mut prev_rank = 0u32;
    for (idx, (chart_hash, points)) in by_chart_points.into_iter().enumerate() {
        let rank = if prev_points == Some(points) {
            prev_rank
        } else {
            idx.saturating_add(1) as u32
        };
        out.insert(chart_hash, rank);
        prev_points = Some(points);
        prev_rank = rank;
    }
}

fn legacy_overall_ranks(
    song_cache: &[deadsync_chart::SongPack],
    by_chart_score: &HashMap<String, u32>,
) -> OnlineItlOverallRanks {
    if by_chart_score.is_empty() {
        return OnlineItlOverallRanks::default();
    }

    let mut single_points = Vec::new();
    let mut double_points = Vec::new();
    for pack in song_cache {
        if !itl_group_name_matches(pack.group_name.as_str()) {
            continue;
        }
        for song in &pack.songs {
            for chart in &song.charts {
                if !chart.has_note_data {
                    continue;
                }
                let Some(ex_hundredths) = by_chart_score.get(chart.short_hash.as_str()).copied()
                else {
                    continue;
                };
                let Some(points) = itl_points_for_chart(chart, ex_hundredths) else {
                    continue;
                };
                if itl_steps_type_from_chart_type(chart.chart_type.as_str())
                    .eq_ignore_ascii_case("double")
                {
                    double_points.push((chart.short_hash.clone(), points));
                } else {
                    single_points.push((chart.short_hash.clone(), points));
                }
            }
        }
    }

    let mut ranks = OnlineItlOverallRanks::with_capacity_and_hasher(
        single_points.len() + double_points.len(),
        FxBuildHasher,
    );
    legacy_apply_itl_overall_ranks(&mut ranks, single_points);
    legacy_apply_itl_overall_ranks(&mut ranks, double_points);
    ranks
}

fn copy_state(state: &OnlineItlSelfCacheState) -> OnlineItlSelfCacheState {
    OnlineItlSelfCacheState {
        session_by_key: state.session_by_key.clone(),
        loaded_profiles: state.loaded_profiles.clone(),
    }
}

fn cache_fixture(count: usize, apis: usize, overlap: bool) -> OnlineItlSelfCacheState {
    let mut state = OnlineItlSelfCacheState::default();
    let mut profile = OnlineItlSelfCacheMap::default();
    for api in 0..apis {
        for index in 0..count {
            let key = OnlineItlSelfScoreKey {
                chart_hash: format!("{index:016x}"),
                api_key: format!("api-{api:02}"),
            };
            profile.insert(key.clone(), index as u32);
            let session_key = if overlap {
                key
            } else {
                OnlineItlSelfScoreKey {
                    chart_hash: format!("s{index:015x}"),
                    ..key
                }
            };
            state.session_by_key.insert(session_key, index as u32 + 17);
        }
    }
    state.loaded_profiles.insert("profile".into(), profile);
    state
}

#[test]
fn borrowed_updates_preserve_state_changes_and_detached_snapshots() {
    let mut actual = cache_fixture(8, 2, true);
    actual
        .loaded_profiles
        .insert("other".into(), OnlineItlSelfCacheMap::default());
    let mut expected = copy_state(&actual);
    for turn in 0..400 {
        let profile = [
            None,
            Some("profile"),
            Some(" profile "),
            Some("other"),
            Some("missing"),
            Some(" "),
        ][turn % 6];
        let api = ["api-00", " api-01 ", "", " \t", "cl\u{e9}"][turn % 5];
        let chart = [
            "0000000000000000",
            " 0000000000000001 ",
            "new",
            "",
            " \n",
            "\u{97f3}\u{697d}",
        ][turn / 5 % 6];
        let value = [None, Some(0), Some(18), Some(u32::MAX), None][turn / 7 % 5];
        let got = actual.set_value(profile, api, chart, value);
        let want = expected.legacy_set_value(profile, api, chart, value);
        assert_eq!(got.changed, want.changed);
        assert_eq!(got.profile_snapshot, want.profile_snapshot);
        assert_eq!(actual.session_by_key, expected.session_by_key);
        assert_eq!(actual.loaded_profiles, expected.loaded_profiles);
        if let Some((_, mut snapshot)) = got.profile_snapshot {
            snapshot.clear();
            assert_eq!(actual.loaded_profiles, expected.loaded_profiles);
        }
    }
}

#[test]
fn repeated_cache_updates_and_missing_deletes_have_no_churn() {
    let mut state = cache_fixture(8, 1, true);
    // Profile and session already agree, so no persistence snapshot is needed.
    state.set_value(Some("profile"), "api-00", "0000000000000000", Some(17));
    crate::perf::assert_no_churn(|| {
        for _ in 0..32 {
            let result = state.set_value(
                Some(" profile "),
                " api-00 ",
                " 0000000000000000 ",
                Some(17),
            );
            assert!(!result.changed && result.profile_snapshot.is_none());
            assert!(
                !state
                    .set_value(Some("profile"), "api-00", "missing", None)
                    .changed
            );
            black_box(state.set_value(None, "api-00", "0000000000000001", Some(42)));
        }
    });
    // Inserting one session key allocates only its two strings in a pre-sized map.
    state.session_by_key.reserve(1);
    crate::perf::assert_churn_budget(2, 32, || {
        assert!(
            state
                .set_value(None, "new-api", "new-chart", Some(1))
                .changed
        );
    });
}

fn overall_fixture(
    count: usize,
    mode: &str,
) -> (Vec<deadsync_chart::SongPack>, HashMap<String, u32>) {
    let mut pack = song_pack("ITL Online 2026", Vec::new());
    pack.songs.clear();
    let mut scores = HashMap::new();
    let mut charts = Vec::new();
    for i in 0..count {
        let unique = if mode == "duplicates" { i / 4 } else { i };
        let hash = format!("{unique:016x}");
        let steps = [
            "dance-single",
            "dance-double",
            "pump-single",
            "DANCE-DOUBLE",
        ][i % 4];
        let points = if mode == "ties" {
            "7500 12000".into()
        } else {
            format!("{} {}", i % 2500 + 100, i % 100 * 100)
        };
        let mut chart = ranked_chart(&hash, steps, &points);
        if mode == "filtered" {
            chart.has_note_data = i % 3 != 0;
            if i % 5 == 0 {
                chart.chart_name = "no points".into();
            }
        }
        charts.push(chart);
        if mode != "filtered" || i % 7 != 0 {
            scores.insert(
                hash,
                if mode == "ties" {
                    10000
                } else {
                    7000 + unique as u32 % 3001
                },
            );
        }
        if charts.len() == 4 || i + 1 == count {
            pack.songs
                .push(song_with_charts(std::mem::take(&mut charts)));
        }
    }
    let ignored = song_pack(
        "Custom Pack",
        vec![ranked_chart("ignored", "dance-single", "999 999")],
    );
    (vec![pack, ignored], scores)
}

#[test]
fn borrowed_overall_ranks_preserve_ties_filters_and_duplicate_hashes() {
    for count in [0, 1, 2, 3, 31, 64, 257, 2048] {
        for mode in ["mixed", "ties", "duplicates", "filtered"] {
            let (packs, scores) = overall_fixture(count, mode);
            assert_eq!(
                itl_overall_ranks_from_song_cache(&packs, &scores),
                legacy_overall_ranks(&packs, &scores)
            );
        }
    }
    let (packs, scores) = overall_fixture(2048, "duplicates");
    let ranks = itl_overall_ranks_from_song_cache(&packs, &scores);
    assert_eq!(ranks.len(), 512);
    assert!(
        ranks.capacity() < 1024,
        "duplicate charts over-reserved the output table"
    );
    // Repeated hashes take the later (lower) rank within a style, then doubles
    // overwrite singles. Removing hash tie ordering must preserve both rules.
    let mut expected = OnlineItlOverallRanks::default();
    let mut actual = OnlineItlOverallRanks::default();
    let singles = [("same", 100), ("other", 100), ("same", 50), ("third", 0)];
    let doubles = [("fourth", 200), ("same", 100), ("same", 100)];
    for entries in [&singles[..], &doubles[..]] {
        legacy_apply_itl_overall_ranks(
            &mut expected,
            entries.iter().map(|(h, p)| ((*h).into(), *p)).collect(),
        );
        apply_itl_overall_ranks(&mut actual, entries.to_vec());
    }
    assert_eq!(actual, expected);
    assert_eq!(actual["same"], 2);
}

#[test]
fn tied_rank_updates_allocate_only_new_distinct_output_keys() {
    let bytes = 256 * std::mem::size_of::<(&str, u32)>();
    let mut out = OnlineItlOverallRanks::with_capacity_and_hasher(1, FxBuildHasher);
    crate::perf::assert_churn_budget(2, bytes + 4, || {
        apply_itl_overall_ranks(&mut out, vec![("same", 100); 256]);
    });
    assert_eq!(out["same"], 1);
}

#[test]
#[ignore = "manual old/new online ITL cache and ranking benchmark"]
fn online_itl_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for label in [
        "unchanged",
        "session_change",
        "missing_delete",
        "profile_change",
        "insert",
    ] {
        let fixture = cache_fixture(if label == "profile_change" { 256 } else { 64 }, 1, true);
        for old in [!reverse, reverse] {
            let mut state = copy_state(&fixture);
            for index in 0..64 {
                state.set_value(
                    Some("profile"),
                    "api-00",
                    &format!("{index:016x}"),
                    Some(index as u32 + 17),
                );
            }
            let hashes: Vec<_> = (0..64).map(|index| format!("{index:016x}")).collect();
            let mut value = 999u32;
            crate::perf::measure_sampled(
                &format!("cache_{label}_{}", if old { "old" } else { "new" }),
                256,
                if label == "profile_change" { 1 } else { 64 },
                || {
                    value ^= 1;
                    let mut fresh = OnlineItlSelfCacheState::default();
                    let target = if label == "insert" {
                        &mut fresh
                    } else {
                        &mut state
                    };
                    for (index, hash) in hashes
                        .iter()
                        .take(if label == "profile_change" { 1 } else { 64 })
                        .enumerate()
                    {
                        let (profile, hash, score) = match label {
                            "unchanged" => {
                                (Some("profile"), hash.as_str(), Some(index as u32 + 17))
                            }
                            "missing_delete" => (Some("profile"), "missing", None),
                            "profile_change" => (Some("profile"), hash.as_str(), Some(value)),
                            _ => (None, hash.as_str(), Some(value)),
                        };
                        let result = if old {
                            target.legacy_set_value(profile, "api-00", hash, score)
                        } else {
                            target.set_value(profile, "api-00", hash, score)
                        };
                        black_box(result);
                    }
                    black_box(target.session_by_key.len());
                },
            );
        }
    }
    for (label, joined, profile, api, invalidating) in [
        (
            "profile",
            true,
            Some("perf-rank-profile"),
            "perf-rank-api",
            false,
        ),
        ("session", true, None, "perf-rank-api", false),
        (
            "long_ids",
            true,
            Some("performance-profile-with-a-long-identifier-\u{97f3}\u{697d}"),
            "performance-api-with-a-long-identifier-\u{97f3}\u{697d}",
            false,
        ),
        (
            "invalidating",
            true,
            Some("perf-rank-profile"),
            "perf-rank-api",
            true,
        ),
        ("not_joined", false, None, "perf-rank-api", false),
    ] {
        for old in [!reverse, reverse] {
            let mut generation = 1u64;
            // Warm lazy state and the profile/cache before timing either path.
            black_box(runtime_online_itl_overall_ranks_for_side(
                0,
                joined,
                api,
                profile,
                generation,
                &[],
                |_| HashMap::new(),
            ));
            crate::perf::measure_sampled(
                &format!("rank_hit_{label}_{}", if old { "old" } else { "new" }),
                2048,
                1,
                || {
                    if invalidating {
                        generation += 1;
                    }
                    let output = if old {
                        legacy_runtime_ranks(
                            black_box(0),
                            black_box(joined),
                            black_box(api),
                            black_box(profile),
                            black_box(generation),
                            &[],
                            |_| HashMap::new(),
                        )
                    } else {
                        runtime_online_itl_overall_ranks_for_side(
                            black_box(0),
                            black_box(joined),
                            black_box(api),
                            black_box(profile),
                            black_box(generation),
                            &[],
                            |_| HashMap::new(),
                        )
                    };
                    black_box(output);
                },
            );
        }
    }
    for (label, count, mode) in [
        ("small", 32, "mixed"),
        ("medium", 2048, "mixed"),
        ("large", 8192, "mixed"),
        ("ties", 2048, "ties"),
        ("duplicates", 2048, "duplicates"),
        ("filtered", 2048, "filtered"),
    ] {
        let (packs, scores) = overall_fixture(count, mode);
        for old in [!reverse, reverse] {
            crate::perf::measure_sampled(
                &format!("overall_{label}_{}", if old { "old" } else { "new" }),
                if count < 100 { 256 } else { 32 },
                count,
                || {
                    let result = if old {
                        legacy_overall_ranks(black_box(&packs), black_box(&scores))
                    } else {
                        itl_overall_ranks_from_song_cache(black_box(&packs), black_box(&scores))
                    };
                    black_box(result);
                },
            );
        }
    }
}

fn legacy_cached_ranks(
    side_idx: usize,
    key: &OnlineItlOverallRankCacheKey,
) -> Option<Arc<OnlineItlOverallRanks>> {
    let cache = ONLINE_ITL_OVERALL_RANK_CACHE.lock().unwrap();
    online_itl_overall_rank_entry_for_side(&cache, side_idx)
        .filter(|entry| entry.key == *key)
        .map(|entry| entry.ranks.clone())
}

fn legacy_runtime_ranks<L>(
    side_idx: usize,
    side_joined: bool,
    api_key: &str,
    profile_id: Option<&str>,
    song_cache_generation: u64,
    song_cache: &[deadsync_chart::SongPack],
    mut load_profile: L,
) -> Arc<OnlineItlOverallRanks>
where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    if !side_joined {
        return empty_online_itl_overall_ranks();
    }

    let api_key = api_key.trim();
    if api_key.is_empty() {
        return empty_online_itl_overall_ranks();
    }

    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        runtime_ensure_online_itl_self_score_profile_loaded(profile_id, &mut load_profile);
    }

    let self_score_generation = online_itl_self_score_generation();
    let key = OnlineItlOverallRankCacheKey {
        api_key: api_key.to_string(),
        profile_id: profile_id.map(str::to_string),
        song_cache_generation,
        self_score_generation,
    };
    if let Some(ranks) = legacy_cached_ranks(side_idx, &key) {
        return ranks;
    }

    let by_chart_score = online_itl_self_scores_by_chart_for_api(profile_id, api_key);
    let ranks = Arc::new(legacy_overall_ranks(song_cache, &by_chart_score));
    store_online_itl_overall_ranks_for_side(side_idx, key, ranks)
}

#[test]
fn borrowed_rank_cache_keys_preserve_identity_side_mapping_and_invalidation() {
    let ranks = Arc::new(OnlineItlOverallRanks::from_iter([("chart".into(), 7)]));
    let other = Arc::new(OnlineItlOverallRanks::from_iter([("chart".into(), 42)]));
    let owned = OnlineItlOverallRankCacheKey {
        api_key: " api-\u{97f3} ".into(),
        profile_id: Some(" profile-\u{97f3} ".into()),
        song_cache_generation: 23,
        self_score_generation: u64::MAX,
    };
    let key = OnlineItlOverallRankCacheKeyRef {
        api_key: &owned.api_key,
        profile_id: owned.profile_id.as_deref(),
        song_cache_generation: 23,
        self_score_generation: u64::MAX,
    };
    let mut state = OnlineItlOverallRankCacheState::default();
    assert!(state.get(0, key).is_none());
    state.p1 = Some(OnlineItlOverallRankCacheEntry {
        key: owned.clone(),
        ranks: ranks.clone(),
    });
    state.p2 = Some(OnlineItlOverallRankCacheEntry {
        key: owned.clone(),
        ranks: other.clone(),
    });
    for side in [0, 1, 2, usize::MAX] {
        let expected = if side == 1 { &other } else { &ranks };
        let found = state.get(side, key).unwrap();
        assert!(Arc::ptr_eq(&found, expected));
        for changed in [
            OnlineItlOverallRankCacheKeyRef {
                api_key: "api-\u{97f3}",
                ..key
            },
            OnlineItlOverallRankCacheKeyRef {
                profile_id: None,
                ..key
            },
            OnlineItlOverallRankCacheKeyRef {
                profile_id: Some("profile-\u{97f3}"),
                ..key
            },
            OnlineItlOverallRankCacheKeyRef {
                song_cache_generation: 24,
                ..key
            },
            OnlineItlOverallRankCacheKeyRef {
                self_score_generation: 0,
                ..key
            },
        ] {
            assert!(state.get(side, changed).is_none());
        }
    }
    crate::perf::assert_no_churn(|| {
        for side in [0, 1, usize::MAX] {
            black_box(state.get(side, key));
        }
    });
    state.p1.as_mut().unwrap().key.profile_id = None;
    assert!(state.get(0, key).is_none());
    let no_profile = OnlineItlOverallRankCacheKeyRef {
        profile_id: None,
        ..key
    };
    assert!(state.get(0, no_profile).is_some());
    assert!(
        state
            .get(
                0,
                OnlineItlOverallRankCacheKeyRef {
                    profile_id: Some(""),
                    ..key
                }
            )
            .is_none()
    );
}
