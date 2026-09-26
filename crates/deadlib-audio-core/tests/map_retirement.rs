pub use deadlib_audio_core::MusicMapSeg;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

macro_rules! snapshot {
    () => {
        pub(super) fn snapshot(map: &PlaybackPosMap) -> (i64, Vec<(i64, i64, u64, u64)>) {
            (
                map.backlog_frames,
                map.queue
                    .iter()
                    .map(|seg| {
                        (
                            seg.stream_frame_start,
                            seg.frames,
                            seg.music_start_sec.to_bits(),
                            seg.music_sec_per_frame.to_bits(),
                        )
                    })
                    .collect(),
            )
        }
    };
}

#[allow(dead_code)]
mod current {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/position.rs"));
    snapshot!();
}

#[allow(dead_code)]
mod before {
    // insert/cleanup are unchanged between this reference and 59179a336.
    include!("audio_work/position_baseline.rs");
    snapshot!();
}

fn segment(start: i64, frames: i64, music: f64, rate: f64) -> MusicMapSeg {
    MusicMapSeg {
        stream_frame_start: start,
        frames,
        music_start_sec: music,
        music_sec_per_frame: rate,
    }
}

fn compare_sequence(segments: impl IntoIterator<Item = MusicMapSeg>) {
    let mut old = before::PlaybackPosMap::default();
    let mut new = current::PlaybackPosMap::default();
    for (index, seg) in segments.into_iter().enumerate() {
        old.insert(seg);
        new.insert(seg);
        assert_eq!(
            current::snapshot(&new),
            before::snapshot(&old),
            "insert {index}: {seg:?}"
        );
    }
}

#[test]
fn retirement_preserves_every_retained_segment_bit() {
    for frames in [1, 64, 256, 511, 512, 79_999, 80_000, 80_001, 160_001] {
        for rate in [
            0.0,
            -0.0,
            1.0 / 48_000.0,
            -1.0 / 48_000.0,
            f64::from_bits(1),
            f64::MAX,
        ] {
            compare_sequence((0..2048).map(|index| {
                let start = index * frames;
                // Alternating origins prevent merging and wrap the retained deque.
                segment(start, frames, (index % 2) as f64, rate)
            }));
        }
    }
    // Ordinary fixed-rate playback merges into one segment, then trims its head.
    compare_sequence((0..8192).map(|index| {
        let start = index * 512;
        segment(start, 512, start as f64 / 48_000.0, 1.0 / 48_000.0)
    }));
}

#[test]
fn retirement_preserves_mixed_valid_and_rejected_updates() {
    let mut seed = 0xdead_beef_u64;
    compare_sequence((0..10_000).map(|index| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let frames = [0, -1, 1, 127, 512, 80_000, 100_000][seed as usize % 7];
        let rate =
            [0.0, -0.0, 0.001, -0.002, f64::MAX, f64::NAN, f64::INFINITY][(seed >> 8) as usize % 7];
        let music = [0.0, -0.0, 1.0, -2.0, f64::MAX, f64::NAN][(seed >> 16) as usize % 6];
        segment(index * 512, frames, music, rate)
    }));
}

#[test]
#[ignore = "manual release benchmark; --ignored --nocapture --test-threads=1"]
fn map_retirement_benchmark() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (case, frames, merged) in [
        ("merged-512", 512, true),
        ("fragmented-512", 512, false),
        ("fragmented-64", 64, false),
        ("replace-80000", 80_000, false),
        ("replace-120000", 120_000, false),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            macro_rules! measure {
                ($module:ident) => {{
                    let mut map = $module::PlaybackPosMap::default();
                    let mut index = 0_i64;
                    let mut next = || {
                        let start = index * frames;
                        let music = if merged {
                            start as f64 / 48_000.0
                        } else {
                            (index % 2) as f64
                        };
                        index += 1;
                        segment(start, frames, music, 1.0 / 48_000.0)
                    };
                    // Warm storage and enter steady-state retirement before timing.
                    for _ in 0..2048 {
                        map.insert(next());
                    }
                    perf::measure_sampled(
                        &format!("{case}/{}", if old { "before" } else { "after" }),
                        1048576,
                        1,
                        || {
                            black_box(&mut map).insert(black_box(next()));
                        },
                    );
                    black_box(map);
                }};
            }
            if old {
                measure!(before);
            } else {
                measure!(current);
            }
        }
    }
}
