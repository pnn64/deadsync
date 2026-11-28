use super::encode_rows::Rows;
use crate::{
    ChunkPosition, ColorType, EncodeOptions, Error, Filter, Png,
    checksum::Crc32,
    codec::{Header, SIGNATURE, ancillary_writable, validate_encoding, write_transparency},
    reserve,
    scanline::{ImageView, SampleView},
    zlib::{Deflater, StreamStatus, png_zero_run},
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
    encoder: &'a mut Deflater,
    output: &'a mut Vec<u8>,
    have: usize,
}
impl<'a, W: Write> Compressed<'a, W> {
    fn new(
        writer: &'a mut Count<W>,
        sequence: Option<&'a mut u32>,
        options: &EncodeOptions,
        rows: &Rows<'_>,
        output: &'a mut Vec<u8>,
        encoder: &'a mut Option<Deflater>,
    ) -> Result<Self, Error> {
        let encoder = match encoder {
            Some(encoder) => {
                encoder.reset();
                encoder
            }
            None => {
                let adaptive = options.filter == Filter::Adaptive;
                let depth = rows.bit_depth();
                let filtered_bytes = rows.filtered_size()?;
                let mut capacity = options.idat_chunk_size.min(65536);
                if png_zero_run(options.compression, adaptive, depth) {
                    capacity = capacity.min(filtered_bytes.saturating_add(64));
                }
                reserve(output, capacity)?;
                output.resize(capacity, 0);
                encoder.insert(Deflater::for_png(
                    options.compression,
                    adaptive,
                    depth,
                    filtered_bytes,
                )?)
            }
        };
        Ok(Self {
            writer,
            sequence,
            encoder,
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
    fn pump(&mut self, input: &[u8], finish: bool) -> Result<(), Error> {
        self.pump_input(Some(input), input.len(), finish)
    }
    fn pump_zeros(&mut self, len: usize) -> Result<(), Error> {
        self.pump_input(None, len, false)
    }
    fn pump_input(
        &mut self,
        mut input: Option<&[u8]>,
        mut len: usize,
        finish: bool,
    ) -> Result<(), Error> {
        loop {
            let output = &mut self.output[self.have..];
            let p = match input {
                Some(bytes) => self.encoder.process(bytes, output, finish)?,
                None => self.encoder.process_zeros(len, output)?,
            };
            self.have += p.written;
            if self.have == self.output.len() {
                self.flush()?;
            }
            match p.status {
                StreamStatus::NeedInput if !finish => {
                    debug_assert_eq!(p.consumed, len);
                    return Ok(());
                }
                StreamStatus::Done if finish => {
                    debug_assert_eq!(p.consumed, len);
                    self.flush()?;
                    return Ok(());
                }
                StreamStatus::NeedOutput => {
                    if let Some(bytes) = &mut input {
                        *bytes = &bytes[p.consumed..];
                    }
                    len -= p.consumed;
                }
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
    rows: &mut Rows<'_>,
    options: &EncodeOptions,
    sequence: Option<&mut u32>,
    output: &mut Vec<u8>,
    encoder: &mut Option<Deflater>,
) -> Result<usize, Error> {
    let mut compressed = Compressed::new(writer, sequence, options, rows, output, encoder)?;
    while let Some(row) = rows.next_row()? {
        if row.zero {
            let prefix = usize::from(row.bytes[0] != 0);
            if prefix != 0 {
                compressed.pump(&row.bytes[..prefix], false)?;
            }
            compressed.pump_zeros(row.bytes.len() - prefix)?;
        } else {
            compressed.pump(row.bytes, false)?;
        }
    }
    // Capacity only grows while rows are processed. Measure it at the last
    // row, before flushing the final deflate block.
    let peak = compressed
        .memory_bytes()
        .saturating_add(rows.memory_bytes());
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
    write_transparency(png, |bytes| chunk(&mut out, b"tRNS", &[bytes]))?;
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
    let mut output = Vec::new();
    let mut encoder = None;
    let mut rows = Rows::new(&png.image, options.interlaced, options.filter)?;
    let mut peak = image(
        &mut out,
        &mut rows,
        options,
        None,
        &mut output,
        &mut encoder,
    )?;
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
            rows.reuse_for_writer(pixels)?;
            peak = peak.max(image(
                &mut out,
                &mut rows,
                options,
                Some(&mut sequence),
                &mut output,
                &mut encoder,
            )?);
        }
    }
    ancillary(&mut out, png, ChunkPosition::AfterData)?;
    chunk(&mut out, b"IEND", &[])?;
    Ok(EncodeStats {
        bytes_written: out.bytes,
        peak_image_buffer_bytes: peak,
    })
}

/// Encode a packed RGBA8 raster without copying the caller's pixels.
pub fn encode_rgba8_writer(
    width: u32,
    height: u32,
    pixels: &[u8],
    options: &EncodeOptions,
    writer: impl Write,
) -> Result<EncodeStats, Error> {
    let header = Header {
        width,
        height,
        color: ColorType::Rgba,
        depth: 8,
        interlaced: options.interlaced,
    };
    if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff
        || options.idat_chunk_size == 0 || options.idat_chunk_size > 0x7fff_ffff
        || header.sample_bytes()? != pixels.len()
    {
        return Err(Error::Invalid("encoding dimensions, samples, or chunk size"));
    }
    let view = ImageView {
        width,
        height,
        color_type: ColorType::Rgba,
        bit_depth: 8,
        samples: SampleView::U8(pixels),
    };
    let mut rows = Rows::from_view(view, options.interlaced, options.filter)?;
    let mut out = Count { writer, bytes: 0 };
    out.write_all(SIGNATURE)?;
    chunk(&mut out, b"IHDR", &[&header.wire()])?;
    let peak = image(&mut out, &mut rows, options, None, &mut Vec::new(), &mut None)?;
    chunk(&mut out, b"IEND", &[])?;
    Ok(EncodeStats {
        bytes_written: out.bytes,
        peak_image_buffer_bytes: peak,
    })
}
