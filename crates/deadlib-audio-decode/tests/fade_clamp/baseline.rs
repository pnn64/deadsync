// Frozen from 8721d6d94.
pub fn apply_fade_envelope(
    samples: &mut [i16],
    channels: usize,
    start_frame: u64,
    fade: (i64, i64),
) {
    let (full_volume_frame, silence_frame) = fade;
    if samples.is_empty() || channels == 0 || full_volume_frame == silence_frame {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let start_frame = saturating_i64_from_u64(start_frame);
    let end_frame = saturating_i64_from_u64(frames as u64).saturating_add(start_frame);
    let start_volume = volume_for_frame(start_frame, full_volume_frame, silence_frame);
    let end_volume = volume_for_frame(end_frame, full_volume_frame, silence_frame);
    if start_volume > 0.9999 && end_volume > 0.9999 {
        return;
    }
    if start_volume == 0.0 && end_volume == 0.0 {
        // Preserve any trailing samples that do not form a complete frame.
        samples[..frames * channels].fill(0);
        return;
    }
    let frames_f = frames as f32;
    for (frame, samples) in samples.chunks_exact_mut(channels).enumerate() {
        let t = frame as f32 / frames_f;
        let volume = (end_volume - start_volume)
            .mul_add(t, start_volume)
            .clamp(0.0, 1.0);
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for sample in samples {
            let scaled = f32::from(*sample) * volume;
            *sample = scaled.round() as i16;
        }
    }
}

#[inline]
#[must_use]
pub fn saturating_i64_from_u64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

#[inline]
fn volume_for_frame(position: i64, full_volume_frame: i64, silence_frame: i64) -> f32 {
    if full_volume_frame == silence_frame {
        return 1.0;
    }
    let full = full_volume_frame as f64;
    let silence = silence_frame as f64;
    let pos = position as f64;
    let denom = silence - full;
    if denom.abs() < f64::EPSILON {
        return if silence > full { 0.0 } else { 1.0 };
    }
    let volume = ((pos - full) * (0.0 - 1.0) / denom) + 1.0;
    volume.clamp(0.0, 1.0) as f32
}
