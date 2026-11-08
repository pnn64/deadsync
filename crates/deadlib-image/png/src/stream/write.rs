use super::encode_rows::Rows;
use crate::{
    ChunkPosition, EncodeOptions, Error, Image, Png,
    checksum::Crc32,
    codec::{SIGNATURE, ancillary_writable, transparency_bytes, validate_encoding},
    reserve,
    zlib::{Deflater, StreamStatus},
};
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodeStats {
    pub bytes_written: usize,
    /// Peak image-pipeline storage, excluding caller input, metadata validation
    /// temporaries and the destination writer's own buffers.
    pub peak_image_buffer_bytes: usize,
}
struct Count<W> {
    writer: W,
    bytes: usize,
}
impl<W: Write> Write for Count<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(bytes)?;
        if n > bytes.len() {
            return Err(io::Error::other("writer returned invalid byte count"));
        }
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or_else(|| io::Error::other("encoded byte count overflow"))?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
fn chunk(out: &mut impl Write, kind: &[u8; 4], parts: &[&[u8]]) -> Result<(), Error> {
    let len = parts
        .iter()
        .try_fold(0usize, |n, part| n.checked_add(part.len()))
        .ok_or(Error::LimitExceeded)?;
    if len > 0x7fff_ffff {
        return Err(Error::LimitExceeded);
    }
    out.write_all(&(len as u32).to_be_bytes())?;
    out.write_all(kind)?;
    let mut crc = Crc32::new();
    crc.update(kind);
    for part in parts {
        out.write_all(part)?;
        crc.update(part);
    }
    out.write_all(&crc.finish().to_be_bytes())?;
    Ok(())
}
fn ancillary(out: &mut impl Write, png: &Png, position: ChunkPosition) -> Result<(), Error> {
    for c in png.ancillary.iter().filter(|c| c.position == position) {
        if ancillary_writable(c)? {
            chunk(
                out,
                &c.kind,
                &[&crate::metadata::encoding_data(c, &png.image)],
            )?;
        }
    }
    Ok(())
}
struct Compressed<'a, W> {
    writer: &'a mut Count<W>,
    sequence: Option<&'a mut u32>,
    encoder: Deflater,
    output: Vec<u8>,
    have: usize,
}
impl<'a, W: Write> Compressed<'a, W> {
    fn new(
        writer: &'a mut Count<W>,
        sequence: Option<&'a mut u32>,
        options: &EncodeOptions,
    ) -> Result<Self, Error> {
        let capacity = options.idat_chunk_size.min(65536);
        let mut output = Vec::new();
        reserve(&mut output, capacity)?;
        output.resize(capacity, 0);
        Ok(Self {
            writer,
            sequence,
            encoder: Deflater::new(options.compression)?,
            output,
            have: 0,
        })
    }
    fn flush(&mut self) -> Result<(), Error> {
        if self.have == 0 {
            return Ok(());
        }
        if let Some(sequence) = &mut self.sequence {
            if **sequence > 0x7fff_ffff {
                return Err(Error::LimitExceeded);
            }
            let prefix = sequence.to_be_bytes();
            chunk(self.writer, b"fdAT", &[&prefix, &self.output[..self.have]])?;
            **sequence += 1;
        } else {
            chunk(self.writer, b"IDAT", &[&self.output[..self.have]])?;
        }
        self.have = 0;
        Ok(())
    }
    fn pump(&mut self, mut input: &[u8], finish: bool) -> Result<(), Error> {
        loop {
            let p = self
                .encoder
                .process(input, &mut self.output[self.have..], finish)?;
            input = &input[p.consumed..];
            self.have += p.written;
            if self.have == self.output.len() {
                self.flush()?;
            }
            match p.status {
                StreamStatus::NeedInput if !finish && input.is_empty() => return Ok(()),
                StreamStatus::Done if finish && input.is_empty() => {
                    self.flush()?;
                    return Ok(());
                }
                StreamStatus::NeedOutput => (),
                _ => return Err(Error::Invalid("deflater progress")),
            }
        }
    }
    fn memory_bytes(&self) -> usize {
        self.encoder.working_memory_bytes() + self.output.capacity()
    }
}
fn image<W: Write>(
    writer: &mut Count<W>,
    image: &Image,
    options: &EncodeOptions,
    sequence: Option<&mut u32>,
) -> Result<usize, Error> {
    let mut rows = Rows::new(image, options.interlaced, options.filter)?;
    let mut compressed = Compressed::new(writer, sequence, options)?;
    let mut peak = compressed.memory_bytes();
    while let Some(row) = rows.next_row()? {
        compressed.pump(row, false)?;
        peak = peak.max(
            compressed
                .memory_bytes()
                .saturating_add(rows.memory_bytes()),
        );
    }
    compressed.pump(&[], true)?;
    Ok(peak)
}

/// Encode PNG/APNG to a possibly short-writing destination using bounded row,
/// compression and chunk buffers. `idat_chunk_size` is an upper bound; the
/// streaming implementation caps compressed chunks at 64 KiB. It never buffers
/// an entire filtered image, frame, compressed stream or output PNG. Writer
/// errors can leave partial output; flushing the destination is caller-owned.
pub fn encode_writer(
    png: &Png,
    options: &EncodeOptions,
    writer: impl Write,
) -> Result<EncodeStats, Error> {
    let header = validate_encoding(png, options)?;
    let mut out = Count { writer, bytes: 0 };
    out.write_all(SIGNATURE)?;
    chunk(&mut out, b"IHDR", &[&header.wire()])?;
    ancillary(&mut out, png, ChunkPosition::BeforePalette)?;
    if !png.palette.is_empty() {
        chunk(&mut out, b"PLTE", &[png.palette.as_flattened()])?;
    }
    if let Some(bytes) = transparency_bytes(png) {
        chunk(&mut out, b"tRNS", &[&bytes])?;
    }
    ancillary(&mut out, png, ChunkPosition::BeforeData)?;
    if let Some(a) = &png.animation {
        let mut control = [0; 8];
        control[..4].copy_from_slice(&(a.frames.len() as u32).to_be_bytes());
        control[4..].copy_from_slice(&a.plays.to_be_bytes());
        chunk(&mut out, b"acTL", &[&control])?;
        if a.default_is_frame {
            chunk(&mut out, b"fcTL", &[&a.frames[0].control.wire(0)])?;
        }
    }
    let mut peak = image(&mut out, &png.image, options, None)?;
    if let Some(a) = &png.animation {
        let mut sequence = u32::from(a.default_is_frame);
        for frame in &a.frames[usize::from(a.default_is_frame)..] {
            if sequence > 0x7fff_ffff {
                return Err(Error::LimitExceeded);
            }
            chunk(&mut out, b"fcTL", &[&frame.control.wire(sequence)])?;
            sequence += 1;
            let pixels = frame
                .image
                .as_ref()
                .ok_or(Error::Invalid("missing APNG frame"))?;
            peak = peak.max(image(&mut out, pixels, options, Some(&mut sequence))?);
        }
    }
    ancillary(&mut out, png, ChunkPosition::AfterData)?;
    chunk(&mut out, b"IEND", &[])?;
    Ok(EncodeStats {
        bytes_written: out.bytes,
        peak_image_buffer_bytes: peak,
    })
}
