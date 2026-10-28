use crate::{Endian, Error, Format, SampleFormat, native_io::NativeIo, timeline::Timeline};

/// Borrowed interleaved PCM. All integer methods retain the container's scale;
/// valid bits remain left justified. Floating methods preserve native bits.
#[derive(Clone, Copy, Debug)]
pub struct Pcm<'a> {
    bytes: &'a [u8],
    format: Format<'a>,
    sample: SampleFormat,
    timeline: Option<&'a Timeline>,
    start: u64,
    frames: u64,
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
            timeline: None,
            start: 0,
            frames: (bytes.len() / usize::from(format.block_align)) as u64,
        })
    }
    pub(crate) fn segmented(
        bytes: &'a [u8],
        timeline: &'a Timeline,
        format: Format<'a>,
    ) -> Result<Self, Error> {
        let format = Format::parse(format.raw, format.endian)?;
        let sample = format.sample_format()?;
        if timeline.align != Some(format.block_align) {
            return Err(Error::Invalid("changed wavl format alignment"));
        }
        Ok(Self {
            bytes,
            format,
            sample,
            timeline: Some(timeline),
            start: 0,
            frames: timeline.frames,
        })
    }
    /// Contiguous sample bytes. Segmented wavl views return None.
    pub fn as_bytes(self) -> Option<&'a [u8]> {
        self.timeline.is_none().then_some(self.bytes)
    }
    pub fn format(self) -> Format<'a> {
        self.format
    }
    pub fn sample_format(self) -> SampleFormat {
        self.sample
    }
    pub fn frames(self) -> u64 {
        self.frames
    }
    /// Exact logical interleaved sample count, including large virtual silence.
    pub fn samples(self) -> u128 {
        u128::from(self.frames) * u128::from(self.format.channels)
    }
    pub fn frame_range(self, start: u64, frames: u64) -> Result<Self, Error> {
        let end = start
            .checked_add(frames)
            .ok_or(Error::Invalid("frame range overflow"))?;
        if end > self.frames() {
            return Err(Error::Invalid("frame range"));
        }
        let align = usize::from(self.format.block_align);
        if self.timeline.is_some() {
            return Ok(Self {
                start: self.start + start,
                frames,
                ..self
            });
        }
        Ok(Self {
            bytes: &self.bytes[start as usize * align..end as usize * align],
            frames,
            ..self
        })
    }
    /// Copy whole native frames, expanding slnt into the caller's storage only.
    pub fn read_frames(self, start: u64, out: &mut [u8]) -> Result<usize, Error> {
        if start > self.frames {
            return Err(Error::Invalid("frame position"));
        }
        let align = usize::from(self.format.block_align);
        let frames = (self.frames - start).min((out.len() / align) as u64) as usize;
        let Some(index) = self.timeline else {
            let offset = start as usize * align;
            out[..frames * align].copy_from_slice(&self.bytes[offset..offset + frames * align]);
            return Ok(frames);
        };
        let mut position = self.start + start;
        let mut done = 0;
        while done < frames {
            let entry = index
                .at(position)
                .ok_or(Error::Invalid("wavl frame index"))?;
            let skip = position - entry.start;
            let n = (entry.frames - skip).min((frames - done) as u64) as usize;
            let dst = &mut out[done * align..(done + n) * align];
            if entry.chunk.id == *b"data" {
                let offset = entry.chunk.offset as usize + skip as usize * align;
                dst.copy_from_slice(&self.bytes[offset..offset + dst.len()]);
            } else if entry.hold == 0 {
                dst.fill(if self.sample == SampleFormat::U8 {
                    128
                } else {
                    0
                });
            } else {
                let offset = entry.hold as usize;
                crate::timeline::repeat(dst, &self.bytes[offset..offset + align]);
            }
            done += n;
            position += n as u64;
        }
        Ok(frames)
    }
    /// Integer values in their native container scale. U8 becomes signed around zero.
    /// This is an explicit conversion for U8/I16/I24; I32 remains exact.
    pub fn read_i32(self, start_frame: u64, out: &mut [i32]) -> Result<usize, Error> {
        if self.sample.is_float() {
            return Err(Error::Unsupported("float to integer conversion"));
        }
        if self.timeline.is_some() {
            return NativeIo::new(self.format)?
                .read_integers(out, |b| self.read_frames(start_frame, b));
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
                if e.native() {
                    copy_native(src, out);
                } else {
                    for (o, b) in out.iter_mut().zip(src.as_chunks::<4>().0) {
                        *o = e.u32(b) as i32;
                    }
                }
            }
            _ => unreachable!(),
        }
        Ok(count)
    }
    pub fn read_i16(self, start_frame: u64, out: &mut [i16]) -> Result<usize, Error> {
        if self.timeline.is_some() {
            return NativeIo::new(self.format)?.read_i16(out, |b| self.read_frames(start_frame, b));
        }
        if self.sample != SampleFormat::I16 {
            return Err(Error::Unsupported("native sample type is not i16"));
        }
        let (src, n) = self.window(start_frame, out.len())?;
        if self.format.endian.native() {
            copy_native(src, &mut out[..n]);
            return Ok(n);
        }
        for (o, b) in out[..n].iter_mut().zip(src.as_chunks::<2>().0) {
            *o = self.format.endian.u16(b) as i16;
        }
        Ok(n)
    }
    /// Only accepts native F32; does not normalize integers or narrow F64.
    pub fn read_f32(self, start_frame: u64, out: &mut [f32]) -> Result<usize, Error> {
        if self.timeline.is_some() {
            return NativeIo::new(self.format)?.read_f32(out, |b| self.read_frames(start_frame, b));
        }
        if self.sample != SampleFormat::F32 {
            return Err(Error::Unsupported("native sample type is not f32"));
        }
        let (src, n) = self.window(start_frame, out.len())?;
        if self.format.endian.native() {
            copy_native(src, &mut out[..n]);
            return Ok(n);
        }
        for (o, b) in out[..n].iter_mut().zip(src.as_chunks::<4>().0) {
            *o = f32::from_bits(self.format.endian.u32(b));
        }
        Ok(n)
    }
    pub fn read_f64(self, start_frame: u64, out: &mut [f64]) -> Result<usize, Error> {
        if self.timeline.is_some() {
            return NativeIo::new(self.format)?.read_f64(out, |b| self.read_frames(start_frame, b));
        }
        if self.sample != SampleFormat::F64 {
            return Err(Error::Unsupported("native sample type is not f64"));
        }
        let (src, n) = self.window(start_frame, out.len())?;
        if self.format.endian.native() {
            copy_native(src, &mut out[..n]);
            return Ok(n);
        }
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
        (self.sample == SampleFormat::U8 && self.timeline.is_none()).then_some(self.bytes)
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
        if self.timeline.is_some()
            || self.sample != sample
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

fn copy_native<T>(src: &[u8], out: &mut [T]) {
    // SAFETY: private callers use only i16/i32/f32/f64 (all bit patterns valid),
    // after checking native byte order and selecting a whole-sample byte window.
    // An exclusive byte view retains output alignment and lifetime; source bytes
    // may be unaligned. copy_from_slice checks exact lengths and uses bulk copy.
    let dst = unsafe {
        core::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), core::mem::size_of_val(out))
    };
    dst.copy_from_slice(src);
}
