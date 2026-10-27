use deadlib_audio_core::{ActiveSfx, MixBus, MixControls, QueuedSfx, SfxPan, mix_active_sfx};
use std::{hint::black_box, sync::Arc};

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn voice(data: Arc<[i16]>, pan: SfxPan) -> ActiveSfx {
    ActiveSfx::from_queued(QueuedSfx {
        data,
        bus: MixBus::new(0),
        generation: 0,
        restart_slot: None,
        pan,
        target_stream_frame: 0,
        voice: None,
    })
}

#[test]
fn panning_matches_fused_reference_bits_and_voice_progress() {
    let source: Arc<[i16]> = (i16::MIN..=i16::MAX)
        .flat_map(|sample| [0, sample])
        .chain([321, -654])
        .collect();
    let controls = MixControls::new();
    for pan in [SfxPan::Left, SfxPan::Right, SfxPan::Center] {
        let gains = [0.0, -0.0, 1.0, 0.375, f32::MIN_POSITIVE, f32::MAX]
            .into_iter()
            .chain((1..255).map(|exponent| f32::from_bits(exponent << 23)))
            .chain((0..256).map(|fraction| f32::from_bits((127 << 23) | (fraction << 15))))
            .chain([1, 15, 16, 254].into_iter().flat_map(|exponent| {
                [1, 3, 127, 255].map(|fraction| f32::from_bits((exponent << 23) | (fraction << 15)))
            }))
            .chain([0x07ff_ffff, 0x0800_0001, 0x3ec0_0001].map(f32::from_bits));
        for gain in gains {
            controls.set_bus_gain(MixBus::new(0), gain);
            for initial in [
                0.0f32,
                -0.0,
                0.25,
                -0.25,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::from_bits(1),
                f32::from_bits(0x7fc0_1234),
                f32::from_bits(0xffc0_4321),
            ] {
                let mut active = vec![voice(source.clone(), pan)];
                // Panning ignores incomplete frames; centered mixing includes their samples.
                let mut actual = vec![initial; source.len() - 1];
                let mut expected = actual.clone();
                let mix = |sample: i16, output: &mut f32| {
                    let sample = f32::from(sample) / 32768.0;
                    *output = if gain == 1.0 {
                        *output + sample
                    } else {
                        sample.mul_add(gain, *output)
                    };
                };
                if pan == SfxPan::Center {
                    for (&input, output) in source.iter().zip(&mut expected) {
                        mix(input, output);
                    }
                } else {
                    let channel = usize::from(pan == SfxPan::Right);
                    for (input, output) in source
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .zip(expected.as_chunks_mut::<2>().0)
                    {
                        mix(input[1], &mut output[channel]);
                    }
                }
                assert!(mix_active_sfx(&mut active, &mut actual, 0, 2, &controls));
                assert_eq!(active.len(), 1);
                assert_eq!(active[0].cursor, actual.len());
                assert_eq!(
                    actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    "{pan:?}, gain={gain}, initial={initial}"
                );
            }
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_panned_mix() {
    let controls = MixControls::new();
    for pan in [SfxPan::Left, SfxPan::Right, SfxPan::Center] {
        for gain in [0.0, 1.0, 0.5, 0.125, 2.0, 0.375] {
            controls.set_bus_gain(MixBus::new(0), gain);
            for frames in [256usize, 4096] {
                let data: Arc<[i16]> = (0..frames * 2 + 2)
                    .map(|i| i.wrapping_mul(71) as i16)
                    .collect();
                let mut active = vec![voice(data, pan)];
                let mut output = vec![0.0; frames * 2];
                let label = format!("pan/{pan:?}/gain={gain}/frames={frames}");
                let mut work = || {
                    active[0].cursor = 0;
                    output.fill(0.0);
                    black_box(mix_active_sfx(
                        black_box(&mut active),
                        black_box(&mut output),
                        0,
                        2,
                        black_box(&controls),
                    ));
                    black_box(&output);
                };
                let (_, counts) = perf::measure(&mut work);
                println!("{label}: allocation {counts:?}");
                bench::measure(&label, 2_000_000 / frames, &mut work);
            }
        }
    }
}
