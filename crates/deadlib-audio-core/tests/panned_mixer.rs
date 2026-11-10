//! Bit-exact panned SFX checks for the production mixer.
#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod alloc;

#[allow(dead_code)]
mod mixer {
    include!("../src/mixer.rs");

    mod kernel {
        use super::*;


        #[expect(
            clippy::chunks_exact_to_as_chunks,
            reason = "Preserve the original traversal"
        )]
        fn reference(src: &[i16], dst: &mut [f32], gain: f32, pan: SfxPan) {
            let channel = usize::from(pan == SfxPan::Right);
            for (source, target) in src.chunks_exact(2).zip(dst.chunks_exact_mut(2)) {
                target[channel] = i16_to_f32(source[1]).mul_add(gain, target[channel]);
            }
        }

        #[test]
        fn panned_mixing_preserves_every_sample_and_exceptional_float_bits() {
            let src: Vec<_> = (i16::MIN..=i16::MAX)
                .flat_map(|sample| [sample.wrapping_neg(), sample])
                .collect();
            for pan in [SfxPan::Center, SfxPan::Left, SfxPan::Right] {
                for gain in [
                    0.0,
                    -0.0,
                    0.375,
                    1.0,
                    1.5,
                    0.7,
                    0.9,
                    1.3,
                    f32::MIN_POSITIVE,
                    f32::from_bits(1),
                    f32::MAX,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    for initial in [
                        0.0,
                        -0.0,
                        0.5,
                        -0.5,
                        f32::MIN_POSITIVE,
                        f32::from_bits(1),
                        f32::MAX,
                        f32::INFINITY,
                        -f32::INFINITY,
                        f32::from_bits(0x7fc01234),
                        f32::from_bits(0xff801234),
                    ] {
                        let mut expected = vec![initial; src.len() + 1];
                        let mut actual = expected.clone();
                        reference(&src, &mut expected, gain, pan);
                        crate::alloc::assert_no_churn(|| {
                            mix_sfx_panned_stereo(&src, &mut actual, gain, pan)
                        });
                        assert!(
                            actual
                                .iter()
                                .zip(&expected)
                                .all(|(a, b)| a.to_bits() == b.to_bits()),
                            "pan={pan:?}, gain={gain}, initial={initial:?}"
                        );
                    }
                }
            }
            let mut seed = 0x12345678u32;
            for length in 0..=65 {
                for src_len in 0..=65 {
                    let mut actual: Vec<_> = (0..length)
                        .map(|_| {
                            seed ^= seed << 13;
                            seed ^= seed >> 17;
                            seed ^= seed << 5;
                            f32::from_bits(seed)
                        })
                        .collect();
                    let mut expected = actual.clone();
                    reference(&src[..src_len], &mut expected, 1.0, SfxPan::Right);
                    mix_sfx_panned_stereo(&src[..src_len], &mut actual, 1.0, SfxPan::Right);
                    assert!(
                        actual
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.to_bits() == b.to_bits())
                    );
                }
            }
        }

        #[test]
        fn general_gain_preserves_channels_tails_and_guard_samples() {
            let src: Vec<_> = (0..68usize).map(|i| i.wrapping_mul(7919) as i16).collect();
            let bits = [
                0_u32, 0x80000000, 0x3e99999a, 0xbf333333, 1, 0x80000001, 0x7f800000, 0xff800000,
                0x7fc01234, 0xff801234,
            ];
            for pan in [SfxPan::Left, SfxPan::Right] {
                for start in [0, 1] {
                    for src_len in 0..=65 {
                        for dst_len in 0..=65 {
                            for gain in [0.7, 0.9, 1.3] {
                                let source = &src[start..start + src_len];
                                let mut expected: Vec<_> = (0..dst_len + 2)
                                    .map(|i| f32::from_bits(bits[i % bits.len()]))
                                    .collect();
                                let mut actual = expected.clone();
                                reference(source, &mut expected[1..dst_len + 1], gain, pan);
                                crate::alloc::assert_no_churn(|| {
                                    mix_sfx_panned_stereo(
                                        source,
                                        &mut actual[1..dst_len + 1],
                                        gain,
                                        pan,
                                    )
                                });
                                assert!(
                                    actual
                                        .iter()
                                        .zip(&expected)
                                        .all(|(a, b)| a.to_bits() == b.to_bits()),
                                    "pan={pan:?}, src={src_len}, dst={dst_len}, start={start}, gain={gain}"
                                );
                            }
                        }
                    }
                }
            }
        }

    }
}
