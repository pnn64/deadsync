//! Incremental native wavl output. Data and silence are separate bounded records.
use crate::{
    Container, DataLength, Endian, Error, RawChunk, SampleFormat, Spec, WriteOptions,
    mux::{chunk, container_id, put16, put32, validate_chunk, validate_fact_count, write32},
};
use std::io::{Seek, SeekFrom, Write};

/// Native PCM/float wavl writer. Every write emits a complete child chunk;
/// `write_silence` holds the preceding frame according to the WAVE specification.
/// Dropping or extracting the sink does not finalize unknown-length headers.
pub struct WavlWriter<W> {
    inner: W,
    spec: Spec,
    options: WriteOptions,
    base: u64,
    list_header: u64,
    fact_offset: u64,
    bytes: u64,
    frames: u64,
    length: Option<DataLength>,
    failed: bool,
}
impl<W: Write + Seek> WavlWriter<W> {
    pub fn new(
        mut inner: W,
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
    ) -> Result<Self, Error> {
        let reserve = options.container.is_64()
            || (options.auto_rf64 && options.container == Container::Riff);
        let header = Header::new(spec, options, metadata, reserve)?;
        let base = inner.stream_position()?;
        let size = header
            .list_header
            .checked_add(4)
            .ok_or(Error::Limit("wavl header size overflow"))?;
        base.checked_add(size)
            .and_then(|v| v.checked_add(8))
            .ok_or(Error::Limit("wavl file offset overflow"))?;
        if !reserve && large(size, 4, 0) {
            return Err(Error::Limit("32-bit RIFF size"));
        }
        header.write(&mut inner, options.container, metadata, None)?;
        Ok(Self {
            inner,
            spec,
            options,
            base,
            list_header: base + header.list_header,
            fact_offset: base + header.fact_offset,
            bytes: 4,
            frames: 0,
            length: None,
            failed: false,
        })
    }
    pub fn finish(mut self) -> Result<W, Error> {
        if self.length.is_some() {
            return self.finish_stream();
        }
        self.check()?;
        let end = self.inner.stream_position()?;
        let size = self.list_header - self.base + self.bytes;
        if end != self.base + 8 + size {
            return Err(Error::Invalid("wavl sink position"));
        }
        let is64 = self.options.container.is_64() || large(size, self.bytes, self.frames);
        let container = if is64 && self.options.container != Container::Bw64 {
            Container::Rf64
        } else {
            self.options.container
        };
        let e = container.endian();
        self.inner.seek(SeekFrom::Start(self.base))?;
        self.inner.write_all(container_id(container))?;
        write32(
            &mut self.inner,
            if is64 { u32::MAX } else { size as u32 },
            e,
        )?;
        if is64 {
            self.inner.seek(SeekFrom::Start(self.base + 12))?;
            chunk(
                &mut self.inner,
                b"ds64",
                &ds64(container, size, self.bytes, self.frames),
                e,
            )?;
        }
        self.inner.seek(SeekFrom::Start(self.fact_offset))?;
        write32(
            &mut self.inner,
            self.frames.min(u64::from(u32::MAX)) as u32,
            e,
        )?;
        self.inner.seek(SeekFrom::Start(self.list_header + 4))?;
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
impl<W: Write> WavlWriter<W> {
    /// `length.bytes` is the complete LIST payload: four bytes for `wavl`, plus
    /// every child header, payload and odd pad. Frames include virtual silence.
    pub fn with_lengths(
        mut inner: W,
        spec: Spec,
        mut options: WriteOptions,
        metadata: &[RawChunk<'_>],
        length: DataLength,
    ) -> Result<Self, Error> {
        validate_fact_count(options.container, length.frames)?;
        if length.bytes < 4 || !length.bytes.is_multiple_of(2) {
            return Err(Error::Invalid("wavl declared byte length"));
        }
        let mut header = Header::new(spec, options, metadata, options.container.is_64())?;
        let mut size = header
            .list_header
            .checked_add(length.bytes)
            .ok_or(Error::Limit("wavl file size overflow"))?;
        if !options.container.is_64() && large(size, length.bytes, length.frames) {
            if options.container != Container::Riff || !options.auto_rf64 {
                return Err(Error::Limit("32-bit RIFF size or sample count"));
            }
            options.container = Container::Rf64;
            header = Header::new(spec, options, metadata, true)?;
            size = header
                .list_header
                .checked_add(length.bytes)
                .ok_or(Error::Limit("wavl file size overflow"))?;
        }
        size.checked_add(8)
            .ok_or(Error::Limit("wavl file size overflow"))?;
        header.write(&mut inner, options.container, metadata, Some(length))?;
        Ok(Self {
            inner,
            spec,
            options,
            base: 0,
            list_header: header.list_header,
            fact_offset: header.fact_offset,
            bytes: 4,
            frames: 0,
            length: Some(length),
            failed: false,
        })
    }
    pub fn finish_stream(mut self) -> Result<W, Error> {
        self.check()?;
        if self.length
            != Some(DataLength {
                bytes: self.bytes,
                frames: self.frames,
            })
        {
            return Err(if self.length.is_none() {
                Error::Unsupported("unknown-length wavl needs seek finalization")
            } else {
                Error::Invalid("incomplete declared wavl length")
            });
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
    pub fn spec(&self) -> Spec {
        self.spec
    }
    pub fn frames_written(&self) -> u64 {
        self.frames
    }
    pub fn list_bytes_written(&self) -> u64 {
        self.bytes
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
    pub fn write_frames(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let align =
            usize::from(self.spec.channels) * usize::from(self.spec.sample_format.bits() / 8);
        if !bytes.len().is_multiple_of(align) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        self.data(bytes.len() as u64, (bytes.len() / align) as u64, |w| {
            w.write_all(bytes)?;
            Ok(())
        })
    }
    /// Counts are split into u32-sized slnt records without materializing PCM.
    /// A zero count deliberately emits a zero-length silence record.
    pub fn write_silence(&mut self, frames: u64) -> Result<(), Error> {
        let records = frames.div_ceil(u64::from(u32::MAX)).max(1);
        let bytes = records
            .checked_mul(12)
            .ok_or(Error::Limit("wavl silence size overflow"))?;
        let e = self.options.container.endian();
        self.write_with(bytes, frames, |w| {
            let mut left = frames;
            for _ in 0..records {
                let n = left.min(u64::from(u32::MAX)) as u32;
                let mut b = [0; 4];
                put32(&mut b, n, e);
                chunk(w, b"slnt", &b, e)?;
                left -= u64::from(n);
            }
            Ok(())
        })
    }
    /// Preserve an ancillary child payload verbatim. Use the audio methods for
    /// data/slnt so byte and logical frame accounting cannot diverge.
    pub fn write_chunk(&mut self, record: RawChunk<'_>) -> Result<(), Error> {
        if [*b"data", *b"slnt"].contains(&record.id) {
            return Err(Error::Invalid("reserved wavl child ID"));
        }
        validate_chunk(record.data)?;
        let e = self.options.container.endian();
        self.write_with(
            8 + record.data.len() as u64 + (record.data.len() & 1) as u64,
            0,
            |w| chunk(w, &record.id, record.data, e),
        )
    }
    pub fn write_i16(&mut self, samples: &[i16]) -> Result<(), Error> {
        self.native(samples, SampleFormat::I16, |v, b, e| put16(b, *v as u16, e))
    }
    pub fn write_i32(&mut self, samples: &[i32]) -> Result<(), Error> {
        self.native(samples, SampleFormat::I32, |v, b, e| put32(b, *v as u32, e))
    }
    pub fn write_f32(&mut self, samples: &[f32]) -> Result<(), Error> {
        self.native(samples, SampleFormat::F32, |v, b, e| {
            put32(b, v.to_bits(), e)
        })
    }
    pub fn write_f64(&mut self, samples: &[f64]) -> Result<(), Error> {
        self.native(samples, SampleFormat::F64, |v, b, e| {
            b.copy_from_slice(&if e == Endian::Little {
                v.to_bits().to_le_bytes()
            } else {
                v.to_bits().to_be_bytes()
            })
        })
    }
    fn native<T>(
        &mut self,
        samples: &[T],
        expected: SampleFormat,
        pack: impl Fn(&T, &mut [u8], Endian),
    ) -> Result<(), Error> {
        if self.spec.sample_format != expected {
            return Err(Error::Unsupported("requested native sample type"));
        }
        let channels = usize::from(self.spec.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let e = self.options.container.endian();
        if e.native() {
            // SAFETY: only private numeric callers with no padding and fully
            // initialized bits reach this immutable byte view.
            let b = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(samples),
                )
            };
            return self.write_frames(b);
        }
        let width = usize::from(expected.bits() / 8);
        self.data(
            std::mem::size_of_val(samples) as u64,
            (samples.len() / channels) as u64,
            |w| {
                let mut scratch = [0; 8192];
                for values in samples.chunks(scratch.len() / width) {
                    for (i, v) in values.iter().enumerate() {
                        pack(v, &mut scratch[i * width..(i + 1) * width], e);
                    }
                    w.write_all(&scratch[..values.len() * width])?;
                }
                Ok(())
            },
        )
    }
    pub fn write_integers(&mut self, samples: &[i32]) -> Result<(), Error> {
        let sample = self.spec.sample_format;
        if sample.is_float() {
            return Err(Error::Unsupported("integer packing to float"));
        }
        if sample == SampleFormat::I32 {
            return self.write_i32(samples);
        }
        let channels = usize::from(self.spec.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let bits = sample.bits();
        let bound = 1i32 << (bits - 1);
        if samples.iter().any(|&v| v < -bound || v >= bound) {
            return Err(Error::Invalid("integer exceeds sample container"));
        }
        let width = usize::from(bits / 8);
        let e = self.options.container.endian();
        self.data(
            samples.len() as u64 * width as u64,
            (samples.len() / channels) as u64,
            |w| {
                let mut scratch = [0; 8192];
                for values in samples.chunks(scratch.len() / width) {
                    for (i, v) in values.iter().enumerate() {
                        let b = &mut scratch[i * width..(i + 1) * width];
                        if bits == 8 {
                            b[0] = (*v + 128) as u8;
                        } else if e == Endian::Little {
                            b.copy_from_slice(&v.to_le_bytes()[..width]);
                        } else {
                            b.copy_from_slice(&v.to_be_bytes()[4 - width..]);
                        }
                    }
                    w.write_all(&scratch[..values.len() * width])?;
                }
                Ok(())
            },
        )
    }
    fn data(
        &mut self,
        bytes: u64,
        frames: u64,
        payload: impl FnOnce(&mut W) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if bytes >= u64::from(u32::MAX) {
            return Err(Error::Limit("wavl child data size"));
        }
        let e = self.options.container.endian();
        self.write_with(8 + bytes + (bytes & 1), frames, |w| {
            w.write_all(b"data")?;
            write32(w, bytes as u32, e)?;
            payload(w)?;
            if bytes & 1 != 0 {
                w.write_all(&[0])?;
            }
            Ok(())
        })
    }
    fn check(&self) -> Result<(), Error> {
        if self.failed {
            Err(Error::Invalid("wavl writer unusable after I/O error"))
        } else {
            Ok(())
        }
    }
    fn write_with(
        &mut self,
        bytes: u64,
        frames: u64,
        write: impl FnOnce(&mut W) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.check()?;
        let bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(Error::Limit("wavl byte count overflow"))?;
        let frames = self
            .frames
            .checked_add(frames)
            .ok_or(Error::Limit("wavl frame count overflow"))?;
        validate_fact_count(self.options.container, frames)?;
        if self
            .length
            .is_some_and(|v| bytes > v.bytes || frames > v.frames)
        {
            return Err(Error::Invalid("declared wavl length exceeded"));
        }
        let size = (self.list_header - self.base)
            .checked_add(bytes)
            .ok_or(Error::Limit("wavl file size overflow"))?;
        self.base
            .checked_add(8)
            .and_then(|v| v.checked_add(size))
            .ok_or(Error::Limit("wavl file offset overflow"))?;
        if !self.options.container.is_64()
            && !(self.options.container == Container::Riff && self.options.auto_rf64)
            && large(size, bytes, frames)
        {
            return Err(Error::Limit("32-bit RIFF size or sample count"));
        }
        self.failed = true;
        write(&mut self.inner)?;
        self.bytes = bytes;
        self.frames = frames;
        self.failed = false;
        Ok(())
    }
}
fn large(size: u64, list: u64, frames: u64) -> bool {
    size >= u64::from(u32::MAX) || list >= u64::from(u32::MAX) || frames >= u64::from(u32::MAX)
}
fn ds64(container: Container, size: u64, list: u64, frames: u64) -> [u8; 40] {
    let mut b = [0; 40];
    b[..8].copy_from_slice(&size.to_le_bytes());
    if container == Container::Rf64 {
        b[16..24].copy_from_slice(&frames.to_le_bytes());
    }
    b[24..28].copy_from_slice(&1u32.to_le_bytes());
    b[28..32].copy_from_slice(b"LIST");
    b[32..].copy_from_slice(&list.to_le_bytes());
    b
}
struct Header {
    format: [u8; 40],
    format_len: usize,
    reserve: bool,
    list_header: u64,
    fact_offset: u64,
}
impl Header {
    fn new(
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
        reserve: bool,
    ) -> Result<Self, Error> {
        let (format, format_len) =
            spec.header(options.container.endian(), options.force_extensible)?;
        let mut bytes =
            12 + if reserve { 48 } else { 0 } + 8 + format_len as u64 + (format_len & 1) as u64;
        let fact_offset = bytes + 8;
        bytes += 12;
        for c in metadata {
            if [
                *b"fmt ", *b"data", *b"fact", *b"ds64", *b"RIFF", *b"RIFX", *b"RF64", *b"BW64",
            ]
            .contains(&c.id)
                || (c.id == *b"LIST" && c.data.starts_with(b"wavl"))
            {
                return Err(Error::Invalid("reserved metadata chunk ID"));
            }
            validate_chunk(c.data)?;
            bytes = bytes
                .checked_add(8 + c.data.len() as u64 + (c.data.len() & 1) as u64)
                .ok_or(Error::Limit("metadata size overflow"))?;
        }
        Ok(Self {
            format,
            format_len,
            reserve,
            list_header: bytes,
            fact_offset,
        })
    }
    fn write(
        &self,
        inner: &mut impl Write,
        container: Container,
        metadata: &[RawChunk<'_>],
        length: Option<DataLength>,
    ) -> Result<(), Error> {
        let e = container.endian();
        let is64 = container.is_64();
        inner.write_all(container_id(container))?;
        write32(
            inner,
            if is64 {
                u32::MAX
            } else {
                length.map_or(0, |v| (self.list_header + v.bytes) as u32)
            },
            e,
        )?;
        inner.write_all(b"WAVE")?;
        if self.reserve {
            chunk(
                inner,
                if is64 { b"ds64" } else { b"JUNK" },
                &if is64 {
                    length.map_or_else(
                        || ds64(container, 0, 0, 0),
                        |v| ds64(container, self.list_header + v.bytes, v.bytes, v.frames),
                    )
                } else {
                    [0; 40]
                },
                e,
            )?;
        }
        chunk(inner, b"fmt ", &self.format[..self.format_len], e)?;
        let mut fact = [0; 4];
        put32(
            &mut fact,
            length.map_or(0, |v| v.frames.min(u64::from(u32::MAX)) as u32),
            e,
        );
        chunk(inner, b"fact", &fact, e)?;
        for c in metadata {
            chunk(inner, &c.id, c.data, e)?;
        }
        inner.write_all(b"LIST")?;
        write32(
            inner,
            if is64 {
                u32::MAX
            } else {
                length.map_or(0, |v| v.bytes as u32)
            },
            e,
        )?;
        inner.write_all(b"wavl")?;
        Ok(())
    }
}
