use super::*;
use std::hint::black_box;
#[path = "position_original.rs"]
mod original;
#[path = "../../../tests/perf/runtime_support.rs"]
mod support;

fn segments(kind: &str, count: usize) -> Vec<MusicMapSeg> {
    let mut music_start = 0.0;
    (0..count as i64)
        .map(|i| {
            let slope =
                if (kind == "rate" && i % 2 == 1) || (kind == "rate-sparse" && (i / 64) % 2 == 1) {
                    1.5 / 48_000.0
                } else {
                    1.0 / 48_000.0
                };
            let seg = MusicMapSeg {
                stream_frame_start: i * if kind == "gap" { 257 } else { 256 },
                frames: 256,
                music_start_sec: music_start
                    + if kind == "music-gap" {
                        (i % 2) as f64
                    } else {
                        0.0
                    },
                music_sec_per_frame: slope,
            };
            music_start += 256.0 * slope;
            seg
        })
        .collect()
}
fn assert_same(old: &PlaybackPosMap, new: &PlaybackPosMap) {
    assert_eq!(old.backlog_frames, new.backlog_frames);
    assert_eq!(old.queue.len(), new.queue.len());
    assert_eq!(old.queue.capacity(), new.queue.capacity());
    for (a, b) in old.queue.iter().zip(&new.queue) {
        assert_eq!(
            (
                a.stream_frame_start,
                a.frames,
                a.music_start_sec.to_bits(),
                a.music_sec_per_frame.to_bits()
            ),
            (
                b.stream_frame_start,
                b.frames,
                b.music_start_sec.to_bits(),
                b.music_sec_per_frame.to_bits()
            )
        );
    }
    for query in [
        -1.0,
        0.0,
        256.0,
        80_000.0,
        262_144.0,
        f64::NAN,
        f64::INFINITY,
    ] {
        assert_eq!(
            old.search(query).map(|(a, b)| (a.to_bits(), b.to_bits())),
            new.search(query).map(|(a, b)| (a.to_bits(), b.to_bits()))
        );
        assert_eq!(
            old.invert(query).map(f64::to_bits),
            new.invert(query).map(f64::to_bits)
        );
    }
}
#[test]
fn short_circuit_insert_preserves_mapping_bits() {
    for kind in ["contiguous", "gap", "rate", "rate-sparse", "music-gap"] {
        let mut old = PlaybackPosMap::default();
        let mut new = PlaybackPosMap::default();
        for seg in segments(kind, 1024) {
            old.insert_original(seg);
            new.insert(seg);
            assert_same(&old, &new);
        }
    }
    let values = [
        -0.0,
        0.0,
        1e-9,
        -1e-9,
        1.0 / 48_000.0,
        -1.0 / 48_000.0,
        f64::MAX,
        f64::NAN,
        f64::INFINITY,
    ];
    for slope in values {
        for start in values {
            for frames in [-1, 0, 1, 80_000, 80_001] {
                let mut old = PlaybackPosMap::default();
                let mut new = PlaybackPosMap::default();
                for i in 0..3 {
                    let seg = MusicMapSeg {
                        stream_frame_start: i * frames,
                        frames,
                        music_start_sec: start,
                        music_sec_per_frame: slope,
                    };
                    old.insert_original(seg);
                    new.insert(seg);
                    assert_same(&old, &new);
                }
            }
        }
    }
}
#[test]
fn mixed_insertions_preserve_coalescing_thresholds_and_cleanup() {
    let mut old = PlaybackPosMap::default();
    let mut new = PlaybackPosMap::default();
    let mut seed = 17_u64;
    for i in 0..5000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let frames = [1, 255, 256, 80000, 80001][(seed as usize >> 16) % 5];
        let slope = [
            0.0,
            1e-9,
            1e-9 + f64::EPSILON,
            1.0 / 48000.0,
            -1.0 / 48000.0,
        ][(seed as usize >> 24) % 5];
        let (stream, start) = if let Some(last) = old.queue.back() {
            let end = last
                .music_sec_per_frame
                .mul_add(last.frames as f64, last.music_start_sec);
            (
                last.stream_frame_start + last.frames,
                end + [0.0, slope.abs(), slope.abs() + 1e-9, -1.0][i % 4],
            )
        } else {
            (0, 0.0)
        };
        let seg = MusicMapSeg {
            stream_frame_start: stream + if i % 7 == 0 { -257 } else { 0 },
            frames,
            music_start_sec: start,
            music_sec_per_frame: slope,
        };
        old.insert_original(seg);
        new.insert(seg);
        assert_same(&old, &new);
        if i % 111 == 0 {
            old.clear();
            new.clear();
        }
    }
}
#[test]
#[ignore = "paired release benchmark"]
fn benchmark_runtime_audio_insert() {
    for kind in ["contiguous", "gap", "rate", "rate-sparse", "music-gap"] {
        for count in [1, 128, 1024] {
            let input = segments(kind, count);
            let label = format!("audio-insert-{kind}-{count}");
            let mut old = PlaybackPosMap::default();
            let mut new = PlaybackPosMap::default();
            support::compare(
                &label,
                100,
                || {
                    old.clear();
                    for &seg in black_box(&input) {
                        old.insert_original(seg);
                    }
                    black_box(&old);
                },
                || {
                    new.clear();
                    for &seg in black_box(&input) {
                        new.insert(seg);
                    }
                    black_box(&new);
                },
            );
        }
    }
}
