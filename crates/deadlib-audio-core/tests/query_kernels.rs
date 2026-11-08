//! Differential checks and CPU/allocation measurements for production queries.
#![allow(dead_code)]
use deadlib_audio_core::{MusicMapSeg, RenderReport};
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "../src/output.rs"]
mod output;

mod position {
    include!("../src/position.rs");
    mod probes {
        use super::*;
        use std::hint::black_box;
        mod original {
            use super::*;
            include!("support/position_query_reference.rs");
        }

        fn verify(map: &PlaybackPosMap, query: f64) {
            assert_eq!(
                map.search(query).map(|(a, b)| (a.to_bits(), b.to_bits())),
                original::search(&map.queue, query).map(|(a, b)| (a.to_bits(), b.to_bits())),
                "search query={query:?}"
            );
            assert_eq!(
                map.invert(query).map(f64::to_bits),
                original::invert(&map.queue, query).map(f64::to_bits),
                "inverse query={query:?}"
            );
        }

        #[test]
        fn queries_preserve_endpoint_ties_overlaps_and_float_bits() {
            let mut state = 23u64;
            for count in [0, 1, 2, 7, 127, 257] {
                let mut map = PlaybackPosMap {
                    ordered: false,
                    music_ordered: false,
                    ..PlaybackPosMap::default()
                };
                for i in 0..count {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    map.queue.push_back(MusicMapSeg {
                        stream_frame_start: [-100, 0, 50, i64::MAX - 4096, i64::MIN + 4096][i % 5]
                            + (state % 1024) as i64,
                        frames: [1, 32, 1024, 80000][i % 4],
                        music_start_sec: [-f64::MAX, -0.0, 0.0, 1.0, f64::MAX][i % 5],
                        music_sec_per_frame: [
                            0.0,
                            -0.0,
                            1.0 / 48000.0,
                            -1.0 / 48000.0,
                            1e-300,
                            1e300,
                        ][i % 6],
                    });
                }
                for query in [
                    -f64::MAX,
                    -1e300,
                    -100.0,
                    -0.0,
                    0.0,
                    0.5,
                    50.0,
                    10000.0,
                    1e300,
                    f64::MAX,
                    f64::NAN,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                ] {
                    verify(&map, query);
                }
                for seg in &map.queue {
                    for boundary in [
                        seg.stream_frame_start as f64,
                        seg.stream_frame_start as f64 + seg.frames as f64,
                        seg.music_start_sec,
                        seg.music_sec_per_frame
                            .mul_add(seg.frames as f64, seg.music_start_sec),
                    ] {
                        for query in [boundary.next_down(), boundary, boundary.next_up()] {
                            verify(&map, query);
                        }
                    }
                }
            }
        }

        #[test]
        fn ordered_shortcut_preserves_overlaps_merges_trimming_and_clear() {
            let mut map = PlaybackPosMap::default();
            for round in 0..3 {
                for i in 0..700 {
                    let start = if round == 1 && i % 7 == 0 {
                        i * 256 - 512
                    } else {
                        i * 256
                    };
                    map.insert(MusicMapSeg {
                        stream_frame_start: start,
                        frames: 256,
                        music_start_sec: i as f64 / 187.5,
                        music_sec_per_frame: if i % 2 == 0 {
                            1.0 / 48000.0
                        } else {
                            1.5 / 48000.0
                        },
                    });
                    for q in [
                        start as f64 - 0.5,
                        start as f64,
                        start as f64 + 127.0,
                        start as f64 + 256.0,
                        1e300,
                    ] {
                        verify(&map, q);
                    }
                    for q in [
                        i as f64 / 187.5 - 1e-9,
                        i as f64 / 187.5,
                        i as f64 / 187.5 + 0.001,
                    ] {
                        verify(&map, q);
                    }
                }
                map.clear();
            }
            for origin in [1i64 << 53, 1i64 << 62, -(1i64 << 62)] {
                for offset in 0..16 {
                    for gap in 0..4 {
                        map.clear();
                        let start = origin + offset;
                        map.insert(MusicMapSeg {
                            stream_frame_start: start,
                            frames: 79999,
                            music_start_sec: 0.0,
                            music_sec_per_frame: 1e-5,
                        });
                        let next = start + 79999 + gap;
                        map.insert(MusicMapSeg {
                            stream_frame_start: next,
                            frames: 2,
                            music_start_sec: 1.0,
                            music_sec_per_frame: 2e-5,
                        });
                        for q in [
                            (next as f64).next_down(),
                            next as f64,
                            (next as f64).next_up(),
                        ] {
                            verify(&map, q);
                        }
                    }
                }
            }
        }

        #[test]
        #[cfg(not(debug_assertions))]
        fn query_shortcuts_preserve_release_overflow_histories() {
            let mut map = PlaybackPosMap::default();
            for i in 0..200 {
                let frame = [1, 80000, i64::MAX, i64::MAX - 1, 256][i % 5];
                let stream = [0, 80000, i64::MAX, i64::MIN + 1][i % 4];
                let start = [0.0, 80000.0, 1.0, 1e300][i % 4];
                map.insert(MusicMapSeg {
                    stream_frame_start: stream,
                    frames: frame,
                    music_start_sec: start,
                    music_sec_per_frame: [1.0, 0.0, -1.0, 1e300][i % 4],
                });
                for q in [-1e300, 0.0, 1.0, 80000.0, 1e300, stream as f64] {
                    verify(&map, q);
                }
                if map.ordered {
                    for (a, b) in map.queue.iter().zip(map.queue.iter().skip(1)) {
                        assert!(
                            a.stream_frame_start as f64 + a.frames as f64
                                <= b.stream_frame_start as f64
                        );
                    }
                }
                if map.music_ordered {
                    assert!(map.queue.iter().all(|s| s.music_sec_per_frame >= 0.0));
                    for (a, b) in map.queue.iter().zip(map.queue.iter().skip(1)) {
                        assert!(
                            a.music_sec_per_frame
                                .mul_add(a.frames as f64, a.music_start_sec)
                                <= b.music_start_sec
                        );
                    }
                }
                if i % 17 == 0 {
                    map.clear();
                }
            }
        }

        #[test]
        #[ignore = "release CPU-cycle and allocation benchmark"]
        fn benchmark_query_positions() {
            for count in [1, 127, 1023] {
                let mut map = PlaybackPosMap::default();
                for i in 0..count {
                    map.insert(MusicMapSeg {
                        stream_frame_start: i as i64 * 33,
                        frames: 32,
                        music_start_sec: i as f64 / 120.0,
                        music_sec_per_frame: if i % 2 == 0 {
                            1.0 / 48000.0
                        } else {
                            1.5 / 48000.0
                        },
                    });
                }
                for _ in 0..count {
                    let seg = map.queue.pop_front().unwrap();
                    map.queue.push_back(seg);
                }
                let mid = count / 2;
                for (kind, frame, seconds) in [
                    ("first", 16.0, 0.0001),
                    (
                        "middle",
                        mid as f64 * 33.0 + 16.0,
                        mid as f64 / 120.0 + 0.0001,
                    ),
                    (
                        "last",
                        (count - 1) as f64 * 33.0 + 16.0,
                        (count - 1) as f64 / 120.0 + 0.0001,
                    ),
                    ("gap", mid as f64 * 33.0 + 32.5, mid as f64 / 120.0 + 0.004),
                    ("before", -1.0, -1.0),
                    (
                        "after",
                        count as f64 * 33.0 + 16.0,
                        count as f64 / 120.0 + 1.0,
                    ),
                    ("huge", 1e300, 1e300),
                ] {
                    for inverse in [false, true] {
                        let label = format!(
                            "query/{}/{kind}/segments={count}",
                            if inverse { "inverse" } else { "search" }
                        );
                        let work = || {
                            if inverse {
                                let _ = black_box(map.invert(black_box(seconds)));
                            } else {
                                let _ = black_box(map.search(black_box(frame)));
                            }
                        };
                        let (_, counts) = crate::allocations::measure(work);
                        assert_eq!(counts, crate::allocations::Churn::default());
                        println!("{label}: allocation {counts:?}");
                        crate::bench::measure(&label, (1_000_000 / count).min(50_000), work);
                    }
                }
            }
        }
    }
}

mod telemetry {
    include!("../src/telemetry.rs");
    mod probes {
        use super::*;
        use std::hint::black_box;
        mod original {
            use super::*;
            include!("support/diagnostic_reference.rs");
        }

        fn fixture(count: usize, kind: &str) -> AudioTelemetryState {
            let state = AudioTelemetryState::new();
            state.publish_output_timing(48000, 1000, 2000, 256, 128, 64, 3000);
            for i in 0..count {
                let at = if kind == "zero" || (kind == "mixed" && i % 3 == 0) {
                    0
                } else {
                    10000 + i as u64
                };
                state.record_stutter_diag_event(
                    StutterDiagAudioEventKind::CallbackGap,
                    at,
                    i as u64,
                    OutputTimingQuality::Trusted,
                );
            }
            if kind == "stale" {
                for slot in &state.stutter_diag_events {
                    slot.version.fetch_add(128, Ordering::Relaxed);
                }
            }
            if kind == "writing" {
                for slot in &state.stutter_diag_events {
                    slot.version.fetch_or(1, Ordering::Relaxed);
                }
            }
            if kind == "invalid" {
                for slot in &state.stutter_diag_events {
                    slot.kind.store(255, Ordering::Relaxed);
                }
            }
            state
        }

        fn fields(event: &StutterDiagAudioEvent) -> [u64; 10] {
            [
                event.at_host_nanos,
                event.kind as u64,
                event.value_ns,
                event.sample_rate_hz as u64,
                event.buffer_frames as u64,
                event.padding_frames as u64,
                event.queued_frames as u64,
                event.device_period_ns,
                event.estimated_output_delay_ns,
                event.timing_quality as u64,
            ]
        }

        #[test]
        fn collection_keeps_correlated_payloads_during_ring_wraps() {
            let state = fixture(64, "live");
            let done = AtomicBool::new(false);
            std::thread::scope(|scope| {
                let writer = scope.spawn(|| {
                    for value in 64..100_064 {
                        state.record_stutter_diag_event(
                            StutterDiagAudioEventKind::CallbackGap,
                            10000 + value,
                            value,
                            OutputTimingQuality::Trusted,
                        );
                    }
                    done.store(true, Ordering::Release);
                });
                let mut output = Vec::with_capacity(64);
                while !done.load(Ordering::Acquire) {
                    output.clear();
                    state.collect_stutter_diag_events(u64::MAX, u64::MAX, &mut output);
                    for event in &output {
                        assert_eq!(event.at_host_nanos, 10000 + event.value_ns);
                        assert_eq!(event.sample_rate_hz, 48000);
                        assert_eq!(event.kind, StutterDiagAudioEventKind::CallbackGap);
                    }
                    assert!(
                        output
                            .windows(2)
                            .all(|pair| pair[0].value_ns < pair[1].value_ns)
                    );
                }
                writer.join().unwrap();
                let mut expected = Vec::new();
                output.clear();
                state.collect_stutter_diag_events(u64::MAX, u64::MAX, &mut output);
                state.collect_stutter_diag_events_reference(u64::MAX, u64::MAX, &mut expected);
                assert_eq!(output.len(), 64);
                assert_eq!(
                    output.iter().map(fields).collect::<Vec<_>>(),
                    expected.iter().map(fields).collect::<Vec<_>>()
                );
            });
        }

        #[test]
        fn collection_preserves_append_order_versions_and_time_boundaries() {
            for count in [0, 1, 63, 64, 65, 127, 255] {
                for kind in [
                    "live", "expired", "zero", "stale", "mixed", "future", "writing", "invalid",
                ] {
                    let state = fixture(count, kind);
                    for now in [0, 5000, 10000, 10064, 1_000_000, u64::MAX] {
                        for window in [0, 1, 64, 1024, u64::MAX] {
                            let mut old = Vec::new();
                            let mut new = Vec::new();
                            state.collect_stutter_diag_events_reference(now, window, &mut old);
                            state.collect_stutter_diag_events(now, window, &mut new);
                            assert_eq!(
                                old.iter().map(fields).collect::<Vec<_>>(),
                                new.iter().map(fields).collect::<Vec<_>>()
                            );
                            let prefix = new.clone();
                            state.collect_stutter_diag_events(now, window, &mut new);
                            assert_eq!(
                                new.iter().map(fields).collect::<Vec<_>>(),
                                prefix.iter().chain(&prefix).map(fields).collect::<Vec<_>>()
                            );
                        }
                    }
                }
            }
            for head in [(1u64 << 63) - 1, 1 << 63, (1 << 63) + 63, u64::MAX] {
                let state = AudioTelemetryState::new();
                state.stutter_diag_event_head.store(head, Ordering::Relaxed);
                for seq in head - 63..=head {
                    let slot = &state.stutter_diag_events
                        [(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
                    slot.version.store(seq << 1, Ordering::Relaxed);
                    slot.at_host_nanos.store(1, Ordering::Relaxed);
                    slot.kind.store(1, Ordering::Relaxed);
                }
                let (mut old, mut new) = (Vec::new(), Vec::new());
                state.collect_stutter_diag_events_reference(1, 1, &mut old);
                state.collect_stutter_diag_events(1, 1, &mut new);
                assert_eq!(
                    old.iter().map(fields).collect::<Vec<_>>(),
                    new.iter().map(fields).collect::<Vec<_>>()
                );
            }
        }

        #[test]
        #[ignore = "release CPU-cycle and allocation benchmark"]
        fn benchmark_query_diagnostics() {
            for count in [0, 1, 8, 64] {
                for kind in ["live", "expired", "zero", "stale", "mixed", "future"] {
                    let state = fixture(count, kind);
                    let now = match kind {
                        "expired" => 1_000_000,
                        "future" => 5000,
                        _ => 11000,
                    };
                    let mut output = Vec::with_capacity(64);
                    let label = format!("query/diagnostics/{kind}/events={count}");
                    let mut work = || {
                        output.clear();
                        state.collect_stutter_diag_events(
                            black_box(now),
                            black_box(1024),
                            black_box(&mut output),
                        );
                        black_box(&output);
                    };
                    let (_, counts) = crate::allocations::measure(&mut work);
                    assert_eq!(counts, crate::allocations::Churn::default());
                    println!("{label}: allocation {counts:?}");
                    crate::bench::measure(&label, 20_000, &mut work);
                }
            }
        }
    }
}
