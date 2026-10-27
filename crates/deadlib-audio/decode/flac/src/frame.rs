//! Container-independent frame decoding, including 33-bit stereo side signals.
use crate::{
    Error, StreamInfo,
    bitstream::BitReader,
    checksum::{crc8, crc16},
    reserve,
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
        let bytes = usize::from(bits.div_ceil(8));
        let mut buffer = [0u8; 4096];
        let mut used = 0;
        for sample in 0..self.samples {
            for channel in 0..self.channels as usize {
                let value = self.data[channel * self.stride + self.offset + sample].to_le_bytes();
                if used + bytes > buffer.len() {
                    digest.update(&buffer[..used]);
                    used = 0;
                }
                buffer[used..used + bytes].copy_from_slice(&value[..bytes]);
                used += bytes;
            }
        }
        digest.update(&buffer[..used]);
    }
}

#[derive(Debug)]
pub struct Frame<'a> {
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
    pub bytes: usize,
}

/// Reuses flat scratch and PCM buffers across frames. Errors leave it reusable.
#[derive(Default)]
pub struct FrameDecoder {
    info: Option<StreamInfo>,
    work: Vec<i64>,
    output: Vec<i32>,
}
impl FrameDecoder {
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
        let header = FrameHeader::parse(data, self.info.as_ref())?;
        let n = header.block_size;
        let len = n * header.channels as usize;
        if len > self.work.len() {
            let additional = len - self.work.len();
            reserve(&mut self.work, additional)?;
        }
        self.work.resize(len, 0);
        let mut r = BitReader::new(&data[header.header_bytes..]);
        for (channel, plane) in self.work.chunks_exact_mut(n).enumerate() {
            let side = (header.channel_assignment == 9 && channel == 0)
                || (matches!(header.channel_assignment, 8 | 10) && channel == 1);
            subframe(
                &mut r,
                plane,
                u32::from(header.bits_per_sample) + u32::from(side),
            )?;
        }
        r.align_zero()?;
        let crc_pos = header.header_bytes + r.position() / 8;
        if r.read(16)? as u16 != crc16(&data[..crc_pos]) {
            return Err(Error::Checksum("frame CRC-16"));
        }
        if header.channel_assignment >= 8 {
            let (left, right) = self.work.split_at_mut(n);
            match header.channel_assignment {
                8 => {
                    for (l, r) in left.iter_mut().zip(right) {
                        *r = *l - *r;
                    }
                }
                9 => {
                    for (l, r) in left.iter_mut().zip(right) {
                        *l += *r;
                    }
                }
                10 => {
                    for (l, r) in left.iter_mut().zip(right) {
                        let mid = (*l << 1) | (*r & 1);
                        let side = *r;
                        *l = (mid + side) >> 1;
                        *r = (mid - side) >> 1;
                    }
                }
                _ => unreachable!(),
            }
        }
        if len > self.output.len() {
            let additional = len - self.output.len();
            reserve(&mut self.output, additional)?;
        }
        self.output.resize(len, 0);
        for (dst, &src) in self.output.iter_mut().zip(&self.work) {
            check_sample(src, header.bits_per_sample.into())?;
            *dst = src as i32;
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

#[inline]
fn check_sample(sample: i64, bits: u32) -> Result<(), Error> {
    let bound = 1i64 << (bits - 1);
    if sample < -bound || sample >= bound {
        Err(Error::Invalid("sample outside declared bit depth"))
    } else {
        Ok(())
    }
}

fn subframe(r: &mut BitReader<'_>, output: &mut [i64], bits: u32) -> Result<(), Error> {
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
        0 => output.fill(r.signed(width)?),
        1 => {
            for sample in output.iter_mut() {
                *sample = r.signed(width)?;
            }
        }
        8..=12 | 32..=63 => {
            let order = if kind < 32 { kind - 8 } else { kind - 31 };
            if order >= output.len() {
                return Err(Error::Invalid("predictor order exceeds block"));
            }
            for sample in &mut output[..order] {
                *sample = r.signed(width)?;
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
                let fixed: &[i64] = match order {
                    0 => &[],
                    1 => &[1],
                    2 => &[2, -1],
                    3 => &[3, -3, 1],
                    4 => &[4, -6, 4, -1],
                    _ => unreachable!(),
                };
                coeff[..order].copy_from_slice(fixed);
            }
            residual(r, output, order)?;
            // Each previous sample was range checked. With <=33-bit samples,
            // <=15-bit coefficients and <=32 taps, this sum fits in i64.
            for i in order..output.len() {
                let mut sum = 0i64;
                for j in 0..order {
                    sum += coeff[j] * output[i - j - 1];
                }
                output[i] += sum >> shift;
                check_sample(output[i], width)?;
            }
        }
        _ => return Err(Error::Invalid("reserved subframe type")),
    }
    if wasted != 0 {
        for sample in output {
            *sample <<= wasted;
        }
    }
    Ok(())
}

fn residual(r: &mut BitReader<'_>, output: &mut [i64], order: usize) -> Result<(), Error> {
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
                *sample = r.signed(width)?;
            }
        } else {
            for sample in part {
                let q = r.unary(u64::from(u32::MAX - 1) >> k)?;
                let folded = (q << k) | r.read(k)?;
                if folded > u64::from(u32::MAX - 1) {
                    return Err(Error::Invalid("residual exceeds 31-bit magnitude"));
                }
                *sample = ((folded >> 1) as i64) ^ -((folded & 1) as i64);
            }
        }
    }
    Ok(())
}
