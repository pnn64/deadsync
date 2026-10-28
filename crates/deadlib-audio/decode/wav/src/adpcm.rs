//! Independent block codecs for the Microsoft and IMA WAV layouts.
//! Integer rounding follows Microsoft RIFFNEW (1994) and IMA revision 3 (1992).
use crate::{Encoding, Endian, Error, Format, Wave};

pub(crate) const COEFFICIENTS: [(i16, i16); 7] = [
    (256, 0),
    (512, -256),
    (0, 0),
    (192, 64),
    (240, 0),
    (460, -208),
    (392, -232),
];
pub(crate) const ADAPT: [i64; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];
pub(crate) const INDEX: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];
pub(crate) const INDEX3: [i32; 4] = [-1, -1, 1, 2];
pub(crate) const STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

/// Validated block parameters; borrows only the optional Microsoft coefficient table.
#[derive(Clone, Copy, Debug)]
pub struct Adpcm<'a> {
    pub(crate) encoding: Encoding,
    pub(crate) endian: Endian,
    pub(crate) channels: usize,
    pub(crate) block_bytes: usize,
    pub(crate) frames_per_block: usize,
    pub(crate) bits: u16,
    pub(crate) coefficients: &'a [u8],
}
impl<'a> Adpcm<'a> {
    pub fn from_format(format: Format<'a>) -> Result<Self, Error> {
        let f = Format::parse(format.raw, format.endian)?;
        if f.tag == 0xfffe && f.encoding != Encoding::ImaAdpcm {
            return Err(Error::Unsupported("extensible Microsoft ADPCM mapping"));
        }
        if f.bits_per_sample != 4 && !(f.encoding == Encoding::ImaAdpcm && f.bits_per_sample == 3) {
            return Err(Error::Unsupported("ADPCM bit depth"));
        }
        if f.extra.len() < 2 {
            return Err(Error::Invalid("ADPCM samples per block"));
        }
        let frames = usize::from(f.endian.u16(f.extra));
        let channels = usize::from(f.channels);
        let block = usize::from(f.block_align);
        let mut coefficients = &[][..];
        match f.encoding {
            Encoding::ImaAdpcm => {
                let group = channels * if f.bits_per_sample == 3 { 12 } else { 4 };
                if block < 4 * channels
                    || !(block - 4 * channels).is_multiple_of(group)
                    || frames
                        != 1 + (block - 4 * channels) * 8
                            / (channels * usize::from(f.bits_per_sample))
                {
                    return Err(Error::Invalid("IMA block geometry"));
                }
            }
            Encoding::MsAdpcm => {
                if channels > 2 {
                    return Err(Error::Unsupported("Microsoft ADPCM channel count"));
                }
                if block < 7 * channels
                    || frames < 2
                    || frames > 2 + (block - 7 * channels) * 2 / channels
                {
                    return Err(Error::Invalid("Microsoft ADPCM block geometry"));
                }
                if f.extra.len() < 4 {
                    return Err(Error::Truncated);
                }
                let n = usize::from(f.endian.u16(&f.extra[2..]));
                if n < 7 {
                    return Err(Error::Invalid("Microsoft coefficient table"));
                }
                coefficients = f.extra.get(4..4 + n * 4).ok_or(Error::Truncated)?;
                for (row, &(a, b)) in coefficients.as_chunks::<4>().0.iter().zip(&COEFFICIENTS) {
                    if f.endian.u16(row) as i16 != a || f.endian.u16(&row[2..]) as i16 != b {
                        return Err(Error::Invalid("Microsoft standard coefficients"));
                    }
                }
            }
            _ => return Err(Error::Unsupported("not a WAV ADPCM block codec")),
        }
        Ok(Self {
            encoding: f.encoding,
            endian: f.endian,
            channels,
            block_bytes: block,
            frames_per_block: frames,
            bits: f.bits_per_sample,
            coefficients,
        })
    }
    pub fn channels(self) -> usize {
        self.channels
    }
    pub fn block_bytes(self) -> usize {
        self.block_bytes
    }
    pub fn frames_per_block(self) -> usize {
        self.frames_per_block
    }
    pub fn bits_per_sample(self) -> u16 {
        self.bits
    }
    pub fn scratch_samples(self) -> usize {
        self.frames_per_block * self.channels
    }
    /// Validate fact/ds64 duration against the encoded byte extent without
    /// reading audio. A complete short last block contributes its actual capacity.
    pub fn validate_frame_count(self, bytes: u64, frames: u64) -> Result<(), Error> {
        if frames > self.data_frames(bytes)? {
            return Err(Error::Invalid("ADPCM fact exceeds encoded data"));
        }
        Ok(())
    }
    /// Decoded capacity of a data payload, including complete short last blocks
    /// and any encoder padding. This checks geometry without reading block headers.
    pub fn data_frames(self, bytes: u64) -> Result<u64, Error> {
        let block = self.block_bytes as u64;
        let full = bytes / block;
        let tail = bytes % block;
        let tail_frames = if tail == 0 {
            0
        } else {
            self.block_frames(tail as usize)? as u64
        };
        full.checked_mul(self.frames_per_block as u64)
            .and_then(|n| n.checked_add(tail_frames))
            .ok_or(Error::Invalid("ADPCM byte extent or frame count overflow"))
    }
    /// Number of decodable frames in a full or structurally complete short block.
    pub fn block_frames(self, bytes: usize) -> Result<usize, Error> {
        if bytes > self.block_bytes {
            return Err(Error::Invalid("ADPCM block too large"));
        }
        let frames = if self.encoding == Encoding::ImaAdpcm {
            let group = self.channels * if self.bits == 3 { 12 } else { 4 };
            if bytes < 4 * self.channels || !(bytes - 4 * self.channels).is_multiple_of(group) {
                return Err(Error::Invalid("partial IMA channel group"));
            }
            1 + (bytes - 4 * self.channels) * 8 / (self.channels * usize::from(self.bits))
        } else {
            if bytes < 7 * self.channels {
                return Err(Error::Truncated);
            }
            2 + (bytes - 7 * self.channels) * 2 / self.channels
        };
        Ok(frames.min(self.frames_per_block))
    }
    /// Decode into caller-owned interleaved i16 storage. Returns frames, not samples.
    /// On malformed input the output may contain a decoded prefix.
    pub fn decode_block(self, b: &[u8], out: &mut [i16]) -> Result<usize, Error> {
        let frames = self.block_frames(b.len())?;
        if out.len() < frames * self.channels {
            return Err(Error::Limit("ADPCM output capacity"));
        }
        if self.encoding == Encoding::ImaAdpcm {
            if self.bits == 3 {
                self.ima3(b, out, frames)?;
            } else {
                self.ima(b, out, frames)?;
            }
        } else {
            self.microsoft(b, out, frames)?;
        }
        Ok(frames)
    }
    /// Locate a byte in one channel's logical 3-bit stream. Physical 32-bit words
    /// alternate channels, including when a code crosses the word boundary.
    pub(crate) fn ima3_byte(self, byte: usize, channel: usize) -> usize {
        4 * self.channels + (byte / 4 * self.channels + channel) * 4 + byte % 4
    }
    fn ima3(self, b: &[u8], out: &mut [i16], frames: usize) -> Result<(), Error> {
        for c in 0..self.channels {
            let header = &b[c * 4..c * 4 + 4];
            let mut predictor = i32::from(self.endian.u16(header) as i16);
            let mut index = i32::from(header[2]);
            if index > 88 {
                return Err(Error::Invalid("IMA step index"));
            }
            out[c] = predictor as i16;
            for frame in 1..frames {
                let bit = (frame - 1) * 3;
                let shift = bit % 8;
                let mut code = b[self.ima3_byte(bit / 8, c)] >> shift;
                if shift > 5 {
                    code |= b[self.ima3_byte(bit / 8 + 1, c)] << (8 - shift);
                }
                code &= 7;
                let step = STEP[index as usize];
                // RIFFNEW's three independent terms, not a combined multiply.
                let mut delta = step >> 2;
                if code & 2 != 0 {
                    delta += step;
                }
                if code & 1 != 0 {
                    delta += step >> 1;
                }
                predictor =
                    (predictor + if code & 4 == 0 { delta } else { -delta }).clamp(-32768, 32767);
                index = (index + INDEX3[(code & 3) as usize]).clamp(0, 88);
                out[frame * self.channels + c] = predictor as i16;
            }
        }
        Ok(())
    }
    fn ima(self, b: &[u8], out: &mut [i16], frames: usize) -> Result<(), Error> {
        for c in 0..self.channels {
            let header = &b[c * 4..c * 4 + 4];
            let mut predictor = i32::from(self.endian.u16(header) as i16);
            let mut index = i32::from(header[2]);
            if index > 88 {
                return Err(Error::Invalid("IMA step index"));
            }
            // Reserved header byte is intentionally ignored by receivers.
            out[c] = predictor as i16;
            for frame in 1..frames {
                let i = frame - 1;
                let offset = self.channels * 4 + (i / 8) * self.channels * 4 + c * 4 + (i % 8) / 2;
                let code = (b[offset] >> ((i & 1) * 4)) & 15;
                let step = STEP[index as usize];
                // Each shift rounds independently, as in the IMA reference algorithm.
                let mut delta = step >> 3;
                if code & 4 != 0 {
                    delta += step;
                }
                if code & 2 != 0 {
                    delta += step >> 1;
                }
                if code & 1 != 0 {
                    delta += step >> 2;
                }
                predictor =
                    (predictor + if code & 8 == 0 { delta } else { -delta }).clamp(-32768, 32767);
                index = (index + INDEX[(code & 7) as usize]).clamp(0, 88);
                out[frame * self.channels + c] = predictor as i16;
            }
        }
        Ok(())
    }
    fn microsoft(self, b: &[u8], out: &mut [i16], frames: usize) -> Result<(), Error> {
        for c in 0..self.channels {
            let row = self
                .coefficients
                .get(usize::from(b[c]) * 4..usize::from(b[c]) * 4 + 4)
                .ok_or(Error::Invalid("Microsoft predictor index"))?;
            let a = i64::from(self.endian.u16(row) as i16);
            let d = i64::from(self.endian.u16(&row[2..]) as i16);
            let mut delta = i64::from(self.endian.u16(&b[self.channels + c * 2..]) as i16);
            if delta < 0 {
                return Err(Error::Invalid("negative Microsoft ADPCM delta"));
            }
            let mut one = i64::from(self.endian.u16(&b[self.channels * 3 + c * 2..]) as i16);
            let mut two = i64::from(self.endian.u16(&b[self.channels * 5 + c * 2..]) as i16);
            out[c] = two as i16;
            out[self.channels + c] = one as i16;
            for frame in 2..frames {
                let i = (frame - 2) * self.channels + c;
                let code = (b[self.channels * 7 + i / 2] >> (4 - (i & 1) * 4)) & 15;
                let signed = if code < 8 {
                    i64::from(code)
                } else {
                    i64::from(code) - 16
                };
                let correction = delta
                    .checked_mul(signed)
                    .ok_or(Error::Invalid("Microsoft delta overflow"))?;
                let predicted = (one * a + two * d) / 256;
                let value = predicted
                    .checked_add(correction)
                    .ok_or(Error::Invalid("Microsoft prediction overflow"))?
                    .clamp(-32768, 32767);
                two = one;
                one = value;
                delta = delta
                    .checked_mul(ADAPT[code as usize])
                    .ok_or(Error::Invalid("Microsoft adaptation overflow"))?
                    / 256;
                delta = delta.max(16);
                out[frame * self.channels + c] = value as i16;
            }
        }
        Ok(())
    }
}

/// Borrowed encoded data with fact-trimmed length and exact frame range access.
#[derive(Clone, Copy, Debug)]
pub struct AdpcmData<'a> {
    codec: Adpcm<'a>,
    bytes: &'a [u8],
    frames: u64,
}
impl<'a> AdpcmData<'a> {
    pub fn new(wave: &Wave<'a>) -> Result<Self, Error> {
        let codec = Adpcm::from_format(wave.format)?;
        let bytes = wave
            .data()
            .ok_or(Error::Unsupported("segmented ADPCM data"))?;
        let frames = wave
            .fact
            .or(wave
                .ds64
                .filter(|_| wave.container == crate::Container::Rf64)
                .map(|d| d.sample_count))
            .ok_or(Error::Invalid("ADPCM requires fact sample count"))?;
        codec.validate_frame_count(bytes.len() as u64, frames)?;
        Ok(Self {
            codec,
            bytes,
            frames,
        })
    }
    pub fn codec(self) -> Adpcm<'a> {
        self.codec
    }
    pub fn frames(self) -> u64 {
        self.frames
    }
    pub fn encoded_bytes(self) -> &'a [u8] {
        self.bytes
    }
    /// Sample-accurate access using caller-owned block scratch. Returns samples.
    pub fn read_frames(
        self,
        start: u64,
        out: &mut [i16],
        scratch: &mut [i16],
    ) -> Result<usize, Error> {
        if start > self.frames {
            return Err(Error::Invalid("frame position"));
        }
        if scratch.len() < self.codec.scratch_samples() {
            return Err(Error::Limit("ADPCM scratch capacity"));
        }
        let channels = self.codec.channels;
        let frames = (self.frames - start).min((out.len() / channels) as u64) as usize;
        self.codec
            .read_data(self.bytes, start, &mut out[..frames * channels], scratch)
    }
}

impl Adpcm<'_> {
    /// The caller has validated the data extent and bounded the requested range.
    pub(crate) fn read_data(
        self,
        bytes: &[u8],
        start: u64,
        out: &mut [i16],
        scratch: &mut [i16],
    ) -> Result<usize, Error> {
        let channels = self.channels;
        let frames = out.len() / channels;
        let mut done = 0;
        while done < frames {
            let position = start + done as u64;
            let block = (position / self.frames_per_block as u64) as usize;
            let skip = (position % self.frames_per_block as u64) as usize;
            let offset = block * self.block_bytes;
            let block = &bytes[offset..bytes.len().min(offset + self.block_bytes)];
            let decoded = self.decode_block(block, scratch)?;
            let n = (decoded - skip).min(frames - done);
            out[done * channels..(done + n) * channels]
                .copy_from_slice(&scratch[skip * channels..(skip + n) * channels]);
            done += n;
        }
        Ok(done * channels)
    }
}
