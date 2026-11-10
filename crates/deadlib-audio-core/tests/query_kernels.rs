//! Audio position and telemetry regressions.
#![allow(dead_code)]
use deadlib_audio_core::{MusicMapSeg, RenderReport};
#[path = "../../../tests/support/allocations.rs"]
mod allocations;
#[path = "../src/output.rs"]
mod output;

mod position {
    include!("../src/position.rs");
    mod probes {
        #[cfg(not(debug_assertions))]
        use super::*;

        #[cfg(not(debug_assertions))]
        fn verify(map: &mut PlaybackPosMap, query: f64) {
            let search = map.search(query).map(|(a, b)| (a.to_bits(), b.to_bits()));
            let inverse = map.invert(query).map(f64::to_bits);
            let flags = (map.ordered, map.music_ordered);
            // Compare each shortcut with the full traversal over the same history.
            map.ordered = false;
            map.music_ordered = false;
            let general_search = map.search(query).map(|(a, b)| (a.to_bits(), b.to_bits()));
            let general_inverse = map.invert(query).map(f64::to_bits);
            (map.ordered, map.music_ordered) = flags;
            assert_eq!(search, general_search, "search query={query:?}");
            assert_eq!(inverse, general_inverse, "inverse query={query:?}");
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
                    verify(&mut map, q);
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

    }
}

mod telemetry {
    include!("../src/telemetry.rs");
    mod probes {
        use super::*;


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
                output.clear();
                state.collect_stutter_diag_events(u64::MAX, u64::MAX, &mut output);
                assert_eq!(output.len(), 64);
                assert_eq!(
                    output.iter().map(|event| event.value_ns).collect::<Vec<_>>(),
                    (100_000..100_064).collect::<Vec<_>>()
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
                            let mut new = Vec::new();
                            state.collect_stutter_diag_events(now, window, &mut new);
                            assert!(new.len() <= AUDIO_STUTTER_DIAG_EVENT_COUNT);
                            assert!(new.iter().all(|event| event.at_host_nanos != 0 && now.saturating_sub(event.at_host_nanos) <= window));
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
            for (head, expected_count) in [((1u64 << 63) - 1, 64), (1 << 63, 63), ((1 << 63) + 63, 0), (u64::MAX, 0)] {
                let state = AudioTelemetryState::new();
                state.stutter_diag_event_head.store(head, Ordering::Relaxed);
                for seq in head - 63..=head {
                    let slot = &state.stutter_diag_events
                        [(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
                    slot.version.store(seq << 1, Ordering::Relaxed);
                    slot.at_host_nanos.store(1, Ordering::Relaxed);
                    slot.kind.store(1, Ordering::Relaxed);
                }
                let mut new = Vec::new();
                state.collect_stutter_diag_events(1, 1, &mut new);
                assert_eq!(new.len(), expected_count);
            }
        }

    }
}
