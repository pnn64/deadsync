use rubato::{SincInterpolationParameters, SincInterpolationType, WindowFunction};

pub const OUT_FRAMES_PER_CALL: usize = 256;
pub const PLANAR_INPUT_CAP_FRAMES: usize = 4096;

const PLANAR_COMPACT_THRESHOLD_FRAMES: usize = 2048;

pub struct PlanarAccum {
    pub channels: Vec<Vec<f32>>,
    pub start_frame: usize,
}

impl PlanarAccum {
    pub fn new(channels: usize, capacity_frames: usize) -> Self {
        let mut planar = Vec::with_capacity(channels);
        for _ in 0..channels {
            planar.push(Vec::with_capacity(capacity_frames));
        }
        Self {
            channels: planar,
            start_frame: 0,
        }
    }

    #[inline(always)]
    pub fn available_frames(&self) -> usize {
        self.channels
            .first()
            .map_or(0, |channel| channel.len().saturating_sub(self.start_frame))
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.available_frames() == 0
    }

    pub fn push_i16_interleaved(&mut self, interleaved: &[i16], channels: usize) {
        if interleaved.is_empty() || channels == 0 {
            return;
        }
        debug_assert_eq!(channels, self.channels.len());
        let frames = interleaved.len() / channels;
        if frames == 0 {
            return;
        }
        for channel in &mut self.channels {
            channel.reserve(frames);
        }
        if channels == 1 {
            if let [channel] = self.channels.as_mut_slice() {
                channel.extend(
                    interleaved
                        .iter()
                        .map(|sample| f32::from(*sample) / 32768.0),
                );
                return;
            }
        } else if channels == 2
            && let [left, right] = self.channels.as_mut_slice()
        {
            for frame in interleaved.chunks_exact(2) {
                left.push(f32::from(frame[0]) / 32768.0);
                right.push(f32::from(frame[1]) / 32768.0);
            }
            return;
        }
        for frame in interleaved.chunks_exact(channels) {
            for (channel, sample) in self.channels.iter_mut().zip(frame.iter()) {
                channel.push(f32::from(*sample) / 32768.0);
            }
        }
    }

    pub fn consume_frames(&mut self, frames: usize) {
        let total_frames = self.channels.first().map_or(0, Vec::len);
        self.start_frame = (self.start_frame + frames).min(total_frames);
        self.compact_if_needed();
    }

    pub fn clear(&mut self) {
        self.start_frame = 0;
        for channel in &mut self.channels {
            channel.clear();
        }
    }

    fn compact_if_needed(&mut self) {
        if self.start_frame == 0 {
            return;
        }
        let total_frames = self.channels.first().map_or(0, Vec::len);
        let remaining_frames = total_frames.saturating_sub(self.start_frame);
        if remaining_frames == 0 {
            self.clear();
            return;
        }
        if self.start_frame < PLANAR_COMPACT_THRESHOLD_FRAMES && self.start_frame * 2 < total_frames
        {
            return;
        }
        for channel in &mut self.channels {
            channel.copy_within(self.start_frame.., 0);
            channel.truncate(remaining_frames);
        }
        self.start_frame = 0;
    }
}

#[inline(always)]
pub fn resampler_params() -> SincInterpolationParameters {
    SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    }
}

pub fn write_resampler_output(
    out: &[Vec<f32>],
    produced_frames: usize,
    out_ch: usize,
    out_tmp: &mut Vec<i16>,
) -> usize {
    if out.is_empty() || produced_frames == 0 || out_ch == 0 {
        out_tmp.clear();
        return 0;
    }
    if out.len() == 2 && out_ch == 2 {
        let produced_frames = produced_frames.min(out[0].len()).min(out[1].len());
        let produced_samples = produced_frames * 2;
        resize_output(out_tmp, produced_samples);
        for frame in 0..produced_frames {
            let base = frame * 2;
            out_tmp[base] = sample_to_i16(out[0][frame]);
            out_tmp[base + 1] = sample_to_i16(out[1][frame]);
        }
        return produced_frames;
    }
    if out.len() == 1 && out_ch == 2 {
        let produced_frames = produced_frames.min(out[0].len());
        let produced_samples = produced_frames * 2;
        resize_output(out_tmp, produced_samples);
        for frame in 0..produced_frames {
            let sample = sample_to_i16(out[0][frame]);
            let base = frame * 2;
            out_tmp[base] = sample;
            out_tmp[base + 1] = sample;
        }
        return produced_frames;
    }
    let produced_frames = produced_frames
        .min(out[0].len())
        .min(out.iter().map(Vec::len).min().unwrap_or(0));
    let produced_samples = produced_frames.saturating_mul(out_ch);
    resize_output(out_tmp, produced_samples);
    for frame in 0..produced_frames {
        let base = frame * out_ch;
        for channel in 0..out_ch {
            let sample = out[channel % out.len()][frame];
            out_tmp[base + channel] = sample_to_i16(sample);
        }
    }
    produced_frames
}

pub fn write_channel_mapped_i16(
    input: &[i16],
    in_ch: usize,
    out_ch: usize,
    out_tmp: &mut Vec<i16>,
) -> usize {
    if input.is_empty() || in_ch == 0 || out_ch == 0 {
        out_tmp.clear();
        return 0;
    }
    let frames = input.len() / in_ch;
    let produced_samples = frames * out_ch;
    resize_output(out_tmp, produced_samples);
    if in_ch == out_ch {
        out_tmp.copy_from_slice(&input[..produced_samples]);
        return frames;
    }
    if in_ch == 1 && out_ch == 2 {
        for frame in 0..frames {
            let sample = input[frame];
            let base = frame * 2;
            out_tmp[base] = sample;
            out_tmp[base + 1] = sample;
        }
        return frames;
    }
    for frame in 0..frames {
        let in_base = frame * in_ch;
        let out_base = frame * out_ch;
        for channel in 0..out_ch {
            out_tmp[out_base + channel] = input[in_base + channel % in_ch];
        }
    }
    frames
}

#[inline(always)]
pub fn drop_front_samples(samples: &mut Vec<i16>, drop_samples: usize) {
    if drop_samples == 0 {
        return;
    }
    if drop_samples >= samples.len() {
        samples.clear();
        return;
    }
    let remaining = samples.len() - drop_samples;
    samples.copy_within(drop_samples.., 0);
    samples.truncate(remaining);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn reuse_seek_tail_for_bench(mut samples: Vec<i16>, drop_samples: usize) -> Vec<i16> {
    drop_front_samples(&mut samples, drop_samples);
    samples
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn reuse_seek_tail_legacy_for_bench(samples: Vec<i16>, drop_samples: usize) -> Vec<i16> {
    samples[drop_samples..].to_vec()
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn push_i16_interleaved_for_bench(
    accum: &mut PlanarAccum,
    interleaved: &[i16],
    channels: usize,
) {
    accum.push_i16_interleaved(interleaved, channels);
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn push_i16_interleaved_legacy_for_bench(
    accum: &mut PlanarAccum,
    interleaved: &[i16],
    channels: usize,
) {
    if interleaved.is_empty() || channels == 0 {
        return;
    }
    debug_assert_eq!(channels, accum.channels.len());
    let frames = interleaved.len() / channels;
    if frames == 0 {
        return;
    }
    for channel in &mut accum.channels {
        channel.reserve(frames);
    }
    for frame in interleaved.chunks_exact(channels) {
        for (channel, sample) in accum.channels.iter_mut().zip(frame.iter()) {
            channel.push(f32::from(*sample) / 32768.0);
        }
    }
}

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
    let frames_f = frames as f32;
    for frame in 0..frames {
        let t = frame as f32 / frames_f;
        let mut volume = (end_volume - start_volume).mul_add(t, start_volume);
        volume = volume.clamp(0.0, 1.0);
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for c in 0..channels {
            let idx = frame * channels + c;
            let scaled = f32::from(samples[idx]) * volume;
            samples[idx] = scaled.round().clamp(-32768.0, 32767.0) as i16;
        }
    }
}

#[inline]
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

#[inline(always)]
fn resize_output(out_tmp: &mut Vec<i16>, produced_samples: usize) {
    if out_tmp.len() < produced_samples {
        out_tmp.resize(produced_samples, 0);
    } else {
        out_tmp.truncate(produced_samples);
    }
}

#[inline(always)]
fn sample_to_i16(sample: f32) -> i16 {
    (sample * 32767.0).round().clamp(-32768.0, 32767.0) as i16
}

#[cfg(test)]
mod tests {
    use super::{
        PlanarAccum, apply_fade_envelope, drop_front_samples,
        push_i16_interleaved_legacy_for_bench, volume_for_frame, write_channel_mapped_i16,
        write_resampler_output,
    };

    #[test]
    fn planar_accum_keeps_channel_order() {
        let mut planar = PlanarAccum::new(2, 4);

        planar.push_i16_interleaved(&[32767, -32768, 0, 16384], 2);

        assert_eq!(planar.available_frames(), 2);
        assert!((planar.channels[0][0] - 32767.0 / 32768.0).abs() < 1e-6);
        assert_eq!(planar.channels[0][1], 0.0);
        assert_eq!(planar.channels[1][0], -1.0);
        assert_eq!(planar.channels[1][1], 0.5);
    }

    #[test]
    fn planar_accum_compacts_consumed_frames() {
        let mut planar = PlanarAccum::new(1, 4);
        planar.push_i16_interleaved(&[1; 5000], 1);

        planar.consume_frames(3000);

        assert_eq!(planar.start_frame, 0);
        assert_eq!(planar.available_frames(), 2000);
        assert_eq!(planar.channels[0].len(), 2000);
    }

    #[test]
    fn specialized_planar_accum_matches_legacy_channel_mapping() {
        for channels in 1..=3 {
            let interleaved = (0..(channels * 17 + 1))
                .map(|sample| sample as i16 * 97 - 1_000)
                .collect::<Vec<_>>();
            let mut expected = PlanarAccum::new(channels, 0);
            push_i16_interleaved_legacy_for_bench(&mut expected, &interleaved, channels);
            let mut actual = PlanarAccum::new(channels, 0);
            actual.push_i16_interleaved(&interleaved, channels);

            assert_eq!(actual.channels, expected.channels);
            assert_eq!(actual.start_frame, expected.start_frame);
        }
    }

    #[test]
    fn resampler_output_duplicates_mono_to_stereo() {
        let mut out_tmp = Vec::new();

        let frames = write_resampler_output(&[vec![0.0, 0.5]], 2, 2, &mut out_tmp);

        assert_eq!(frames, 2);
        assert_eq!(out_tmp, [0, 0, 16384, 16384]);
    }

    #[test]
    fn resampler_output_wraps_source_channels() {
        let mut out_tmp = Vec::new();

        let frames = write_resampler_output(&[vec![0.0, 1.0], vec![-1.0, 0.5]], 2, 4, &mut out_tmp);

        assert_eq!(frames, 2);
        assert_eq!(out_tmp, [0, -32767, 0, -32767, 32767, 16384, 32767, 16384]);
    }

    #[test]
    fn channel_map_duplicates_mono_to_stereo() {
        let mut out_tmp = Vec::new();

        let frames = write_channel_mapped_i16(&[1, 2, 3], 1, 2, &mut out_tmp);

        assert_eq!(frames, 3);
        assert_eq!(out_tmp, [1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn channel_map_wraps_input_channels() {
        let mut out_tmp = Vec::new();

        let frames = write_channel_mapped_i16(&[1, 2, 3, 4], 2, 4, &mut out_tmp);

        assert_eq!(frames, 2);
        assert_eq!(out_tmp, [1, 2, 1, 2, 3, 4, 3, 4]);
    }

    #[test]
    fn drop_front_samples_trims_in_place() {
        let mut samples = vec![1, 2, 3, 4, 5];

        drop_front_samples(&mut samples, 2);

        assert_eq!(samples, [3, 4, 5]);
    }

    #[test]
    fn seek_tail_reuse_matches_legacy_slices() {
        let original = (0..64).collect::<Vec<i16>>();
        for drop_samples in [0, 1, 17, 63, 64] {
            let expected = original[drop_samples..].to_vec();
            let mut actual = original.clone();

            drop_front_samples(&mut actual, drop_samples);

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn fade_out_longer_than_clip_starts_near_silent() {
        let clip_frames = 48i64;
        let fade_frames = 72_000i64;
        let start_volume = volume_for_frame(0, clip_frames - fade_frames, clip_frames);

        assert!((start_volume - (clip_frames as f32 / fade_frames as f32)).abs() < 0.00001);
    }

    #[test]
    fn fade_envelope_does_not_compress_long_fade_to_short_clip() {
        let mut samples = [30_000i16; 48];

        apply_fade_envelope(&mut samples, 1, 0, (-71_952, 48));

        assert!(samples[0].abs() <= 25);
        assert_eq!(samples[47], 0);
    }
}
