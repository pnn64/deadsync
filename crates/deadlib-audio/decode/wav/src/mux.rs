//! Codec-independent WAVE output. The producer supplies encoded bytes
//! and their decoded frame count; codec-specific block rules stay with the codec.
use crate::{Container, Encoding, Endian, Error, Format, RawChunk, WriteOptions};
use std::io::{Seek, SeekFrom, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataLength {
    pub bytes: u64,
    pub frames: u64,
}

pub struct Muxer<W> {
    inner: W,
    options: WriteOptions,
    base: u64,
    data_header: u64,
    fact_offset: Option<u64>,
    native_align: Option<u16>,
    bytes: u64,
    frames: u64,
    length: Option<DataLength>,
    failed: bool,
}
impl<W: Write + Seek> Muxer<W> {
    /// Preserve the supplied fmt payload exactly. `force_extensible` applies to
    /// the native Writer's Spec; this generic muxer never rewrites codec headers.
    pub fn new(
        mut inner: W,
        format: &[u8],
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
    ) -> Result<Self, Error> {
        let reserve_64 = options.container.is_64()
            || (options.auto_rf64 && options.container == Container::Riff);
        let plan = HeaderPlan::new(format, options, metadata, reserve_64)?;
        if !reserve_64 && plan.bytes - 8 >= u64::from(u32::MAX) {
            return Err(Error::Limit("32-bit RIFF size"));
        }
        let base = inner.stream_position()?;
        base.checked_add(plan.bytes)
            .ok_or(Error::Limit("file offset overflow"))?;
        write_header(&mut inner, format, options, metadata, &plan, None)?;
        Ok(Self {
            inner,
            options,
            base,
            data_header: base + plan.bytes - 8,
            fact_offset: plan.fact_offset.map(|v| base + v),
            native_align: plan.native_align,
            bytes: 0,
            frames: 0,
            length: None,
            failed: false,
        })
    }
    /// Writes odd padding and patches sizes. Does not truncate the underlying sink.
    pub fn finish(mut self) -> Result<W, Error> {
        if self.length.is_some() {
            return self.finish_stream();
        }
        if self.failed {
            return Err(Error::Invalid("writer unusable after I/O error"));
        }
        let e = self.options.container.endian();
        if self.bytes & 1 != 0 {
            self.inner.write_all(&[0])?;
        }
        let end = self.inner.stream_position()?;
        let size = end
            .checked_sub(self.base)
            .and_then(|n| n.checked_sub(8))
            .ok_or(Error::Invalid("sink position"))?;
        let large = is_large(size, self.bytes, self.frames, self.fact_offset.is_some());
        let is64 = self.options.container.is_64() || large;
        if large
            && !self.options.container.is_64()
            && !(self.options.auto_rf64 && self.options.container == Container::Riff)
        {
            return Err(Error::Limit("32-bit RIFF size or sample count"));
        }
        let container = if is64 && self.options.container != Container::Bw64 {
            Container::Rf64
        } else {
            self.options.container
        };
        self.inner.seek(SeekFrom::Start(self.base))?;
        self.inner.write_all(container_id(container))?;
        write32(
            &mut self.inner,
            if is64 { u32::MAX } else { size as u32 },
            e,
        )?;
        if is64 {
            self.inner.seek(SeekFrom::Start(self.base + 12))?;
            let d = size64(size, self.bytes, self.frames);
            chunk(&mut self.inner, b"ds64", &d, Endian::Little)?;
        }
        if let Some(pos) = self.fact_offset {
            self.inner.seek(SeekFrom::Start(pos))?;
            write32(
                &mut self.inner,
                self.frames.min(u64::from(u32::MAX)) as u32,
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
impl<W: Write> Muxer<W> {
    /// Emit final sizes up front for a nonseekable sink. No JUNK reservation is
    /// needed: RIFF promotes to RF64 before writing if auto_rf64 permits it.
    /// Every successful write must stay within both declared counts; finalizing
    /// requires exact counts. Encoded block validity remains the producer's job.
    pub fn with_lengths(
        mut inner: W,
        format: &[u8],
        mut options: WriteOptions,
        metadata: &[RawChunk<'_>],
        length: DataLength,
    ) -> Result<Self, Error> {
        let mut plan = HeaderPlan::new(format, options, metadata, options.container.is_64())?;
        if let Some(align) = plan.native_align
            && length.frames.checked_mul(u64::from(align)) != Some(length.bytes)
        {
            return Err(Error::Invalid("native data/frame count mismatch"));
        }
        let mut size = form_size(plan.bytes, length.bytes)?;
        if !options.container.is_64()
            && is_large(
                size,
                length.bytes,
                length.frames,
                plan.fact_offset.is_some(),
            )
        {
            if options.container != Container::Riff || !options.auto_rf64 {
                return Err(Error::Limit("32-bit RIFF size or sample count"));
            }
            options.container = Container::Rf64;
            plan = HeaderPlan::new(format, options, metadata, true)?;
            size = form_size(plan.bytes, length.bytes)?;
        }
        // Ensure the complete form (not only ckSize) fits u64 before any I/O.
        size.checked_add(8)
            .ok_or(Error::Limit("file size overflow"))?;
        write_header(
            &mut inner,
            format,
            options,
            metadata,
            &plan,
            Some((length, size)),
        )?;
        Ok(Self {
            inner,
            options,
            base: 0,
            data_header: plan.bytes - 8,
            fact_offset: plan.fact_offset,
            native_align: plan.native_align,
            bytes: 0,
            frames: 0,
            length: Some(length),
            failed: false,
        })
    }
    pub fn expected_length(&self) -> Option<DataLength> {
        self.length
    }
    pub fn bytes_written(&self) -> u64 {
        self.bytes
    }
    pub fn frames_written(&self) -> u64 {
        self.frames
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
    /// The caller is responsible for codec block validity and decoded counts.
    /// Native PCM/float byte counts are additionally checked against frame count.
    pub fn write_data(&mut self, bytes: &[u8], decoded_frames: u64) -> Result<(), Error> {
        self.write_with(bytes.len() as u64, decoded_frames, |inner| {
            inner.write_all(bytes)?;
            Ok(())
        })
    }
    pub(crate) fn write_with(
        &mut self,
        bytes: u64,
        frames: u64,
        write: impl FnOnce(&mut W) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("writer unusable after I/O error"));
        }
        if let Some(align) = self.native_align
            && frames.checked_mul(u64::from(align)) != Some(bytes)
        {
            return Err(Error::Invalid("native data/frame count mismatch"));
        }
        let count = self
            .bytes
            .checked_add(bytes)
            .ok_or(Error::Limit("data size overflow"))?;
        let frames = self
            .frames
            .checked_add(frames)
            .ok_or(Error::Limit("sample count overflow"))?;
        if let Some(length) = self.length
            && (count > length.bytes || frames > length.frames)
        {
            return Err(Error::Invalid("declared WAVE data length exceeded"));
        }
        let size = (self.data_header - self.base)
            .checked_add(count)
            .and_then(|v| v.checked_add(count & 1))
            .ok_or(Error::Limit("file size overflow"))?;
        self.base
            .checked_add(8)
            .and_then(|v| v.checked_add(size))
            .ok_or(Error::Limit("file offset overflow"))?;
        if !self.options.container.is_64()
            && !(self.options.auto_rf64 && self.options.container == Container::Riff)
            && (size >= u64::from(u32::MAX)
                || count >= u64::from(u32::MAX)
                || (self.fact_offset.is_some() && frames >= u64::from(u32::MAX)))
        {
            return Err(Error::Limit("32-bit RIFF size or sample count"));
        }
        self.failed = true;
        write(&mut self.inner)?;
        self.bytes = count;
        self.frames = frames;
        self.failed = false;
        Ok(())
    }
    /// Finalize known-length output without Seek. Underfilled data is rejected
    /// before padding or flushing; an I/O failure leaves a partial sink.
    pub fn finish_stream(mut self) -> Result<W, Error> {
        if self.failed {
            return Err(Error::Invalid("writer unusable after I/O error"));
        }
        let expected = self.length.ok_or(Error::Unsupported(
            "unknown-length output requires seek finalization",
        ))?;
        if expected
            != (DataLength {
                bytes: self.bytes,
                frames: self.frames,
            })
        {
            return Err(Error::Invalid("incomplete declared WAVE data length"));
        }
        if self.bytes & 1 != 0 {
            self.inner.write_all(&[0])?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }
}
struct HeaderPlan {
    bytes: u64,
    fact_offset: Option<u64>,
    native_align: Option<u16>,
    reserve_64: bool,
}
impl HeaderPlan {
    fn new(
        format: &[u8],
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
        reserve_64: bool,
    ) -> Result<Self, Error> {
        validate_chunk(format)?;
        let f = Format::parse(format, options.container.endian())?;
        let native_align =
            matches!(f.encoding, Encoding::Pcm | Encoding::Float).then_some(f.block_align);
        let mut bytes = 12
            + if reserve_64 { 36 } else { 0 }
            + 8
            + format.len() as u64
            + (format.len() & 1) as u64;
        let fact_offset = if f.encoding != Encoding::Pcm {
            let pos = bytes + 8;
            bytes += 12;
            Some(pos)
        } else {
            None
        };
        for c in metadata {
            if [
                *b"fmt ", *b"data", *b"fact", *b"ds64", *b"RIFF", *b"RF64", *b"BW64", *b"RIFX",
            ]
            .contains(&c.id)
            {
                return Err(Error::Invalid("reserved metadata chunk ID"));
            }
            validate_chunk(c.data)?;
            bytes = bytes
                .checked_add(8 + c.data.len() as u64 + (c.data.len() & 1) as u64)
                .ok_or(Error::Limit("metadata sizes overflow"))?;
        }
        bytes = bytes
            .checked_add(8)
            .ok_or(Error::Limit("header size overflow"))?;
        Ok(Self {
            bytes,
            fact_offset,
            native_align,
            reserve_64,
        })
    }
}
fn form_size(header_bytes: u64, data_bytes: u64) -> Result<u64, Error> {
    header_bytes
        .checked_add(data_bytes)
        .and_then(|v| v.checked_add(data_bytes & 1))
        .and_then(|v| v.checked_sub(8))
        .ok_or(Error::Limit("file size overflow"))
}
fn is_large(size: u64, bytes: u64, frames: u64, fact: bool) -> bool {
    size >= u64::from(u32::MAX)
        || bytes >= u64::from(u32::MAX)
        || (fact && frames >= u64::from(u32::MAX))
}
fn size64(size: u64, bytes: u64, frames: u64) -> [u8; 28] {
    let mut d = [0; 28];
    d[..8].copy_from_slice(&size.to_le_bytes());
    d[8..16].copy_from_slice(&bytes.to_le_bytes());
    d[16..24].copy_from_slice(&frames.to_le_bytes());
    d
}
fn write_header(
    inner: &mut impl Write,
    format: &[u8],
    options: WriteOptions,
    metadata: &[RawChunk<'_>],
    plan: &HeaderPlan,
    known: Option<(DataLength, u64)>,
) -> Result<(), Error> {
    let e = options.container.endian();
    let is64 = options.container.is_64();
    inner.write_all(container_id(options.container))?;
    write32(
        inner,
        if is64 {
            u32::MAX
        } else {
            known.map_or(0, |(_, size)| size as u32)
        },
        e,
    )?;
    inner.write_all(b"WAVE")?;
    if plan.reserve_64 {
        let d = known.filter(|_| is64).map_or([0; 28], |(length, size)| {
            size64(size, length.bytes, length.frames)
        });
        chunk(inner, if is64 { b"ds64" } else { b"JUNK" }, &d, e)?;
    }
    chunk(inner, b"fmt ", format, e)?;
    if plan.fact_offset.is_some() {
        let frames = known.map_or(0, |(length, _)| {
            length.frames.min(u64::from(u32::MAX)) as u32
        });
        let mut b = [0; 4];
        put32(&mut b, frames, e);
        chunk(inner, b"fact", &b, e)?;
    }
    for c in metadata {
        chunk(inner, &c.id, c.data, e)?;
    }
    inner.write_all(b"data")?;
    write32(
        inner,
        if is64 {
            u32::MAX
        } else {
            known.map_or(0, |(length, _)| length.bytes as u32)
        },
        e,
    )
}
pub(crate) fn container_id(container: Container) -> &'static [u8; 4] {
    match container {
        Container::Riff => b"RIFF",
        Container::Rifx => b"RIFX",
        Container::Rf64 => b"RF64",
        Container::Bw64 => b"BW64",
    }
}
pub(crate) fn validate_chunk(data: &[u8]) -> Result<(), Error> {
    if data.len() as u64 >= u64::from(u32::MAX) {
        Err(Error::Limit("header or metadata chunk size"))
    } else {
        Ok(())
    }
}
pub(crate) fn put16(b: &mut [u8], v: u16, e: Endian) {
    b[..2].copy_from_slice(&match e {
        Endian::Little => v.to_le_bytes(),
        Endian::Big => v.to_be_bytes(),
    });
}
pub(crate) fn put32(b: &mut [u8], v: u32, e: Endian) {
    b[..4].copy_from_slice(&match e {
        Endian::Little => v.to_le_bytes(),
        Endian::Big => v.to_be_bytes(),
    });
}
pub(crate) fn write32(w: &mut impl Write, v: u32, e: Endian) -> Result<(), Error> {
    let mut b = [0; 4];
    put32(&mut b, v, e);
    w.write_all(&b)?;
    Ok(())
}
pub(crate) fn chunk(w: &mut impl Write, id: &[u8; 4], data: &[u8], e: Endian) -> Result<(), Error> {
    w.write_all(id)?;
    write32(w, data.len() as u32, e)?;
    w.write_all(data)?;
    if data.len() & 1 != 0 {
        w.write_all(&[0])?;
    }
    Ok(())
}
