//! General-gain mixing must preserve fused rounding, tails and voice progress.
use deadlib_audio_core::{ActiveSfx, MixBus, MixControls, QueuedSfx, SfxPan, mix_active_sfx};
use std::sync::Arc;

#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod allocations;

fn voice(data: Arc<[i16]>, start: u64) -> ActiveSfx {
    ActiveSfx::from_queued(QueuedSfx {
        data,
        bus: MixBus::new(0),
        generation: 0,
        restart_slot: None,
        pan: SfxPan::Center,
        target_stream_frame: start,
        voice: None,
    })
}

fn reference(sample: i16, initial: f32, gain: f32) -> f32 {
    (f32::from(sample) / 32768.0).mul_add(gain, initial)
}

fn assert_bits(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(actual.len(), expected.len());
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "{context}, sample={i}");
    }
}

#[test]
fn all_pcm_values_match_fused_reference_without_allocations() {
    let source: Arc<[i16]> = (i16::MIN..=i16::MAX).chain([123, -456]).collect();
    let controls = MixControls::new();
    let initial = [
        0.0,
        -0.0,
        0.3,
        -0.7,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::from_bits(0x7fc0_1234),
        f32::from_bits(0xff80_1234),
    ];
    for gain in [
        0.0,
        -0.0,
        0.5,
        0.375,
        0.7,
        0.9,
        1.01,
        1.3,
        f32::from_bits(0x07ff_ffff),
        f32::from_bits(0x0800_0001),
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        f32::MAX,
    ] {
        controls.set_bus_gain(MixBus::new(0), gain);
        for seed in initial {
            let mut active = vec![voice(Arc::clone(&source), 0)];
            let mut actual = vec![seed; 65536];
            let expected: Vec<_> = source[..65536]
                .iter()
                .map(|&sample| reference(sample, seed, gain))
                .collect();
            allocations::assert_no_churn(|| {
                assert!(mix_active_sfx(&mut active, &mut actual, 0, 2, &controls));
            });
            assert_bits(
                &actual,
                &expected,
                &format!("gain={gain:?}, initial={seed:?}"),
            );
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].cursor, actual.len());
        }
    }
}

#[test]
fn short_buffers_offsets_and_exhausted_voices_match_reference() {
    let controls = MixControls::new();
    controls.set_bus_gain(MixBus::new(0), 0.9);
    for source_len in [0, 1, 2, 3, 7, 15, 16, 31, 32, 33, 63, 64, 65] {
        let data: Arc<[i16]> = (0..source_len)
            .map(|i: usize| i.wrapping_mul(7919) as i16)
            .collect();
        for output_len in 0..=65 {
            for start_frame in [0, 1, 5, 40] {
                let mut active = vec![voice(Arc::clone(&data), start_frame)];
                let mut actual: Vec<f32> =
                    (0..output_len).map(|i| i as f32 / 100.0 - 0.3).collect();
                let mut expected = actual.clone();
                let offset = start_frame as usize * 2;
                let pending = start_frame != 0 && offset >= output_len;
                let mixed = if pending {
                    0
                } else {
                    source_len.min(output_len - offset)
                };
                for i in 0..mixed {
                    expected[offset + i] = reference(data[i], expected[offset + i], 0.9);
                }
                assert_eq!(
                    mix_active_sfx(&mut active, &mut actual, 0, 2, &controls),
                    mixed != 0
                );
                assert_bits(&actual, &expected, "tail/offset");
                assert_eq!(active.len(), usize::from(pending || mixed < source_len));
                if let Some(voice) = active.first() {
                    assert_eq!(voice.cursor, mixed);
                    assert_eq!(
                        voice.target_stream_frame,
                        if pending { start_frame } else { 0 }
                    );
                }
            }
        }
    }
}

#[test]
fn overlapping_voices_preserve_accumulation_order() {
    let controls = MixControls::new();
    controls.set_bus_gain(MixBus::new(0), 0.7);
    for voices in [1, 2, 8, 32] {
        let sources: Vec<Arc<[i16]>> = (0..voices)
            .map(|v| {
                (0..1026usize)
                    .map(|i| i.wrapping_mul(7919).wrapping_add(v * 421) as i16)
                    .collect()
            })
            .collect();
        let mut expected = vec![0.0; 1024];
        for source in &sources {
            for (&sample, output) in source.iter().zip(&mut expected) {
                *output = reference(sample, *output, 0.7);
            }
        }
        let mut active: Vec<_> = sources.into_iter().map(|data| voice(data, 0)).collect();
        let mut actual = vec![0.0; 1024];
        allocations::assert_no_churn(|| {
            assert!(mix_active_sfx(&mut active, &mut actual, 0, 2, &controls));
        });
        assert_bits(&actual, &expected, "overlapping voices");
        assert!(active.iter().all(|voice| voice.cursor == 1024));
    }
}
