use crate::{
    Error, Frame, FrameHeader, FreeFormat, Limits, MpegVersion, Reservoir,
    bands::Bands,
    bits::BitReader,
    scalefactors::{self, ScaleFactors},
    spectral,
    synthesis::{Hybrid, Synthesis},
};

#[derive(Clone, Copy, Debug)]
pub struct DecodeOptions {
    pub verify_crc: bool,
    pub allow_mpeg25: bool,
    pub limits: Limits,
}
impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            verify_crc: true,
            allow_mpeg25: true,
            limits: Limits::default(),
        }
    }
}
#[derive(Clone, Copy)]
pub struct Pcm<'a> {
    data: &'a [f32],
    channels: usize,
    samples: usize,
    stride: usize,
}
impl<'a> Pcm<'a> {
    pub(crate) fn planar(data: &'a [f32], channels: usize, samples: usize) -> Self {
        debug_assert!((1..=2).contains(&channels) && data.len() == channels * samples);
        Self {
            data,
            channels,
            samples,
            stride: samples,
        }
    }
    pub fn channels(self) -> usize {
        self.channels
    }
    pub fn samples(self) -> usize {
        self.samples
    }
    pub fn plane(self, channel: usize) -> Option<&'a [f32]> {
        if channel >= self.channels {
            None
        } else {
            Some(&self.data[channel * self.stride..channel * self.stride + self.samples])
        }
    }
    pub fn planes(self) -> impl ExactSizeIterator<Item = &'a [f32]> + DoubleEndedIterator {
        (0..self.channels).map(move |channel| self.plane(channel).unwrap())
    }
    /// Borrows a sample range from every plane, preserving its physical stride.
    pub fn slice(self, range: std::ops::Range<usize>) -> Option<Self> {
        if range.start > range.end || range.end > self.samples {
            return None;
        }
        Some(Self {
            data: &self.data[range.start..(self.channels - 1) * self.stride + range.end],
            samples: range.end - range.start,
            ..self
        })
    }
}
pub struct DecodedFrame<'a> {
    pub header: FrameHeader,
    pub bytes: usize,
    pub pcm: Pcm<'a>,
}
/// Stateful raw frame decoder. PCM is planar, normalized, unclipped f32. It
/// includes algorithmic delay and metadata frames; file/gapless mapping is a
/// separate layer. Ordinary errors require reset; incomplete frame input does
/// not consume state. Missing-reservoir errors retain payload for recovery and
/// reset DSP history; subsequent decoded output needs synthesis warmup.
pub struct FrameDecoder {
    options: DecodeOptions,
    reservoir: Reservoir,
    hybrid: [Hybrid; 2],
    synthesis: [Synthesis; 2],
    quantized: [[i16; 576]; 2],
    spectra: [[f64; 576]; 2],
    subbands: [f64; 576],
    output: [f32; 2304],
    failed: bool,
    payload_consumed: bool,
}
impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}
impl FrameDecoder {
    pub fn new() -> Self {
        Self::with_options(DecodeOptions::default())
    }
    pub fn with_options(options: DecodeOptions) -> Self {
        Self {
            options,
            reservoir: Reservoir::new(),
            hybrid: [Hybrid::new(), Hybrid::new()],
            synthesis: [Synthesis::new(), Synthesis::new()],
            quantized: [[0; 576]; 2],
            spectra: [[0.0; 576]; 2],
            subbands: [0.0; 576],
            output: [0.0; 2304],
            failed: false,
            payload_consumed: false,
        }
    }
    fn reset_dsp(&mut self) {
        for h in &mut self.hybrid {
            h.reset();
        }
        for s in &mut self.synthesis {
            s.reset();
        }
    }
    pub fn reset(&mut self) {
        self.reservoir.clear();
        self.reset_dsp();
        self.failed = false;
        self.payload_consumed = false;
    }
    pub fn decode<'a>(
        &'a mut self,
        data: &[u8],
        free: Option<FreeFormat>,
    ) -> Result<DecodedFrame<'a>, Error> {
        if self.failed {
            return Err(Error::Invalid("raw decoder requires reset after error"));
        }
        match self.decode_inner(data, free) {
            Ok((header, bytes)) => Ok(DecodedFrame {
                header,
                bytes,
                pcm: self.pcm(header),
            }),
            Err(error) => {
                match error {
                    Error::MissingReservoir { .. } => self.reset_dsp(),
                    Error::Truncated | Error::Unsupported(_) => (),
                    _ => self.failed = true,
                }
                Err(error)
            }
        }
    }
    pub(crate) fn pcm(&self, header: FrameHeader) -> Pcm<'_> {
        Pcm {
            data: &self.output[..header.samples() * header.channels()],
            channels: header.channels(),
            samples: header.samples(),
            stride: header.samples(),
        }
    }
    pub(crate) fn conceal(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.reset_dsp();
        self.failed = false;
        if !self.payload_consumed {
            self.reservoir.push(0, payload, 0)?;
            self.payload_consumed = true;
        }
        self.output.fill(0.0);
        Ok(())
    }
    pub(crate) fn prime_reservoir(&mut self, payload: &[u8]) -> Result<(), Error> {
        self.reservoir.push(0, payload, 0)?;
        Ok(())
    }
    pub(crate) fn drain(&mut self, header: FrameHeader) -> Result<(), Error> {
        for channel in 0..header.channels() {
            self.spectra[channel].fill(0.0);
            self.hybrid[channel].process(&mut self.spectra[channel], 0, 0, &mut self.subbands)?;
            for t in 0..18 {
                let mut pcm = [0.0; 32];
                self.synthesis[channel].process(
                    self.subbands[t * 32..t * 32 + 32].try_into().unwrap(),
                    &mut pcm,
                );
                let start = channel * 576 + t * 32;
                for (out, sample) in self.output[start..start + 32].iter_mut().zip(pcm) {
                    *out = sample as f32;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn drained_pcm(&self, header: FrameHeader) -> Pcm<'_> {
        Pcm {
            data: &self.output[..576 * header.channels()],
            channels: header.channels(),
            samples: 576,
            stride: 576,
        }
    }
    fn decode_inner(
        &mut self,
        data: &[u8],
        free: Option<FreeFormat>,
    ) -> Result<(FrameHeader, usize), Error> {
        self.payload_consumed = false;
        let frame = Frame::parse(data, free, self.options.verify_crc, self.options.limits)?;
        self.decode_complete(&frame).map_err(|error| match error {
            Error::Truncated => Error::Invalid("truncated main data in complete frame"),
            other => other,
        })?;
        Ok((frame.header, frame.data.len()))
    }
    fn decode_complete(&mut self, frame: &Frame<'_>) -> Result<(), Error> {
        let header = frame.header;
        if !self.options.allow_mpeg25 && header.version() == MpegVersion::Mpeg25 {
            return Err(Error::Unsupported("MPEG-2.5 disabled"));
        }
        let bands = Bands::for_rate(header.sample_rate())?;
        // Missing/truncated main data still retains the physical payload.
        // Allocation/limit failures are fatal and are never concealed.
        self.payload_consumed = true;
        let main = self.reservoir.push(
            frame.side_info.main_data_begin as usize,
            frame.main_data,
            frame.side_info.main_data_bits(header),
        )?;
        let mut previous: [Option<ScaleFactors>; 2] = [None, None];
        let mut position = 0;
        for granule in 0..header.granules() {
            let mut factors = [ScaleFactors::default(), ScaleFactors::default()];
            for channel in 0..header.channels() {
                let info = &frame.side_info.granules[granule][channel];
                let mut bits = BitReader::bounded(main, position, info.part2_3_bits as usize)?;
                factors[channel] = scalefactors::read(
                    &mut bits,
                    header,
                    &frame.side_info,
                    granule,
                    channel,
                    previous[channel].as_ref(),
                )?;
                spectral::quantized(&mut bits, info, bands, &mut self.quantized[channel])?;
                spectral::requantize(
                    &self.quantized[channel],
                    info,
                    &factors[channel],
                    bands,
                    &mut self.spectra[channel],
                )?;
                position += info.part2_3_bits as usize;
            }
            if header.channels() == 2 {
                spectral::stereo(
                    header,
                    &frame.side_info.granules[granule],
                    &factors,
                    bands,
                    &mut self.spectra,
                )?;
            }
            for channel in 0..header.channels() {
                let info = &frame.side_info.granules[granule][channel];
                let mixed = if info.mixed_block {
                    bands.mixed_subbands()
                } else {
                    0
                };
                self.hybrid[channel].process(
                    &mut self.spectra[channel],
                    info.block_type,
                    mixed,
                    &mut self.subbands,
                )?;
                for t in 0..18 {
                    let mut pcm = [0.0; 32];
                    self.synthesis[channel].process(
                        self.subbands[t * 32..t * 32 + 32].try_into().unwrap(),
                        &mut pcm,
                    );
                    let start = channel * header.samples() + granule * 576 + t * 32;
                    for (target, sample) in self.output[start..start + 32].iter_mut().zip(pcm) {
                        *target = sample as f32;
                    }
                }
            }
            previous[0] = Some(std::mem::take(&mut factors[0]));
            previous[1] = Some(std::mem::take(&mut factors[1]));
        }
        if header.channels() == 1 {
            self.hybrid[1].reset();
            self.synthesis[1].reset();
        }
        Ok(())
    }
}
