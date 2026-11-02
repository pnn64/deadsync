use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/perf.rs"]
mod allocations;
#[path = "../../../../tests/support/paired_bench.rs"]
mod bench;
#[path = "perf_original_processing.rs"]
mod original_processing;
#[path = "perf_original_sfx.rs"]
mod original_sfx;

fn samples(frames: usize, channels: usize) -> Vec<i16> {
    (0..frames * channels)
        .map(|i| ((i as u32).wrapping_mul(104729) >> 3) as i16)
        .collect()
}

#[test]
fn reused_padding_matches_original_pcm_and_timestamps() {
    for channels in [1, 2, 6, 8] {
        for (input_hz, output_hz) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (8_000, 48_000),
            (96_000, 8_000),
        ] {
            let mut current = processing::MusicStages::new(channels, input_hz, output_hz);
            let mut original = original_processing::MusicStages::new(channels, input_hz, output_hz);
            for (rate, preserve, frames, packet) in [
                (1.0, false, 0, 137),
                (1.0, false, 1, 137),
                (1.2, false, 731, 137),
                (0.8, true, 12_001, 997),
                (1.2, true, 12_001, 997),
                (1.0, false, 8_192, 4096),
            ] {
                current.set_rate(rate, preserve).unwrap();
                original.set_rate(rate, preserve).unwrap();
                for _ in 0..2 {
                    current.reset();
                    original.reset();
                    let mut a = Vec::new();
                    let mut b = Vec::new();
                    let mut drain =
                        |current: &mut processing::MusicStages,
                         original: &mut original_processing::MusicStages| {
                            for _ in 0..10000 {
                                let x = current.pull(&mut a, channels).unwrap();
                                let y = original.pull(&mut b, channels).unwrap();
                                assert_eq!(x.map(f64::to_bits), y.map(f64::to_bits));
                                assert_eq!(
                                    a, b,
                                    "{channels}ch {input_hz}->{output_hz} rate={rate} preserve={preserve} frames={frames}"
                                );
                                if x.is_none() {
                                    return;
                                }
                            }
                            panic!("drain did not end");
                        };
                    for packet in samples(frames, channels).chunks(packet * channels) {
                        current.push(packet);
                        original.push(packet);
                        drain(&mut current, &mut original);
                    }
                    current.finish();
                    original.finish();
                    drain(&mut current, &mut original);
                    drain(&mut current, &mut original);
                }
            }
        }
    }
}

struct WavFile(PathBuf);
impl WavFile {
    fn new(channels: usize, hz: u32, frames: usize) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "deadsync-perf-sfx-{}-{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let pcm = samples(frames, channels);
        let bytes = (pcm.len() * 2) as u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&(channels as u16).to_le_bytes());
        wav.extend_from_slice(&hz.to_le_bytes());
        wav.extend_from_slice(&(hz * channels as u32 * 2).to_le_bytes());
        wav.extend_from_slice(&(channels as u16 * 2).to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&bytes.to_le_bytes());
        for sample in pcm {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        std::fs::write(&path, wav).unwrap();
        Self(path)
    }
}
impl Drop for WavFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn sfx_loading_matches_original_for_empty_short_and_partial_clips() {
    for channels in [1, 2, 6] {
        for frames in [0, 1, 731, 8192] {
            let file = WavFile::new(channels, 44_100, frames);
            for sample_rate_hz in [8_000, 44_100, 48_000, 96_000] {
                for out_channels in [1, 2, 6] {
                    let output = OutputFormat {
                        sample_rate_hz,
                        channels: out_channels,
                    };
                    let a =
                        load_and_resample_sfx(&file.0, output, decode::DecodeOptions::default());
                    let b = original_sfx::load_and_resample_sfx(
                        &file.0,
                        output,
                        decode::DecodeOptions::default(),
                    );
                    match (a, b) {
                        (Ok(a), Ok(b)) => assert_eq!(
                            a, b,
                            "{channels}->{out_channels}ch {sample_rate_hz}Hz {frames} frames"
                        ),
                        (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
                        _ => panic!("success/error mismatch"),
                    }
                }
            }
        }
    }
}

fn finalize<T>(input: Vec<T>, current: bool) -> Arc<[T]> {
    if current {
        Arc::from(input)
    } else {
        Arc::from(input.into_boxed_slice())
    }
}

#[test]
fn direct_arc_finalization_preserves_values_and_removes_shrink() {
    for len in [0, 1, 4096, 100_000] {
        for extra in [0, 1, 4096] {
            let make = || {
                let mut v = Vec::with_capacity(len + extra);
                v.extend((0..len).map(|i| i as i16));
                v
            };
            let input = make();
            let (old, a) = allocations::measure(|| finalize(input, false));
            let input = make();
            let (new, b) = allocations::measure(|| finalize(input, true));
            assert_eq!(old, new);
            assert_eq!(b.reallocs, 0);
            if len > 0 && extra > 0 {
                assert_eq!(a.reallocs, 1);
            }
        }
    }
}

#[test]
fn warmed_music_tail_needs_no_allocator_work() {
    for channels in [1, 2, 8] {
        let mut stages = processing::MusicStages::new(channels, 44_100, 48_000);
        stages.set_rate(1.0, false).unwrap();
        stages.push(&samples(731, channels));
        let mut out = Vec::with_capacity(OUT_FRAMES_PER_CALL * channels);
        while stages.pull(&mut out, channels).unwrap().is_some() {}
        allocations::assert_no_churn(|| {
            stages.finish();
            while stages.pull(&mut out, channels).unwrap().is_some() {
                black_box(&out);
            }
        });
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_arc_finalization() {
    for (len, extra, iterations) in [
        (4096, 0, 1000),
        (4096, 4096, 1000),
        (96_000, 32_000, 100),
        (1_000_000, 1_000_000, 16),
    ] {
        bench::compare_prepared(
            &format!("Arc PCM {len} spare={extra}"),
            iterations,
            || {
                let mut v = Vec::with_capacity(len + extra);
                v.extend((0..len).map(|i| i as i16));
                v
            },
            |input, current| {
                black_box(finalize(black_box(input), current));
            },
        );
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_audio_padding_storage() {
    for channels in [2, 8] {
        let (_, old) = allocations::measure(|| {
            let mut s = original_processing::MusicStages::new(channels, 44_100, 48_000);
            s.set_rate(1.0, false).unwrap();
            s
        });
        let (_, new) = allocations::measure(|| {
            let mut s = processing::MusicStages::new(channels, 44_100, 48_000);
            s.set_rate(1.0, false).unwrap();
            s
        });
        println!("converter {channels}ch: original {old:?}, current {new:?}");
        assert!(new.allocs < old.allocs);
        assert!(new.allocated_bytes < old.allocated_bytes);
        bench::compare(&format!("converter setup {channels}ch"), 64, |current| {
            if current {
                let mut s = processing::MusicStages::new(channels, 44_100, 48_000);
                s.set_rate(1.0, false).unwrap();
                black_box(s);
            } else {
                let mut s = original_processing::MusicStages::new(channels, 44_100, 48_000);
                s.set_rate(1.0, false).unwrap();
                black_box(s);
            }
        });
    }
    for frames in [731, 44_100] {
        let file = WavFile::new(2, 44_100, frames);
        let output = OutputFormat {
            sample_rate_hz: 48_000,
            channels: 2,
        };
        for current in [false, true] {
            let (_, churn) = allocations::measure(|| {
                if current {
                    load_and_resample_sfx(&file.0, output, decode::DecodeOptions::default())
                        .unwrap()
                } else {
                    original_sfx::load_and_resample_sfx(
                        &file.0,
                        output,
                        decode::DecodeOptions::default(),
                    )
                    .unwrap()
                }
            });
            println!("SFX {frames} current={current}: {churn:?}");
        }
        bench::compare(&format!("SFX load {frames}"), 32, |current| {
            black_box(if current {
                load_and_resample_sfx(&file.0, output, decode::DecodeOptions::default()).unwrap()
            } else {
                original_sfx::load_and_resample_sfx(
                    &file.0,
                    output,
                    decode::DecodeOptions::default(),
                )
                .unwrap()
            });
        });
    }
}
