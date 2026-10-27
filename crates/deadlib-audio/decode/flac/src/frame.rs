//! Container-independent frame decoding, including 33-bit stereo side signals.
use crate::{
    Error, StreamInfo,
    bitstream::BitReader,
    checksum::{crc8, crc16},
    reserve,
    sample::Sample,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameHeader {
    pub variable_block_size: bool,
    /// Sample number for variable blocks, frame number for fixed blocks.
    pub number: u64,
    pub block_size: usize,
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub channel_assignment: u8,
    pub header_bytes: usize,
}
impl FrameHeader {
    pub fn parse(data: &[u8], info: Option<&StreamInfo>) -> Result<Self, Error> {
        let mut r = BitReader::new(data);
        if r.read(15)? != 0x7ffc {
            return Err(Error::Invalid("frame sync"));
        }
        let variable_block_size = r.read(1)? != 0;
        let block_code = r.read(4)? as u8;
        let rate_code = r.read(4)? as usize;
        let channel_assignment = r.read(4)? as u8;
        let channels = match channel_assignment {
            0..=7 => channel_assignment + 1,
            8..=10 => 2,
            _ => return Err(Error::Invalid("channel assignment")),
        };
        let bits_per_sample = match r.read(3)? {
            0 => {
                info.ok_or(Error::Invalid("bit depth requires STREAMINFO"))?
                    .bits_per_sample
            }
            1 => 8,
            2 => 12,
            4 => 16,
            5 => 20,
            6 => 24,
            7 => 32,
            _ => return Err(Error::Invalid("reserved bit depth")),
        };
        if !(4..=32).contains(&bits_per_sample) {
            return Err(Error::Invalid("bit depth"));
        }
        if r.read(1)? != 0 {
            return Err(Error::Invalid("frame reserved bit"));
        }
        let number = read_number(&mut r)?;
        if !variable_block_size && number >= 1 << 31 {
            return Err(Error::Invalid("frame number exceeds 31 bits"));
        }
        let block_size = match block_code {
            0 => return Err(Error::Invalid("reserved block size")),
            1 => 192,
            2..=5 => 144 << block_code,
            6 => r.read(8)? as usize + 1,
            7 => r.read(16)? as usize + 1,
            _ => 1 << block_code,
        };
        if block_size > 65535 {
            return Err(Error::Invalid("forbidden block size"));
        }
        const RATES: [u32; 12] = [
            0, 88200, 176400, 192000, 8000, 16000, 22050, 24000, 32000, 44100, 48000, 96000,
        ];
        let sample_rate = match rate_code {
            0 => {
                info.ok_or(Error::Invalid("sample rate requires STREAMINFO"))?
                    .sample_rate
            }
            1..=11 => RATES[rate_code],
            12 => r.read(8)? as u32 * 1000,
            13 => r.read(16)? as u32,
            14 => r.read(16)? as u32 * 10,
            _ => return Err(Error::Invalid("forbidden sample rate code")),
        };
        let crc_pos = r.position() / 8;
        if r.read(8)? as u8 != crc8(&data[..crc_pos]) {
            return Err(Error::Checksum("header CRC-8"));
        }
        Ok(Self {
            variable_block_size,
            number,
            block_size,
            sample_rate,
            channels,
            bits_per_sample,
            channel_assignment,
            header_bytes: crc_pos + 1,
        })
    }
}

fn read_number(r: &mut BitReader<'_>) -> Result<u64, Error> {
    let first = r.read(8)? as u8;
    if first < 128 {
        return Ok(first as u64);
    }
    let count = first.leading_ones();
    if !(2..=7).contains(&count) {
        return Err(Error::Invalid("coded number prefix"));
    }
    let mut result = u64::from(first & (0x7f >> count));
    for _ in 1..count {
        let next = r.read(8)?;
        if next & 0xc0 != 0x80 {
            return Err(Error::Invalid("coded number continuation"));
        }
        result = (result << 6) | (next & 63);
    }
    let minimum = [0, 0, 0x80, 0x800, 0x10000, 0x200000, 0x4000000, 0x80000000][count as usize];
    if result < minimum {
        return Err(Error::Invalid("overlong coded number"));
    }
    Ok(result)
}

/// Signed, right-aligned, exact PCM in FLAC channel order, with no float conversion.
#[derive(Clone, Copy, Debug)]
pub struct Pcm<'a> {
    data: &'a [i32],
    stride: usize,
    offset: usize,
    samples: usize,
    channels: u8,
}
impl<'a> Pcm<'a> {
    pub fn samples(&self) -> usize {
        self.samples
    }
    pub fn channels(&self) -> u8 {
        self.channels
    }
    pub fn plane(&self, channel: usize) -> &'a [i32] {
        assert!(channel < self.channels as usize);
        let start = channel * self.stride + self.offset;
        &self.data[start..start + self.samples]
    }
    pub fn planes(&self) -> impl ExactSizeIterator<Item = &'a [i32]> + '_ {
        (0..self.channels as usize).map(|i| self.plane(i))
    }
    pub fn trim_start(self, n: usize) -> Self {
        assert!(n <= self.samples);
        Self {
            offset: self.offset + n,
            samples: self.samples - n,
            ..self
        }
    }
    /// Feed little-endian, sign-extended interleaved PCM to the stream digest.
    pub fn update_md5(&self, digest: &mut crate::checksum::Md5, bits: u8) {
        crate::pcm_md5::update(self, digest, bits);
    }
}

#[derive(Debug)]
pub struct Frame<'a> {
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
    pub bytes: usize,
}

/// Reuses PCM storage across frames. Only 33-bit side samples need wide scratch.
/// Errors leave it reusable.
#[derive(Default)]
pub struct FrameDecoder {
    info: Option<StreamInfo>,
    side: Vec<i64>,
    output: Vec<i32>,
    partial: Option<PartialFrame>,
}
#[derive(Clone, Copy)]
struct PartialFrame {
    header: FrameHeader,
    channel: usize,
    position: usize,
    plane: Option<PartialPlane>,
}
#[derive(Clone, Copy)]
struct PartialPlane {
    kind: usize,
    order: usize,
    width: u32,
    wasted: u32,
    coeff: [i64; 32],
    shift: u32,
    residual: PartialResidual,
    position: usize,
}
#[derive(Clone, Copy)]
struct PartialResidual {
    param_bits: u32,
    partitions: usize,
    partition_size: usize,
    partition: usize,
    next: usize,
    code: Option<ResidualCode>,
}
#[derive(Clone, Copy)]
enum ResidualCode {
    Rice(u32),
    Escape(u32),
}
impl FrameDecoder {
    /// Change header defaults while retaining allocated PCM and predictor buffers.
    pub fn set_stream_info(&mut self, info: Option<StreamInfo>) -> Result<(), Error> {
        if let Some(info) = info {
            info.validate()?;
        }
        self.info = info;
        self.partial = None;
        Ok(())
    }
    /// Find the first complete CRC-valid frame in a bounded input slice. Useful
    /// for joining headerless multicast streams. Returns the skipped byte count.
    /// Input is never silently skipped by `decode`, which requires exact framing.
    pub fn find_frame(&mut self, data: &[u8]) -> Result<(usize, Frame<'_>), Error> {
        let mut last_error = Error::Truncated;
        for offset in 0..data.len().saturating_sub(1) {
            if data[offset] != 0xff || data[offset + 1] & 0xfe != 0xf8 {
                continue;
            }
            match self.decode(&data[offset..]) {
                Ok(frame) => {
                    let header = frame.header;
                    let bytes = frame.bytes;
                    return Ok((
                        offset,
                        Frame {
                            header,
                            bytes,
                            pcm: self.pcm(header),
                        },
                    ));
                }
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
    pub(crate) fn pcm(&self, header: FrameHeader) -> Pcm<'_> {
        Pcm {
            data: &self.output,
            stride: header.block_size,
            offset: 0,
            samples: header.block_size,
            channels: header.channels,
        }
    }
    pub fn new(info: Option<StreamInfo>) -> Result<Self, Error> {
        if let Some(info) = info {
            info.validate()?;
        }
        Ok(Self {
            info,
            ..Self::default()
        })
    }
    pub fn decode(&mut self, data: &[u8]) -> Result<Frame<'_>, Error> {
        self.partial = None;
        self.decode_inner::<false>(data)
    }
    pub(crate) fn clear_partial(&mut self) {
        self.partial = None;
    }
    /// The reader appends to the same frame prefix until decoding succeeds.
    /// Retain completed planes and residual samples within incomplete planes.
    pub(crate) fn decode_buffer(&mut self, data: &[u8]) -> Result<Frame<'_>, Error> {
        // STREAMINFO is only a performance hint. A small or incorrect maximum
        // can trigger an incomplete optimistic attempt; resume normally then.
        // Checksums and actual input bounds always decide frame validity.
        if self.partial.is_none()
            && self
                .info
                .is_some_and(|i| i.max_frame_size != 0 && data.len() >= i.max_frame_size as usize)
        {
            match self.decode_inner::<false>(data) {
                Ok(frame) => {
                    let header = frame.header;
                    let bytes = frame.bytes;
                    return Ok(Frame {
                        header,
                        bytes,
                        pcm: self.pcm(header),
                    });
                }
                Err(Error::Truncated) => (),
                Err(error) => return Err(error),
            }
        }
        self.decode_inner::<true>(data)
    }
    fn decode_inner<const RESUME: bool>(&mut self, data: &[u8]) -> Result<Frame<'_>, Error> {
        let partial = if RESUME && self.partial.is_some() {
            self.partial.take()
        } else {
            None
        };
        let header = FrameHeader::parse(data, self.info.as_ref())?;
        let n = header.block_size;
        let len = n * header.channels as usize;
        if len > self.output.len() {
            let additional = len - self.output.len();
            reserve(&mut self.output, additional)?;
        }
        self.output.resize(len, 0);
        let wide_side = header.bits_per_sample == 32 && header.channel_assignment >= 8;
        if wide_side {
            if n > self.side.len() {
                let additional = n - self.side.len();
                reserve(&mut self.side, additional)?;
            }
            self.side.resize(n, 0);
        }
        let partial = partial.filter(|p| p.header == header);
        let first_channel = partial.map_or(0, |p| p.channel);
        let mut r = BitReader::at_position(
            &data[header.header_bytes..],
            partial.map_or(0, |p| p.position),
        )?;
        for channel in first_channel..header.channels as usize {
            let side = (header.channel_assignment == 9 && channel == 0)
                || (matches!(header.channel_assignment, 8 | 10) && channel == 1);
            let position = r.position();
            let mut plane = if RESUME && channel == first_channel {
                partial.and_then(|p| p.plane)
            } else {
                None
            };
            let bits = u32::from(header.bits_per_sample) + u32::from(side);
            let result = if wide_side && side {
                if RESUME {
                    subframe_buffer(&mut r, &mut self.side, bits, &mut plane)
                } else {
                    subframe(&mut r, &mut self.side, bits)
                }
            } else {
                let output = &mut self.output[channel * n..(channel + 1) * n];
                if RESUME {
                    subframe_buffer(&mut r, output, bits, &mut plane)
                } else {
                    subframe(&mut r, output, bits)
                }
            };
            if let Err(error) = result {
                if RESUME && error == Error::Truncated {
                    self.partial = Some(PartialFrame {
                        header,
                        channel,
                        position: plane.map_or(position, |p| p.position),
                        plane,
                    });
                }
                return Err(error);
            }
        }
        let position = r.position();
        let crc_pos = match footer(&mut r, data, header.header_bytes) {
            Ok(position) => position,
            Err(error) => {
                if RESUME && error == Error::Truncated {
                    self.partial = Some(PartialFrame {
                        header,
                        channel: header.channels as usize,
                        position,
                        plane: None,
                    });
                }
                return Err(error);
            }
        };
        if header.channel_assignment >= 8 {
            if wide_side {
                stereo::<true>(&mut self.output, &self.side, header)?;
            } else {
                stereo::<false>(&mut self.output, &[], header)?;
            }
        }
        Ok(Frame {
            header,
            pcm: Pcm {
                data: &self.output,
                stride: n,
                offset: 0,
                samples: n,
                channels: header.channels,
            },
            bytes: crc_pos + 2,
        })
    }
}

fn stereo<const WIDE: bool>(
    output: &mut [i32],
    side: &[i64],
    header: FrameHeader,
) -> Result<(), Error> {
    let (left, right) = output.split_at_mut(header.block_size);
    let bits = u32::from(header.bits_per_sample);
    match header.channel_assignment {
        8 => {
            for (i, (l, r)) in left.iter().zip(right).enumerate() {
                let s = if WIDE { side[i] } else { i64::from(*r) };
                let value = i64::from(*l) - s;
                check_sample(value, bits)?;
                *r = value as i32;
            }
        }
        9 => {
            for (i, (l, r)) in left.iter_mut().zip(right).enumerate() {
                let s = if WIDE { side[i] } else { i64::from(*l) };
                let value = s + i64::from(*r);
                check_sample(value, bits)?;
                *l = value as i32;
            }
        }
        10 => {
            for (i, (l, r)) in left.iter_mut().zip(right).enumerate() {
                let s = if WIDE { side[i] } else { i64::from(*r) };
                let mid = (i64::from(*l) << 1) | (s & 1);
                let a = (mid + s) >> 1;
                let b = (mid - s) >> 1;
                check_sample(a, bits)?;
                check_sample(b, bits)?;
                *l = a as i32;
                *r = b as i32;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn footer(r: &mut BitReader<'_>, data: &[u8], header_bytes: usize) -> Result<usize, Error> {
    r.align_zero()?;
    let crc_pos = header_bytes + r.position() / 8;
    if r.read(16)? as u16 != crc16(&data[..crc_pos]) {
        return Err(Error::Checksum("frame CRC-16"));
    }
    Ok(crc_pos)
}

#[inline]
pub(crate) fn check_sample(sample: i64, bits: u32) -> Result<(), Error> {
    let bound = 1u64 << (bits - 1);
    if (sample as u64).wrapping_add(bound) >= (1u64 << bits) {
        Err(Error::Invalid("sample outside declared bit depth"))
    } else {
        Ok(())
    }
}

// The raw path below stays independent of checkpoint bookkeeping. Streaming
// reconstructs each predictor only after its complete residual has arrived.
fn subframe_buffer<T: Sample>(
    r: &mut BitReader<'_>,
    output: &mut [T],
    bits: u32,
    checkpoint: &mut Option<PartialPlane>,
) -> Result<(), Error> {
    debug_assert!(bits <= T::BITS);
    let mut state = if let Some(state) = checkpoint.take() {
        state
    } else {
        if r.read(1)? != 0 {
            return Err(Error::Invalid("subframe reserved bit"));
        }
        let kind = r.read(6)? as usize;
        let wasted = if r.read(1)? != 0 {
            r.unary(u64::from(bits - 2))? as u32 + 1
        } else {
            0
        };
        let width = bits - wasted;
        if kind == 0 {
            output.fill(T::store(r.signed(width)? << wasted));
            return Ok(());
        }
        let mut state = PartialPlane {
            kind,
            order: 0,
            width,
            wasted,
            coeff: [0; 32],
            shift: 0,
            residual: PartialResidual {
                param_bits: 0,
                partitions: 1,
                partition_size: output.len(),
                partition: 0,
                next: 0,
                code: Some(ResidualCode::Escape(width)),
            },
            position: 0,
        };
        match kind {
            1 => (),
            8..=12 | 32..=63 => {
                state.order = if kind < 32 { kind - 8 } else { kind - 31 };
                if state.order >= output.len() {
                    return Err(Error::Invalid("predictor order exceeds block"));
                }
                for sample in &mut output[..state.order] {
                    *sample = T::store(r.signed(width)?);
                }
                if kind >= 32 {
                    let precision = r.read(4)? as u32 + 1;
                    if precision == 16 {
                        return Err(Error::Invalid("forbidden LPC precision"));
                    }
                    let shift = r.signed(5)?;
                    if shift < 0 {
                        return Err(Error::Invalid("negative LPC shift"));
                    }
                    state.shift = shift as u32;
                    for c in &mut state.coeff[..state.order] {
                        *c = r.signed(precision)?;
                    }
                }
                let method = r.read(2)?;
                if method > 1 {
                    return Err(Error::Invalid("reserved residual method"));
                }
                let partitions = 1usize << r.read(4)?;
                let partition_size = output.len() / partitions;
                if !output.len().is_multiple_of(partitions) || partition_size <= state.order {
                    return Err(Error::Invalid("residual partition size"));
                }
                state.residual = PartialResidual {
                    param_bits: method as u32 + 4,
                    partitions,
                    partition_size,
                    partition: 0,
                    next: state.order,
                    code: None,
                };
            }
            _ => return Err(Error::Invalid("reserved subframe type")),
        }
        state.position = r.position();
        state
    };
    if let Err(error) = residual_buffer(r, output, &mut state) {
        if error == Error::Truncated {
            *checkpoint = Some(state);
        }
        return Err(error);
    }
    if state.kind >= 32 {
        crate::predictor::restore(
            output,
            &state.coeff[..state.order],
            state.shift,
            state.width,
        )?;
    } else if state.kind >= 8 {
        restore_fixed(output, state.order, state.width)?;
    }
    if state.wasted != 0 {
        for sample in output {
            *sample = T::store(sample.wide() << state.wasted);
        }
    }
    Ok(())
}

fn residual_buffer<T: Sample>(
    r: &mut BitReader<'_>,
    output: &mut [T],
    state: &mut PartialPlane,
) -> Result<(), Error> {
    let part = &mut state.residual;
    while part.partition < part.partitions {
        let end = (part.partition + 1) * part.partition_size;
        let code = if let Some(code) = part.code {
            code
        } else {
            // Until both fields arrive, retain the start of this header.
            let k = r.read(part.param_bits)? as u32;
            let code = if k == (1 << part.param_bits) - 1 {
                ResidualCode::Escape(r.read(5)? as u32)
            } else {
                ResidualCode::Rice(k)
            };
            part.code = Some(code);
            state.position = r.position();
            code
        };
        match code {
            ResidualCode::Rice(k) => {
                if let Err(progress) = r.rice_resume(&mut output[part.next..end], k) {
                    if progress.error == Error::Truncated {
                        part.next += progress.samples;
                        state.position = progress.position;
                    }
                    return Err(progress.error);
                }
            }
            ResidualCode::Escape(width) => {
                if width == 0 {
                    output[part.next..end].fill(T::store(0));
                } else {
                    let count = (end - part.next).min(r.remaining() / width as usize);
                    verbatim(r, &mut output[part.next..part.next + count], width)?;
                    part.next += count;
                    state.position = r.position();
                    if part.next != end {
                        return Err(Error::Truncated);
                    }
                }
            }
        }
        part.next = end;
        part.partition += 1;
        part.code = None;
        state.position = r.position();
    }
    Ok(())
}

fn subframe<T: Sample>(r: &mut BitReader<'_>, output: &mut [T], bits: u32) -> Result<(), Error> {
    debug_assert!(bits <= T::BITS);
    if r.read(1)? != 0 {
        return Err(Error::Invalid("subframe reserved bit"));
    }
    let kind = r.read(6)? as usize;
    let wasted = if r.read(1)? != 0 {
        r.unary(u64::from(bits - 2))? as u32 + 1
    } else {
        0
    };
    let width = bits - wasted;
    match kind {
        0 => output.fill(T::store(r.signed(width)?)),
        1 => verbatim(r, output, width)?,
        8..=12 | 32..=63 => {
            let order = if kind < 32 { kind - 8 } else { kind - 31 };
            if order >= output.len() {
                return Err(Error::Invalid("predictor order exceeds block"));
            }
            for sample in &mut output[..order] {
                *sample = T::store(r.signed(width)?);
            }
            let mut coeff = [0i64; 32];
            let shift;
            if kind >= 32 {
                let precision = r.read(4)? as u32 + 1;
                if precision == 16 {
                    return Err(Error::Invalid("forbidden LPC precision"));
                }
                shift = r.signed(5)?;
                if shift < 0 {
                    return Err(Error::Invalid("negative LPC shift"));
                }
                for value in &mut coeff[..order] {
                    *value = r.signed(precision)?;
                }
            } else {
                shift = 0;
            }
            residual(r, output, order)?;
            // Each previous sample was range checked. With <=33-bit samples,
            // <=15-bit coefficients and <=32 taps, this sum fits in i64.
            if kind < 32 {
                restore_fixed(output, order, width)?;
            } else {
                crate::predictor::restore(output, &coeff[..order], shift as u32, width)?;
            }
        }
        _ => return Err(Error::Invalid("reserved subframe type")),
    }
    if wasted != 0 {
        for sample in output {
            *sample = T::store(sample.wide() << wasted);
        }
    }
    Ok(())
}

// Keep the format-specific copy loops out of the predictor's hot code.
#[inline(never)]
fn verbatim<T: Sample>(r: &mut BitReader<'_>, output: &mut [T], width: u32) -> Result<(), Error> {
    if r.position().is_multiple_of(8) && matches!(width, 8 | 16 | 24 | 32) {
        let bytes = r.aligned_bytes(output.len() * (width / 8) as usize)?;
        match width {
            8 => {
                for (sample, &byte) in output.iter_mut().zip(bytes) {
                    *sample = T::store(i64::from(byte as i8));
                }
            }
            16 => {
                for (sample, &bytes) in output.iter_mut().zip(bytes.as_chunks::<2>().0) {
                    *sample = T::store(i64::from(i16::from_be_bytes(bytes)));
                }
            }
            24 => {
                for (sample, &bytes) in output.iter_mut().zip(bytes.as_chunks::<3>().0) {
                    *sample = T::store(i64::from(
                        i32::from_be_bytes([bytes[0], bytes[1], bytes[2], 0]) >> 8,
                    ));
                }
            }
            32 => {
                for (sample, &bytes) in output.iter_mut().zip(bytes.as_chunks::<4>().0) {
                    *sample = T::store(i64::from(i32::from_be_bytes(bytes)));
                }
            }
            _ => unreachable!(),
        }
    } else {
        for sample in output.iter_mut() {
            *sample = T::store(r.signed(width)?);
        }
    }
    Ok(())
}

fn restore_fixed<T: Sample>(output: &mut [T], order: usize, width: u32) -> Result<(), Error> {
    match order {
        0 => {
            for &sample in output.iter() {
                check_sample(sample.wide(), width)?;
            }
        }
        1 => {
            let mut previous = output[0].wide();
            for sample in &mut output[1..] {
                let value = sample.wide() + previous;
                check_sample(value, width)?;
                *sample = T::store(value);
                previous = value;
            }
        }
        2 => {
            let (mut a, mut b) = (output[1].wide(), output[0].wide());
            for sample in &mut output[2..] {
                let value = sample.wide() + 2 * a - b;
                check_sample(value, width)?;
                *sample = T::store(value);
                (a, b) = (value, a);
            }
        }
        3 => {
            let (mut a, mut b, mut c) = (output[2].wide(), output[1].wide(), output[0].wide());
            for sample in &mut output[3..] {
                let value = sample.wide() + 3 * a - 3 * b + c;
                check_sample(value, width)?;
                *sample = T::store(value);
                (a, b, c) = (value, a, b);
            }
        }
        4 => {
            let (mut a, mut b, mut c, mut d) = (
                output[3].wide(),
                output[2].wide(),
                output[1].wide(),
                output[0].wide(),
            );
            for sample in &mut output[4..] {
                let value = sample.wide() + 4 * a - 6 * b + 4 * c - d;
                check_sample(value, width)?;
                *sample = T::store(value);
                (a, b, c, d) = (value, a, b, c);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn residual<T: Sample>(r: &mut BitReader<'_>, output: &mut [T], order: usize) -> Result<(), Error> {
    let method = r.read(2)?;
    if method > 1 {
        return Err(Error::Invalid("reserved residual method"));
    }
    let param_bits = method as u32 + 4;
    let partition_order = r.read(4)? as usize;
    let partitions = 1usize << partition_order;
    let partition_size = output.len() >> partition_order;
    if !output.len().is_multiple_of(partitions) || partition_size <= order {
        return Err(Error::Invalid("residual partition size"));
    }
    for partition in 0..partitions {
        let k = r.read(param_bits)? as u32;
        let start = if partition == 0 {
            order
        } else {
            partition * partition_size
        };
        let part = &mut output[start..(partition + 1) * partition_size];
        if k == (1 << param_bits) - 1 {
            let width = r.read(5)? as u32;
            for sample in part {
                *sample = T::store(r.signed(width)?);
            }
        } else {
            r.rice(part, k)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_md5_covers_every_width_channel_count_batch_tail_and_trim() {
        use crate::checksum::Md5;
        for bits in 4u8..=32 {
            for channels in 1..=8 {
                for samples in [0, 1, 127, 257, 4097] {
                    let stride = samples + 7;
                    let data: Vec<i32> = (0..stride * channels)
                        .map(|i| {
                            let value = (i as i32).wrapping_mul(123456789).wrapping_add(i32::MIN);
                            value >> (32 - bits)
                        })
                        .collect();
                    let pcm = Pcm {
                        data: &data,
                        stride,
                        offset: 3,
                        samples,
                        channels: channels as u8,
                    };
                    for skip in [0, samples / 3] {
                        let pcm = pcm.trim_start(skip);
                        let mut bytes = Vec::new();
                        for sample in 0..pcm.samples() {
                            for channel in 0..channels {
                                bytes.extend_from_slice(
                                    &pcm.plane(channel)[sample].to_le_bytes()
                                        [..usize::from(bits.div_ceil(8))],
                                );
                            }
                        }
                        let mut expected = Md5::new();
                        expected.update(&bytes);
                        let mut actual = Md5::new();
                        pcm.update_md5(&mut actual, bits);
                        assert_eq!(
                            actual.finalize(),
                            expected.finalize(),
                            "bits={bits}, channels={channels}, samples={samples}, skip={skip}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn resumable_planes_match_full_decode_at_every_prefix_and_reset() {
        use crate::{EncodeOptions, FrameEncoder};
        for bits in [4, 12, 17, 24, 32] {
            for channels in [1, 2, 8] {
                let options = EncodeOptions {
                    bits_per_sample: bits,
                    channels,
                    block_size: 96,
                    ..Default::default()
                };
                let planes: Vec<Vec<i32>> = (0..channels)
                    .map(|ch| {
                        (0..96)
                            .map(|i| {
                                if bits == 32 && channels == 2 {
                                    if (i + ch as usize).is_multiple_of(2) {
                                        i32::MIN
                                    } else {
                                        i32::MAX
                                    }
                                } else {
                                    let value = match ch % 4 {
                                        0 => 0x12340000,
                                        1 => (i as i32 - 48).wrapping_mul(128123),
                                        2 => ((i as f64 * 0.27).sin() * 1e9) as i32,
                                        _ => (i as i32).wrapping_mul(123456789),
                                    };
                                    value >> (32 - bits)
                                }
                            })
                            .collect()
                    })
                    .collect();
                let refs: Vec<_> = planes.iter().map(Vec::as_slice).collect();
                let mut encoder = FrameEncoder::new(options).unwrap();
                let data = encoder.encode(&refs, 0).unwrap().to_vec();
                let mut decoder = FrameDecoder::new(Some(options.stream_info().unwrap())).unwrap();
                let mut resumed_plane = false;
                for end in 0..data.len() {
                    assert!(
                        matches!(decoder.decode_buffer(&data[..end]), Err(Error::Truncated)),
                        "bits={bits}, channels={channels}, end={end}"
                    );
                    if let Some(partial) = decoder.partial {
                        resumed_plane |= (1..channels as usize).contains(&partial.channel);
                    }
                }
                let frame = decoder.decode_buffer(&data).unwrap();
                assert_eq!(frame.bytes, data.len());
                for (actual, expected) in frame.pcm.planes().zip(&planes) {
                    assert_eq!(actual, expected);
                }
                assert!(decoder.partial.is_none());
                // An untrusted small maximum only selects an optimistic raw
                // attempt. Every truncated prefix must still resume exactly.
                let mut info = options.stream_info().unwrap();
                info.min_frame_size = 0;
                info.max_frame_size = 1;
                let mut optimistic = FrameDecoder::new(Some(info)).unwrap();
                for end in 0..data.len() {
                    assert!(matches!(
                        optimistic.decode_buffer(&data[..end]),
                        Err(Error::Truncated)
                    ));
                }
                let frame = optimistic.decode_buffer(&data).unwrap();
                for (actual, expected) in frame.pcm.planes().zip(&planes) {
                    assert_eq!(actual, expected);
                }
                if channels > 1 {
                    assert!(resumed_plane);
                }

                let other = vec![vec![1; 96]; channels as usize];
                let refs: Vec<_> = other.iter().map(Vec::as_slice).collect();
                let different = encoder.encode(&refs, 0).unwrap().to_vec();
                assert!(decoder.decode_buffer(&data[..data.len() - 1]).is_err());
                let frame = decoder.decode(&different).unwrap();
                assert!(frame.pcm.planes().all(|p| p == [1; 96]));
                let mut bad = data.clone();
                *bad.last_mut().unwrap() ^= 1;
                assert!(decoder.decode_buffer(&data[..data.len() - 1]).is_err());
                assert!(matches!(
                    decoder.decode_buffer(&bad),
                    Err(Error::Checksum(_))
                ));
                assert!(decoder.partial.is_none());
                assert!(
                    decoder
                        .decode_buffer(&different)
                        .unwrap()
                        .pcm
                        .planes()
                        .all(|p| p == [1; 96])
                );
            }
        }
    }

    #[test]
    fn biased_range_check_covers_signed_and_wrapping_extremes() {
        for bits in 1..=33 {
            let bound = 1i64 << (bits - 1);
            for sample in [
                i64::MIN,
                i64::MIN + 1,
                -bound - 1,
                -bound,
                -bound + 1,
                0,
                bound - 1,
                bound,
                bound + 1,
                i64::MAX - 1,
                i64::MAX,
            ] {
                assert_eq!(
                    check_sample(sample, bits).is_ok(),
                    (-bound..bound).contains(&sample)
                );
            }
        }
    }
}
