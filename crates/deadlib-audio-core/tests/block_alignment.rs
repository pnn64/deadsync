use deadlib_audio_core::MusicMapSeg;

#[allow(dead_code)]
mod transport {
    include!("../src/ring.rs");
}

use transport::{MUSIC_BLOCK_FRAMES, MusicBlockTiming, music_transport};

#[test]
fn pushes_preserve_complete_frames_samples_and_timing() {
    for channels in [0, 1, 2, 3, 4, 6, 8, 16, 32] {
        let (mut actual, mut actual_render) = music_transport(48000, channels);
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
            let expected_len = len.min(capacity) / channels * channels;
            let after = actual.writer.try_push(&samples[..len], timing);
            assert_eq!(after, expected_len, "channels {channels}, len {len}");
            assert_eq!(actual.writer.outstanding_blocks(), usize::from(after != 0));
            let after_block = actual_render.pop_block();
            assert_eq!(after_block.is_some(), expected_len != 0);
            if let Some(after) = after_block {
                assert_eq!(
                    after.samples(),
                    &samples[..len.min(capacity) / channels * channels]
                );
                assert_eq!(after.timing().generation, timing.generation);
                assert_eq!(
                    after.timing().music_start_sec.to_bits(),
                    timing.music_start_sec.to_bits()
                );
                assert_eq!(
                    after.timing().music_sec_per_frame.to_bits(),
                    timing.music_sec_per_frame.to_bits()
                );
                assert!(actual_render.recycle_block(after).is_ok());
            }
        }
    }
}
