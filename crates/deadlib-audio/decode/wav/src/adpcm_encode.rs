//! Native lossy Microsoft and WAV IMA ADPCM encoding. Block calls allocate nothing.
use crate::{
    Adpcm, Encoding, Endian, Error, Format, Muxer, RawChunk, WriteOptions,
    adpcm::{ADAPT, COEFFICIENTS, INDEX, INDEX3, STEP},
    mux::{put16, put32},
    reserve,
};
use std::io::{Seek, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdpcmKind {
    Microsoft,
    /// Standard 4-bit WAV IMA ADPCM.
    Ima,
    /// Standard 3-bit WAV IMA ADPCM, with 12-byte channel groups.
    Ima3,
}

#[derive(Clone, Copy, Debug)]
pub struct AdpcmSpec {
    pub kind: AdpcmKind,
    pub channels: u16,
    pub sample_rate: u32,
    pub block_bytes: u16,
}
impl AdpcmSpec {
    /// Emit a standard descriptor. Custom MS padding/coefficient tables can instead
    /// be supplied to Format/Adpcm directly and passed to Muxer unchanged.
    pub fn format(self, endian: Endian, out: &mut [u8]) -> Result<usize, Error> {
        let ima = self.kind != AdpcmKind::Microsoft;
        let bits = if self.kind == AdpcmKind::Ima3 { 3 } else { 4 };
        let n = if ima { 20 } else { 50 };
        if out.len() < n {
            return Err(Error::Limit("ADPCM format output capacity"));
        }
        let channels = u32::from(self.channels);
        let block = u32::from(self.block_bytes);
        if channels == 0 || self.sample_rate == 0 {
            return Err(Error::Invalid("ADPCM channels or sample rate"));
        }
        let header = channels * if ima { 4 } else { 7 };
        if block < header {
            return Err(Error::Invalid("ADPCM block smaller than header"));
        }
        let frames = if ima { 1 } else { 2 } + (block - header) * 8 / (channels * bits);
        let frames = u16::try_from(frames)
            .map_err(|_| Error::Invalid("ADPCM samples per block overflow"))?;
        let rate =
            u32::try_from(u64::from(self.sample_rate) * u64::from(block) / u64::from(frames))
                .map_err(|_| Error::Invalid("ADPCM byte rate overflow"))?;
        let b = &mut out[..n];
        b.fill(0);
        put16(b, if ima { 17 } else { 2 }, endian);
        put16(&mut b[2..], self.channels, endian);
        put32(&mut b[4..], self.sample_rate, endian);
        put32(&mut b[8..], rate, endian);
        put16(&mut b[12..], self.block_bytes, endian);
        put16(&mut b[14..], bits as u16, endian);
        put16(&mut b[16..], (n - 18) as u16, endian);
        put16(&mut b[18..], frames, endian);
        if self.kind == AdpcmKind::Microsoft {
            put16(&mut b[20..], 7, endian);
            for (i, &(a, d)) in COEFFICIENTS.iter().enumerate() {
                put16(&mut b[22 + i * 4..], a as u16, endian);
                put16(&mut b[24 + i * 4..], d as u16, endian);
            }
        }
        Adpcm::from_format(Format::parse(b, endian)?)?;
        Ok(n)
    }
}

impl Adpcm<'_> {
    /// Encode 1..=frames_per_block interleaved frames. Repeat the last real frame
    /// to fill the block; mux the original frame count to trim this lossy padding.
    /// Returns block bytes. Errors in arguments occur before modifying output.
    pub fn encode_block(self, samples: &[i16], out: &mut [u8]) -> Result<usize, Error> {
        if samples.is_empty()
            || !samples.len().is_multiple_of(self.channels)
            || samples.len() / self.channels > self.frames_per_block
        {
            return Err(Error::Invalid("ADPCM input frame count"));
        }
        if out.len() < self.block_bytes {
            return Err(Error::Limit("ADPCM encoded output capacity"));
        }
        let out = &mut out[..self.block_bytes];
        out.fill(0);
        let frames = samples.len() / self.channels;
        for c in 0..self.channels {
            if self.encoding == Encoding::ImaAdpcm {
                let mut predictor = i32::from(samples[c]);
                let mean = mean_difference(samples, self.channels, c);
                let mut index = STEP.partition_point(|&s| s < mean).min(88);
                put16(&mut out[c * 4..], samples[c] as u16, self.endian);
                out[c * 4 + 2] = index as u8;
                if self.bits == 3 {
                    self.encode_ima3_channel(samples, out, c, frames, predictor, index);
                    continue;
                }
                for frame in 1..self.frames_per_block {
                    let target = i32::from(samples[frame.min(frames - 1) * self.channels + c]);
                    let difference = target - predictor;
                    let mut remaining = difference.abs();
                    let mut code = if difference < 0 { 8u8 } else { 0 };
                    let step = STEP[index];
                    let mut delta = step >> 3;
                    for (mask, shift) in [(4, 0), (2, 1), (1, 2)] {
                        let part = step >> shift;
                        if remaining >= part {
                            code |= mask;
                            remaining -= part;
                            delta += part;
                        }
                    }
                    predictor = (predictor + if code & 8 == 0 { delta } else { -delta })
                        .clamp(-32768, 32767);
                    index = (index as i32 + INDEX[(code & 7) as usize]).clamp(0, 88) as usize;
                    let i = frame - 1;
                    let offset =
                        self.channels * 4 + (i / 8) * self.channels * 4 + c * 4 + (i % 8) / 2;
                    out[offset] |= code << ((i & 1) * 4);
                }
            } else {
                let two = i64::from(samples[c]);
                let one = i64::from(samples[(frames - 1).min(1) * self.channels + c]);
                let initial =
                    i64::from(mean_difference(samples, self.channels, c).clamp(16, 32767));
                let mut best = (u64::MAX, 0usize);
                // Predictor selection evaluates a short real-sample prefix. The
                // reconstruction loop then tracks its own quantization error.
                for (i, row) in self
                    .coefficients
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .take(256)
                    .enumerate()
                {
                    let co = (
                        i64::from(self.endian.u16(row) as i16),
                        i64::from(self.endian.u16(&row[2..]) as i16),
                    );
                    let (mut a, mut b, mut delta, mut score) = (one, two, initial, 0u64);
                    for frame in 2..frames.min(32) {
                        let target = i64::from(samples[frame * self.channels + c]);
                        let (_, value, next) = ms_quantize(target, a, b, co, delta);
                        let error = target - value;
                        score += (error * error) as u64;
                        b = a;
                        a = value;
                        delta = next;
                    }
                    if score < best.0 {
                        best = (score, i);
                    }
                }
                let row = &self.coefficients[best.1 * 4..best.1 * 4 + 4];
                let co = (
                    i64::from(self.endian.u16(row) as i16),
                    i64::from(self.endian.u16(&row[2..]) as i16),
                );
                out[c] = best.1 as u8;
                put16(
                    &mut out[self.channels + c * 2..],
                    initial as u16,
                    self.endian,
                );
                put16(
                    &mut out[self.channels * 3 + c * 2..],
                    one as u16,
                    self.endian,
                );
                put16(
                    &mut out[self.channels * 5 + c * 2..],
                    two as u16,
                    self.endian,
                );
                let (mut a, mut b, mut delta) = (one, two, initial);
                for frame in 2..self.frames_per_block {
                    let target = i64::from(samples[frame.min(frames - 1) * self.channels + c]);
                    let (code, value, next) = ms_quantize(target, a, b, co, delta);
                    let i = (frame - 2) * self.channels + c;
                    out[self.channels * 7 + i / 2] |= code << (4 - (i & 1) * 4);
                    b = a;
                    a = value;
                    delta = next;
                }
            }
        }
        Ok(self.block_bytes)
    }
    fn encode_ima3_channel(
        self,
        samples: &[i16],
        out: &mut [u8],
        c: usize,
        frames: usize,
        mut predictor: i32,
        mut index: usize,
    ) {
        for frame in 1..self.frames_per_block {
            let target = i32::from(samples[frame.min(frames - 1) * self.channels + c]);
            let difference = target - predictor;
            let mut remaining = difference.abs();
            let mut code = if difference < 0 { 4u8 } else { 0 };
            let step = STEP[index];
            let mut delta = step >> 2;
            for (mask, shift) in [(2, 0), (1, 1)] {
                let part = step >> shift;
                if remaining >= part {
                    code |= mask;
                    remaining -= part;
                    delta += part;
                }
            }
            predictor =
                (predictor + if code & 4 == 0 { delta } else { -delta }).clamp(-32768, 32767);
            index = (index as i32 + INDEX3[(code & 3) as usize]).clamp(0, 88) as usize;
            let bit = (frame - 1) * 3;
            let shift = bit % 8;
            out[self.ima3_byte(bit / 8, c)] |= code << shift;
            if shift > 5 {
                out[self.ima3_byte(bit / 8 + 1, c)] |= code >> (8 - shift);
            }
        }
    }
}
fn mean_difference(samples: &[i16], channels: usize, channel: usize) -> i32 {
    let frames = (samples.len() / channels).min(32);
    if frames < 2 {
        return 0;
    }
    let sum: i32 = (1..frames)
        .map(|i| {
            (i32::from(samples[i * channels + channel])
                - i32::from(samples[(i - 1) * channels + channel]))
            .abs()
        })
        .sum();
    sum / (frames - 1) as i32
}
fn ms_quantize(target: i64, one: i64, two: i64, co: (i64, i64), delta: i64) -> (u8, i64, i64) {
    let predicted = (one * co.0 + two * co.1) / 256;
    let error = target - predicted;
    let signed = (if error < 0 {
        -((-error + delta / 2) / delta)
    } else {
        (error + delta / 2) / delta
    })
    .clamp(-8, 7);
    let code = (signed & 15) as u8;
    let value = (predicted + signed * delta).clamp(-32768, 32767);
    (code, value, (delta * ADAPT[code as usize] / 256).max(16))
}

/// Incremental i16 encoder with bounded block buffers allocated once. The last
/// block is padded on finish and fact records only the real input frame count.
pub struct AdpcmWriter<W> {
    mux: Muxer<W>,
    header: [u8; 50],
    header_len: usize,
    endian: Endian,
    channels: usize,
    pending: Vec<i16>,
    encoded: Vec<u8>,
    filled: usize,
    frames: u64,
    failed: bool,
}
impl<W: Write + Seek> AdpcmWriter<W> {
    pub fn new(
        inner: W,
        spec: AdpcmSpec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
    ) -> Result<Self, Error> {
        Self::initialize(inner, spec, options.container.endian(), |inner, header| {
            Muxer::new(inner, header, options, metadata)
        })
    }
    pub fn finish(mut self) -> Result<W, Error> {
        self.finish_pending()?;
        self.mux.finish()
    }
}
impl<W: Write> AdpcmWriter<W> {
    /// Known-length ADPCM output for a nonseekable sink. Final block padding is
    /// included in encoded bytes; fact/ds64 retain exactly the declared frames.
    pub fn with_frames(
        inner: W,
        spec: AdpcmSpec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
        frames: u64,
    ) -> Result<Self, Error> {
        let endian = options.container.endian();
        Self::initialize(inner, spec, endian, |inner, header| {
            let codec = Adpcm::from_format(Format::parse(header, endian)?)?;
            let blocks = frames.div_ceil(codec.frames_per_block() as u64);
            let bytes = blocks
                .checked_mul(codec.block_bytes() as u64)
                .ok_or(Error::Limit("ADPCM data size overflow"))?;
            Muxer::with_lengths(
                inner,
                header,
                options,
                metadata,
                crate::DataLength { bytes, frames },
            )
        })
    }
    fn initialize(
        inner: W,
        spec: AdpcmSpec,
        endian: Endian,
        mux: impl FnOnce(W, &[u8]) -> Result<Muxer<W>, Error>,
    ) -> Result<Self, Error> {
        let mut header = [0; 50];
        let header_len = spec.format(endian, &mut header)?;
        let codec = Adpcm::from_format(Format::parse(&header[..header_len], endian)?)?;
        let mut pending = Vec::new();
        reserve(&mut pending, codec.scratch_samples())?;
        pending.resize(codec.scratch_samples(), 0);
        let mut encoded = Vec::new();
        reserve(&mut encoded, codec.block_bytes())?;
        encoded.resize(codec.block_bytes(), 0);
        let mux = mux(inner, &header[..header_len])?;
        Ok(Self {
            mux,
            header,
            header_len,
            endian,
            channels: usize::from(spec.channels),
            pending,
            encoded,
            filled: 0,
            frames: 0,
            failed: false,
        })
    }
    /// Successfully accepted input frames, including pending samples.
    pub fn frames_written(&self) -> u64 {
        self.frames
    }
    pub fn into_inner(self) -> W {
        self.mux.into_inner()
    }
    pub fn write_i16(&mut self, mut samples: &[i16]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("ADPCM writer unusable after I/O error"));
        }
        if !samples.len().is_multiple_of(self.channels) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let frames = self
            .frames
            .checked_add((samples.len() / self.channels) as u64)
            .ok_or(Error::Limit("sample count overflow"))?;
        if let Some(length) = self.mux.expected_length()
            && frames > length.frames
        {
            return Err(Error::Invalid("declared WAVE data length exceeded"));
        }
        let codec =
            Adpcm::from_format(Format::parse(&self.header[..self.header_len], self.endian)?)?;
        self.failed = true;
        while !samples.is_empty() {
            if self.filled == 0 && samples.len() >= self.pending.len() {
                let n = self.pending.len();
                codec.encode_block(&samples[..n], &mut self.encoded)?;
                self.mux
                    .write_data(&self.encoded, codec.frames_per_block() as u64)?;
                samples = &samples[n..];
            } else {
                let n = samples.len().min(self.pending.len() - self.filled);
                self.pending[self.filled..self.filled + n].copy_from_slice(&samples[..n]);
                self.filled += n;
                samples = &samples[n..];
                if self.filled == self.pending.len() {
                    codec.encode_block(&self.pending, &mut self.encoded)?;
                    self.mux
                        .write_data(&self.encoded, codec.frames_per_block() as u64)?;
                    self.filled = 0;
                }
            }
        }
        self.frames = frames;
        self.failed = false;
        Ok(())
    }
    fn finish_pending(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("ADPCM writer unusable after I/O error"));
        }
        if let Some(length) = self.mux.expected_length()
            && self.frames != length.frames
        {
            return Err(Error::Invalid("incomplete declared WAVE data length"));
        }
        if self.filled != 0 {
            let codec =
                Adpcm::from_format(Format::parse(&self.header[..self.header_len], self.endian)?)?;
            codec.encode_block(&self.pending[..self.filled], &mut self.encoded)?;
            self.mux
                .write_data(&self.encoded, (self.filled / self.channels) as u64)?;
            self.filled = 0;
        }
        Ok(())
    }
    pub fn finish_stream(mut self) -> Result<W, Error> {
        self.finish_pending()?;
        self.mux.finish_stream()
    }
}
