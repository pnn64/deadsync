use deadlib_audio_decode::resample::apply_fade_envelope;

// Frozen scalar behavior at 8cfb89586, including endpoint saturation and tails.
fn reference(samples: &mut [i16], channels: usize, start: u64, fade: (i64, i64)) {
    let (full, silence) = fade;
    if samples.is_empty() || channels == 0 || full == silence {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let volume = |position: i64| {
        let full = full as f64;
        let silence = silence as f64;
        let denom = silence - full;
        if denom.abs() < f64::EPSILON {
            return if silence > full { 0.0 } else { 1.0 };
        }
        ((-(position as f64 - full) / denom) + 1.0).clamp(0.0, 1.0) as f32
    };
    let start = start.min(i64::MAX as u64) as i64;
    let end = (frames as u64).min(i64::MAX as u64) as i64;
    let a = volume(start);
    let b = volume(end.saturating_add(start));
    if a > 0.9999 && b > 0.9999 {
        return;
    }
    if a == 0.0 && b == 0.0 {
        samples[..frames * channels].fill(0);
        return;
    }
    for (index, frame) in samples.chunks_exact_mut(channels).enumerate() {
        let value = (b - a).mul_add(index as f32 / frames as f32, a);
        if (value - 1.0).abs() < 0.0001 {
            continue;
        }
        for sample in frame {
            *sample = (f32::from(*sample) * value).round() as i16;
        }
    }
}

#[test]
fn fade_preserves_every_pcm_value_across_rising_falling_and_extreme_envelopes() {
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for fade in [
        (0, 1000),
        (1000, 0),
        (-71_952, 48),
        (i64::MIN, i64::MAX),
        (i64::MAX - 1, i64::MAX),
        (i64::MIN, i64::MIN + 1),
        (0, 0),
    ] {
        for start in [0, 1, 2, 17, 48, 125, 333, 500, 750, 999, 1000, u64::MAX] {
            let mut expected = input.clone();
            let mut actual = input.clone();
            reference(&mut expected, input.len(), start, fade);
            apply_fade_envelope(&mut actual, input.len(), start, fade);
            assert!(actual == expected, "start={start}, fade={fade:?}");
        }
    }
}

#[test]
fn fade_preserves_interpolation_rounding_incomplete_frames_and_unity_threshold() {
    let mut seed = 0x1234_5678u32;
    for case in 0..600 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let channels = case % 11;
        let frames = seed as usize % 4097;
        let mut actual: Vec<_> = (0..frames * channels + case % 11)
            .map(|i| (i as u32).wrapping_mul(seed) as i16)
            .collect();
        let mut expected = actual.clone();
        let start = (seed % 1000) as u64;
        let end = (seed % 5000) as i64;
        let fade = if case % 2 == 0 {
            (-17, end)
        } else {
            (end, -17)
        };
        reference(&mut expected, channels, start, fade);
        apply_fade_envelope(&mut actual, channels, start, fade);
        assert!(actual == expected, "case={case}, channels={channels}");
    }
}
