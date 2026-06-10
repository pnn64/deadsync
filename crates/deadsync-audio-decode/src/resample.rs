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
    use super::{PlanarAccum, write_resampler_output};

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
}
