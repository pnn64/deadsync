use crate::{Container, Endian, Error, Format, SampleFormat};
use std::io::{Seek, SeekFrom, Write};

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub sample_format: SampleFormat,
    pub channels: u16,
    pub sample_rate: u32,
    /// Must be nonzero and no greater than the container width.
    pub valid_bits: u16,
    pub channel_mask: u32,
}
impl Spec {
    pub fn new(sample_format: SampleFormat, channels: u16, sample_rate: u32) -> Self {
        Self {
            sample_format,
            channels,
            sample_rate,
            valid_bits: sample_format.bits(),
            channel_mask: 0,
        }
    }
    fn header(self, e: Endian, force_extensible: bool) -> Result<([u8; 40], usize), Error> {
        let bits = self.sample_format.bits();
        let align = self
            .channels
            .checked_mul(bits / 8)
            .ok_or(Error::Invalid("block alignment overflow"))?;
        let rate = self
            .sample_rate
            .checked_mul(u32::from(align))
            .ok_or(Error::Invalid("byte rate overflow"))?;
        let ext = force_extensible
            || self.channels > 2
            || self.valid_bits != bits
            || self.channel_mask != 0;
        let tag = if self.sample_format.is_float() { 3 } else { 1 };
        let mut b = [0u8; 40];
        put16(&mut b, if ext { 0xfffe } else { tag }, e);
        put16(&mut b[2..], self.channels, e);
        put32(&mut b[4..], self.sample_rate, e);
        put32(&mut b[8..], rate, e);
        put16(&mut b[12..], align, e);
        put16(&mut b[14..], bits, e);
        let n = if ext {
            put16(&mut b[16..], 22, e);
            put16(&mut b[18..], self.valid_bits, e);
            put32(&mut b[20..], self.channel_mask, e);
            put32(&mut b[24..], u32::from(tag), e);
            put16(&mut b[30..], 0x10, e);
            b[32..].copy_from_slice(&[0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
            40
        } else if tag == 3 {
            18
        } else {
            16
        };
        Format::parse(&b[..n], e)?;
        Ok((b, n))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    pub container: Container,
    /// Reserve a JUNK chunk and promote RIFF to RF64 when 32-bit sizes overflow.
    /// Applies only to Container::Riff.
    pub auto_rf64: bool,
    pub force_extensible: bool,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            container: Container::Riff,
            auto_rf64: true,
            force_extensible: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RawChunk<'a> {
    pub id: [u8; 4],
    pub data: &'a [u8],
}

/// Streaming native PCM/float writer. Audio goes straight to the sink. `finish`
/// writes padding and patches sizes; dropping the writer does not finalize it.
pub struct Writer<W> {
    inner: W,
    spec: Spec,
    options: WriteOptions,
    base: u64,
    data_header: u64,
    fact_offset: Option<u64>,
    bytes: u64,
    failed: bool,
}
impl<W: Write + Seek> Writer<W> {
    pub fn new(
        mut inner: W,
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
    ) -> Result<Self, Error> {
        let e = options.container.endian();
        let (f, n) = spec.header(e, options.force_extensible)?;
        for c in metadata {
            if [
                *b"fmt ", *b"data", *b"fact", *b"ds64", *b"RIFF", *b"RF64", *b"BW64", *b"RIFX",
            ]
            .contains(&c.id)
            {
                return Err(Error::Invalid("reserved metadata chunk ID"));
            }
            if c.data.len() as u64 >= u64::from(u32::MAX) {
                return Err(Error::Limit("metadata chunk size"));
            }
        }
        let base = inner.stream_position()?;
        inner.write_all(match options.container {
            Container::Riff => b"RIFF",
            Container::Rifx => b"RIFX",
            Container::Rf64 => b"RF64",
            Container::Bw64 => b"BW64",
        })?;
        write32(
            &mut inner,
            if options.container.is_64() {
                u32::MAX
            } else {
                0
            },
            e,
        )?;
        inner.write_all(b"WAVE")?;
        if options.container.is_64() || (options.auto_rf64 && options.container == Container::Riff)
        {
            chunk(
                &mut inner,
                if options.container.is_64() {
                    b"ds64"
                } else {
                    b"JUNK"
                },
                &[0; 28],
                e,
            )?;
        }
        chunk(&mut inner, b"fmt ", &f[..n], e)?;
        let fact_offset = if spec.sample_format.is_float() {
            let pos = inner.stream_position()? + 8;
            chunk(&mut inner, b"fact", &[0; 4], e)?;
            Some(pos)
        } else {
            None
        };
        for c in metadata {
            chunk(&mut inner, &c.id, c.data, e)?;
        }
        let data_header = inner.stream_position()?;
        inner.write_all(b"data")?;
        write32(
            &mut inner,
            if options.container.is_64() {
                u32::MAX
            } else {
                0
            },
            e,
        )?;
        Ok(Self {
            inner,
            spec,
            options,
            base,
            data_header,
            fact_offset,
            bytes: 0,
            failed: false,
        })
    }
    pub fn spec(&self) -> Spec {
        self.spec
    }
    pub fn frames_written(&self) -> u64 {
        self.bytes / (u64::from(self.spec.channels) * u64::from(self.spec.sample_format.bits() / 8))
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
    /// Complete interleaved frames already in the specified container byte order.
    /// Preserves sample bits, including low unused valid-bit padding.
    pub fn write_frames(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("writer unusable after I/O error"));
        }
        let align =
            usize::from(self.spec.channels) * usize::from(self.spec.sample_format.bits() / 8);
        if !bytes.len().is_multiple_of(align) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let count = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit("data size overflow"))?;
        let form_size = (self.data_header - self.base)
            .checked_add(count)
            .and_then(|v| v.checked_add(count & 1))
            .ok_or(Error::Limit("file size overflow"))?;
        if !self.options.container.is_64()
            && !(self.options.auto_rf64 && self.options.container == Container::Riff)
            && (count >= u64::from(u32::MAX) || form_size >= u64::from(u32::MAX))
        {
            return Err(Error::Limit("32-bit RIFF size"));
        }
        self.failed = true;
        self.inner.write_all(bytes)?;
        self.bytes = count;
        self.failed = false;
        Ok(())
    }
    pub fn write_i16(&mut self, samples: &[i16]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::I16, |v, b, e| put16(b, *v as u16, e))
    }
    /// Native I32 only. Use `write_integers` for packed U8/I16/I24 containers.
    pub fn write_i32(&mut self, samples: &[i32]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::I32, |v, b, e| put32(b, *v as u32, e))
    }
    pub fn write_f32(&mut self, samples: &[f32]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::F32, |v, b, e| {
            put32(b, v.to_bits(), e)
        })
    }
    pub fn write_f64(&mut self, samples: &[f64]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::F64, |v, b, e| {
            b.copy_from_slice(&match e {
                Endian::Little => v.to_bits().to_le_bytes(),
                Endian::Big => v.to_bits().to_be_bytes(),
            });
        })
    }
    fn write_native<T>(
        &mut self,
        samples: &[T],
        expected: SampleFormat,
        pack: impl Fn(&T, &mut [u8], Endian),
    ) -> Result<(), Error> {
        if self.spec.sample_format != expected {
            return Err(Error::Unsupported("requested native sample type"));
        }
        if !samples
            .len()
            .is_multiple_of(usize::from(self.spec.channels))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let e = self.options.container.endian();
        if e.native() {
            // SAFETY: private callers supply only fully initialized numeric values
            // without padding; the immutable byte borrow covers their exact storage.
            let b = unsafe {
                core::slice::from_raw_parts(
                    samples.as_ptr().cast::<u8>(),
                    core::mem::size_of_val(samples),
                )
            };
            self.write_frames(b)
        } else {
            // A frame may be larger than scratch. Pack samples and write bytes
            // through a private path, while preserving frame boundaries externally.
            let mut scratch = [0u8; 8192];
            let width = usize::from(expected.bits() / 8);
            self.pack_samples(samples, &mut scratch, width, |v, b| pack(v, b, e))
        }
    }
    /// Explicit integer packing in native container scale; out-of-range values
    /// fail before writing any samples. U8 input is signed around zero.
    pub fn write_integers(&mut self, samples: &[i32]) -> Result<(), Error> {
        let bits = self.spec.sample_format.bits();
        if self.spec.sample_format.is_float() {
            return Err(Error::Unsupported("integer packing to float"));
        }
        if !samples
            .len()
            .is_multiple_of(usize::from(self.spec.channels))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        if bits < 32 {
            let bound = 1i32 << (bits - 1);
            if samples.iter().any(|&n| n < -bound || n >= bound) {
                return Err(Error::Invalid("integer exceeds sample container"));
            }
        }
        if bits == 32 {
            return self.write_i32(samples);
        }
        let e = self.options.container.endian();
        self.pack_samples(samples, &mut [0; 8192], usize::from(bits / 8), |n, b| {
            if bits == 8 {
                b[0] = (*n + 128) as u8;
            } else if e == Endian::Little {
                b.copy_from_slice(&n.to_le_bytes()[..bits as usize / 8]);
            } else {
                b.copy_from_slice(&n.to_be_bytes()[4 - bits as usize / 8..]);
            }
        })
    }
    fn pack_samples<T>(
        &mut self,
        samples: &[T],
        scratch: &mut [u8],
        width: usize,
        pack: impl Fn(&T, &mut [u8]),
    ) -> Result<(), Error> {
        // Group full frames when possible. For extreme channel counts, emit one
        // sample at a time into the same sink with a total-frame accounting guard.
        if self.failed {
            return Err(Error::Invalid("writer unusable after I/O error"));
        }
        let align = usize::from(self.spec.channels) * width;
        let per_batch = if align <= scratch.len() {
            scratch.len() / align * usize::from(self.spec.channels)
        } else {
            scratch.len() / width
        };
        let total = (samples.len() as u64)
            .checked_mul(width as u64)
            .ok_or(Error::Limit("data size overflow"))?;
        // Validate the full call's size before writing the first batch.
        let count = self
            .bytes
            .checked_add(total)
            .ok_or(Error::Limit("data size overflow"))?;
        let form_size = (self.data_header - self.base)
            .checked_add(count)
            .and_then(|v| v.checked_add(count & 1))
            .ok_or(Error::Limit("file size overflow"))?;
        if !self.options.container.is_64()
            && !(self.options.auto_rf64 && self.options.container == Container::Riff)
            && (count >= u64::from(u32::MAX) || form_size >= u64::from(u32::MAX))
        {
            return Err(Error::Limit("32-bit RIFF size"));
        }
        self.failed = true;
        for batch in samples.chunks(per_batch) {
            for (v, b) in batch.iter().zip(scratch.chunks_exact_mut(width)) {
                pack(v, b);
            }
            self.inner.write_all(&scratch[..batch.len() * width])?;
        }
        self.bytes = count;
        self.failed = false;
        Ok(())
    }
    pub fn finish(mut self) -> Result<W, Error> {
        if self.failed {
            return Err(Error::Invalid("writer unusable after I/O error"));
        }
        let e = self.options.container.endian();
        if self.bytes & 1 != 0 {
            self.inner.write_all(&[0])?;
        }
        let end = self.inner.stream_position()?;
        let riff_size = end - self.base - 8;
        let large = riff_size >= u64::from(u32::MAX) || self.bytes >= u64::from(u32::MAX);
        let is64 = self.options.container.is_64() || large;
        if large
            && !self.options.container.is_64()
            && !(self.options.auto_rf64 && self.options.container == Container::Riff)
        {
            return Err(Error::Limit("32-bit RIFF size"));
        }
        self.inner.seek(SeekFrom::Start(self.base))?;
        let id: &[u8; 4] = if is64 {
            if self.options.container == Container::Bw64 {
                b"BW64"
            } else {
                b"RF64"
            }
        } else if self.options.container == Container::Rifx {
            b"RIFX"
        } else {
            b"RIFF"
        };
        self.inner.write_all(id)?;
        write32(
            &mut self.inner,
            if is64 { u32::MAX } else { riff_size as u32 },
            e,
        )?;
        let frames = self.frames_written();
        if is64 {
            self.inner.seek(SeekFrom::Start(self.base + 12))?;
            let mut d = [0u8; 28];
            d[..8].copy_from_slice(&riff_size.to_le_bytes());
            d[8..16].copy_from_slice(&self.bytes.to_le_bytes());
            d[16..24].copy_from_slice(&frames.to_le_bytes());
            chunk(&mut self.inner, b"ds64", &d, Endian::Little)?;
        }
        if let Some(pos) = self.fact_offset {
            self.inner.seek(SeekFrom::Start(pos))?;
            write32(
                &mut self.inner,
                if frames >= u64::from(u32::MAX) {
                    u32::MAX
                } else {
                    frames as u32
                },
                e,
            )?;
        }
        self.inner.seek(SeekFrom::Start(self.data_header + 4))?;
        write32(
            &mut self.inner,
            if is64 { u32::MAX } else { self.bytes as u32 },
            e,
        )?;
        self.inner.seek(SeekFrom::Start(end))?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}
fn put16(b: &mut [u8], v: u16, e: Endian) {
    b[..2].copy_from_slice(&match e {
        Endian::Little => v.to_le_bytes(),
        Endian::Big => v.to_be_bytes(),
    });
}
fn put32(b: &mut [u8], v: u32, e: Endian) {
    b[..4].copy_from_slice(&match e {
        Endian::Little => v.to_le_bytes(),
        Endian::Big => v.to_be_bytes(),
    });
}
fn write32(w: &mut impl Write, v: u32, e: Endian) -> Result<(), Error> {
    let mut b = [0; 4];
    put32(&mut b, v, e);
    w.write_all(&b)?;
    Ok(())
}
fn chunk(w: &mut impl Write, id: &[u8; 4], data: &[u8], e: Endian) -> Result<(), Error> {
    w.write_all(id)?;
    write32(w, data.len() as u32, e)?;
    w.write_all(data)?;
    if data.len() & 1 != 0 {
        w.write_all(&[0])?;
    }
    Ok(())
}
