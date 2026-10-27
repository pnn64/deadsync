use super::{DecodeEvent, DecodeProgress, DecodeStatus, ImageInfo, ImageKind, rows::ImageRows};
use crate::{
    AncillaryChunk, AnimationPolicy, ChunkPosition, ColorType, DecodeOptions, Error, FrameControl,
    Metadata, Transparency,
    checksum::Crc32,
    codec::{Header, SIGNATURE, be16, be32},
    metadata::Validator,
    reserve,
};

#[derive(Clone, Copy)]
enum State {
    Signature,
    ChunkHeader,
    ChunkStart,
    ChunkBody,
    ChunkCrc,
    Done,
}
#[derive(Clone, Copy)]
enum Event {
    Header,
    Palette,
    Transparency,
    Metadata,
    Animation,
    AnimationError,
    Control {
        index: u32,
        control: FrameControl,
        default: bool,
    },
    Row,
    ImageEnd(ImageKind),
}

#[derive(Clone, Copy)]
enum AnimationFault {
    Begin,
    Body,
    Complete,
    EndFrame,
}
fn animation_chunk(kind: &[u8; 4]) -> bool {
    matches!(kind, b"acTL" | b"fcTL" | b"fdAT")
}

/// Incremental PNG/APNG parser and native-row decoder.
///
/// A default fcTL preceding acTL is retained until acTL is verified, so the
/// public `Animation` event always precedes its first `FrameControl` event.
///
/// The image pipeline owns a 32 KiB DEFLATE window and a bounded number of rows.
/// Metadata and suggested-palette names are bounded by the configured metadata
/// budget. No complete image, frame, compressed payload or IDAT chunk is stored.
/// `process` stops at each event and leaves bytes after IEND unconsumed. Every
/// returned event must be handled before calling `process` again.
pub struct StreamDecoder {
    options: DecodeOptions,
    state: State,
    event: Option<Event>,
    failure: Option<Error>,
    fixed: [u8; 8],
    have: usize,
    input_left: usize,
    chunks: usize,
    kind: [u8; 4],
    length: usize,
    left: usize,
    crc: Crc32,
    data: Vec<u8>,
    prefix: [u8; 4],
    prefix_have: usize,
    header: Option<Header>,
    palette: Vec<[u8; 3]>,
    transparency: Option<Transparency>,
    validator: Validator,
    ancillary: Option<AncillaryChunk>,
    metadata: Option<Metadata>,
    seen_data: bool,
    idat_closed: bool,
    animation: Option<(u32, u32)>,
    animation_error: Option<Error>,
    animation_fault: Option<AnimationFault>,
    controls: u32,
    sequence: u32,
    default_control: bool,
    pre_control: Option<FrameControl>,
    frames_ended: u32,
    rows: Option<ImageRows>,
    image_kind: Option<ImageKind>,
    sample_bytes: usize,
    peak_image_buffers: usize,
}

fn fill(
    input: &[u8],
    pos: &mut usize,
    dst: &mut [u8],
    have: &mut usize,
    budget: &mut usize,
) -> Result<bool, Error> {
    let n = (dst.len() - *have).min(input.len() - *pos).min(*budget);
    dst[*have..*have + n].copy_from_slice(&input[*pos..*pos + n]);
    *have += n;
    *pos += n;
    *budget -= n;
    if *have < dst.len() && *pos < input.len() && *budget == 0 {
        return Err(Error::LimitExceeded);
    }
    Ok(*have == dst.len())
}

impl StreamDecoder {
    pub fn new(options: DecodeOptions) -> Self {
        Self {
            options,
            state: State::Signature,
            event: None,
            failure: None,
            fixed: [0; 8],
            have: 0,
            input_left: options.limits.max_input_bytes,
            chunks: 0,
            kind: [0; 4],
            length: 0,
            left: 0,
            crc: Crc32::new(),
            data: Vec::new(),
            prefix: [0; 4],
            prefix_have: 0,
            header: None,
            palette: Vec::new(),
            transparency: None,
            validator: Validator::new(options.limits.max_metadata_bytes),
            ancillary: None,
            metadata: None,
            seen_data: false,
            idat_closed: false,
            animation: None,
            animation_error: None,
            animation_fault: None,
            controls: 0,
            sequence: 0,
            default_control: false,
            pre_control: None,
            frames_ended: 0,
            rows: None,
            image_kind: None,
            sample_bytes: 0,
            peak_image_buffers: 0,
        }
    }
    pub fn info(&self) -> Option<ImageInfo> {
        self.header.map(Into::into)
    }
    pub fn total_in(&self) -> usize {
        self.options.limits.max_input_bytes - self.input_left
    }
    /// Peak capacity of the image pipeline's owned buffers, excluding metadata.
    pub fn peak_image_buffer_bytes(&self) -> usize {
        self.peak_image_buffers
    }
    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done) && self.failure.is_none()
    }
    /// First APNG fault recovered under `AnimationPolicy::RecoverDefault`.
    pub fn animation_error(&self) -> Option<&Error> {
        self.animation_error.as_ref()
    }
    pub fn finish(&self) -> Result<(), Error> {
        if let Some(error) = &self.failure {
            Err(error.clone())
        } else if self.is_done() {
            Ok(())
        } else {
            Err(Error::Truncated)
        }
    }
    pub fn event(&self) -> Option<DecodeEvent<'_>> {
        Some(match self.event? {
            Event::Header => DecodeEvent::Header(self.info()?),
            Event::Palette => DecodeEvent::Palette(&self.palette),
            Event::Transparency => DecodeEvent::Transparency(self.transparency.as_ref()?),
            Event::Metadata => DecodeEvent::Metadata {
                chunk: self.ancillary.as_ref()?,
                value: self.metadata.as_ref()?,
            },
            Event::Animation => {
                let (frames, plays) = self.animation?;
                DecodeEvent::Animation { frames, plays }
            }
            Event::AnimationError => DecodeEvent::AnimationError(self.animation_error.as_ref()?),
            Event::Control {
                index,
                control,
                default,
            } => DecodeEvent::FrameControl {
                index,
                control,
                uses_default_image: default,
            },
            Event::Row => DecodeEvent::Row(self.rows.as_ref()?.row()),
            Event::ImageEnd(kind) => DecodeEvent::ImageEnd(kind),
        })
    }
    pub fn process(&mut self, input: &[u8]) -> Result<DecodeProgress, Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        self.event = None;
        if let Some(chunk) = self.ancillary.take() {
            self.data = chunk.data;
            self.data.clear();
        }
        self.metadata = None;
        let initial = self.total_in();
        loop {
            self.animation_fault = None;
            let consumed = self.total_in() - initial;
            match self.run(&input[consumed..]) {
                Ok(mut progress) => {
                    progress.consumed += consumed;
                    return Ok(progress);
                }
                Err(error) => {
                    if self.options.animation_policy == AnimationPolicy::RecoverDefault
                        && matches!(
                            error,
                            Error::Invalid(_) | Error::Checksum | Error::Truncated
                        )
                        && let Some(fault) = self.animation_fault.take()
                    {
                        let first = self.animation_error.is_none();
                        // Only container-committed bytes count as consumed.
                        // An errored frame inflater may have inspected more;
                        // discard it and replay those bytes through CRC/skip.
                        self.recover_animation(error, fault);
                        if first {
                            return Ok(self.emit(Event::AnimationError, self.total_in() - initial));
                        }
                    } else {
                        self.failure = Some(error.clone());
                        return Err(error);
                    }
                }
            }
        }
    }
    fn animation_result<T>(
        &mut self,
        result: Result<T, Error>,
        fault: AnimationFault,
    ) -> Result<T, Error> {
        result.inspect_err(|_| self.animation_fault = Some(fault))
    }
    fn discarded_animation_chunk(&self) -> bool {
        self.animation_error.is_some() && animation_chunk(&self.kind)
    }
    fn recover_animation(&mut self, error: Error, fault: AnimationFault) {
        if self.animation_error.is_none() {
            self.animation_error = Some(error);
        }
        self.animation = None;
        self.default_control = false;
        self.pre_control = None;
        if matches!(self.image_kind, Some(ImageKind::AnimationFrame(_))) {
            self.rows = None;
            self.image_kind = None;
        }
        match fault {
            AnimationFault::Begin | AnimationFault::Body | AnimationFault::EndFrame => (),
            AnimationFault::Complete => {
                self.have = 0;
                self.state = if &self.kind == b"IEND" {
                    State::Done
                } else {
                    State::ChunkHeader
                };
            }
        }
    }
    fn emit(&mut self, event: Event, consumed: usize) -> DecodeProgress {
        self.event = Some(event);
        DecodeProgress {
            consumed,
            status: DecodeStatus::Event,
        }
    }
    fn update_memory(&mut self) {
        if let Some(rows) = &self.rows {
            self.peak_image_buffers = self.peak_image_buffers.max(rows.memory_bytes());
        }
    }
    fn start_image(&mut self, h: Header, kind: ImageKind) -> Result<(), Error> {
        self.sample_bytes = self
            .sample_bytes
            .checked_add(h.sample_bytes()?)
            .ok_or(Error::LimitExceeded)?;
        if self.sample_bytes > self.options.limits.max_output_bytes {
            return Err(Error::LimitExceeded);
        }
        self.rows = Some(ImageRows::new(
            h,
            kind,
            self.options.limits.max_output_bytes,
        )?);
        self.image_kind = Some(kind);
        self.update_memory();
        Ok(())
    }
    fn end_image(&mut self) -> Result<ImageKind, Error> {
        let mut rows = self
            .rows
            .take()
            .ok_or(Error::Invalid("missing image data"))?;
        let kind = rows.finish()?;
        if kind != ImageKind::Default || self.default_control {
            self.frames_ended += 1;
        }
        self.image_kind = None;
        Ok(kind)
    }
    fn sequence(&mut self, value: u32) -> Result<(), Error> {
        if value > 0x7fff_ffff || value != self.sequence {
            return Err(Error::Invalid("APNG sequence number"));
        }
        self.sequence += 1;
        Ok(())
    }
    fn begin_chunk(&mut self) -> Result<(), Error> {
        if self.header.is_none() && &self.kind != b"IHDR" {
            return Err(Error::Invalid("IHDR must be first"));
        }
        match &self.kind {
            _ if self.discarded_animation_chunk() => (),
            b"IHDR" => {
                if self.header.is_some() || self.length != 13 {
                    return Err(Error::Invalid("IHDR length or duplication"));
                }
            }
            b"PLTE" => {
                if self.seen_data
                    || !self.palette.is_empty()
                    || self.transparency.is_some()
                    || self.validator.has(b"bKGD")
                {
                    return Err(Error::Invalid("PLTE ordering or duplication"));
                }
                if self.length == 0 || self.length > 768 || !self.length.is_multiple_of(3) {
                    return Err(Error::Invalid("PLTE contents"));
                }
            }
            b"tRNS" => {
                if self.seen_data || self.transparency.is_some() || self.length > 256 {
                    return Err(Error::Invalid("tRNS ordering, length or duplication"));
                }
            }
            b"IDAT" => {
                if self.idat_closed {
                    return Err(Error::Invalid("nonconsecutive IDAT"));
                }
                if !self.seen_data {
                    if self.pre_control.is_some() && self.animation.is_none() {
                        self.animation_fault = Some(AnimationFault::Begin);
                        return Err(Error::Invalid("fcTL without acTL before IDAT"));
                    }
                    let h = self.header.ok_or(Error::Invalid("missing IHDR"))?;
                    if h.color == ColorType::Indexed && self.palette.is_empty() {
                        return Err(Error::Invalid("missing PLTE"));
                    }
                    self.validator.finish()?;
                    self.start_image(h, ImageKind::Default)?;
                    self.seen_data = true;
                }
            }
            b"IEND" => {
                if self.length != 0 || !self.seen_data {
                    return Err(Error::Invalid("IEND contents or missing IDAT"));
                }
            }
            b"acTL" => {
                if self.length != 8 || self.seen_data || self.animation.is_some() {
                    return Err(Error::Invalid("acTL length, order or duplication"));
                }
            }
            b"fcTL" => {
                if self.length != 26 || (self.animation.is_none() && self.seen_data) {
                    return Err(Error::Invalid("fcTL length or missing acTL"));
                }
            }
            b"fdAT" => {
                if self.length < 4
                    || !self.seen_data
                    || !matches!(self.image_kind, Some(ImageKind::AnimationFrame(_)))
                {
                    return Err(Error::Invalid("fdAT length or order"));
                }
                self.prefix_have = 0;
            }
            _ if self.kind[0] & 32 == 0 => return Err(Error::UnsupportedCriticalChunk(self.kind)),
            _ => {
                if self.length > self.validator.remaining() {
                    return Err(Error::LimitExceeded);
                }
            }
        }
        self.left = self.length;
        self.have = 0;
        self.crc = Crc32::new();
        self.crc.update(&self.kind);
        self.data.clear();
        if !matches!(&self.kind, b"IDAT" | b"fdAT") && !self.discarded_animation_chunk() {
            reserve(&mut self.data, self.length)?;
            self.data.resize(self.length, 0);
        }
        self.state = State::ChunkBody;
        Ok(())
    }
    fn complete_chunk(&mut self) -> Result<Option<Event>, Error> {
        if self.discarded_animation_chunk() {
            return Ok(None);
        }
        let h = self.header;
        let d = self.data.as_slice();
        let event = match &self.kind {
            b"IHDR" => {
                let h = Header::parse(d)?;
                if h.sample_bytes()? > self.options.limits.max_output_bytes {
                    return Err(Error::LimitExceeded);
                }
                self.header = Some(h);
                Some(Event::Header)
            }
            b"PLTE" => {
                let h = h.ok_or(Error::Invalid("missing IHDR"))?;
                if matches!(h.color, ColorType::Grayscale | ColorType::GrayscaleAlpha)
                    || (h.color == ColorType::Indexed && self.length / 3 > 1usize << h.depth)
                {
                    return Err(Error::Invalid("PLTE contents"));
                }
                self.palette.extend_from_slice(d.as_chunks::<3>().0);
                Some(Event::Palette)
            }
            b"tRNS" => {
                let h = h.ok_or(Error::Invalid("missing IHDR"))?;
                self.transparency = Some(match h.color {
                    ColorType::Grayscale if d.len() == 2 => Transparency::Grayscale(be16(d)),
                    ColorType::Rgb if d.len() == 6 => {
                        Transparency::Rgb([be16(d), be16(&d[2..]), be16(&d[4..])])
                    }
                    ColorType::Indexed
                        if !self.palette.is_empty() && d.len() <= self.palette.len() =>
                    {
                        Transparency::Palette(d.to_vec())
                    }
                    _ => return Err(Error::Invalid("tRNS contents")),
                });
                Some(Event::Transparency)
            }
            b"IDAT" | b"fdAT" => None,
            b"acTL" => {
                let frames = be32(d);
                let plays = be32(&d[4..]);
                if frames == 0 || frames > 0x7fff_ffff || plays > 0x7fff_ffff {
                    return Err(Error::Invalid("APNG animation control"));
                }
                if frames as usize > self.options.limits.max_frames {
                    return Err(Error::LimitExceeded);
                }
                self.animation = Some((frames, plays));
                Some(Event::Animation)
            }
            b"fcTL" => {
                if self
                    .animation
                    .is_some_and(|(frames, _)| self.controls >= frames)
                    || (!self.seen_data && self.controls != 0)
                {
                    return Err(Error::Invalid("too many APNG frame controls"));
                }
                if self.controls as usize >= self.options.limits.max_frames {
                    return Err(Error::LimitExceeded);
                }
                let c = FrameControl::parse(
                    d,
                    h.ok_or(Error::Invalid("missing IHDR"))?,
                    !self.seen_data,
                )?;
                self.sequence(be32(d))?;
                let index = self.controls;
                self.controls += 1;
                if self.seen_data {
                    self.start_image(
                        Header {
                            width: c.width,
                            height: c.height,
                            ..h.unwrap()
                        },
                        ImageKind::AnimationFrame(index),
                    )?;
                } else {
                    self.default_control = true;
                }
                if self.animation.is_none() {
                    // PNG 5.6 only requires acTL before IDAT. Keep one bounded
                    // default control, and publish Animation before Control.
                    self.pre_control = Some(c);
                    None
                } else {
                    Some(Event::Control {
                        index,
                        control: c,
                        default: !self.seen_data,
                    })
                }
            }
            b"IEND" => {
                self.validator.finish()?;
                if self.animation.is_some_and(|(frames, _)| {
                    frames != self.controls || frames != self.frames_ended
                }) {
                    self.animation_fault = Some(AnimationFault::Complete);
                    return Err(Error::Invalid("APNG frame count mismatch"));
                }
                self.state = State::Done;
                None
            }
            _ => {
                let position = if self.seen_data {
                    ChunkPosition::AfterData
                } else if self.palette.is_empty() {
                    ChunkPosition::BeforePalette
                } else {
                    ChunkPosition::BeforeData
                };
                let chunk = AncillaryChunk {
                    kind: self.kind,
                    data: core::mem::take(&mut self.data),
                    position,
                };
                let value = self.validator.check(
                    &chunk,
                    h.ok_or(Error::Invalid("missing IHDR"))?,
                    self.palette.len(),
                )?;
                self.ancillary = Some(chunk);
                self.metadata = Some(value);
                Some(Event::Metadata)
            }
        };
        Ok(event)
    }
    fn run(&mut self, input: &[u8]) -> Result<DecodeProgress, Error> {
        let mut pos = 0;
        if self.animation.is_some()
            && let Some(control) = self.pre_control.take()
        {
            return Ok(self.emit(
                Event::Control {
                    index: 0,
                    control,
                    default: true,
                },
                0,
            ));
        }
        if let Some(rows) = &mut self.rows {
            rows.advance()?;
        }
        loop {
            match self.state {
                State::Signature => {
                    if !fill(
                        input,
                        &mut pos,
                        &mut self.fixed,
                        &mut self.have,
                        &mut self.input_left,
                    )? {
                        break;
                    }
                    if &self.fixed != SIGNATURE {
                        return Err(Error::Invalid("signature"));
                    }
                    self.have = 0;
                    self.state = State::ChunkHeader;
                }
                State::ChunkHeader => {
                    if !fill(
                        input,
                        &mut pos,
                        &mut self.fixed,
                        &mut self.have,
                        &mut self.input_left,
                    )? {
                        break;
                    }
                    self.length = be32(&self.fixed) as usize;
                    if self.length > 0x7fff_ffff {
                        return Err(Error::Invalid("chunk length exceeds 31 bits"));
                    }
                    self.kind = self.fixed[4..8].try_into().unwrap();
                    if !self.kind.iter().all(u8::is_ascii_alphabetic) {
                        return Err(Error::Invalid("chunk type"));
                    }
                    self.chunks = self.chunks.checked_add(1).ok_or(Error::LimitExceeded)?;
                    if self.chunks > self.options.limits.max_chunks {
                        return Err(Error::LimitExceeded);
                    }
                    self.state = State::ChunkStart;
                    let close_default =
                        self.seen_data && !self.idat_closed && &self.kind != b"IDAT";
                    if close_default {
                        self.idat_closed = true;
                    }
                    let close_frame = matches!(self.image_kind, Some(ImageKind::AnimationFrame(_)))
                        && matches!(&self.kind, b"fcTL" | b"IEND");
                    if close_default || close_frame {
                        let result = self.end_image();
                        let kind = if close_frame {
                            self.animation_result(result, AnimationFault::EndFrame)?
                        } else {
                            result?
                        };
                        return Ok(self.emit(Event::ImageEnd(kind), pos));
                    }
                }
                State::ChunkStart => {
                    let result = self.begin_chunk();
                    if self.header.is_some() && animation_chunk(&self.kind) {
                        self.animation_result(result, AnimationFault::Begin)?;
                    } else {
                        result?;
                    }
                }
                State::ChunkBody => {
                    if &self.kind == b"fdAT"
                        && self.prefix_have < 4
                        && !self.discarded_animation_chunk()
                    {
                        let start = pos;
                        let complete = fill(
                            input,
                            &mut pos,
                            &mut self.prefix,
                            &mut self.prefix_have,
                            &mut self.input_left,
                        )?;
                        self.crc.update(&input[start..pos]);
                        self.left -= pos - start;
                        if !complete {
                            break;
                        }
                        let result = self.sequence(be32(&self.prefix));
                        self.animation_result(result, AnimationFault::Body)?;
                    }
                    if matches!(&self.kind, b"IDAT" | b"fdAT") && !self.discarded_animation_chunk()
                    {
                        let available = self.left.min(input.len() - pos).min(self.input_left);
                        let rows = self
                            .rows
                            .as_mut()
                            .ok_or(Error::Invalid("missing image decoder"))?;
                        if !rows.is_done() {
                            let result = rows.feed(
                                &input[pos..pos + available],
                                self.palette.len(),
                                self.options.palette_policy,
                            );
                            let (used, event) = if &self.kind == b"fdAT" {
                                // This inflater is capped at the exact filtered
                                // frame length; exceeding it is malformed frame
                                // data, not a caller resource-limit exception.
                                let result = result.map_err(|error| {
                                    if error == Error::LimitExceeded {
                                        Error::Invalid("APNG frame data length")
                                    } else {
                                        error
                                    }
                                });
                                self.animation_result(result, AnimationFault::Body)?
                            } else {
                                result?
                            };
                            self.crc.update(&input[pos..pos + used]);
                            self.left -= used;
                            self.input_left -= used;
                            pos += used;
                            self.update_memory();
                            if event {
                                return Ok(self.emit(Event::Row, pos));
                            }
                        }
                        if self.rows.as_ref().is_some_and(ImageRows::is_done) {
                            let n = self.left.min(input.len() - pos).min(self.input_left);
                            self.crc.update(&input[pos..pos + n]);
                            self.left -= n;
                            self.input_left -= n;
                            pos += n;
                        }
                    } else {
                        let n = self.left.min(input.len() - pos).min(self.input_left);
                        let start = self.length - self.left;
                        if !self.discarded_animation_chunk() {
                            self.data[start..start + n].copy_from_slice(&input[pos..pos + n]);
                        }
                        self.crc.update(&input[pos..pos + n]);
                        self.left -= n;
                        self.input_left -= n;
                        pos += n;
                    }
                    if self.left != 0 {
                        if pos < input.len() && self.input_left == 0 {
                            return Err(Error::LimitExceeded);
                        }
                        break;
                    }
                    self.have = 0;
                    self.state = State::ChunkCrc;
                }
                State::ChunkCrc => {
                    if !fill(
                        input,
                        &mut pos,
                        &mut self.fixed[..4],
                        &mut self.have,
                        &mut self.input_left,
                    )? {
                        break;
                    }
                    if be32(&self.fixed) != self.crc.finish() {
                        if animation_chunk(&self.kind) {
                            self.animation_fault = Some(AnimationFault::Complete);
                        }
                        return Err(Error::Checksum);
                    }
                    let result = self.complete_chunk();
                    let event = if animation_chunk(&self.kind) {
                        self.animation_result(result, AnimationFault::Complete)?
                    } else {
                        result?
                    };
                    if !matches!(self.state, State::Done) {
                        self.have = 0;
                        self.state = State::ChunkHeader;
                    }
                    if let Some(event) = event {
                        return Ok(self.emit(event, pos));
                    }
                }
                State::Done => {
                    return Ok(DecodeProgress {
                        consumed: pos,
                        status: DecodeStatus::Done,
                    });
                }
            }
        }
        Ok(DecodeProgress {
            consumed: pos,
            status: DecodeStatus::NeedInput,
        })
    }
}
