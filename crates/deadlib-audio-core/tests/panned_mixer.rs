//! Bit-exact panned SFX checks and CPU-cycle benchmarks of the real mixer.
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod alloc;
#[allow(dead_code)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

#[allow(dead_code)]
mod mixer {
    include!("../src/mixer.rs");

    mod kernel {
        use super::*;
        use std::hint::black_box;

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
                for gain in [0.0, -0.0, 0.375, 1.0, 1.5, f32::INFINITY, f32::NAN] {
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
        #[ignore = "release CPU benchmark"]
        fn benchmark_panned_mixing() {
            for frames in [64, 2048] {
                let src: Vec<_> = (0..frames * 2).map(|i| i as i16).collect();
                let mut dst = vec![0.0; src.len()];
                for gain in [0.0, 0.375, 1.0] {
                    let label = format!("panned/frames={frames}/gain={gain}");
                    let (_, churn) = crate::alloc::measure(|| {
                        mix_sfx_panned_stereo(&src, &mut dst, gain, SfxPan::Right)
                    });
                    println!("{label}: allocation {churn:?}");
                    crate::bench::measure(&label, 4096, || {
                        mix_sfx_panned_stereo(
                            black_box(&src),
                            black_box(&mut dst),
                            black_box(gain),
                            black_box(SfxPan::Right),
                        );
                        black_box(&dst);
                    });
                }
            }
            let controls = MixControls::new();
            let data: Arc<[i16]> = Arc::from(vec![1000; 4098]);
            let mut active: Vec<_> = (0..16)
                .map(|i| {
                    ActiveSfx::from_queued(QueuedSfx {
                        data: Arc::clone(&data),
                        bus: MixBus::new(0),
                        generation: 0,
                        restart_slot: None,
                        pan: if i % 2 == 0 {
                            SfxPan::Right
                        } else {
                            SfxPan::Left
                        },
                        target_stream_frame: 0,
                        voice: None,
                    })
                })
                .collect();
            let mut dst = vec![0.0; 4096];
            let label = "panned/active=16/frames=2048/gain=1";
            let mut work = || {
                for sfx in &mut active {
                    sfx.cursor = 0;
                }
                black_box(mix_active_sfx(
                    black_box(&mut active),
                    black_box(&mut dst),
                    0,
                    2,
                    black_box(&controls),
                ));
                black_box(&dst);
            };
            let (_, churn) = crate::alloc::measure(&mut work);
            println!("{label}: allocation {churn:?}");
            crate::bench::measure(label, 128, work);
        }
    }
}
