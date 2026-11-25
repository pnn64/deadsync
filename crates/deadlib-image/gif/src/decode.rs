use crate::{Error, lzw, model::*, reserve};

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_frame_pixels: usize,
    pub max_total_pixels: usize,
    pub max_metadata_bytes: usize,
    pub max_blocks: usize,
    pub max_frames: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 256 << 20,
            max_frame_pixels: 64 << 20,
            max_total_pixels: 256 << 20,
            max_metadata_bytes: 16 << 20,
            max_blocks: 1_000_000,
            max_frames: 100_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DecodeOptions {
    pub limits: Limits,
    /// Check reserved bits, block versions, duplicate/dangling controls and
    /// the logical background index. Require LZW EOI and reject trailing bytes.
    /// With false, a terminated image may omit EOI only when its decoded sample
    /// count is exact, and monochrome streams may use a one-bit LZW minimum.
    /// Invalid/overflowing LZW output is never recovered.
    pub strict: bool,
    /// Reject rectangles extending beyond the logical screen. With false,
    /// raw indices are retained and the compositor clips to the screen.
    pub check_bounds: bool,
    /// Optional color table retained from a previous GIF data stream. Absence
    /// of every palette is legal for raw indexed decoding, never for rendering.
    pub external_palette: Option<Palette>,
}
impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            strict: true,
            check_bounds: true,
            external_palette: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Header,
    Block(Block),
    End,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Progress {
    pub consumed: usize,
    pub event: Option<Event>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Header,
    GlobalPalette,
    Block,
    Extension,
    Control,
    Application,
    PlainText,
    Image,
    LocalPalette,
    Minimum,
    ImageData,
    MetadataSize,
    MetadataData,
    Done,
    Failed,
}

struct FixedField {
    bytes: [u8; 768],
    filled: usize,
}

impl FixedField {
    #[inline(always)]
    fn read<'a>(
        &'a mut self,
        input: &'a [u8],
        at: &mut usize,
        size: usize,
        remaining: &mut usize,
    ) -> Result<Option<&'a [u8]>, Error> {
        let count = (size - self.filled).min(input.len() - *at);
        charge(remaining, count)?;
        let bytes = &input[*at..*at + count];
        *at += count;
        // Only fields that cross input chunks need the scratch buffer.
        if count == size {
            return Ok(Some(bytes));
        }
        let end = self.filled + count;
        self.bytes[self.filled..end].copy_from_slice(bytes);
        self.filled = end;
        Ok((end == size).then(|| &self.bytes[..size]))
    }
}

/// Push decoder. Each call consumes a prefix and returns at most one event.
/// Keep submitting the unconsumed suffix, even after an event. `finish` checks
/// that the trailer was received. No compressed image copy or input buffering
/// is needed beyond the current fixed-size header and data sub-block.
pub struct Decoder {
    options: DecodeOptions,
    state: State,
    header: Option<Header>,
    field: FixedField,
    // Palette size or bytes still needed in a sub-block.
    wanted: usize,
    pending: Option<GraphicControl>,
    frame: Frame,
    spare: Vec<u8>,
    interlace: Vec<u8>,
    extension: Option<Extension>,
    lzw: lzw::Decoder,
    remaining_input_bytes: usize,
    remaining_metadata_bytes: usize,
    remaining_blocks: usize,
    remaining_frames: usize,
    remaining_pixels: usize,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new(DecodeOptions::default())
    }
}
impl Decoder {
    pub fn new(options: DecodeOptions) -> Self {
        Self {
            state: State::Header,
            header: None,
            field: FixedField {
                bytes: [0; 768],
                filled: 0,
            },
            wanted: 0,
            pending: None,
            frame: Frame::default(),
            spare: Vec::new(),
            interlace: Vec::new(),
            extension: None,
            lzw: lzw::Decoder::new(),
            remaining_input_bytes: options.limits.max_input_bytes,
            remaining_metadata_bytes: options.limits.max_metadata_bytes,
            remaining_blocks: options.limits.max_blocks,
            remaining_frames: options.limits.max_frames,
            remaining_pixels: options.limits.max_total_pixels,
            options,
        }
    }
    pub fn header(&self) -> Option<&Header> {
        self.header.as_ref()
    }
    /// Reuse a returned frame's pixel allocation for a later image. Retained
    /// frames must not be recycled. Only one spare allocation is kept.
    pub fn recycle_frame(&mut self, frame: Frame) {
        if frame.indices.capacity() > self.spare.capacity() {
            self.spare = frame.indices;
        }
    }
    pub fn finish(&self) -> Result<(), Error> {
        if self.state == State::Done {
            Ok(())
        } else if self.state == State::Failed {
            Err(Error::Invalid("decoder is failed"))
        } else {
            Err(Error::Truncated)
        }
    }
    pub fn update(&mut self, input: &[u8]) -> Result<Progress, Error> {
        self.update_with_palette(input, None)
    }
    fn update_with_palette(
        &mut self,
        input: &[u8],
        external_palette: Option<&Palette>,
    ) -> Result<Progress, Error> {
        let result = self.process(input, external_palette);
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }
    fn transition(&mut self, state: State) {
        self.state = state;
        self.field.filled = 0;
    }
    fn byte(&mut self, input: &[u8], at: &mut usize) -> Result<Option<u8>, Error> {
        let Some(&byte) = input.get(*at) else {
            return Ok(None);
        };
        charge(&mut self.remaining_input_bytes, 1)?;
        *at += 1;
        Ok(Some(byte))
    }
    fn version89(&self) -> Result<(), Error> {
        if self.options.strict && self.header.as_ref().unwrap().version != Version::Gif89a {
            Err(Error::Invalid("GIF89a extension in GIF87a stream"))
        } else {
            Ok(())
        }
    }
    fn data_mut(extension: &mut Extension) -> &mut SubBlocks {
        match extension {
            Extension::Comment(data)
            | Extension::Application { data, .. }
            | Extension::Unknown { data, .. } => data,
            Extension::PlainText(text) => &mut text.data,
        }
    }
    fn process(
        &mut self,
        input: &[u8],
        external_palette: Option<&Palette>,
    ) -> Result<Progress, Error> {
        let mut at = 0;
        let event = 'events: loop {
            match self.state {
                State::Header => {
                    let Some(s) =
                        self.field
                            .read(input, &mut at, 13, &mut self.remaining_input_bytes)?
                    else {
                        break None;
                    };
                    let version = match &s[..6] {
                        b"GIF87a" => Version::Gif87a,
                        b"GIF89a" => Version::Gif89a,
                        _ => return Err(Error::Invalid("signature/version")),
                    };
                    if self.options.strict
                        && version == Version::Gif87a
                        && (s[10] & 8 != 0 || s[12] != 0)
                    {
                        return Err(Error::Invalid("GIF87a reserved screen fields"));
                    }
                    let header = Header {
                        version,
                        width: le(&s[6..]),
                        height: le(&s[8..]),
                        color_resolution: ((s[10] >> 4) & 7) + 1,
                        global_palette: None,
                        sorted: s[10] & 8 != 0,
                        background_index: s[11],
                        pixel_aspect_ratio: s[12],
                    };
                    header.validate()?;
                    if header.width as usize * header.height as usize
                        > self.options.limits.max_frame_pixels
                    {
                        return Err(Error::LimitExceeded);
                    }
                    if let Some(p) = external_palette.or(self.options.external_palette.as_ref()) {
                        validate_palette(p)?;
                    }
                    let size = if s[10] & 128 != 0 {
                        3usize << ((s[10] & 7) + 1)
                    } else {
                        0
                    };
                    self.header = Some(header);
                    if size != 0 {
                        self.transition(State::GlobalPalette);
                        self.wanted = size;
                    } else {
                        self.state = State::Block;
                        break Some(Event::Header);
                    }
                }
                State::GlobalPalette => {
                    let Some(s) = self.field.read(
                        input,
                        &mut at,
                        self.wanted,
                        &mut self.remaining_input_bytes,
                    )?
                    else {
                        break None;
                    };
                    let colors = palette(s)?;
                    let count = colors.len();
                    let header = self.header.as_mut().unwrap();
                    header.global_palette = Some(colors);
                    // The header is already valid, and the packed size defines a valid palette length.
                    if self.options.strict && usize::from(header.background_index) >= count {
                        return Err(Error::Invalid("background index outside global palette"));
                    }
                    self.state = State::Block;
                    break Some(Event::Header);
                }
                State::Block => {
                    let Some(byte) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    match byte {
                        0x2c => {
                            charge(&mut self.remaining_blocks, 1)?;
                            self.transition(State::Image);
                        }
                        0x21 => {
                            charge(&mut self.remaining_blocks, 1)?;
                            self.state = State::Extension;
                        }
                        0x3b => {
                            if self.options.strict && self.pending.is_some() {
                                return Err(Error::Invalid("dangling graphic control"));
                            }
                            self.state = State::Done;
                            break Some(Event::End);
                        }
                        _ => return Err(Error::Invalid("block introducer")),
                    }
                }
                State::Extension => {
                    let Some(label) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    match label {
                        0xf9 => {
                            self.version89()?;
                            self.transition(State::Control);
                        }
                        0xff => {
                            self.version89()?;
                            self.transition(State::Application);
                        }
                        0x01 => {
                            self.version89()?;
                            self.transition(State::PlainText);
                        }
                        0xfe => {
                            self.version89()?;
                            self.extension = Some(Extension::Comment(Vec::new()));
                            self.state = State::MetadataSize;
                        }
                        label => {
                            self.extension = Some(Extension::Unknown {
                                label,
                                control: if is_graphic_label(label) {
                                    self.pending.take()
                                } else {
                                    None
                                },
                                data: Vec::new(),
                            });
                            self.state = State::MetadataSize;
                        }
                    }
                }
                State::Control => {
                    let Some(s) =
                        self.field
                            .read(input, &mut at, 6, &mut self.remaining_input_bytes)?
                    else {
                        break None;
                    };
                    if s[0] != 4
                        || s[5] != 0
                        || (self.options.strict && (s[1] & 0xe0 != 0 || self.pending.is_some()))
                    {
                        return Err(Error::Invalid("graphic control block"));
                    }
                    self.pending = Some(GraphicControl {
                        disposal: Disposal::from_bits(s[1] >> 2),
                        delay: le(&s[2..]),
                        transparent_index: (s[1] & 1 != 0).then_some(s[4]),
                        user_input: s[1] & 2 != 0,
                    });
                    self.state = State::Block;
                }
                State::Application => {
                    let Some(s) =
                        self.field
                            .read(input, &mut at, 12, &mut self.remaining_input_bytes)?
                    else {
                        break None;
                    };
                    if s[0] != 11 {
                        return Err(Error::Invalid("application header length"));
                    }
                    charge(&mut self.remaining_metadata_bytes, 11)?;
                    self.extension = Some(Extension::Application {
                        identifier: s[1..9].try_into().unwrap(),
                        authentication: s[9..12].try_into().unwrap(),
                        data: Vec::new(),
                    });
                    self.state = State::MetadataSize;
                }
                State::PlainText => {
                    let Some(s) =
                        self.field
                            .read(input, &mut at, 13, &mut self.remaining_input_bytes)?
                    else {
                        break None;
                    };
                    if s[0] != 12 {
                        return Err(Error::Invalid("plain-text header length"));
                    }
                    charge(&mut self.remaining_metadata_bytes, 12)?;
                    let text = PlainText {
                        left: le(&s[1..]),
                        top: le(&s[3..]),
                        width: le(&s[5..]),
                        height: le(&s[7..]),
                        cell_width: s[9],
                        cell_height: s[10],
                        foreground_index: s[11],
                        background_index: s[12],
                        control: self.pending.take(),
                        data: Vec::new(),
                    };
                    let header = self.header.as_ref().unwrap();
                    validate_text(
                        &text,
                        (header.width, header.height),
                        header
                            .global_palette
                            .as_ref()
                            .or(external_palette)
                            .or(self.options.external_palette.as_ref())
                            .map(Vec::len),
                        self.options.check_bounds,
                    )?;
                    self.extension = Some(Extension::PlainText(text));
                    self.state = State::MetadataSize;
                }
                State::Image => {
                    let Some(s) =
                        self.field
                            .read(input, &mut at, 9, &mut self.remaining_input_bytes)?
                    else {
                        break None;
                    };
                    let header = self.header.as_ref().unwrap();
                    let flags = s[8];
                    if self.options.strict
                        && (flags & 0x18 != 0
                            || (flags & 0x20 != 0 && header.version == Version::Gif87a))
                    {
                        return Err(Error::Invalid("reserved image descriptor bits"));
                    }
                    let mut indices = std::mem::take(&mut self.spare);
                    indices.clear();
                    self.frame = Frame {
                        left: le(s),
                        top: le(&s[2..]),
                        width: le(&s[4..]),
                        height: le(&s[6..]),
                        interlaced: flags & 0x40 != 0,
                        sorted: flags & 0x20 != 0,
                        local_palette: None,
                        control: self.pending.take(),
                        indices,
                    };
                    validate_rect(
                        self.frame.left,
                        self.frame.top,
                        self.frame.width,
                        self.frame.height,
                        (header.width, header.height),
                        self.options.check_bounds,
                    )?;
                    let count = self.frame.width as usize * self.frame.height as usize;
                    if count > self.options.limits.max_frame_pixels {
                        return Err(Error::LimitExceeded);
                    }
                    charge(&mut self.remaining_pixels, count)?;
                    charge(&mut self.remaining_frames, 1)?;
                    reserve(&mut self.frame.indices, count)?;
                    let size = if flags & 128 != 0 {
                        3usize << ((flags & 7) + 1)
                    } else {
                        0
                    };
                    if size != 0 {
                        self.transition(State::LocalPalette);
                        self.wanted = size;
                    } else {
                        self.state = State::Minimum;
                    }
                }
                State::LocalPalette => {
                    let Some(s) = self.field.read(
                        input,
                        &mut at,
                        self.wanted,
                        &mut self.remaining_input_bytes,
                    )?
                    else {
                        break None;
                    };
                    self.frame.local_palette = Some(palette(s)?);
                    self.state = State::Minimum;
                }
                State::Minimum => {
                    let Some(minimum) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    if self.options.strict {
                        self.lzw.reset(minimum)?;
                    } else {
                        self.lzw.reset_compatible(minimum)?;
                    }
                    self.wanted = 0;
                    self.state = State::ImageData;
                }
                State::ImageData => {
                    let width = usize::from(self.frame.width);
                    let height = usize::from(self.frame.height);
                    let pixels = width * height;
                    let mut remaining = self.wanted;
                    loop {
                        if remaining == 0 {
                            let Some(size) = self.byte(input, &mut at)? else {
                                self.wanted = 0;
                                break 'events None;
                            };
                            if size == 0 {
                                break;
                            }
                            remaining = usize::from(size);
                        }
                        let count = remaining.min(input.len() - at);
                        if count != 0 {
                            charge(&mut self.remaining_input_bytes, count)?;
                            self.lzw.update(
                                &input[at..at + count],
                                &mut self.frame.indices,
                                pixels,
                            )?;
                            at += count;
                        }
                        remaining -= count;
                        if remaining != 0 {
                            self.wanted = remaining;
                            break 'events None;
                        }
                    }
                    self.wanted = 0;
                    // The image-data terminator supplies the framing needed
                    // to recover a missing EOI. LZW errors stay fatal, and
                    // neither truncated framing nor missing pixels qualifies.
                    if (self.options.strict && !self.lzw.ended())
                        || self.frame.indices.len() != pixels
                    {
                        return Err(Error::Invalid("incomplete LZW image"));
                    }
                    if self.frame.interlaced {
                        deinterlace(&mut self.frame.indices, width, height, &mut self.interlace)?;
                    }
                    let header = self.header.as_ref().unwrap();
                    if let Some(palette) = self
                        .frame
                        .local_palette
                        .as_ref()
                        .or(header.global_palette.as_ref())
                        .or(external_palette)
                        .or(self.options.external_palette.as_ref())
                    {
                        // Every expanded phrase contains only LZW literals.
                        if self.lzw.literal_count() > palette.len()
                            && usize::from(lzw::index_bits(&self.frame.indices)) >= palette.len()
                        {
                            return Err(Error::Invalid("pixel index outside active palette"));
                        }
                        validate_control(self.frame.control, palette.len())?;
                    }
                    self.state = State::Block;
                    break Some(Event::Block(Block::Frame(std::mem::take(&mut self.frame))));
                }
                State::MetadataSize | State::MetadataData => {
                    let data = Self::data_mut(self.extension.as_mut().unwrap());
                    if self.state == State::MetadataData {
                        let count = self.wanted.min(input.len() - at);
                        charge(&mut self.remaining_input_bytes, count)?;
                        data.last_mut()
                            .unwrap()
                            .extend_from_slice(&input[at..at + count]);
                        at += count;
                        self.wanted -= count;
                        if self.wanted != 0 {
                            break None;
                        }
                    }
                    loop {
                        let Some(&size) = input.get(at) else {
                            self.state = State::MetadataSize;
                            break 'events None;
                        };
                        charge(&mut self.remaining_input_bytes, 1)?;
                        at += 1;
                        if size == 0 {
                            break;
                        }
                        let size = usize::from(size);
                        charge(&mut self.remaining_blocks, 1)?;
                        charge(
                            &mut self.remaining_metadata_bytes,
                            size + std::mem::size_of::<Vec<u8>>(),
                        )?;
                        let mut block = Vec::new();
                        block
                            .try_reserve_exact(size)
                            .map_err(|_| Error::Allocation)?;
                        if data.is_empty() && input.get(at + size) == Some(&0) {
                            data.try_reserve_exact(1).map_err(|_| Error::Allocation)?;
                        } else {
                            reserve(data, 1)?;
                        }
                        let count = size.min(input.len() - at);
                        charge(&mut self.remaining_input_bytes, count)?;
                        block.extend_from_slice(&input[at..at + count]);
                        data.push(block);
                        at += count;
                        if count != size {
                            self.wanted = size - count;
                            self.state = State::MetadataData;
                            break 'events None;
                        }
                    }
                    self.state = State::Block;
                    break Some(Event::Block(Block::Extension(
                        self.extension.take().unwrap(),
                    )));
                }
                State::Done => {
                    if self.options.strict && !input[at..].is_empty() {
                        return Err(Error::Invalid("bytes after GIF trailer"));
                    }
                    // A caller may stop at Event::End for an embedded stream.
                    // If it submits the suffix, discarded bytes still count.
                    charge(&mut self.remaining_input_bytes, input.len() - at)?;
                    at = input.len();
                    break None;
                }
                State::Failed => return Err(Error::Invalid("decoder is failed")),
            }
        };
        Ok(Progress {
            consumed: at,
            event,
        })
    }
}

fn charge(remaining: &mut usize, count: usize) -> Result<(), Error> {
    *remaining = remaining.checked_sub(count).ok_or(Error::LimitExceeded)?;
    Ok(())
}

fn le(s: &[u8]) -> u16 {
    u16::from_le_bytes([s[0], s[1]])
}
fn palette(s: &[u8]) -> Result<Palette, Error> {
    let colors = s.as_chunks::<3>().0;
    let mut out = Vec::new();
    out.try_reserve_exact(colors.len())
        .map_err(|_| Error::Allocation)?;
    out.extend_from_slice(colors);
    Ok(out)
}
pub(crate) fn interlace_rows(height: usize) -> impl Iterator<Item = usize> {
    [(0, 8), (4, 8), (2, 4), (1, 2)]
        .into_iter()
        .flat_map(move |(start, step)| (start..height).step_by(step))
}

#[inline(never)]
fn deinterlace(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    row: &mut Vec<u8>,
) -> Result<(), Error> {
    if height <= 2 {
        return Ok(());
    }
    if height <= 64 {
        if width == 1 {
            permute_interlace::<1, true>(pixels, width, height, row)
        } else {
            permute_interlace::<1, false>(pixels, width, height, row)
        }
    } else if height <= 1024 {
        if width == 1 {
            permute_interlace::<16, true>(pixels, width, height, row)
        } else {
            permute_interlace::<16, false>(pixels, width, height, row)
        }
    } else if width == 1 {
        permute_interlace::<1024, true>(pixels, width, height, row)
    } else {
        permute_interlace::<1024, false>(pixels, width, height, row)
    }
}

#[inline(never)]
fn permute_interlace<const WORDS: usize, const SINGLE_PIXEL: bool>(
    pixels: &mut [u8],
    width: usize,
    height: usize,
    row: &mut Vec<u8>,
) -> Result<(), Error> {
    let width = if SINGLE_PIXEL { 1 } else { width };
    let mut narrow = [0; 64];
    let row = if width <= narrow.len() {
        &mut narrow[..width]
    } else {
        let extra = width.saturating_sub(row.len());
        reserve(row, extra)?;
        row.resize(width, 0);
        row.as_mut_slice()
    };
    // GIF heights fit u16. Walk each row-permutation cycle once, keeping only
    // its first row aside. Every other row moves directly to its destination.
    let mut visited = [0u64; WORDS];
    let word = |row: usize| if WORDS == 1 { 0 } else { row / 64 };
    let pass2 = height.div_ceil(8);
    let pass3 = pass2 + (height + 3) / 8;
    let pass4 = pass3 + (height + 1) / 4;
    // The row's low three bits select its pass. Pack its pass offset and
    // log2(row step) together so each lookup needs one load.
    let second = (pass2 << 2) | 3;
    let third = (pass3 << 2) | 2;
    let fourth = (pass4 << 2) | 1;
    let passes = [3, fourth, third, fourth, second, fourth, third, fourth];
    let source_row = |dest: usize| {
        let pass = passes[dest & 7];
        (pass >> 2) + (dest >> (pass & 3))
    };
    let word_count = if WORDS == 1 { 1 } else { height.div_ceil(64) };
    for word_index in 0..word_count {
        let base = word_index * 64;
        let end = (base + 64).min(height);
        loop {
            let bit = (!visited[word_index]).trailing_zeros();
            let start = base + bit as usize;
            if start >= end {
                break;
            }
            let mut source = source_row(start);
            if source == start {
                visited[word_index] |= 1 << bit;
                continue;
            }
            row.copy_from_slice(&pixels[start * width..(start + 1) * width]);
            let mut dest = start;
            loop {
                visited[word(dest)] |= 1 << (dest % 64);
                if source == start {
                    pixels[dest * width..(dest + 1) * width].copy_from_slice(row);
                    break;
                }
                if SINGLE_PIXEL {
                    pixels[dest] = pixels[source];
                } else {
                    pixels.copy_within(source * width..(source + 1) * width, dest * width);
                }
                dest = source;
                source = source_row(dest);
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_text(
    text: &PlainText,
    screen: (u16, u16),
    palette_len: Option<usize>,
    bounds: bool,
) -> Result<(), Error> {
    validate_rect(text.left, text.top, text.width, text.height, screen, bounds)?;
    if text.cell_width == 0 || text.cell_height == 0 {
        return Err(Error::Invalid("zero text cell dimensions"));
    }
    if let Some(len) = palette_len {
        if text.foreground_index as usize >= len || text.background_index as usize >= len {
            return Err(Error::Invalid("plain-text palette index"));
        }
        validate_control(text.control, len)?;
    }
    Ok(())
}

pub fn decode(input: &[u8]) -> Result<Gif, Error> {
    decode_with_options(input, &DecodeOptions::default())
}
pub fn decode_with_options(input: &[u8], options: &DecodeOptions) -> Result<Gif, Error> {
    if input.len() > options.limits.max_input_bytes {
        return Err(Error::LimitExceeded);
    }
    let mut decoder = Decoder::new(DecodeOptions {
        limits: options.limits.clone(),
        strict: options.strict,
        check_bounds: options.check_bounds,
        external_palette: None,
    });
    let external_palette = options.external_palette.as_ref();
    let mut at = 0;
    let mut blocks = Vec::new();
    loop {
        let progress = decoder.update_with_palette(&input[at..], external_palette)?;
        at += progress.consumed;
        match progress.event {
            Some(Event::Header) => {}
            Some(Event::Block(block)) => {
                if blocks.is_empty() && input.get(at) == Some(&0x3b) {
                    blocks.try_reserve_exact(1).map_err(|_| Error::Allocation)?;
                } else {
                    reserve(&mut blocks, 1)?;
                }
                blocks.push(block);
            }
            Some(Event::End) => {
                if options.strict && at != input.len() {
                    return Err(Error::Invalid("bytes after GIF trailer"));
                }
                decoder.finish()?;
                return Ok(Gif {
                    header: decoder.header.unwrap(),
                    blocks,
                });
            }
            None => {
                decoder.finish()?;
                return Err(Error::Truncated);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interlace_permutation_all_pass_tails_and_long_cycles() {
        let mut row = Vec::new();
        for height in (1..=257).chain([
            511, 512, 513, 1023, 1024, 1025, 8193, 65535, 17, 17, 3, 65, 2, 1,
        ]) {
            for width in [1, 3, 17, 64, 65] {
                let expected: Vec<_> = (0..height)
                    .flat_map(|y| (0..width).map(move |x| ((y * 137 + x * 71) % 251) as u8))
                    .collect();
                let mut wire: Vec<_> = interlace_rows(height)
                    .flat_map(|y| expected[y * width..(y + 1) * width].iter().copied())
                    .collect();
                let previous_scratch_len = row.len();
                deinterlace(&mut wire, width, height, &mut row).unwrap();
                assert_eq!(wire, expected, "{width}x{height}");
                assert_eq!(
                    row.len(),
                    if height <= 2 || width <= 64 {
                        previous_scratch_len
                    } else {
                        width
                    }
                );
            }
        }
    }
}
