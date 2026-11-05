//! Frozen audio math from 490a84271.
use deadlib_audio_core::i16_to_f32;
const MUSIC_GAIN_RAMP_FRAMES: f32 = 4000.0;
const MUSIC_GAIN_MAX_STEP: f32 = 1.0 / MUSIC_GAIN_RAMP_FRAMES;
#[inline(always)]
pub fn advance_gain(current: &mut f32, target: f32) -> bool {
    let diff = target - *current;
    if diff.abs() <= MUSIC_GAIN_MAX_STEP {
        *current = target;
        true
    } else {
        *current = diff.signum().mul_add(MUSIC_GAIN_MAX_STEP, *current);
        false
    }
}

pub fn convert_music_samples(
    src: &[i16],
    dst: &mut [f32],
    channels: usize,
    music_vol: f32,
    target_gain: f32,
    current_gain: &mut f32,
) {
    if channels == 2
        && *current_gain != target_gain
        && (target_gain - *current_gain).abs()
            <= MUSIC_GAIN_MAX_STEP * (src.len().min(dst.len()) / 2) as f32
    {
        // Only split callbacks whose ramp is close to settling. Check every 64
        // frames, then convert the settled tail in one constant-gain chunk.
        let samples = src.len().min(dst.len()) / 2 * 2;
        let mut start = 0;
        while start < samples {
            let end = if *current_gain == target_gain {
                samples
            } else {
                (start + 128).min(samples)
            };
            convert_music_chunk(
                &src[start..end],
                &mut dst[start..end],
                channels,
                music_vol,
                target_gain,
                current_gain,
            );
            start = end;
        }
    } else {
        convert_music_chunk(src, dst, channels, music_vol, target_gain, current_gain);
    }
}

pub fn convert_music_chunk(
    src: &[i16],
    dst: &mut [f32],
    channels: usize,
    music_vol: f32,
    target_gain: f32,
    current_gain: &mut f32,
) {
    if *current_gain == target_gain {
        let scale = music_vol * target_gain;
        if (scale.abs() >= f32::MIN_POSITIVE * 32768.0 && scale.is_finite()) || scale == 0.0 {
            // Dividing the gain by 32768 is exact above this bound. Fold PCM
            // normalization into it once; keep the original order near underflow.
            // Signed zero must still multiply each sample to preserve its sign.
            let pcm_scale = scale / 32768.0;
            for (dst, &src) in dst.iter_mut().zip(src) {
                *dst = f32::from(src) * pcm_scale;
            }
            return;
        }
        for (dst, &src) in dst.iter_mut().zip(src) {
            *dst = i16_to_f32(src) * scale;
        }
        return;
    }
    if channels == 2 {
        let src_frames = src.as_chunks::<2>().0;
        let dst_frames = dst.as_chunks_mut::<2>().0;
        for (src_frame, dst_frame) in src_frames.iter().zip(dst_frames) {
            advance_gain(current_gain, target_gain);
            let scale = music_vol * *current_gain;
            dst_frame[0] = i16_to_f32(src_frame[0]) * scale;
            dst_frame[1] = i16_to_f32(src_frame[1]) * scale;
        }
        return;
    }
    for (src_frame, dst_frame) in src
        .chunks_exact(channels)
        .zip(dst.chunks_exact_mut(channels))
    {
        advance_gain(current_gain, target_gain);
        let scale = music_vol * *current_gain;
        for (dst, &src) in dst_frame.iter_mut().zip(src_frame) {
            *dst = i16_to_f32(src) * scale;
        }
    }
}

#[inline(always)]
#[must_use]
pub fn normalized_music_rate(rate: f32) -> f32 {
    if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    }
}

#[inline(always)]
pub fn valid_gain(gain: f32) -> f32 {
    if gain.is_finite() && gain >= 0.0 {
        gain
    } else {
        1.0
    }
}
