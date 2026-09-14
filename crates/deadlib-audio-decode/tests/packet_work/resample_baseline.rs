// Frozen from 0.5.1218 (c5fb50443).
use rubato::{SincInterpolationParameters, SincInterpolationType, WindowFunction};

pub const OUT_FRAMES_PER_CALL: usize = 256;
pub const PLANAR_INPUT_CAP_FRAMES: usize = 4096;

const PLANAR_COMPACT_THRESHOLD_FRAMES: usize = 2048;

pub struct PlanarAccum {
    pub channels: Vec<Vec<f32>>,
    pub start_frame: usize,
}

impl PlanarAccum {
    #[must_use]
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
    #[must_use]
    pub fn available_frames(&self) -> usize {
        self.channels
            .first()
            .map_or(0, |channel| channel.len().saturating_sub(self.start_frame))
    }

    #[inline(always)]
    #[must_use]
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
#[must_use]
pub const fn resampler_params() -> SincInterpolationParameters {
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
    map_channels_i16(input, in_ch, out_ch, out_tmp);
    frames
}

/// Append channel-mapped frames directly to an existing output allocation.
///
/// This is the collection-oriented counterpart to [`write_channel_mapped_i16`]:
/// callers building a complete clip can avoid a temporary packet buffer and a
/// second copy into the final collection.
#[inline]
pub fn append_channel_mapped_i16(
    input: &[i16],
    in_ch: usize,
    out_ch: usize,
    out: &mut Vec<i16>,
) -> usize {
    if input.is_empty() || in_ch == 0 || out_ch == 0 {
        return 0;
    }
    let frames = input.len() / in_ch;
    let produced_samples = frames * out_ch;
    out.reserve(produced_samples);
    if in_ch == out_ch {
        out.extend_from_slice(&input[..produced_samples]);
        return frames;
    }
    if in_ch == 1 && out_ch == 2 {
        out.extend(input[..frames].iter().flat_map(|&sample| [sample, sample]));
        return frames;
    }
    if out_ch == 2 {
        out.extend(
            input
                .chunks_exact(in_ch)
                .flat_map(|frame| [frame[0], frame[1]]),
        );
        return frames;
    }
    for frame in input.chunks_exact(in_ch) {
        for channel in 0..out_ch {
            out.push(frame[channel % in_ch]);
        }
    }
    frames
}

#[inline]
fn map_channels_i16(input: &[i16], in_ch: usize, out_ch: usize, output: &mut [i16]) {
    if in_ch == out_ch {
        output.copy_from_slice(&input[..output.len()]);
        return;
    }
    if in_ch == 1 && out_ch == 2 {
        for (frame, sample) in output.as_chunks_mut::<2>().0.iter_mut().zip(input) {
            *frame = [*sample, *sample];
        }
        return;
    }
    if out_ch == 2 {
        for (output, input) in output
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(input.chunks_exact(in_ch))
        {
            *output = [input[0], input[1]];
        }
        return;
    }
    let frames = input.len() / in_ch;
    for frame in 0..frames {
        let in_base = frame * in_ch;
        let out_base = frame * out_ch;
        for channel in 0..out_ch {
            output[out_base + channel] = input[in_base + channel % in_ch];
        }
    }
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

#[inline]
pub(crate) fn take_cleared_i16(slot: &mut Option<Vec<i16>>) -> Vec<i16> {
    let mut buffer = slot.take().unwrap_or_default();
    buffer.clear();
    buffer
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
        let volume = (end_volume - start_volume)
            .mul_add(t, start_volume)
            .clamp(0.0, 1.0);
        if (volume - 1.0).abs() < 0.0001 {
            continue;
        }
        for channel in 0..channels {
            let index = frame * channels + channel;
            let scaled = f32::from(samples[index]) * volume;
            samples[index] = scaled.round().clamp(-32768.0, 32767.0) as i16;
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
