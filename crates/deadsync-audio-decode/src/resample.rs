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

    #[inline]
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
            let frames = interleaved.as_chunks::<2>().0;
            left.extend(frames.iter().map(|frame| f32::from(frame[0]) / 32768.0));
            right.extend(frames.iter().map(|frame| f32::from(frame[1]) / 32768.0));
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
        f_cutoff: Some(0.95),
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    }
}

#[inline]
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
        let (output_chunks, output_tail) = out_tmp.as_mut_slice().as_chunks_mut::<8>();
        let (left_chunks, left_tail) = out[0][..produced_frames].as_chunks::<4>();
        let (right_chunks, right_tail) = out[1][..produced_frames].as_chunks::<4>();
        for ((output, left), right) in output_chunks.iter_mut().zip(left_chunks).zip(right_chunks) {
            *output = [
                sample_to_i16(left[0]),
                sample_to_i16(right[0]),
                sample_to_i16(left[1]),
                sample_to_i16(right[1]),
                sample_to_i16(left[2]),
                sample_to_i16(right[2]),
                sample_to_i16(left[3]),
                sample_to_i16(right[3]),
            ];
        }
        for ((output, left), right) in output_tail
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(left_tail)
            .zip(right_tail)
        {
            *output = [sample_to_i16(*left), sample_to_i16(*right)];
        }
        return produced_frames;
    }
    if out.len() == 1 && out_ch == 2 {
        let produced_frames = produced_frames.min(out[0].len());
        let produced_samples = produced_frames * 2;
        resize_output(out_tmp, produced_samples);
        for (frame, sample) in out_tmp
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(&out[0][..produced_frames])
        {
            let sample = sample_to_i16(*sample);
            *frame = [sample, sample];
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

#[inline]
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
        for (frame, sample) in out_tmp
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(&input[..frames])
        {
            *frame = [*sample, *sample];
        }
        return frames;
    }
    if out_ch == 2 {
        for (output, input) in out_tmp
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(input.chunks_exact(in_ch))
        {
            *output = [input[0], input[1]];
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
    // Rust float-to-integer casts already saturate and map NaN to zero.
    (sample * 32767.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::{
        PlanarAccum, apply_fade_envelope, drop_front_samples, volume_for_frame,
        write_channel_mapped_i16, write_resampler_output,
    };

    fn push_i16_legacy(planar: &mut PlanarAccum, interleaved: &[i16], channels: usize) {
        if interleaved.is_empty() || channels == 0 {
            return;
        }
        let frames = interleaved.len() / channels;
        for channel in &mut planar.channels {
            channel.reserve(frames);
        }
        for frame in interleaved.chunks_exact(channels) {
            for (channel, sample) in planar.channels.iter_mut().zip(frame) {
                channel.push(f32::from(*sample) / 32768.0);
            }
        }
    }

    fn write_resampler_legacy(
        out: &[Vec<f32>],
        produced_frames: usize,
        out_ch: usize,
        out_tmp: &mut Vec<i16>,
    ) -> usize {
        if out.is_empty() || produced_frames == 0 || out_ch == 0 {
            out_tmp.clear();
            return 0;
        }
        let produced_frames = produced_frames
            .min(out[0].len())
            .min(out.iter().map(Vec::len).min().unwrap_or(0));
        out_tmp.resize(produced_frames.saturating_mul(out_ch), 0);
        let mut frame = 0;
        while frame < produced_frames {
            let base = frame * out_ch;
            for channel in 0..out_ch {
                let sample = out[channel % out.len()][frame];
                out_tmp[base + channel] = super::sample_to_i16(sample);
            }
            frame += 1;
        }
        produced_frames
    }

    fn write_channel_map_legacy(
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
        out_tmp.resize(frames * out_ch, 0);
        for frame in 0..frames {
            let in_base = frame * in_ch;
            let out_base = frame * out_ch;
            for channel in 0..out_ch {
                out_tmp[out_base + channel] = input[in_base + channel % in_ch];
            }
        }
        frames
    }

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
    fn specialized_planar_append_matches_generic_layouts() {
        for channels in [1usize, 2, 6] {
            let input = (0..257 * channels + channels - 1)
                .map(|index| index.wrapping_mul(25_173) as i16)
                .collect::<Vec<_>>();
            let mut expected = PlanarAccum::new(channels, 257);
            let mut actual = PlanarAccum::new(channels, 257);

            push_i16_legacy(&mut expected, &input, channels);
            actual.push_i16_interleaved(&input, channels);

            assert_eq!(actual.channels, expected.channels, "channels={channels}");
            assert_eq!(actual.available_frames(), 257);
        }
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
    fn specialized_resampler_output_matches_generic_layouts() {
        let sample_edges = vec![
            f32::NEG_INFINITY,
            -1.5,
            -1.0,
            -0.0,
            0.0,
            0.5,
            1.0,
            1.5,
            f32::INFINITY,
            f32::NAN,
        ];
        for (out, requested, out_ch) in [
            (vec![sample_edges.clone()], 14, 2),
            (vec![sample_edges.clone(), sample_edges.clone()], 14, 2),
            (
                vec![sample_edges.clone(), sample_edges[..7].to_vec()],
                14,
                4,
            ),
        ] {
            let mut expected = vec![123; 64];
            let mut actual = expected.clone();

            let expected_frames = write_resampler_legacy(&out, requested, out_ch, &mut expected);
            let actual_frames = write_resampler_output(&out, requested, out_ch, &mut actual);

            assert_eq!(actual_frames, expected_frames);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn resampler_sample_cast_matches_clamped_conversion() {
        fn legacy(sample: f32) -> i16 {
            (sample * 32767.0).round().clamp(-32768.0, 32767.0) as i16
        }

        let edges = [
            f32::NEG_INFINITY,
            -2.0,
            -1.000_000_1,
            -1.0,
            -0.5,
            -0.0,
            0.0,
            0.5,
            1.0,
            1.000_000_1,
            2.0,
            f32::INFINITY,
            f32::NAN,
        ];
        for sample in edges {
            assert_eq!(super::sample_to_i16(sample), legacy(sample));
        }
        for exponent in 0..=u8::MAX {
            for mantissa in (0..=0x7f_ffffu32).step_by(65_521) {
                for sign in [0, 1u32 << 31] {
                    let sample = f32::from_bits(sign | u32::from(exponent) << 23 | mantissa);
                    assert_eq!(
                        super::sample_to_i16(sample),
                        legacy(sample),
                        "bits={:08x}",
                        sample.to_bits()
                    );
                }
            }
        }
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
    fn specialized_channel_map_matches_generic_layouts() {
        for (in_ch, out_ch) in [(1usize, 2usize), (2, 2), (2, 4), (3, 2), (6, 2), (2, 1)] {
            let input = (0..257 * in_ch + in_ch - 1)
                .map(|index| index.wrapping_mul(25_173) as i16)
                .collect::<Vec<_>>();
            let mut expected = vec![123; 1024];
            let mut actual = expected.clone();

            let expected_frames = write_channel_map_legacy(&input, in_ch, out_ch, &mut expected);
            let actual_frames = write_channel_mapped_i16(&input, in_ch, out_ch, &mut actual);

            assert_eq!(actual_frames, expected_frames, "{in_ch} -> {out_ch}");
            assert_eq!(actual, expected, "{in_ch} -> {out_ch}");
        }
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
