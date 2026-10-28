use crate::{Endian, Error, Format, SampleFormat};

/// Borrowed interleaved PCM. All integer methods retain the container's scale;
/// valid bits remain left justified. Floating methods preserve native bits.
#[derive(Clone, Copy, Debug)]
pub struct Pcm<'a> {
    bytes: &'a [u8],
    format: Format<'a>,
    sample: SampleFormat,
}
impl<'a> Pcm<'a> {
    pub fn new(bytes: &'a [u8], format: Format<'a>) -> Result<Self, Error> {
        // Format has public fields: revalidate before using alignment in arithmetic.
        let format = Format::parse(format.raw, format.endian)?;
        let sample = format.sample_format()?;
        if !bytes.len().is_multiple_of(usize::from(format.block_align)) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        Ok(Self {
            bytes,
            format,
            sample,
        })
    }
    pub fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn format(self) -> Format<'a> {
        self.format
    }
    pub fn sample_format(self) -> SampleFormat {
        self.sample
    }
    pub fn frames(self) -> u64 {
        (self.bytes.len() / usize::from(self.format.block_align)) as u64
    }
    pub fn samples(self) -> usize {
        self.bytes.len() / usize::from(self.sample.bits() / 8)
    }
    pub fn frame_range(self, start: u64, frames: u64) -> Result<Self, Error> {
        let end = start
            .checked_add(frames)
            .ok_or(Error::Invalid("frame range overflow"))?;
        if end > self.frames() {
            return Err(Error::Invalid("frame range"));
        }
        let align = usize::from(self.format.block_align);
        Ok(Self {
            bytes: &self.bytes[start as usize * align..end as usize * align],
            ..self
        })
    }
    /// Integer values in their native container scale. U8 becomes signed around zero.
    /// This is an explicit conversion for U8/I16/I24; I32 remains exact.
    pub fn read_i32(self, start_frame: u64, out: &mut [i32]) -> Result<usize, Error> {
        if self.sample.is_float() {
            return Err(Error::Unsupported("float to integer conversion"));
        }
        let (src, count) = self.window(start_frame, out.len())?;
        let out = &mut out[..count];
        let e = self.format.endian;
        match self.sample {
            SampleFormat::U8 => {
                for (o, &b) in out.iter_mut().zip(src) {
                    *o = i32::from(b) - 128;
                }
            }
            SampleFormat::I16 => {
                for (o, b) in out.iter_mut().zip(src.as_chunks::<2>().0) {
                    *o = i32::from(e.u16(b) as i16);
                }
            }
            SampleFormat::I24 => {
                for (o, b) in out.iter_mut().zip(src.as_chunks::<3>().0) {
                    *o = match e {
                        Endian::Little => {
                            ((u32::from(b[0]) << 8 | u32::from(b[1]) << 16 | u32::from(b[2]) << 24)
                                as i32)
                                >> 8
                        }
                        Endian::Big => {
                            ((u32::from(b[2]) << 8 | u32::from(b[1]) << 16 | u32::from(b[0]) << 24)
                                as i32)
                                >> 8
                        }
                    };
                }
            }
            SampleFormat::I32 => {
                for (o, b) in out.iter_mut().zip(src.as_chunks::<4>().0) {
                    *o = e.u32(b) as i32;
                }
            }
            _ => unreachable!(),
        }
        Ok(count)
    }
    pub fn read_i16(self, start_frame: u64, out: &mut [i16]) -> Result<usize, Error> {
        if self.sample != SampleFormat::I16 {
            return Err(Error::Unsupported("native sample type is not i16"));
        }
        let (src, n) = self.window(start_frame, out.len())?;
        for (o, b) in out[..n].iter_mut().zip(src.as_chunks::<2>().0) {
            *o = self.format.endian.u16(b) as i16;
        }
        Ok(n)
    }
    /// Only accepts native F32; does not normalize integers or narrow F64.
    pub fn read_f32(self, start_frame: u64, out: &mut [f32]) -> Result<usize, Error> {
        if self.sample != SampleFormat::F32 {
            return Err(Error::Unsupported("native sample type is not f32"));
        }
        let (src, n) = self.window(start_frame, out.len())?;
        for (o, b) in out[..n].iter_mut().zip(src.as_chunks::<4>().0) {
            *o = f32::from_bits(self.format.endian.u32(b));
        }
        Ok(n)
    }
    pub fn read_f64(self, start_frame: u64, out: &mut [f64]) -> Result<usize, Error> {
        if self.sample != SampleFormat::F64 {
            return Err(Error::Unsupported("native sample type is not f64"));
        }
        let (src, n) = self.window(start_frame, out.len())?;
        for (o, b) in out[..n].iter_mut().zip(src.as_chunks::<8>().0) {
            *o = f64::from_bits(self.format.endian.u64(b));
        }
        Ok(n)
    }
    fn window(self, start: u64, capacity: usize) -> Result<(&'a [u8], usize), Error> {
        if start > self.frames() {
            return Err(Error::Invalid("frame position"));
        }
        let channels = usize::from(self.format.channels);
        let count = ((self.frames() - start).min((capacity / channels) as u64) as usize) * channels;
        let offset = start as usize * usize::from(self.format.block_align);
        let len = count * usize::from(self.sample.bits() / 8);
        Ok((&self.bytes[offset..offset + len], count))
    }
    pub fn as_u8(self) -> Option<&'a [u8]> {
        (self.sample == SampleFormat::U8).then_some(self.bytes)
    }
    pub fn as_i16(self) -> Option<&'a [i16]> {
        self.cast(SampleFormat::I16)
    }
    pub fn as_i32(self) -> Option<&'a [i32]> {
        self.cast(SampleFormat::I32)
    }
    pub fn as_f32(self) -> Option<&'a [f32]> {
        self.cast(SampleFormat::F32)
    }
    pub fn as_f64(self) -> Option<&'a [f64]> {
        self.cast(SampleFormat::F64)
    }
    fn cast<T>(self, sample: SampleFormat) -> Option<&'a [T]> {
        if self.sample != sample
            || !self.format.endian.native()
            || self.bytes.as_ptr().align_offset(core::mem::align_of::<T>()) != 0
        {
            return None;
        }
        // SAFETY: private callers use only integer/float types with all bit patterns
        // valid, matching byte width, native endianness and checked alignment.
        // The borrowed, immutable byte storage covers the full returned lifetime.
        Some(unsafe {
            core::slice::from_raw_parts(
                self.bytes.as_ptr().cast(),
                self.bytes.len() / core::mem::size_of::<T>(),
            )
        })
    }
}
