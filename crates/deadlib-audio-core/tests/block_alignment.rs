use deadlib_audio_core::MusicMapSeg;
use std::hint::black_box;

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

#[allow(dead_code)]
mod transport {
    include!("../src/ring.rs");

    impl MusicBlockWriter {
        // Frozen try_push from eaba07c81. Queue and block types are unchanged.
        pub fn baseline_push(&mut self, samples: &[i16], timing: MusicBlockTiming) -> usize {
            let channels = self.channels;
            let sample_len = samples.len().min(MUSIC_BLOCK_FRAMES * channels) / channels * channels;
            if sample_len == 0 {
                return 0;
            }
            let Some(mut block) = self.spare.take().or_else(|| self.recycled.pop().ok()) else {
                return 0;
            };
            block.samples[..sample_len].copy_from_slice(&samples[..sample_len]);
            block.sample_len = sample_len;
            block.timing = timing;
            match self.ready.push(block) {
                Ok(()) => sample_len,
                Err(PushError::Full(mut block)) => {
                    block.sample_len = 0;
                    self.spare = Some(block);
                    0
                }
            }
        }
    }
}

use transport::{MUSIC_BLOCK_FRAMES, MusicBlockTiming, MusicBlockWriter, music_transport};

type Push = fn(&mut MusicBlockWriter, &[i16], MusicBlockTiming) -> usize;

#[test]
fn pushes_preserve_complete_frames_samples_and_timing() {
    for channels in [0, 1, 2, 3, 4, 6, 8, 16, 32] {
        let (mut actual, mut actual_render) = music_transport(48000, channels);
        let (mut expected, mut expected_render) = music_transport(48000, channels);
        let channels = channels.max(1);
        let capacity = MUSIC_BLOCK_FRAMES * channels;
        let samples: Vec<_> = (0..2 * capacity + channels)
            .map(|index| index.wrapping_mul(7919) as i16)
            .collect();
        for len in 0..=samples.len() {
            let timing = MusicBlockTiming {
                generation: len as u64,
                music_start_sec: f64::from_bits((len as u64).wrapping_mul(0x9e3779b97f4a7c15)),
                music_sec_per_frame: 1.5 / 48000.0,
            };
            let before = expected.writer.baseline_push(&samples[..len], timing);
            let after = actual.writer.try_push(&samples[..len], timing);
            assert_eq!(after, before, "channels {channels}, len {len}");
            assert_eq!(actual.writer.outstanding_blocks(), usize::from(after != 0));
            let before_block = expected_render.pop_block();
            let after_block = actual_render.pop_block();
            assert_eq!(after_block.is_some(), before_block.is_some());
            if let (Some(after), Some(before)) = (after_block, before_block) {
                assert_eq!(after.samples(), before.samples());
                assert_eq!(
                    after.samples(),
                    &samples[..len.min(capacity) / channels * channels]
                );
                assert_eq!(after.timing().generation, before.timing().generation);
                assert_eq!(
                    after.timing().music_start_sec.to_bits(),
                    before.timing().music_start_sec.to_bits()
                );
                assert_eq!(
                    after.timing().music_sec_per_frame.to_bits(),
                    before.timing().music_sec_per_frame.to_bits()
                );
                assert!(actual_render.recycle_block(after).is_ok());
                assert!(expected_render.recycle_block(before).is_ok());
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_block_alignment() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for channels in [1, 2, 3, 6, 8] {
        let capacity = MUSIC_BLOCK_FRAMES * channels;
        for (case, len) in [
            ("full", capacity),
            ("larger", capacity * 4 + 1),
            ("partial", 137 * channels),
            ("partial-tail", 137 * channels + channels - 1),
            ("empty", 0),
        ] {
            let samples = vec![173i16; len];
            let timing = MusicBlockTiming {
                generation: 1,
                music_start_sec: -0.5,
                music_sec_per_frame: 1.0 / 48000.0,
            };
            let variants: [(&str, Push); 2] = [
                ("before", MusicBlockWriter::baseline_push),
                ("after", MusicBlockWriter::try_push),
            ];
            for index in if reverse { [1, 0] } else { [0, 1] } {
                let (name, push) = variants[index];
                let push = black_box(push);
                let (mut stream, mut render) = music_transport(48000, black_box(channels));
                perf::measure_sampled(&format!("{case}/{channels}/{name}"), 131072, 1, || {
                    let accepted = push(
                        black_box(&mut stream.writer),
                        black_box(&samples),
                        black_box(timing),
                    );
                    if let Some(block) = render.pop_block() {
                        black_box(block.samples());
                        assert!(render.recycle_block(block).is_ok());
                    }
                    black_box(accepted);
                });
            }
        }
    }
}

#[test]
#[ignore = "manual mixed-packet release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_packet_alignment() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for channels in [1, 2, 6] {
        for (case, frames) in [
            ("decoder-packets", &[137, 256, 512, 1152, 4096][..]),
            ("small-packets", &[1, 7, 31, 137, 255, 256][..]),
        ] {
            let samples = vec![173i16; frames.iter().sum::<usize>() * channels];
            let variants: [(&str, Push); 2] = [
                ("before", MusicBlockWriter::baseline_push),
                ("after", MusicBlockWriter::try_push),
            ];
            for index in if reverse { [1, 0] } else { [0, 1] } {
                let (name, push) = variants[index];
                let push = black_box(push);
                let (mut stream, mut render) = music_transport(48000, black_box(channels));
                perf::measure_sampled(
                    &format!("{case}/{channels}/{name}"),
                    65536,
                    samples.len(),
                    || {
                        let mut timing = MusicBlockTiming {
                            generation: 1,
                            music_start_sec: -0.5,
                            music_sec_per_frame: 1.0 / 48000.0,
                        };
                        let mut offset = 0;
                        for &frames in black_box(frames) {
                            let end = offset + frames * channels;
                            while offset < end {
                                let accepted = push(
                                    black_box(&mut stream.writer),
                                    black_box(&samples[offset..end]),
                                    timing,
                                );
                                assert_ne!(accepted, 0);
                                offset += accepted;
                                timing.music_start_sec = ((accepted / channels) as f64)
                                    .mul_add(timing.music_sec_per_frame, timing.music_start_sec);
                                let block = render.pop_block().expect("pushed block is available");
                                black_box(block.samples());
                                assert!(render.recycle_block(block).is_ok());
                            }
                        }
                        black_box(timing);
                    },
                );
            }
        }
    }
}
