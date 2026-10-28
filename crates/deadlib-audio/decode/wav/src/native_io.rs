//! Shared native conversion for seekable and forward-only frame sources.
use crate::{Endian, Error, Format, SampleFormat};

pub(crate) struct NativeIo {
    sample: SampleFormat,
    channels: usize,
    endian: Endian,
}
impl NativeIo {
    pub(crate) fn new(format: Format<'_>) -> Result<Self, Error> {
        Ok(Self::from_parts(
            format.sample_format()?,
            usize::from(format.channels),
            format.endian,
        ))
    }
    /// Private callers supply already validated native layout fields.
    pub(crate) fn from_parts(sample: SampleFormat, channels: usize, endian: Endian) -> Self {
        Self {
            sample,
            channels,
            endian,
        }
    }
    pub(crate) fn read_i16(
        self,
        out: &mut [i16],
        read: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::I16, read)?;
        if !self.endian.native() {
            for v in &mut out[..n] {
                *v = v.swap_bytes();
            }
        }
        Ok(n)
    }
    pub(crate) fn read_i32(
        self,
        out: &mut [i32],
        read: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::I32, read)?;
        if !self.endian.native() {
            for v in &mut out[..n] {
                *v = v.swap_bytes();
            }
        }
        Ok(n)
    }
    pub(crate) fn read_f32(
        self,
        out: &mut [f32],
        read: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::F32, read)?;
        if !self.endian.native() {
            for v in &mut out[..n] {
                *v = f32::from_bits(v.to_bits().swap_bytes());
            }
        }
        Ok(n)
    }
    pub(crate) fn read_f64(
        self,
        out: &mut [f64],
        read: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::F64, read)?;
        if !self.endian.native() {
            for v in &mut out[..n] {
                *v = f64::from_bits(v.to_bits().swap_bytes());
            }
        }
        Ok(n)
    }
    // Only the four typed private callers above can supply T. Each has all bit
    // patterns valid and the byte width corresponding to its SampleFormat.
    fn read_native<T>(
        &self,
        out: &mut [T],
        expected: SampleFormat,
        read: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<usize, Error> {
        if self.sample != expected {
            return Err(Error::Unsupported("requested native sample type"));
        }
        // SAFETY: private callers supply initialized, padding-free numeric types
        // with every bit pattern valid; the view covers their exclusive borrow.
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                out.as_mut_ptr().cast::<u8>(),
                core::mem::size_of_val(out),
            )
        };
        Ok(read(bytes)? * self.channels)
    }
    pub(crate) fn read_integers(
        self,
        out: &mut [i32],
        read: impl FnOnce(&mut [u8]) -> Result<usize, Error>,
    ) -> Result<usize, Error> {
        if self.sample.is_float() {
            return Err(Error::Unsupported("float to integer conversion"));
        }
        let width = usize::from(self.sample.bits() / 8);
        let capacity = out.len() / self.channels * self.channels;
        // SAFETY: i32 has no padding and every bit pattern is valid. Expansion
        // proceeds backwards in this exclusive storage after its read completes.
        let b = unsafe {
            core::slice::from_raw_parts_mut(
                out.as_mut_ptr().cast::<u8>(),
                core::mem::size_of_val(out),
            )
        };
        let n = read(&mut b[..capacity * width])? * self.channels;
        for i in (0..n).rev() {
            let src = &b[i * width..(i + 1) * width];
            let value = match self.sample {
                SampleFormat::U8 => i32::from(src[0]) - 128,
                SampleFormat::I16 => i32::from(self.endian.u16(src) as i16),
                SampleFormat::I24 => match self.endian {
                    Endian::Little => {
                        ((u32::from(src[0]) << 8
                            | u32::from(src[1]) << 16
                            | u32::from(src[2]) << 24) as i32)
                            >> 8
                    }
                    Endian::Big => {
                        ((u32::from(src[2]) << 8
                            | u32::from(src[1]) << 16
                            | u32::from(src[0]) << 24) as i32)
                            >> 8
                    }
                },
                SampleFormat::I32 => self.endian.u32(src) as i32,
                _ => unreachable!(),
            };
            b[i * 4..i * 4 + 4].copy_from_slice(&value.to_ne_bytes());
        }
        Ok(n)
    }
}
