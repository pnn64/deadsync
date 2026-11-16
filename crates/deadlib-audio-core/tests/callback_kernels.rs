//! Measure the production callback kernels with setup outside the measured work.
#![allow(dead_code)]

use deadlib_audio_core::{
    ActiveSfx, CallbackClockSource, MAX_ACTIVE_SFX, MixBus, MixControls, MusicMapSeg, QueuedSfx,
    SfxPan, activate_music_track, bump_music_map_generation, f32_to_i16, i16_to_f32,
    mark_music_track_started, mix_active_sfx, music_gain_snap_generation, music_map_generation,
    music_target_gain, music_total_frames, music_track_active_relaxed, music_track_has_started,
    music_track_start_frame, note_timing_diag_callback_gap, publish_callback_window_end,
    publish_callback_window_start_nanos, push_queued_sfx, reset_music_target_gain,
    set_music_target_gain, snap_music_gain_generation, stop_music_track,
};

#[path = "../../../tests/support/allocations.rs"]
mod perf;
#[path = "../src/ring.rs"]
mod ring;

mod mode_parser {
    use deadlib_audio_core::AudioOutputMode;


    #[test]
    fn modes_preserve_ascii_case_and_unicode_trimming() {
        for (name, expected) in [
            ("auto", AudioOutputMode::Auto),
            ("shared", AudioOutputMode::Shared),
            ("exclusive", AudioOutputMode::Exclusive),
        ] {
            for mask in 0..1 << name.len() {
                let case: String = name
                    .bytes()
                    .enumerate()
                    .map(|(i, byte)| {
                        if mask & (1 << i) == 0 {
                            byte as char
                        } else {
                            byte.to_ascii_uppercase() as char
                        }
                    })
                    .collect();
                for pad in ["", " \t\n", "\u{2003}\u{3000}"] {
                    assert_eq!(format!("{pad}{case}{pad}").parse(), Ok(expected));
                }
            }
        }
        for input in [
            "",
            "a",
            "automatic",
            "shaRed!",
            "exclusiv",
            "exclus\u{0131}ve",
            "\0auto",
            "auto\0",
            "auto shared",
        ] {
            assert!(input.parse::<AudioOutputMode>().is_err());
        }
    }

}

mod quantization {


    #[test]
    fn pcm_conversion_preserves_saturation_truncation_and_nan() {
        let check = |sample: f32| {
            assert_eq!(
                deadlib_audio_core::f32_to_i16(sample),
                (sample * 32768.0) as i16,
                "sample bits={:08x}",
                sample.to_bits()
            );
        };
        for sample in i16::MIN..=i16::MAX {
            let normalized = f32::from(sample) / 32768.0;
            check(normalized);
            check(normalized.next_up());
            check(normalized.next_down());
        }
        let mut seed = 37u32;
        for _ in 0..2_000_000 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            check(f32::from_bits(seed));
        }
    }

}

mod render {
    include!("../src/render.rs");

    #[inline]
    pub(super) fn write_pcm(src: &[f32], dst: &mut [i16], has_audio: bool) {
        write_i16_samples(src, dst, has_audio);
    }

    mod kernels {
        use super::*;


        #[test]
        fn pcm_batches_preserve_samples_and_unmatched_output_tails() {
            let source: Vec<_> = (0..1024u32)
                .map(|i| f32::from_bits(i.wrapping_mul(1664525)))
                .collect();
            for src_len in 0..=65 {
                for dst_len in 0..=65 {
                    for has_audio in [false, true] {
                        let mut expected = vec![1234; dst_len];
                        let mut actual = expected.clone();
                        if has_audio {
                            for (dst, &src) in expected.iter_mut().zip(&source[..src_len]) {
                                *dst = (src * 32768.0) as i16;
                            }
                        } else {
                            expected.fill(0);
                        }
                        write_i16_samples(&source[..src_len], &mut actual, has_audio);
                        assert_eq!(actual, expected, "src_len={src_len}, dst_len={dst_len}");
                    }
                }
            }
        }

        #[test]
        fn gain_steps_preserve_fused_reference_bits() {
            let mut seed = 19u32;
            for _ in 0..200_000 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let initial = f32::from_bits(seed);
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let target = f32::from_bits(seed);
                let mut expected = initial;
                let mut actual = initial;
                for _ in 0..4 {
                    let diff = target - expected;
                    let settled = diff.abs() <= MUSIC_GAIN_MAX_STEP;
                    expected = if settled {
                        target
                    } else {
                        diff.signum().mul_add(MUSIC_GAIN_MAX_STEP, expected)
                    };
                    assert_eq!(advance_gain(&mut actual, target), settled);
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }
            }
        }

        #[test]
        fn steady_conversion_preserves_bits_at_every_gain_exponent() {
            let source: Vec<_> = (i16::MIN..=i16::MAX).collect();
            let mut output = vec![0.0; source.len()];
            let mut seed = 31u32;
            for exponent in 0..=255u32 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                for mantissa in [0, 1, 0x7fffff, seed & 0x7fffff] {
                    for sign in [0, 1 << 31] {
                        let gain = f32::from_bits(sign | exponent << 23 | mantissa);
                        let mut current = 1.0;
                        convert_music_samples(&source, &mut output, 2, gain, 1.0, &mut current);
                        for (&sample, &actual) in source.iter().zip(&output) {
                            let expected = i16_to_f32(sample) * gain;
                            assert_eq!(
                                actual.to_bits(),
                                expected.to_bits(),
                                "sample={sample}, gain={gain:?}"
                            );
                        }
                    }
                }
            }
        }

    }
}
