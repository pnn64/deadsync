use crate::{
    Error,
    decode::{interlace_rows, validate_text},
    lzw,
    model::*,
    reserve,
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
    writer: W,
    header: Header,
    options: EncodeOptions,
    lzw: lzw::Encoder,
    compressed: Vec<u8>,
    interlace: Vec<u8>,
    failed: bool,
}
impl<W: Write> Encoder<W> {
    pub fn new(mut writer: W, header: Header, options: EncodeOptions) -> Result<Self, Error> {
        header.validate()?;
        if options.image_sub_block_size == 0 {
            return Err(Error::Invalid("zero image sub-block size"));
        }
        if header.version == Version::Gif87a && (header.sorted || header.pixel_aspect_ratio != 0) {
            return Err(Error::Invalid("GIF89a fields in GIF87a header"));
        }
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
            write_palette(&mut writer, p)?;
        }
        Ok(Self {
            writer,
            header,
            options,
            lzw: lzw::Encoder::new(),
            compressed: Vec::new(),
            interlace: Vec::new(),
            failed: false,
        })
    }
    pub fn write_block(&mut self, block: &Block) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is failed"));
        }
        let result = match block {
            Block::Frame(f) => self.frame(f),
            Block::Extension(e) => self.extension(e),
        };
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn finish(mut self) -> Result<W, Error> {
        if self.failed {
            return Err(Error::Invalid("encoder is failed"));
        }
        self.writer.write_all(&[0x3b])?;
        Ok(self.writer)
    }
    fn require89(&self) -> Result<(), Error> {
        if self.header.version == Version::Gif87a {
            Err(Error::Invalid("GIF89a block in GIF87a stream"))
        } else {
            Ok(())
        }
    }
    fn control(&mut self, control: Option<GraphicControl>) -> Result<(), Error> {
        if let Some(c) = control {
            self.require89()?;
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
    fn frame(&mut self, f: &Frame) -> Result<(), Error> {
        f.validate(&self.header, true)?;
        if f.sorted {
            self.require89()?;
        }
        if f.control.is_some() {
            self.require89()?;
        }
        // Derive the minimum from actual indices; unused palette colors need
        // not increase the code width. No indices or palette entries change.
        let maximum = f.indices.iter().copied().max().unwrap_or(0);
        let minimum = (8 - maximum.leading_zeros() as u8).max(2);
        self.compressed.clear();
        if f.interlaced {
            self.interlace.clear();
            reserve(&mut self.interlace, f.indices.len())?;
            let w = f.width as usize;
            for y in interlace_rows(f.height as usize) {
                self.interlace
                    .extend_from_slice(&f.indices[y * w..(y + 1) * w]);
            }
            self.lzw
                .encode(&self.interlace, minimum, &mut self.compressed)?;
        } else {
            self.lzw.encode(&f.indices, minimum, &mut self.compressed)?;
        }
        self.control(f.control)?;
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
            write_palette(&mut self.writer, p)?;
        }
        self.writer.write_all(&[minimum])?;
        for part in self
            .compressed
            .chunks(self.options.image_sub_block_size as usize)
        {
            self.writer.write_all(&[part.len() as u8])?;
            self.writer.write_all(part)?;
        }
        self.writer.write_all(&[0])?;
        Ok(())
    }
    fn extension(&mut self, extension: &Extension) -> Result<(), Error> {
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
                self.require89()?;
                self.writer.write_all(&[0x21, 0xfe])?;
            }
            Extension::Application {
                identifier,
                authentication,
                ..
            } => {
                self.require89()?;
                self.writer.write_all(&[0x21, 0xff, 11])?;
                self.writer.write_all(identifier)?;
                self.writer.write_all(authentication)?;
            }
            Extension::PlainText(text) => {
                self.require89()?;
                validate_text(text, &self.header, None, true)?;
                self.control(text.control)?;
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
                self.control(*control)?;
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
fn write_palette<W: Write>(writer: &mut W, p: &[[u8; 3]]) -> Result<(), Error> {
    let mut data = [0; 768];
    for (dest, src) in data.as_chunks_mut::<3>().0.iter_mut().zip(p) {
        dest.copy_from_slice(src);
    }
    writer.write_all(&data[..p.len() * 3])?;
    Ok(())
}
pub fn encode(gif: &Gif, options: &EncodeOptions) -> Result<Vec<u8>, Error> {
    encode_into(gif, options, Vec::new())
}
pub fn encode_into<W: Write>(gif: &Gif, options: &EncodeOptions, writer: W) -> Result<W, Error> {
    let mut encoder = Encoder::new(writer, gif.header.clone(), options.clone())?;
    for block in &gif.blocks {
        encoder.write_block(block)?;
    }
    encoder.finish()
}
