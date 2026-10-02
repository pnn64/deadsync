use super::*;
use std::hint::black_box;

include!("event_preparation_baseline.rs");

fn timing_fixture(size: usize, mode: &str) -> TimingData {
    let mut segments = deadsync_rules::timing::TimingSegments {
        bpms: (0..size)
            .map(|i| {
                let beat = i as f32 * 4.0;
                (
                    if mode == "subrow" { beat + 0.001 } else { beat },
                    90.0 + (i % 7) as f32 * 25.0,
                )
            })
            .collect(),
        ..Default::default()
    };
    if mode == "pauses" {
        segments.stops.push(deadsync_rules::timing::StopSegment {
            beat: 8.0,
            duration: 0.125,
        });
        segments.delays.push(deadsync_rules::timing::DelaySegment {
            beat: 16.0,
            duration: 0.25,
        });
        segments.warps.push(deadsync_rules::timing::WarpSegment {
            beat: 24.0,
            length: 3.0,
        });
    }
    if mode == "duplicate" {
        segments.bpms.push((4.0, 150.0));
    }
    if mode == "invalid" {
        segments.bpms[1].1 = f32::NAN;
    }
    TimingData::from_segments(0.125, -0.037, &segments, &[])
}

fn seconds_bits(values: &[Option<f32>]) -> Vec<Option<u32>> {
    values.iter().map(|v| v.map(f32::to_bits)).collect()
}

#[test]
fn exact_message_batches_match_parent_for_fractional_beats_and_fallback_maps() {
    for size in [0, 1, 8, 9, 64, 1024] {
        for mode in ["plain", "pauses", "subrow", "duplicate", "invalid"] {
            if mode == "invalid" && size < 2 {
                continue;
            }
            let timing = timing_fixture(size, mode);
            let mut beats: Vec<_> = (0..256).map(|i| i as f32 * 0.123 - 2.0).collect();
            // A rewind within the same quantized row must restore the BPM state.
            beats.extend([
                4.001,
                3.999,
                4.0,
                3.999,
                -0.0,
                0.0,
                f32::INFINITY,
                f32::NAN,
                f32::NEG_INFINITY,
                f32::MAX,
                f32::MIN,
                12.0,
            ]);
            for sequence in [beats.clone(), beats.into_iter().rev().collect()] {
                let old =
                    old_build_song_lua_message_seconds(sequence.iter().copied(), &timing, 0.2);
                let new = build_song_lua_message_seconds(sequence.iter().copied(), &timing, 0.2);
                assert_eq!(seconds_bits(&old), seconds_bits(&new), "{size} {mode}");
                let old_direct: Vec<_> = sequence
                    .iter()
                    .map(|&beat| timing.get_time_for_beat_exact(beat).to_bits())
                    .collect();
                let new_direct: Vec<_> = timing
                    .get_times_for_beats_exact(sequence.iter().copied())
                    .map(f32::to_bits)
                    .collect();
                assert_eq!(old_direct, new_direct, "direct {size} {mode}");
            }
        }
    }
}

#[test]
fn exact_batch_iterator_is_lazy_and_allocates_no_scratch() {
    let timing = timing_fixture(1024, "plain");
    let calls = std::cell::Cell::new(0);
    let beats = (0..512).map(|i| {
        calls.set(calls.get() + 1);
        i as f32 * 1.123
    });
    let mut times = timing.get_times_for_beats_exact(beats);
    assert_eq!(calls.get(), 0);
    assert_eq!(
        times.next().unwrap().to_bits(),
        timing.get_time_for_beat_exact(0.0).to_bits()
    );
    assert_eq!(calls.get(), 1);
    perf::assert_no_churn(|| {
        for time in times {
            black_box(time);
        }
    });
    assert_eq!(calls.get(), 512);
}

fn ease_fixture(size: usize, mode: &str) -> Vec<SongLuaRuntimeEaseWindow> {
    (0..size)
        .map(|i| SongLuaRuntimeEaseWindow {
            approach_speed: Some(0.75),
            player: if mode == "wrong_player" {
                Some(2)
            } else {
                None
            },
            unit: SongLuaRuntimeTimeUnit::Second,
            start: if mode == "invalid" {
                f32::NAN
            } else {
                i as f32 * 0.125
            },
            limit: if mode == "invalid" { f32::NAN } else { 0.5 },
            span_mode: SongLuaRuntimeSpanMode::Len,
            target: match mode {
                "ignored" => SongLuaRuntimeEaseTargetOwned::Function,
                "unsupported" => SongLuaRuntimeEaseTargetOwned::Mod("unsupportedTarget".into()),
                "alias" => SongLuaRuntimeEaseTargetOwned::Mod("incoming".into()),
                "mixed" if i % 3 == 0 => {
                    SongLuaRuntimeEaseTargetOwned::Mod("unsupportedTarget".into())
                }
                _ => SongLuaRuntimeEaseTargetOwned::Player(SongLuaEaseMaskTarget::PlayerX),
            },
            from: -100.0,
            to: 200.0,
            easing: Some("inOutCubic".into()),
            sustain: Some(0.25),
            opt1: Some(1.0),
            opt2: Some(0.5),
        })
        .collect()
}

#[test]
fn lazy_ease_storage_preserves_outputs_callbacks_and_iterator_consumption() {
    let timing = timing_fixture(1, "plain");
    for mode in [
        "wrong_player",
        "unsupported",
        "ignored",
        "invalid",
        "valid",
        "alias",
        "mixed",
    ] {
        let windows = ease_fixture(129, mode);
        let mut old_callbacks = Vec::new();
        let mut new_callbacks = Vec::new();
        let old =
            old_build_song_lua_ease_windows_for_player(&windows, &timing, 0, 0.125, &[], |w| {
                old_callbacks.push(w.start.to_bits())
            });
        let new = build_song_lua_ease_windows_for_player(&windows, &timing, 0, 0.125, &[], |w| {
            new_callbacks.push(w.start.to_bits())
        });
        assert_eq!(old, new, "{mode}");
        assert_eq!(old_callbacks, new_callbacks);
        let consumed = std::cell::Cell::new(0);
        let iter = windows
            .iter()
            .cloned()
            .inspect(|_| consumed.set(consumed.get() + 1));
        let new_iter =
            build_song_lua_ease_windows_for_player_iter(iter, &timing, 0, 0.125, &[], |_| {});
        assert_eq!(old, new_iter, "iterator {mode}");
        assert_eq!(consumed.get(), windows.len());
        let old_iter = old_build_song_lua_ease_windows_for_player_iter(
            windows.iter().cloned(),
            &timing,
            0,
            0.125,
            &[],
            |_| {},
        );
        assert_eq!(old_iter, new_iter);
    }
}

#[test]
fn empty_ease_outputs_avoid_allocation_and_valid_outputs_reserve_once() {
    let timing = timing_fixture(1, "plain");
    for mode in ["valid", "alias"] {
        let windows = ease_fixture(1, mode);
        perf::assert_churn_budget(1, 2 * std::mem::size_of::<SongLuaEaseMaskWindow>(), || {
            black_box(build_song_lua_ease_windows_for_player(
                &windows,
                &timing,
                0,
                0.0,
                &[],
                |_| {},
            ));
        });
    }
    for mode in ["wrong_player", "unsupported", "ignored", "invalid"] {
        let windows = ease_fixture(1024, mode);
        perf::assert_no_churn(|| {
            let (output, _) =
                build_song_lua_ease_windows_for_player(&windows, &timing, 0, 0.0, &[], |_| {});
            assert!(output.is_empty());
            assert_eq!(output.capacity(), 0);
        });
    }
    for mode in ["valid", "alias"] {
        let windows = ease_fixture(1024, mode);
        // One output reservation plus the existing large-batch tail indices.
        perf::assert_churn_budget(
            2,
            2048 * (std::mem::size_of::<SongLuaEaseMaskWindow>() + std::mem::size_of::<usize>()),
            || {
                black_box(build_song_lua_ease_windows_for_player(
                    &windows,
                    &timing,
                    0,
                    0.0,
                    &[],
                    |_| {},
                ));
            },
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
struct WideDelta {
    id: usize,
    payload: [u64; 64],
}

fn overlay_fixture(size: usize, mode: &str) -> Vec<SongLuaOverlayEaseWindowRuntime<WideDelta>> {
    let mut values: Vec<_> = (0..size)
        .map(|i| {
            let key = if mode == "ties" { i % 8 } else { i };
            SongLuaOverlayEaseWindowRuntime {
                overlay_index: key % 16,
                start_second: (key / 16) as f32,
                end_second: (key / 16) as f32 + 0.5,
                sustain_end_second: (key / 16) as f32 + 1.0,
                cutoff_second: None,
                from: WideDelta {
                    id: i,
                    payload: [i as u64; 64],
                },
                to: WideDelta {
                    id: i + size,
                    payload: [(i + size) as u64; 64],
                },
                easing: SongLuaEase::InOutCubic,
                opt1: None,
                opt2: None,
            }
        })
        .collect();
    if mode == "sorted" {
        values.sort_by_key(|v| (v.overlay_index, v.start_second as usize));
    } else if size != 0 {
        for i in 0..size {
            values.swap(i, (i * 7919 + 101) % size);
        }
    }
    values
}

fn assert_overlay_match<D: PartialEq + std::fmt::Debug>(
    old: &(
        Vec<SongLuaOverlayEaseWindowRuntime<D>>,
        Vec<std::ops::Range<usize>>,
    ),
    new: &(
        Vec<SongLuaOverlayEaseWindowRuntime<D>>,
        Vec<std::ops::Range<usize>>,
    ),
) {
    assert_eq!(old.1, new.1);
    assert_eq!(old.0.len(), new.0.len());
    for (old, new) in old.0.iter().zip(&new.0) {
        assert_eq!(old.overlay_index, new.overlay_index);
        assert_eq!(
            [old.start_second, old.end_second, old.sustain_end_second].map(f32::to_bits),
            [new.start_second, new.end_second, new.sustain_end_second].map(f32::to_bits)
        );
        assert_eq!(
            old.cutoff_second.map(f32::to_bits),
            new.cutoff_second.map(f32::to_bits)
        );
        assert_eq!(
            (&old.from, &old.to, old.easing),
            (&new.from, &new.to, new.easing)
        );
    }
}

#[test]
fn indexed_grouping_preserves_stable_ties_nonfinite_keys_and_ranges() {
    for size in [0, 1, 8, 63, 64, 65, 512, 4096] {
        for mode in ["shuffled", "ties", "sorted"] {
            let mut input = overlay_fixture(size, mode);
            for (i, window) in input.iter_mut().enumerate().take(16) {
                window.overlay_index = [0, 15, 16, usize::MAX][i % 4];
                window.start_second = [-0.0, 0.0, f32::NAN, f32::INFINITY][i % 4];
                window.end_second = [f32::NEG_INFINITY, f32::NAN, -0.0, 0.0][i % 4];
            }
            let old = old_group_song_lua_overlay_eases(16, input.clone());
            let new = group_song_lua_overlay_eases(16, input);
            assert_overlay_match(&old, &new);
            for (overlay, range) in new.1.iter().enumerate() {
                assert!(
                    new.0[range.clone()]
                        .iter()
                        .all(|w| w.overlay_index == overlay)
                );
            }
        }
    }
}

#[test]
fn indexed_grouping_moves_nonclone_deltas_and_drops_each_value_once() {
    struct Tracked {
        id: usize,
        drops: std::rc::Rc<Vec<std::cell::Cell<usize>>>,
        _payload: [u64; 64],
    }
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.drops[self.id].set(self.drops[self.id].get() + 1);
        }
    }
    let drops = std::rc::Rc::new(
        (0..258)
            .map(|_| std::cell::Cell::new(0))
            .collect::<Vec<_>>(),
    );
    let input: Vec<_> = overlay_fixture(129, "ties")
        .into_iter()
        .enumerate()
        .map(|(i, v)| SongLuaOverlayEaseWindowRuntime {
            overlay_index: if i == 3 { 16 } else { v.overlay_index },
            start_second: v.start_second,
            end_second: v.end_second,
            sustain_end_second: v.sustain_end_second,
            cutoff_second: None,
            from: Tracked {
                id: 2 * i,
                drops: drops.clone(),
                _payload: [0; 64],
            },
            to: Tracked {
                id: 2 * i + 1,
                drops: drops.clone(),
                _payload: [0; 64],
            },
            easing: v.easing,
            opt1: None,
            opt2: None,
        })
        .collect();
    let (output, _) = group_song_lua_overlay_eases(16, input);
    assert_eq!(drops[6].get(), 1);
    assert_eq!(drops[7].get(), 1);
    assert!(
        output
            .iter()
            .all(|v| drops[v.from.id].get() == 0 && drops[v.to.id].get() == 0)
    );
    drop(output);
    assert!(drops.iter().all(|count| count.get() == 1));
}

#[test]
fn indexed_grouping_bounds_scratch_to_compact_indices() {
    let mut input = overlay_fixture(4096, "shuffled");
    perf::assert_churn_budget(
        2,
        4096 * std::mem::size_of::<usize>() + 16 * std::mem::size_of::<std::ops::Range<usize>>(),
        || {
            let (values, ranges) = group_song_lua_overlay_eases(16, std::mem::take(&mut input));
            black_box(&ranges);
            input = values;
        },
    );
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_event_preparation() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let order = if reverse {
        [false, true]
    } else {
        [true, false]
    };
    for (size, mode) in [
        (8, "shuffled"),
        (64, "shuffled"),
        (1024, "shuffled"),
        (8192, "shuffled"),
        (1024, "ties"),
        (1024, "sorted"),
    ] {
        let fixture = overlay_fixture(size, mode);
        for old in order {
            let name = format!(
                "group_wide_{size}_{mode}_{}",
                if old { "old" } else { "new" }
            );
            perf::measure_sampled_with_setup(
                &name,
                if size >= 8192 { 8 } else { 32 },
                size,
                || fixture.clone(),
                |input| {
                    let (values, ranges) = if old {
                        old_group_song_lua_overlay_eases(16, std::mem::take(input))
                    } else {
                        group_song_lua_overlay_eases(16, std::mem::take(input))
                    };
                    black_box(&ranges);
                    *input = values;
                },
            );
        }
    }
    let fixture: Vec<_> = overlay_fixture(1024, "shuffled")
        .into_iter()
        .map(|v| SongLuaOverlayEaseWindowRuntime {
            overlay_index: v.overlay_index,
            start_second: v.start_second,
            end_second: v.end_second,
            sustain_end_second: v.sustain_end_second,
            cutoff_second: None,
            from: v.from.id,
            to: v.to.id,
            easing: v.easing,
            opt1: None,
            opt2: None,
        })
        .collect();
    for old in order {
        perf::measure_sampled_with_setup(
            &format!(
                "group_small_1024_shuffled_{}",
                if old { "old" } else { "new" }
            ),
            32,
            1024,
            || fixture.clone(),
            |input| {
                let (values, ranges) = if old {
                    old_group_song_lua_overlay_eases(16, std::mem::take(input))
                } else {
                    group_song_lua_overlay_eases(16, std::mem::take(input))
                };
                black_box(&ranges);
                *input = values;
            },
        );
    }
    let timing = timing_fixture(1, "plain");
    for (size, mode) in [
        (0, "valid"),
        (1, "valid"),
        (1024, "wrong_player"),
        (1024, "unsupported"),
        (1024, "ignored"),
        (1024, "valid"),
        (1024, "alias"),
        (1024, "mixed"),
    ] {
        let windows = ease_fixture(size, mode);
        for old in order {
            let name = format!("ease_{size}_{mode}_{}", if old { "old" } else { "new" });
            perf::measure_sampled(&name, 64, size.max(1), || {
                black_box(if old {
                    old_build_song_lua_ease_windows_for_player(
                        black_box(&windows),
                        &timing,
                        0,
                        0.0,
                        &[],
                        |w| {
                            black_box(w.start);
                        },
                    )
                } else {
                    build_song_lua_ease_windows_for_player(
                        black_box(&windows),
                        &timing,
                        0,
                        0.0,
                        &[],
                        |w| {
                            black_box(w.start);
                        },
                    )
                });
            });
        }
    }
    for (size, queries, mode) in [
        (1, 128, "forward"),
        (64, 512, "forward"),
        (1024, 2048, "forward"),
        (64, 128, "rewind"),
        (64, 128, "random"),
        (64, 512, "fractional"),
        (64, 512, "pauses"),
        (64, 512, "subrow"),
    ] {
        let timing = timing_fixture(size, mode);
        let beats: Vec<_> = (0..queries)
            .map(|i| {
                let beat = i as f32 * (size * 4) as f32 / queries as f32;
                match mode {
                    "rewind" => size as f32 * 4.0 - beat,
                    "random" => ((i * 7919) % queries) as f32 * size as f32 * 4.0 / queries as f32,
                    "fractional" => beat + 0.0037,
                    _ => beat,
                }
            })
            .collect();
        assert_eq!(
            seconds_bits(&old_build_song_lua_message_seconds(
                beats.iter().copied(),
                &timing,
                0.0
            )),
            seconds_bits(&build_song_lua_message_seconds(
                beats.iter().copied(),
                &timing,
                0.0
            ))
        );
        for old in order {
            let name = format!(
                "message_times_{size}_{mode}_{}",
                if old { "old" } else { "new" }
            );
            perf::measure_sampled(&name, if size >= 1024 { 4 } else { 32 }, queries, || {
                black_box(if old {
                    old_build_song_lua_message_seconds(
                        black_box(&beats).iter().copied(),
                        &timing,
                        0.0,
                    )
                } else {
                    build_song_lua_message_seconds(black_box(&beats).iter().copied(), &timing, 0.0)
                });
            });
        }
    }
}
