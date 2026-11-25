use crate::{
    Error,
    decode::{interlace_rows, validate_text},
    lzw,
    model::*,
};
use std::io::Write;

#[derive(Clone, Debug)]
pub struct EncodeOptions {
    /// 1..=255, useful for exercising streaming and sub-block boundaries.
    pub image_sub_block_size: u8,
}
impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            image_sub_block_size: 255,
        }
    }
}

/// Incremental writer. Input frames are already indexed; no quantization,
/// palette remapping or RGBA conversion ever happens. `finish` writes a trailer.
pub struct Encoder<W: Write> {
    output: EncoderOutput<W>,
    header: Header,
}
impl<W: Write> Encoder<W> {
    pub fn new(writer: W, header: Header, options: EncodeOptions) -> Result<Self, Error> {
        validate_header(&header, &options)?;
        let mut output = EncoderOutput::new(writer, options);
        output.write_header(&header)?;
        Ok(Self { output, header })
    }
    pub fn write_block(&mut self, block: &Block) -> Result<(), Error> {
        self.output.write_block(&self.header, block)
    }
    pub fn finish(mut self) -> Result<W, Error> {
        self.output.write_trailer()?;
        Ok(self.output.writer)
    }
}

struct EncoderOutput<W: Write> {
    writer: W,
    options: EncodeOptions,
    failed: bool,
}
impl<W: Write> EncoderOutput<W> {
    fn new(writer: W, options: EncodeOptions) -> Self {
        Self {
            writer,
            options,
            failed: false,
        }
    }
    fn write_header(&mut self, header: &Header) -> Result<(), Error> {
        let writer = &mut self.writer;
        writer.write_all(match header.version {
            Version::Gif87a => b"GIF87a",
            Version::Gif89a => b"GIF89a",
        })?;
        writer.write_all(&header.width.to_le_bytes())?;
        writer.write_all(&header.height.to_le_bytes())?;
        let size = header
            .global_palette
            .as_ref()
            .map_or(0, |p| (p.len().ilog2() - 1) as u8);
        writer.write_all(&[
            u8::from(header.global_palette.is_some()) << 7
                | (header.color_resolution - 1) << 4
                | u8::from(header.sorted) << 3
                | size,
            header.background_index,
            header.pixel_aspect_ratio,
        ])?;
        if let Some(p) = &header.global_palette {
            writer.write_all(p.as_flattened())?;
        }
        Ok(())
    }
    fn write_block(&mut self, header: &Header, block: &Block) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is failed"));
        }
        let result = match block {
            Block::Frame(f) => self.frame(header, f),
            Block::Extension(e) => self.extension(header, e),
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn write_trailer(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is failed"));
        }
        self.writer.write_all(&[0x3b])?;
        Ok(())
    }
    fn require89(version: Version) -> Result<(), Error> {
        if version == Version::Gif87a {
            Err(Error::Invalid("GIF89a block in GIF87a stream"))
        } else {
            Ok(())
        }
    }
    fn control(&mut self, version: Version, control: Option<GraphicControl>) -> Result<(), Error> {
        if let Some(c) = control {
            Self::require89(version)?;
            self.writer.write_all(&[
                0x21,
                0xf9,
                4,
                (c.disposal as u8) << 2
                    | u8::from(c.user_input) << 1
                    | u8::from(c.transparent_index.is_some()),
                c.delay as u8,
                (c.delay >> 8) as u8,
                c.transparent_index.unwrap_or(0),
                0,
            ])?;
        }
        Ok(())
    }
    fn frame(&mut self, header: &Header, f: &Frame) -> Result<(), Error> {
        let maximum = f.validate::<true>(
            (header.width, header.height),
            header.global_palette.as_ref().map(Vec::len),
            true,
        )?;
        if f.sorted || f.control.is_some() {
            Self::require89(header.version)?;
        }
        // Derive the minimum from actual indices; unused palette colors need
        // not increase the code width. No indices or palette entries change.
        let minimum = (8 - maximum.leading_zeros() as u8).max(2);
        self.control(header.version, f.control)?;
        self.writer.write_all(&[0x2c])?;
        for n in [f.left, f.top, f.width, f.height] {
            self.writer.write_all(&n.to_le_bytes())?;
        }
        let size = f
            .local_palette
            .as_ref()
            .map_or(0, |p| (p.len().ilog2() - 1) as u8);
        self.writer
            .write_all(&[u8::from(f.local_palette.is_some()) << 7
                | u8::from(f.interlaced) << 6
                | u8::from(f.sorted) << 5
                | size])?;
        if let Some(p) = &f.local_palette {
            self.writer.write_all(p.as_flattened())?;
        }
        self.writer.write_all(&[minimum])?;
        let size = self.options.image_sub_block_size;
        let capacity = usize::from(size);
        // Pad for whole-word stores and for loading the tail after a drain.
        let mut buffer = [0; 255 + 2 * size_of::<u64>()];
        let writer = &mut self.writer;
        let mut filled = 0;
        let emit = |bits: u64, count: usize| {
            buffer[filled..filled + size_of::<u64>()].copy_from_slice(&bits.to_le_bytes());
            filled += count;
            if filled >= capacity {
                if capacity == 1 {
                    for byte in buffer[..filled].as_chunks::<1>().0 {
                        writer.write_all(&[1])?;
                        writer.write_all(byte)?;
                    }
                    filled = 0;
                    return Ok(());
                }
                let mut written = 0;
                loop {
                    let end = written + capacity;
                    writer.write_all(&[size])?;
                    writer.write_all(&buffer[written..end])?;
                    written = end;
                    if filled - written < capacity {
                        break;
                    }
                }
                let tail = *buffer[written..].first_chunk::<8>().unwrap();
                buffer[..8].copy_from_slice(&tail);
                filled -= written;
            }
            Ok(())
        };
        if f.interlaced {
            let w = f.width as usize;
            let rows = interlace_rows(f.height as usize).map(|y| {
                let start = y * w;
                &f.indices[start..start + w]
            });
            lzw::Encoder::encode_rows(rows, &f.indices, minimum, emit)?;
        } else {
            lzw::Encoder::encode_rows(
                std::iter::once(f.indices.as_slice()),
                &f.indices,
                minimum,
                emit,
            )?;
        }
        if filled != 0 {
            writer.write_all(&[filled as u8])?;
            writer.write_all(&buffer[..filled])?;
        }
        writer.write_all(&[0])?;
        Ok(())
    }
    fn extension(&mut self, header: &Header, extension: &Extension) -> Result<(), Error> {
        // Validate all data before writing any part of this extension.
        let data = match extension {
            Extension::Comment(data)
            | Extension::Application { data, .. }
            | Extension::Unknown { data, .. } => data,
            Extension::PlainText(text) => &text.data,
        };
        if data.iter().any(|p| p.is_empty() || p.len() > 255) {
            return Err(Error::Invalid("extension sub-block size"));
        }
        match extension {
            Extension::Comment(_) => {
                Self::require89(header.version)?;
                self.writer.write_all(&[0x21, 0xfe])?;
            }
            Extension::Application {
                identifier,
                authentication,
                ..
            } => {
                Self::require89(header.version)?;
                self.writer.write_all(&[0x21, 0xff, 11])?;
                self.writer.write_all(identifier)?;
                self.writer.write_all(authentication)?;
            }
            Extension::PlainText(text) => {
                Self::require89(header.version)?;
                validate_text(
                    text,
                    (header.width, header.height),
                    header.global_palette.as_ref().map(Vec::len),
                    true,
                )?;
                self.control(header.version, text.control)?;
                self.writer.write_all(&[0x21, 1, 12])?;
                for n in [text.left, text.top, text.width, text.height] {
                    self.writer.write_all(&n.to_le_bytes())?;
                }
                self.writer.write_all(&[
                    text.cell_width,
                    text.cell_height,
                    text.foreground_index,
                    text.background_index,
                ])?;
            }
            Extension::Unknown { label, control, .. } => {
                if matches!(label, 0xf9 | 0xfe | 0xff | 1) {
                    return Err(Error::Invalid("known extension represented as unknown"));
                }
                if control.is_some() && !is_graphic_label(*label) {
                    return Err(Error::Invalid("graphic control on non-graphic extension"));
                }
                self.control(header.version, *control)?;
                self.writer.write_all(&[0x21, *label])?;
            }
        }
        for part in data {
            self.writer.write_all(&[part.len() as u8])?;
            self.writer.write_all(part)?;
        }
        self.writer.write_all(&[0])?;
        Ok(())
    }
}
fn validate_header(header: &Header, options: &EncodeOptions) -> Result<(), Error> {
    header.validate()?;
    if options.image_sub_block_size == 0 {
        return Err(Error::Invalid("zero image sub-block size"));
    }
    if header.version == Version::Gif87a && (header.sorted || header.pixel_aspect_ratio != 0) {
        return Err(Error::Invalid("GIF89a fields in GIF87a header"));
    }
    Ok(())
}

pub fn encode(gif: &Gif, options: &EncodeOptions) -> Result<Vec<u8>, Error> {
    validate_header(&gif.header, options)?;
    let palette_bytes = gif
        .header
        .global_palette
        .as_ref()
        .map_or(0, |p| p.len() * 3);
    let header_bytes = 13 + palette_bytes;
    let capacity = if gif.blocks.is_empty() {
        header_bytes + 1
    } else {
        // Match the capacity reached by the header's separate Vec writes so
        // that later payloads keep their existing growth pattern.
        header_bytes.max(if palette_bytes == 0 { 16 } else { 32 })
    };
    encode_validated(gif, options, Vec::with_capacity(capacity))
}
pub fn encode_into<W: Write>(gif: &Gif, options: &EncodeOptions, writer: W) -> Result<W, Error> {
    validate_header(&gif.header, options)?;
    encode_validated(gif, options, writer)
}
fn encode_validated<W: Write>(gif: &Gif, options: &EncodeOptions, writer: W) -> Result<W, Error> {
    let mut encoder = EncoderOutput::new(writer, options.clone());
    encoder.write_header(&gif.header)?;
    for block in &gif.blocks {
        encoder.write_block(&gif.header, block)?;
    }
    encoder.write_trailer()?;
    Ok(encoder.writer)
}
