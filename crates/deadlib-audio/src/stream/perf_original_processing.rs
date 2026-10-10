// Frozen from 0530013f08d5b481d29533127de2dc41033f63be; test-only behavioral and benchmark oracle.
//! Independent stretching and sample-rate conversion on the music decoder worker.

use super::stretch::SolaStretcher;
use super::{
    OUT_FRAMES_PER_CALL, PLANAR_INPUT_CAP_FRAMES, PlanarAccum, RATE_EPS,
    RESAMPLE_MAX_RELATIVE_RATIO, compat_lead_frames, new_resampler, process_resampler,
    trim_resampler_lead, write_resampler_output,
};
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
            process_resampler(
                &mut converter.resampler,
                &converter.input.channels,
                converter.input.start_frame,
                &mut converter.output,
            )?
            .1
        } else {
            let start = converter.input.start_frame;
            for (dst, source) in converter.padded.iter_mut().zip(&converter.input.channels) {
                dst[consumed..need].fill(0.0);
                dst[..consumed].copy_from_slice(&source[start..start + consumed]);
            }
            converter.drained = consumed == 0;
            process_resampler(
                &mut converter.resampler,
                &converter.padded,
                0,
                &mut converter.output,
            )?
            .1
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
