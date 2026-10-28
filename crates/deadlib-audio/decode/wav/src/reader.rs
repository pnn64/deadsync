use crate::{ChunkInfo, Container, Error, Format, Limits, SampleFormat, Stream, reserve};
use std::io::{Read, Seek, SeekFrom};

/// File-backed reader: scans headers once, skips audio with Seek, retains only
/// format bytes and metadata descriptors. No audio-size-dependent allocation.
pub struct Reader<R> {
    inner: R,
    base: u64,
    end: u64,
    container: Container,
    format: Vec<u8>,
    chunks: Vec<ChunkInfo>,
    data: ChunkInfo,
    fact: Option<u64>,
    pos: u64,
    failed: bool,
}
impl<R: Read + Seek> Reader<R> {
    pub fn new(inner: R) -> Result<Self, Error> {
        Self::with_limits(inner, Limits::default())
    }
    pub fn with_limits(mut inner: R, limits: Limits) -> Result<Self, Error> {
        let base = inner.stream_position()?;
        let physical_end = inner.seek(SeekFrom::End(0))?;
        inner.seek(SeekFrom::Start(base))?;
        let mut s = Stream::with_limits(inner, limits)?;
        let end = base
            .checked_add(s.form_bytes())
            .ok_or(Error::Invalid("RIFF offset overflow"))?;
        if end > physical_end {
            return Err(Error::Truncated);
        }
        let container = s.container();
        let endian = container.endian();
        let mut format = Vec::new();
        let mut chunks = Vec::new();
        let mut data = None;
        let mut fact = None;
        while let Some(c) = s.next_chunk_seek()? {
            reserve(&mut chunks, 1)?;
            chunks.push(c);
            match &c.id {
                b"fmt " => {
                    if !format.is_empty() {
                        return Err(Error::Invalid("duplicate format chunk"));
                    }
                    if c.size > limits.format_bytes as u64 {
                        return Err(Error::Limit("format size"));
                    }
                    reserve(&mut format, c.size as usize)?;
                    format.resize(c.size as usize, 0);
                    s.read_chunk(&mut format)?;
                    Format::parse(&format, endian)?;
                }
                b"data" => {
                    if data.replace(c).is_some() {
                        return Err(Error::Unsupported("multiple data chunks"));
                    }
                }
                b"fact" => {
                    if fact.is_some() {
                        return Err(Error::Invalid("duplicate fact chunk"));
                    }
                    if c.size < 4 {
                        return Err(Error::Truncated);
                    }
                    let mut b = [0; 4];
                    s.read_chunk(&mut b)?;
                    let n = endian.u32(&b);
                    fact = Some(if n == u32::MAX {
                        s.ds64().map_or(u64::from(n), |d| d.sample_count)
                    } else {
                        u64::from(n)
                    });
                }
                _ => {}
            }
        }
        let f = Format::parse(&format, endian)?;
        let data = data.ok_or(Error::Invalid("missing data chunk"))?;
        if matches!(f.encoding, crate::Encoding::Pcm | crate::Encoding::Float)
            && !data.size.is_multiple_of(u64::from(f.block_align))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let mut inner = s.into_inner();
        inner.seek(SeekFrom::Start(base + data.offset))?;
        Ok(Self {
            inner,
            base,
            end,
            container,
            format,
            chunks,
            data,
            fact,
            pos: 0,
            failed: false,
        })
    }
    pub fn container(&self) -> Container {
        self.container
    }
    pub fn format(&self) -> Format<'_> {
        Format::parse(&self.format, self.container.endian()).unwrap()
    }
    pub fn chunks(&self) -> &[ChunkInfo] {
        &self.chunks
    }
    pub fn fact(&self) -> Option<u64> {
        self.fact
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
    pub fn position(&self) -> u64 {
        self.pos
    }
    pub fn frames(&self) -> Result<u64, Error> {
        let f = self.format();
        f.sample_format()?;
        Ok(self.data.size / u64::from(f.block_align))
    }
    pub fn seek_frame(&mut self, frame: u64) -> Result<(), Error> {
        if frame > self.frames()? {
            return Err(Error::Invalid("frame position"));
        }
        let offset = frame * u64::from(self.format().block_align);
        self.failed = true;
        self.inner
            .seek(SeekFrom::Start(self.base + self.data.offset + offset))?;
        self.pos = frame;
        self.failed = false;
        Ok(())
    }
    /// Raw container bytes in complete frames, returned as a frame count.
    pub fn read_frames(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Invalid(
                "reader requires a successful seek after I/O error",
            ));
        }
        let align = usize::from(self.format().block_align);
        let frames = (self.frames()? - self.pos).min((out.len() / align) as u64) as usize;
        self.failed = true;
        self.inner.read_exact(&mut out[..frames * align])?;
        self.pos += frames as u64;
        self.failed = false;
        Ok(frames)
    }
    pub fn read_i16(&mut self, out: &mut [i16]) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::I16)?;
        if !self.container.endian().native() {
            for v in &mut out[..n] {
                *v = v.swap_bytes();
            }
        }
        Ok(n)
    }
    pub fn read_i32(&mut self, out: &mut [i32]) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::I32)?;
        if !self.container.endian().native() {
            for v in &mut out[..n] {
                *v = v.swap_bytes();
            }
        }
        Ok(n)
    }
    /// Explicit U8/I16/I24/I32 expansion in container scale. Reads packed bytes
    /// into the output's own storage and expands backwards, with no scratch allocation.
    pub fn read_integers(&mut self, out: &mut [i32]) -> Result<usize, Error> {
        let f = self.format();
        let sample = f.sample_format()?;
        if sample.is_float() {
            return Err(Error::Unsupported("float to integer conversion"));
        }
        let e = f.endian;
        let channels = usize::from(f.channels);
        let width = usize::from(sample.bits() / 8);
        let capacity = out.len() / channels * channels;
        // SAFETY: all i32 bit patterns are valid. The temporary byte view covers
        // only the exclusive output borrow, which is not used until it ends.
        let b = unsafe {
            core::slice::from_raw_parts_mut(
                out.as_mut_ptr().cast::<u8>(),
                core::mem::size_of_val(out),
            )
        };
        let n = self.read_frames(&mut b[..capacity * width])? * channels;
        for i in (0..n).rev() {
            let src = &b[i * width..(i + 1) * width];
            let v = match sample {
                SampleFormat::U8 => i32::from(src[0]) - 128,
                SampleFormat::I16 => i32::from(e.u16(src) as i16),
                SampleFormat::I24 => match e {
                    crate::Endian::Little => {
                        ((u32::from(src[0]) << 8
                            | u32::from(src[1]) << 16
                            | u32::from(src[2]) << 24) as i32)
                            >> 8
                    }
                    crate::Endian::Big => {
                        ((u32::from(src[2]) << 8
                            | u32::from(src[1]) << 16
                            | u32::from(src[0]) << 24) as i32)
                            >> 8
                    }
                },
                SampleFormat::I32 => e.u32(src) as i32,
                _ => unreachable!(),
            };
            b[i * 4..i * 4 + 4].copy_from_slice(&v.to_ne_bytes());
        }
        Ok(n)
    }
    pub fn read_f32(&mut self, out: &mut [f32]) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::F32)?;
        if !self.container.endian().native() {
            for v in &mut out[..n] {
                *v = f32::from_bits(v.to_bits().swap_bytes());
            }
        }
        Ok(n)
    }
    pub fn read_f64(&mut self, out: &mut [f64]) -> Result<usize, Error> {
        let n = self.read_native(out, SampleFormat::F64)?;
        if !self.container.endian().native() {
            for v in &mut out[..n] {
                *v = f64::from_bits(v.to_bits().swap_bytes());
            }
        }
        Ok(n)
    }
    fn read_native<T>(&mut self, out: &mut [T], sample: SampleFormat) -> Result<usize, Error> {
        let f = self.format();
        if f.sample_format()? != sample {
            return Err(Error::Unsupported("requested native sample type"));
        }
        let channels = usize::from(f.channels);
        // SAFETY: private callers pass only numeric types with all bit patterns
        // valid. The byte view covers exactly the exclusive output borrow.
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(
                out.as_mut_ptr().cast::<u8>(),
                core::mem::size_of_val(out),
            )
        };
        Ok(self.read_frames(bytes)? * channels)
    }
    /// Read a retained metadata payload into a caller buffer, preserving audio position.
    pub fn read_metadata(&mut self, c: ChunkInfo, out: &mut [u8]) -> Result<usize, Error> {
        if !self.chunks.contains(&c) {
            return Err(Error::Invalid("unknown chunk descriptor"));
        }
        let absolute = self.base + c.offset;
        if c.size > self.end - absolute {
            return Err(Error::Invalid("metadata boundary"));
        }
        let n = c.size.min(out.len() as u64) as usize;
        self.failed = true;
        self.inner.seek(SeekFrom::Start(absolute))?;
        self.inner.read_exact(&mut out[..n])?;
        self.inner.seek(SeekFrom::Start(
            self.base + self.data.offset + self.pos * u64::from(self.format().block_align),
        ))?;
        self.failed = false;
        Ok(n)
    }
}
