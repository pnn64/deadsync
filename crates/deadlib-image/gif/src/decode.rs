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
    /// Check reserved bits, block versions and duplicate/dangling controls.
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
    ImageSize,
    ImageData,
    MetadataSize,
    MetadataData,
    Done,
    Failed,
}

/// Push decoder. Each call consumes a prefix and returns at most one event.
/// Keep submitting the unconsumed suffix, even after an event. `finish` checks
/// that the trailer was received. No compressed image copy or input buffering
/// is needed beyond the current fixed-size header and data sub-block.
pub struct Decoder {
    options: DecodeOptions,
    state: State,
    header: Option<Header>,
    scratch: [u8; 768],
    filled: usize,
    wanted: usize,
    remaining: usize,
    pending: Option<GraphicControl>,
    frame: Frame,
    spare: Vec<u8>,
    interlace: Vec<u8>,
    extension: Option<Extension>,
    lzw: lzw::Decoder,
    input_bytes: usize,
    metadata_bytes: usize,
    block_count: usize,
    frame_count: usize,
    total_pixels: usize,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new(DecodeOptions::default())
    }
}
impl Decoder {
    pub fn new(options: DecodeOptions) -> Self {
        Self {
            options,
            state: State::Header,
            header: None,
            scratch: [0; 768],
            filled: 0,
            wanted: 13,
            remaining: 0,
            pending: None,
            frame: Frame::default(),
            spare: Vec::new(),
            interlace: Vec::new(),
            extension: None,
            lzw: lzw::Decoder::new(),
            input_bytes: 0,
            metadata_bytes: 0,
            block_count: 0,
            frame_count: 0,
            total_pixels: 0,
        }
    }
    pub fn header(&self) -> Option<&Header> {
        self.header.as_ref()
    }
    /// Reuse a returned frame's pixel allocation for a later image. Retained
    /// frames must not be recycled. Only one spare allocation is kept.
    pub fn recycle_frame(&mut self, mut frame: Frame) {
        frame.indices.clear();
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
        let result = self.process(input);
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }
    fn transition(&mut self, state: State, wanted: usize) {
        self.state = state;
        self.wanted = wanted;
        self.filled = 0;
    }
    fn charge_input(&mut self, amount: usize) -> Result<(), Error> {
        self.input_bytes = self
            .input_bytes
            .checked_add(amount)
            .ok_or(Error::LimitExceeded)?;
        if self.input_bytes > self.options.limits.max_input_bytes {
            Err(Error::LimitExceeded)
        } else {
            Ok(())
        }
    }
    fn charge_block(&mut self) -> Result<(), Error> {
        self.block_count += 1;
        if self.block_count > self.options.limits.max_blocks {
            Err(Error::LimitExceeded)
        } else {
            Ok(())
        }
    }
    fn charge_metadata(&mut self, amount: usize) -> Result<(), Error> {
        self.metadata_bytes = self
            .metadata_bytes
            .checked_add(amount)
            .ok_or(Error::LimitExceeded)?;
        if self.metadata_bytes > self.options.limits.max_metadata_bytes {
            Err(Error::LimitExceeded)
        } else {
            Ok(())
        }
    }
    fn fill(&mut self, input: &[u8], at: &mut usize) -> Result<bool, Error> {
        let count = (self.wanted - self.filled).min(input.len() - *at);
        self.charge_input(count)?;
        self.scratch[self.filled..self.filled + count].copy_from_slice(&input[*at..*at + count]);
        self.filled += count;
        *at += count;
        Ok(self.filled == self.wanted)
    }
    fn byte(&mut self, input: &[u8], at: &mut usize) -> Result<Option<u8>, Error> {
        if *at == input.len() {
            return Ok(None);
        }
        self.charge_input(1)?;
        let byte = input[*at];
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
    fn data_mut(&mut self) -> &mut SubBlocks {
        match self.extension.as_mut().unwrap() {
            Extension::Comment(data)
            | Extension::Application { data, .. }
            | Extension::Unknown { data, .. } => data,
            Extension::PlainText(text) => &mut text.data,
        }
    }
    fn process(&mut self, input: &[u8]) -> Result<Progress, Error> {
        let mut at = 0;
        let event = loop {
            match self.state {
                State::Header => {
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    let s = &self.scratch;
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
                    if let Some(p) = &self.options.external_palette {
                        validate_palette(p)?;
                    }
                    let size = if s[10] & 128 != 0 {
                        3usize << ((s[10] & 7) + 1)
                    } else {
                        0
                    };
                    self.header = Some(header);
                    if size != 0 {
                        self.transition(State::GlobalPalette, size);
                    } else {
                        self.transition(State::Block, 0);
                        break Some(Event::Header);
                    }
                }
                State::GlobalPalette => {
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    self.header.as_mut().unwrap().global_palette =
                        Some(palette(&self.scratch[..self.wanted])?);
                    self.header.as_ref().unwrap().validate()?;
                    self.transition(State::Block, 0);
                    break Some(Event::Header);
                }
                State::Block => {
                    let Some(byte) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    match byte {
                        0x2c => {
                            self.charge_block()?;
                            self.transition(State::Image, 9);
                        }
                        0x21 => {
                            self.charge_block()?;
                            self.transition(State::Extension, 0);
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
                            self.transition(State::Control, 6);
                        }
                        0xff => {
                            self.version89()?;
                            self.transition(State::Application, 12);
                        }
                        0x01 => {
                            self.version89()?;
                            self.transition(State::PlainText, 13);
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
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    let s = &self.scratch;
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
                    self.transition(State::Block, 0);
                }
                State::Application => {
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    if self.scratch[0] != 11 {
                        return Err(Error::Invalid("application header length"));
                    }
                    self.charge_metadata(11)?;
                    self.extension = Some(Extension::Application {
                        identifier: self.scratch[1..9].try_into().unwrap(),
                        authentication: self.scratch[9..12].try_into().unwrap(),
                        data: Vec::new(),
                    });
                    self.transition(State::MetadataSize, 0);
                }
                State::PlainText => {
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    if self.scratch[0] != 12 {
                        return Err(Error::Invalid("plain-text header length"));
                    }
                    self.charge_metadata(12)?;
                    let s = &self.scratch;
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
                    validate_text(
                        &text,
                        self.header.as_ref().unwrap(),
                        self.options.external_palette.as_deref(),
                        self.options.check_bounds,
                    )?;
                    self.extension = Some(Extension::PlainText(text));
                    self.transition(State::MetadataSize, 0);
                }
                State::Image => {
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    let s = &self.scratch;
                    if self.options.strict
                        && (s[8] & 0x18 != 0
                            || (self.header.as_ref().unwrap().version == Version::Gif87a
                                && s[8] & 0x20 != 0))
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
                        interlaced: s[8] & 0x40 != 0,
                        sorted: s[8] & 0x20 != 0,
                        local_palette: None,
                        control: self.pending.take(),
                        indices,
                    };
                    validate_rect(
                        self.frame.left,
                        self.frame.top,
                        self.frame.width,
                        self.frame.height,
                        self.header.as_ref().unwrap(),
                        self.options.check_bounds,
                    )?;
                    let count = self.frame.width as usize * self.frame.height as usize;
                    self.total_pixels = self
                        .total_pixels
                        .checked_add(count)
                        .ok_or(Error::LimitExceeded)?;
                    self.frame_count += 1;
                    if count > self.options.limits.max_frame_pixels
                        || self.total_pixels > self.options.limits.max_total_pixels
                        || self.frame_count > self.options.limits.max_frames
                    {
                        return Err(Error::LimitExceeded);
                    }
                    reserve(&mut self.frame.indices, count)?;
                    let size = if s[8] & 128 != 0 {
                        3usize << ((s[8] & 7) + 1)
                    } else {
                        0
                    };
                    if size != 0 {
                        self.transition(State::LocalPalette, size);
                    } else {
                        self.transition(State::Minimum, 0);
                    }
                }
                State::LocalPalette => {
                    if !self.fill(input, &mut at)? {
                        break None;
                    }
                    self.frame.local_palette = Some(palette(&self.scratch[..self.wanted])?);
                    self.transition(State::Minimum, 0);
                }
                State::Minimum => {
                    let Some(minimum) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    self.lzw.reset(minimum)?;
                    self.state = State::ImageSize;
                }
                State::ImageSize => {
                    let Some(size) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    if size == 0 {
                        let count = self.frame.width as usize * self.frame.height as usize;
                        if !self.lzw.ended() || self.frame.indices.len() != count {
                            return Err(Error::Invalid("incomplete LZW image"));
                        }
                        if self.frame.interlaced {
                            self.interlace.clear();
                            reserve(&mut self.interlace, count)?;
                            self.interlace.resize(count, 0);
                            let w = self.frame.width as usize;
                            for (row, y) in interlace_rows(self.frame.height as usize).enumerate() {
                                self.interlace[y * w..(y + 1) * w]
                                    .copy_from_slice(&self.frame.indices[row * w..(row + 1) * w]);
                            }
                            std::mem::swap(&mut self.interlace, &mut self.frame.indices);
                        }
                        self.frame
                            .validate(self.header.as_ref().unwrap(), self.options.check_bounds)?;
                        if let Some(p) = self
                            .frame
                            .palette(self.header.as_ref().unwrap())
                            .or(self.options.external_palette.as_deref())
                        {
                            validate_indices(&self.frame.indices, p.len())?;
                            validate_control(self.frame.control, p.len())?;
                        }
                        self.state = State::Block;
                        break Some(Event::Block(Block::Frame(std::mem::take(&mut self.frame))));
                    }
                    self.remaining = size as usize;
                    self.state = State::ImageData;
                }
                State::ImageData => {
                    let count = self.remaining.min(input.len() - at);
                    if count == 0 {
                        break None;
                    }
                    self.charge_input(count)?;
                    self.lzw.update(
                        &input[at..at + count],
                        &mut self.frame.indices,
                        self.frame.width as usize * self.frame.height as usize,
                    )?;
                    at += count;
                    self.remaining -= count;
                    if self.remaining == 0 {
                        self.state = State::ImageSize;
                    }
                }
                State::MetadataSize => {
                    let Some(size) = self.byte(input, &mut at)? else {
                        break None;
                    };
                    if size == 0 {
                        self.state = State::Block;
                        break Some(Event::Block(Block::Extension(
                            self.extension.take().unwrap(),
                        )));
                    }
                    self.charge_block()?;
                    self.charge_metadata(size as usize + std::mem::size_of::<Vec<u8>>())?;
                    let mut block = Vec::new();
                    reserve(&mut block, size as usize)?;
                    let data = self.data_mut();
                    reserve(data, 1)?;
                    data.push(block);
                    self.remaining = size as usize;
                    self.state = State::MetadataData;
                }
                State::MetadataData => {
                    let count = self.remaining.min(input.len() - at);
                    if count == 0 {
                        break None;
                    }
                    self.charge_input(count)?;
                    self.data_mut()
                        .last_mut()
                        .unwrap()
                        .extend_from_slice(&input[at..at + count]);
                    at += count;
                    self.remaining -= count;
                    if self.remaining == 0 {
                        self.state = State::MetadataSize;
                    }
                }
                State::Done => {
                    if !input[at..].is_empty() {
                        return Err(Error::Invalid("bytes after GIF trailer"));
                    }
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

fn le(s: &[u8]) -> u16 {
    u16::from_le_bytes([s[0], s[1]])
}
fn palette(s: &[u8]) -> Result<Palette, Error> {
    let mut out = Vec::new();
    reserve(&mut out, s.len() / 3)?;
    out.extend_from_slice(s.as_chunks::<3>().0);
    Ok(out)
}
pub(crate) fn interlace_rows(height: usize) -> impl Iterator<Item = usize> {
    [(0, 8), (4, 8), (2, 4), (1, 2)]
        .into_iter()
        .flat_map(move |(start, step)| (start..height).step_by(step))
}
pub(crate) fn validate_text(
    text: &PlainText,
    header: &Header,
    external: Option<&[[u8; 3]]>,
    bounds: bool,
) -> Result<(), Error> {
    validate_rect(text.left, text.top, text.width, text.height, header, bounds)?;
    if text.cell_width == 0 || text.cell_height == 0 {
        return Err(Error::Invalid("zero text cell dimensions"));
    }
    if let Some(p) = header.global_palette.as_deref().or(external) {
        if text.foreground_index as usize >= p.len() || text.background_index as usize >= p.len() {
            return Err(Error::Invalid("plain-text palette index"));
        }
        validate_control(text.control, p.len())?;
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
    let mut decoder = Decoder::new(options.clone());
    let mut at = 0;
    let mut gif = None;
    loop {
        let progress = decoder.update(&input[at..])?;
        at += progress.consumed;
        match progress.event {
            Some(Event::Header) => gif = Some(Gif::new(decoder.header().unwrap().clone())),
            Some(Event::Block(block)) => {
                let blocks = &mut gif.as_mut().unwrap().blocks;
                reserve(blocks, 1)?;
                blocks.push(block);
            }
            Some(Event::End) => {
                if at != input.len() {
                    return Err(Error::Invalid("bytes after GIF trailer"));
                }
                decoder.finish()?;
                return Ok(gif.unwrap());
            }
            None => {
                decoder.finish()?;
                return Err(Error::Truncated);
            }
        }
    }
}
