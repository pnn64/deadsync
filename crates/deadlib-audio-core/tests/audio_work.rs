//! Audio mapping and mixing behavior and benchmarks against 0.5.1215.
pub use deadlib_audio_core::MusicMapSeg;
use std::hint::black_box;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
#[allow(dead_code)]
mod position {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/position.rs"));
    pub(super) fn state(map: &PlaybackPosMap) -> (usize, usize, i64) {
        (map.queue.len(), map.queue.capacity(), map.backlog_frames)
    }
}
#[allow(dead_code)]
#[path = "audio_work/position_baseline.rs"]
mod old_position;
#[allow(dead_code)]
mod mixer {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/mixer.rs"));
    pub(super) fn samples(src: &[i16], dst: &mut [f32], gain: f32) {
        mix_sfx_samples(src, dst, gain);
    }
}
#[allow(dead_code)]
#[path = "audio_work/mixer_baseline.rs"]
mod old_mixer;

fn assert_f64(actual: f64, expected: f64) {
    assert!(
        actual.to_bits() == expected.to_bits() || (actual.is_nan() && expected.is_nan()),
        "{actual:?} ({:x}) != {expected:?} ({:x})",
        actual.to_bits(),
        expected.to_bits()
    );
}
fn assert_search(actual: Option<(f64, f64)>, expected: Option<(f64, f64)>) {
    match (actual, expected) {
        (Some((a, b)), Some((c, d))) => {
            assert_f64(a, c);
            assert_f64(b, d);
        }
        (None, None) => {}
        pair => panic!("search result mismatch: {pair:?}"),
    }
}
fn assert_inverse(actual: Option<f64>, expected: Option<f64>) {
    match (actual, expected) {
        (Some(a), Some(b)) => assert_f64(a, b),
        (None, None) => {}
        pair => panic!("inverse result mismatch: {pair:?}"),
    }
}
fn compare_queries(
    map: &position::PlaybackPosMap,
    old: &old_position::PlaybackPosMap,
    frames: &[f64],
    seconds: &[f64],
) {
    for &frame in frames {
        assert_search(map.search(frame), old.search(frame));
    }
    for &seconds in seconds {
        assert_inverse(map.invert(seconds), old.invert(seconds));
    }
}
fn segment(start: i64, frames: i64, music: f64, rate: f64) -> MusicMapSeg {
    MusicMapSeg {
        stream_frame_start: start,
        frames,
        music_start_sec: music,
        music_sec_per_frame: rate,
    }
}
fn mapped_segments(count: usize) -> Vec<MusicMapSeg> {
    let mut music = 0.0;
    (0..count)
        .map(|i| {
            let rate = [1.0, 1.15, 0.75][i % 3] / 48_000.0;
            let seg = segment((i * 256) as i64, 256, music, rate);
            music = rate.mul_add(256.0, music);
            seg
        })
        .collect()
}
fn maps(segments: &[MusicMapSeg]) -> (position::PlaybackPosMap, old_position::PlaybackPosMap) {
    let mut current = position::PlaybackPosMap::default();
    let mut old = old_position::PlaybackPosMap::default();
    for &seg in segments {
        current.insert(seg);
        old.insert(seg);
    }
    (current, old)
}
fn random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

#[test]
fn queries_preserve_endpoints_ties_overlaps_and_nonfinite_inputs() {
    let cases = vec![
        vec![],
        vec![segment(0, 100, 0.0, 0.01)],
        vec![segment(0, 100, 0.0, 0.01), segment(200, 100, 5.0, 0.02)],
        vec![segment(200, 100, 5.0, 0.02), segment(0, 100, 0.0, 0.01)],
        vec![segment(0, 200, 0.0, 0.01), segment(100, 200, 5.0, 0.02)],
        vec![segment(0, 100, 1.0, -0.01), segment(200, 100, 1.0, -0.02)],
        vec![segment(0, 100, -0.0, -0.0), segment(200, 100, 0.0, 0.0)],
        vec![
            segment(0, 100, f64::MAX, f64::MAX),
            segment(200, 100, -f64::MAX, -f64::MAX),
        ],
        vec![
            segment(i64::MAX - 1000, 100, 0.0, f64::MIN_POSITIVE),
            segment(i64::MIN + 1000, 100, 0.0, -f64::MIN_POSITIVE),
        ],
    ];
    let frames = [
        f64::NEG_INFINITY,
        -f64::MAX,
        -1.0,
        -0.0,
        0.0,
        1.0,
        99.999,
        100.0,
        150.0,
        199.99,
        200.0,
        300.0,
        f64::MAX,
        f64::INFINITY,
        f64::NAN,
    ];
    let seconds = [
        f64::NEG_INFINITY,
        -f64::MAX,
        -1.0,
        -0.0,
        0.0,
        0.5,
        1.0,
        3.0,
        5.0,
        7.0,
        f64::MAX,
        f64::INFINITY,
        f64::NAN,
    ];
    for segments in cases {
        let (map, old) = maps(&segments);
        compare_queries(&map, &old, &frames, &seconds);
    }
    // Stable tie selection and half-open boundaries are observable across jumps.
    let (map, old) = maps(&[segment(0, 100, 0.0, 0.01), segment(200, 100, 5.0, 0.02)]);
    assert_search(map.search(150.0), old.search(150.0));
    assert_eq!(map.search(150.0).unwrap().1, 0.01);
    assert_inverse(map.invert(3.0), old.invert(3.0));
}

#[test]
fn randomized_maps_match_after_insertion_eviction_and_clear() {
    let mut seed = 0x1216;
    let mut map = position::PlaybackPosMap::default();
    let mut old = old_position::PlaybackPosMap::default();
    for i in 0..2500 {
        if i % 173 == 0 {
            map.clear();
            old.clear();
        }
        let start = (random(&mut seed) % 200_000) as i64 - 100_000;
        let frames = (random(&mut seed) % 1600) as i64 - 10;
        let music = (random(&mut seed) % 20000) as f64 / 100.0 - 100.0;
        let rate = match i % 17 {
            0 => 0.0,
            1 => -0.0,
            2 => f64::NAN,
            3 => f64::INFINITY,
            _ => (random(&mut seed) % 4000) as f64 / 100_000.0 - 0.02,
        };
        let seg = segment(start, frames, music, rate);
        map.insert(seg);
        old.insert(seg);
        assert_eq!(position::state(&map), old_position::state(&old));
        let query = f64::from_bits(random(&mut seed));
        compare_queries(
            &map,
            &old,
            &[start as f64, start as f64 + frames as f64, query],
            &[music, rate.mul_add(frames as f64, music), query],
        );
    }
}

#[test]
fn coalescing_and_wrapped_deque_queries_match() {
    for count in [1, 8, 64, 256, 1200] {
        let segments = mapped_segments(count);
        let (map, old) = maps(&segments);
        for seg in segments {
            compare_queries(
                &map,
                &old,
                &[
                    seg.stream_frame_start as f64 - 0.5,
                    seg.stream_frame_start as f64 + 128.0,
                    seg.stream_frame_start as f64 + 256.0,
                ],
                &[
                    seg.music_start_sec,
                    seg.music_sec_per_frame.mul_add(128.0, seg.music_start_sec),
                ],
            );
        }
        assert_eq!(position::state(&map), old_position::state(&old));
    }
    let (map, old) = maps(
        &(0..1000)
            .map(|i| segment(i * 256, 256, (i * 256) as f64 / 48000.0, 1.0 / 48000.0))
            .collect::<Vec<_>>(),
    );
    compare_queries(&map, &old, &[-1.0, 250_000.0, 300_000.0], &[-1.0, 5.0, 8.0]);
}

#[test]
fn mapping_remains_allocation_free_with_unchanged_storage() {
    let (map, old) = maps(&mapped_segments(256));
    assert_eq!(std::mem::size_of_val(&map), std::mem::size_of_val(&old));
    assert_eq!(position::state(&map), old_position::state(&old));
    perf::assert_no_churn(|| {
        for i in 0..2048 {
            black_box(map.search(i as f64 * 32.0));
            black_box(map.invert(i as f64 / 1024.0));
        }
    });
    eprintln!(
        "map object {} bytes, deque {:?}, segment {} bytes",
        std::mem::size_of_val(&map),
        position::state(&map),
        std::mem::size_of::<MusicMapSeg>()
    );
}

#[test]
fn muted_samples_preserve_signed_zero_and_all_i16_values() {
    let src: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for gain in [0.0, -0.0, 1.0, 0.5, f32::MIN_POSITIVE] {
        for value in [
            0.0,
            -0.0,
            0.25,
            -0.25,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::from_bits(1),
            -f32::from_bits(1),
            f32::MAX,
            -f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::from_bits(0x7f800001),
            f32::from_bits(0xffc12345),
        ] {
            let mut actual = vec![value; src.len()];
            let mut expected = actual.clone();
            mixer::samples(&src, &mut actual, gain);
            old_mixer::samples(&src, &mut expected, gain);
            for (a, b) in actual.into_iter().zip(expected) {
                assert!(
                    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
                    "gain {gain:?}, initial {value:?}: {a:?} != {b:?}"
                );
            }
        }
    }
}

#[test]
fn sample_slices_preserve_short_destinations_and_random_values() {
    let mut seed = 0xdead;
    for len in [0, 1, 2, 3, 31, 32, 33, 1024] {
        let src: Vec<_> = (0..len).map(|_| random(&mut seed) as i16).collect();
        for dst_len in [0, len / 2, len, len + 7] {
            let dst: Vec<_> = (0..dst_len)
                .map(|_| f32::from_bits(random(&mut seed) as u32))
                .collect();
            for gain in [0.0, -0.0, 1.0, 0.7, f32::NAN, f32::INFINITY] {
                let mut actual = dst.clone();
                let mut expected = dst.clone();
                mixer::samples(&src, &mut actual, gain);
                old_mixer::samples(&src, &mut expected, gain);
                for (a, b) in actual.iter().zip(expected) {
                    assert!(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()));
                }
            }
        }
    }
}

macro_rules! mixed_case {
    ($module:ident, $gain:expr, $channels:expr, $before:expr) => {{
        let controls = $module::MixControls::new();
        let bus = $module::MixBus::new(0);
        controls.set_bus_gain(bus, $gain);
        let data: Arc<[i16]> = (0..257)
            .map(|i| (i * 137) as i16)
            .collect::<Vec<_>>()
            .into();
        let mut active: Vec<_> = [0, 50, 150, $module::MAX_SCHEDULE_AHEAD_FRAMES + 101]
            .into_iter()
            .map(|target| $module::ActiveSfx {
                data: Arc::clone(&data),
                cursor: 0,
                bus,
                generation: 0,
                target_stream_frame: target,
            })
            .collect();
        let mut dst = vec![-0.0; 128];
        let mixed = $module::mix_active_sfx(&mut active, &mut dst, $before, $channels, &controls);
        let state = active
            .iter()
            .map(|s| (s.cursor, s.generation, s.target_stream_frame, s.data.len()))
            .collect::<Vec<_>>();
        controls.stop_bus(bus);
        let after_stop =
            $module::mix_active_sfx(&mut active, &mut dst, $before, $channels, &controls);
        (mixed, after_stop, state, active.len(), dst)
    }};
}
#[test]
fn muted_mixer_preserves_scheduling_cursors_generations_and_output() {
    for gain in [0.0, -0.0, 1.0, 0.5] {
        for channels in [1, 2, 6] {
            for before in [0, 100] {
                let actual = mixed_case!(mixer, gain, channels, before);
                let expected = mixed_case!(old_mixer, gain, channels, before);
                assert_eq!(
                    (&actual.0, &actual.1, &actual.2, &actual.3),
                    (&expected.0, &expected.1, &expected.2, &expected.3)
                );
                assert_eq!(
                    actual.4.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                    expected.4.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn muted_samples_allocate_nothing() {
    let src = vec![-12345; 8192];
    let mut dst = vec![0.0; 8192];
    perf::assert_no_churn(|| mixer::samples(&src, &mut dst, black_box(0.0)));
}

macro_rules! bench_active_mixer {
    ($module:ident, $variant:expr, $voices:expr, $label:expr, $gain:expr) => {{
        let controls = $module::MixControls::new();
        let bus = $module::MixBus::new(0);
        controls.set_bus_gain(bus, $gain);
        let data: Arc<[i16]> = (0..2048)
            .map(|i| (i * 137) as i16)
            .collect::<Vec<_>>()
            .into();
        let mut active: Vec<_> = (0..$voices)
            .map(|_| $module::ActiveSfx {
                data: Arc::clone(&data),
                cursor: 0,
                bus,
                generation: 0,
                target_stream_frame: 0,
            })
            .collect();
        let mut dst = vec![0.0; 1024];
        perf::measure_sampled(
            &format!("mixer/{}-{}-voices/{}", $label, $voices, $variant),
            512,
            1024 * $voices,
            || {
                dst.fill(0.0);
                for sfx in &mut active {
                    sfx.cursor = 0;
                }
                black_box($module::mix_active_sfx(
                    black_box(&mut active),
                    black_box(&mut dst),
                    black_box(10_000),
                    2,
                    black_box(&controls),
                ));
                black_box(&dst);
            },
        );
    }};
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_audio_work() {
    let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [1, 8, 64, 256] {
        let segments = mapped_segments(count);
        let (map, old_map) = maps(&segments);
        for kind in ["tail", "after", "first"] {
            if kind == "first" && count != 256 {
                continue;
            }
            let last = if kind == "first" {
                &segments[0]
            } else {
                segments.last().unwrap()
            };
            let frames: Vec<_> = (0..32)
                .map(|i| {
                    last.stream_frame_start as f64
                        + if kind == "after" {
                            256.0 + i as f64
                        } else {
                            128.0 + i as f64
                        }
                })
                .collect();
            let seconds: Vec<_> = frames
                .iter()
                .map(|f| {
                    (f - last.stream_frame_start as f64)
                        .mul_add(last.music_sec_per_frame, last.music_start_sec)
                })
                .collect();
            compare_queries(&map, &old_map, &frames, &seconds);
            for inverse in [false, true] {
                for old in versions {
                    let name = format!(
                        "map/{}/{count}-{kind}/{}",
                        if inverse { "inverse" } else { "forward" },
                        if old { "old" } else { "new" }
                    );
                    perf::measure_sampled(&name, if count < 64 { 4096 } else { 512 }, 32, || {
                        let map = black_box(&map);
                        let old_map = black_box(&old_map);
                        if inverse {
                            for &q in &seconds {
                                if old {
                                    black_box(old_map.invert(black_box(q)));
                                } else {
                                    black_box(map.invert(black_box(q)));
                                }
                            }
                        } else {
                            for &q in &frames {
                                if old {
                                    black_box(old_map.search(black_box(q)));
                                } else {
                                    black_box(map.search(black_box(q)));
                                }
                            }
                        }
                    });
                }
            }
        }
    }
    for len in [64, 1024, 8192] {
        let src: Vec<_> = (0..len).map(|i| (i * 137) as i16).collect();
        for (name, gain) in [
            ("muted", 0.0),
            ("negative-zero", -0.0),
            ("unity", 1.0),
            ("half", 0.5),
        ] {
            let mut dst = vec![0.0; len];
            for old in versions {
                perf::measure_sampled(
                    &format!("samples/{name}-{len}/{}", if old { "old" } else { "new" }),
                    if len < 1024 { 8192 } else { 2048 },
                    len,
                    || {
                        dst.fill(0.0);
                        if old {
                            old_mixer::samples(
                                black_box(&src),
                                black_box(&mut dst),
                                black_box(gain),
                            );
                        } else {
                            mixer::samples(black_box(&src), black_box(&mut dst), black_box(gain));
                        }
                        black_box(&dst);
                    },
                );
            }
        }
    }

    for voices in [1, 8, 32] {
        for old in versions {
            if old {
                bench_active_mixer!(old_mixer, "old", voices, "muted", 0.0);
            } else {
                bench_active_mixer!(mixer, "new", voices, "muted", 0.0);
            }
        }
    }
    for (label, gain) in [("unity", 1.0), ("half", 0.5)] {
        for old in versions {
            if old {
                bench_active_mixer!(old_mixer, "old", 8, label, gain);
            } else {
                bench_active_mixer!(mixer, "new", 8, label, gain);
            }
        }
    }
}
