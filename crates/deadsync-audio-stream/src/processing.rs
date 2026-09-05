//! Independent stretching and sample-rate conversion on the music decoder worker.

use super::stretch::SolaStretcher;
use super::{
    OUT_FRAMES_PER_CALL, PLANAR_INPUT_CAP_FRAMES, PlanarAccum, RATE_EPS,
    RESAMPLE_MAX_RELATIVE_RATIO, compat_lead_frames, new_resampler, planar_window,
    process_resampler, trim_resampler_lead, write_resampler_output,
};
use rubato::audioadapter_buffers::direct::{SequentialSliceOfSlices, SequentialSliceOfVecs};
use rubato::{Adjustable, Async, ResampleError, Resampler, ResamplerConstructionError};

struct RateConverter {
    resampler: Async<f32>,
    input: PlanarAccum,
    padded: Vec<Vec<f32>>,
    output: Vec<Vec<f32>>,
    ratio: f64,
    lead_frames: usize,
    drained: bool,
}

impl RateConverter {
    fn new(channels: usize, ratio: f64) -> Result<Self, ResamplerConstructionError> {
        let resampler = new_resampler(ratio, RESAMPLE_MAX_RELATIVE_RATIO, channels)?;
        Ok(Self {
            padded: vec![vec![0.0; resampler.input_frames_max()]; channels],
            output: vec![vec![0.0; resampler.output_frames_max()]; channels],
            input: PlanarAccum::new(channels, PLANAR_INPUT_CAP_FRAMES),
            resampler,
            ratio,
            lead_frames: compat_lead_frames(ratio),
            drained: false,
        })
    }

    fn reset(&mut self) {
        self.resampler.reset();
        self.resampler
            .set_resample_ratio(self.ratio, false)
            .expect("stored ratio was accepted by this resampler");
        self.input.clear();
        self.lead_frames = compat_lead_frames(self.ratio);
        self.drained = false;
    }
}

/// Decoder-worker-owned stages, retained across loop iterations.
/// Scratch buffers are allocated when a stage becomes active and reused per packet.
/// Matching-rate pitch preservation owns only SOLA and one bounded output chunk;
/// it has no sinc filter, input accumulator, or resampler lead/tail padding.
pub(super) struct MusicStages {
    channels: usize,
    input_hz: u32,
    output_hz: u32,
    rate: f32,
    stretch: Option<SolaStretcher>,
    converter: Option<RateConverter>,
    stretch_output: Vec<Vec<f32>>,
    finishing: bool,
}

impl MusicStages {
    pub(super) const fn new(channels: usize, input_hz: u32, output_hz: u32) -> Self {
        Self {
            channels,
            input_hz,
            output_hz,
            rate: 1.0,
            stretch: None,
            converter: None,
            stretch_output: Vec::new(),
            finishing: false,
        }
    }

    pub(super) fn set_rate(
        &mut self,
        rate: f32,
        preserve_pitch: bool,
    ) -> Result<(), ResamplerConstructionError> {
        let changing_speed = (rate - 1.0).abs() > RATE_EPS;
        let needs_stretch = preserve_pitch && changing_speed;
        let needs_conversion =
            self.input_hz != self.output_hz || (!preserve_pitch && changing_speed);
        let stretch_changed = needs_stretch != self.stretch.is_some();
        if needs_stretch {
            let stretch = self
                .stretch
                .get_or_insert_with(|| SolaStretcher::new(self.channels, self.input_hz));
            stretch.set_speed_ratio(rate);
        } else {
            self.stretch = None;
        }
        if needs_conversion {
            let ratio = f64::from(self.output_hz)
                / f64::from(self.input_hz)
                / if needs_stretch { 1.0 } else { f64::from(rate) };
            if !stretch_changed
                && let Some(converter) = &mut self.converter
                && converter.resampler.set_resample_ratio(ratio, false).is_ok()
            {
                // Pitch-preserving rate changes leave the converter's ratio and
                // filter history intact. Switching stretch modes rebuilds the
                // sinc filter with the new mode's initial cutoff and input.
                if converter.ratio != ratio {
                    converter.resampler.reset();
                    converter
                        .resampler
                        .set_resample_ratio(ratio, false)
                        .expect("ratio was just accepted by this resampler");
                    converter.lead_frames = compat_lead_frames(ratio);
                    converter.drained = false;
                }
                converter.ratio = ratio;
            } else {
                self.converter = Some(RateConverter::new(self.channels, ratio)?);
            }
        } else {
            self.converter = None;
        }
        if needs_stretch && !needs_conversion && self.stretch_output.is_empty() {
            self.stretch_output = (0..self.channels)
                .map(|_| Vec::with_capacity(OUT_FRAMES_PER_CALL))
                .collect();
        }
        self.rate = rate;
        Ok(())
    }

    pub(super) fn reset(&mut self) {
        if let Some(stretch) = &mut self.stretch {
            stretch.reset();
        }
        if let Some(converter) = &mut self.converter {
            converter.reset();
        }
        self.finishing = false;
    }

    pub(super) const fn is_direct(&self) -> bool {
        self.stretch.is_none() && self.converter.is_none()
    }

    pub(super) fn push(&mut self, samples: &[i16]) {
        if let Some(stretch) = &mut self.stretch {
            stretch.push_interleaved_i16(samples);
        } else if let Some(converter) = &mut self.converter {
            converter.input.push_i16_interleaved(samples, self.channels);
        }
    }

    pub(super) fn finish(&mut self) {
        self.finishing = true;
        if let Some(stretch) = &mut self.stretch {
            // Every loop iteration resets the stages after draining this tail.
            stretch.finish();
        }
    }

    /// Convert one ready chunk, returning its source-time step, or None when empty.
    pub(super) fn pull(
        &mut self,
        output: &mut Vec<i16>,
        channels: usize,
    ) -> Result<Option<f64>, ResampleError> {
        let nominal_step = f64::from(self.rate) / f64::from(self.output_hz.max(1));
        let Some(converter) = &mut self.converter else {
            let Some(stretch) = &mut self.stretch else {
                return Ok(None);
            };
            for channel in &mut self.stretch_output {
                channel.clear();
            }
            let frames = stretch.pull(&mut self.stretch_output, OUT_FRAMES_PER_CALL);
            write_resampler_output(&self.stretch_output, frames, channels, output);
            return Ok((frames > 0).then_some(nominal_step));
        };
        let need = converter.resampler.input_frames_next();
        if let Some(stretch) = &mut self.stretch {
            let missing = need.saturating_sub(converter.input.available_frames());
            stretch.pull(&mut converter.input.channels, missing);
        }
        let available = converter.input.available_frames();
        if available < need && (!self.finishing || converter.drained) {
            return Ok(None);
        }
        let consumed = available.min(need);
        let frames = if consumed == need {
            let slices = planar_window(&converter.input, need);
            let input = SequentialSliceOfSlices::new(slices.as_slice(), self.channels, need)
                .expect("planar accumulator exposes every requested input frame");
            process_resampler(&mut converter.resampler, &input, &mut converter.output)?.1
        } else {
            let start = converter.input.start_frame;
            for (dst, source) in converter.padded.iter_mut().zip(&converter.input.channels) {
                dst[..need].fill(0.0);
                dst[..consumed].copy_from_slice(&source[start..start + consumed]);
            }
            let input =
                SequentialSliceOfVecs::new(converter.padded.as_slice(), self.channels, need)
                    .expect("resampler padding holds the final input chunk");
            converter.drained = consumed == 0;
            process_resampler(&mut converter.resampler, &input, &mut converter.output)?.1
        };
        converter.input.consume_frames(consumed);
        write_resampler_output(&converter.output, frames, channels, output);
        trim_resampler_lead(output, channels, &mut converter.lead_frames);
        let step = if self.stretch.is_some() || consumed == 0 || frames == 0 {
            nominal_step
        } else {
            consumed as f64 / f64::from(self.input_hz.max(1)) / frames as f64
        };
        Ok(Some(step))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, sample_rate: u32, frames: usize) -> Vec<i16> {
        (0..frames)
            .map(|i| {
                (12_000.0 * (i as f32 * hz * std::f32::consts::TAU / sample_rate as f32).sin())
                    as i16
            })
            .collect()
    }

    fn drain(stages: &mut MusicStages, output: &mut Vec<i16>) {
        let mut chunk = Vec::new();
        while let Some(step) = stages.pull(&mut chunk, 1).unwrap() {
            assert!(step.is_finite() && step > 0.0);
            output.extend_from_slice(&chunk);
        }
    }

    fn run(stages: &mut MusicStages, input: &[i16], packet_frames: usize) -> Vec<i16> {
        let mut output = Vec::new();
        for packet in input.chunks(packet_frames) {
            stages.push(packet);
            drain(stages, &mut output);
        }
        stages.finish();
        drain(stages, &mut output);
        output
    }

    fn pitch(samples: &[i16], hz: u32) -> f64 {
        let crossings: Vec<_> = samples
            .windows(2)
            .enumerate()
            .filter_map(|(i, pair)| (pair[0] <= 0 && pair[1] > 0).then_some(i))
            .collect();
        (crossings.len() - 1) as f64 * f64::from(hz)
            / (crossings.last().unwrap() - crossings[0]) as f64
    }

    #[test]
    fn stretch_keeps_onset_pitch_and_duration_without_a_converter() {
        let input = sine(440.0, 48_000, 48_000);
        for rate in [0.5, 0.8, 1.2, 2.0] {
            let mut stages = MusicStages::new(1, 48_000, 48_000);
            stages.set_rate(rate, true).unwrap();
            let output = run(&mut stages, &input, 997);
            assert!(stages.converter.is_none());
            // SOLA's first window starts at the source onset without sinc delay.
            assert_eq!(&output[..256], &input[..256]);
            let expected = input.len() as f64 / f64::from(rate);
            // SOLA's initial/final search windows bound duration rounding;
            // the bound grows in output frames when slowing playback.
            let slack = 2.0 * stages.stretch.as_ref().unwrap().window_frames() as f64
                / f64::from(rate.min(1.0));
            assert!(
                (output.len() as f64 - expected).abs() < slack,
                "rate {rate}: got {}, expected {expected}",
                output.len()
            );
            assert!((pitch(&output[4_000..output.len() - 4_000], 48_000) - 440.0).abs() < 4.0);
        }
    }

    #[test]
    fn short_stretched_stream_has_no_filtering_or_padded_tail() {
        // Above the old unity sinc cutoff: bypass intentionally preserves these
        // samples. The short source also needs SOLA's final partial-window drain.
        let input: Vec<i16> = (0..731)
            .map(|i| if i % 2 == 0 { 12_000 } else { -12_000 })
            .collect();
        let mut stages = MusicStages::new(1, 48_000, 48_000);
        stages.set_rate(1.2, true).unwrap();
        let output = run(&mut stages, &input, 137);
        assert_eq!(output, input);
        assert_eq!(stages.pull(&mut Vec::new(), 1).unwrap(), None);
    }

    #[test]
    fn real_conversion_and_pitch_changing_rates_still_resample() {
        let input = sine(440.0, 44_100, 44_100);
        for (output_hz, rate, preserve_pitch) in [
            (48_000, 1.2, true),
            (48_000, 0.5, true),
            (48_000, 1.0, true),
            (44_100, 1.2, false),
            (48_000, 1.2, false),
        ] {
            let mut stages = MusicStages::new(1, 44_100, output_hz);
            stages.set_rate(rate, preserve_pitch).unwrap();
            assert!(stages.converter.is_some());
            let output = run(&mut stages, &input, 10_007);
            let expected_len = f64::from(output_hz) / f64::from(rate);
            assert!((output.len() as f64 - expected_len).abs() < 2_880.0);
            let expected_pitch = 440.0 * if preserve_pitch { 1.0 } else { f64::from(rate) };
            assert!(
                (pitch(&output[4_000..output.len() - 4_000], output_hz) - expected_pitch).abs()
                    < 4.0
            );
        }
    }

    #[test]
    fn live_stretch_changes_keep_buffered_audio_and_rate_timestamps() {
        let input = sine(440.0, 48_000, 24_000);
        let mut stages = MusicStages::new(1, 48_000, 48_000);
        let mut reference = SolaStretcher::new(1, 48_000);
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        for (packet, rate) in input.chunks(6_000).zip([1.2, 0.8, 1.5, 0.5]) {
            stages.set_rate(rate, true).unwrap();
            reference.set_speed_ratio(rate);
            stages.push(packet);
            reference.push_interleaved_i16(packet);
            loop {
                let step = stages.pull(&mut actual, 1).unwrap();
                let mut planar = vec![Vec::new()];
                let frames = reference.pull(&mut planar, OUT_FRAMES_PER_CALL);
                write_resampler_output(&planar, frames, 1, &mut expected);
                assert_eq!(actual, expected);
                if frames == 0 {
                    assert_eq!(step, None);
                    break;
                }
                assert_eq!(step, Some(f64::from(rate) / 48_000.0));
            }
        }
    }

    #[test]
    fn stage_switches_and_loop_resets_keep_current_ratio() {
        let input = sine(440.0, 48_000, 24_000);
        let mut stages = MusicStages::new(1, 48_000, 48_000);
        for (rate, preserve_pitch) in [(1.2, true), (1.2, false), (0.8, false), (0.8, true)] {
            stages.set_rate(rate, preserve_pitch).unwrap();
            stages.reset();
            let first = run(&mut stages, &input, 997);
            stages.reset();
            let second = run(&mut stages, &input, 997);
            assert_eq!(first, second);
            let expected_pitch = 440.0 * if preserve_pitch { 1.0 } else { f64::from(rate) };
            assert!(
                (pitch(&first[4_000..first.len() - 4_000], 48_000) - expected_pitch).abs() < 4.0
            );
        }
        stages.set_rate(1.0, true).unwrap();
        assert!(stages.is_direct());
        stages.set_rate(1.0001, false).unwrap();
        assert!(stages.is_direct());
        stages.set_rate(1.2, true).unwrap();
        stages.reset();
        let replay = run(&mut stages, &input, 997);
        assert_eq!(&replay[..256], &input[..256]);
    }

    #[test]
    #[ignore = "manual release benchmark of the decoder DSP stages"]
    fn music_stages_benchmark() {
        use std::hint::black_box;
        use std::time::Instant;

        let mono = sine(440.0, 48_000, 96_000);
        let input: Vec<i16> = mono.iter().flat_map(|&sample| [sample, -sample]).collect();
        for rate in [0.8, 1.2, 1.5] {
            let mut stages = MusicStages::new(2, 48_000, 48_000);
            stages.set_rate(rate, true).unwrap();
            let mut output = Vec::with_capacity(OUT_FRAMES_PER_CALL * 2);
            let mut times = Vec::with_capacity(40);
            for iteration in 0..41 {
                stages.reset();
                let started = Instant::now();
                for packet in input.chunks(4096 * 2) {
                    stages.push(black_box(packet));
                    while stages.pull(&mut output, 2).unwrap().is_some() {
                        black_box(&output);
                    }
                }
                stages.finish();
                while stages.pull(&mut output, 2).unwrap().is_some() {
                    black_box(&output);
                }
                if iteration > 0 {
                    times.push(started.elapsed().as_secs_f64() * 1_000.0);
                }
            }
            times.sort_by(f64::total_cmp);
            eprintln!(
                "rate={rate} converter={} median_ms={:.3} p95_ms={:.3}",
                stages.converter.is_some(),
                times[20],
                times[38]
            );
        }
    }
}
