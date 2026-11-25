use super::{
    DecodeEvent, DecodeProgress, DecodeStatus, ImageInfo, ImageKind,
    read::row_image,
    rows::{ImageRows, RowBuffers},
};
use crate::{
    AncillaryChunk, AnimationPolicy, ChunkPosition, ColorType, DecodeOptions, Error, FrameControl,
    Metadata, MetadataPolicy, Png, Transparency,
    checksum::Crc32,
    codec::{Header, SIGNATURE, be16, be32, chunk_type_valid},
    metadata::Validator,
    reserve,
};

const SMALL_IDAT_BYTES: usize = 512;
const IDAT_BATCH_BYTES: usize = 4096;

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
/// The image pipeline owns at most 32 KiB of DEFLATE history and a bounded number of rows.
/// Metadata and suggested-palette names are bounded by the configured metadata
/// budget. No complete image, frame, compressed payload or IDAT chunk is stored.
/// `process` stops at each event and leaves bytes after IEND unconsumed. Every
/// returned event must be handled before calling `process` again.
pub struct StreamDecoder {
    options: DecodeOptions,
    state: State,
    event: Option<Event>,
    failure: Option<Error>,
    fixed: [u8; 13],
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
    full_events: bool,
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
    buffers: RowBuffers,
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

#[inline]
fn field<'a, const N: usize>(
    input: &'a [u8],
    pos: &mut usize,
    scratch: &'a mut [u8; N],
    have: &mut usize,
    budget: &mut usize,
) -> Result<Option<&'a [u8; N]>, Error> {
    if *have == 0
        && *budget >= N
        && let Some(bytes) = input[*pos..].first_chunk::<N>()
    {
        *pos += N;
        *budget -= N;
        *have = N;
        return Ok(Some(bytes));
    }
    if fill(input, pos, scratch, have, budget)? {
        Ok(Some(scratch))
    } else {
        Ok(None)
    }
}

impl StreamDecoder {
    pub fn new(options: DecodeOptions) -> Self {
        Self::with_events(options, true)
    }
    pub(super) fn for_reader(options: DecodeOptions) -> Self {
        Self::with_events(options, false)
    }
    fn with_events(options: DecodeOptions, full_events: bool) -> Self {
        Self {
            options,
            state: State::Signature,
            event: None,
            failure: None,
            fixed: [0; 13],
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
            validator: Validator::new(options.limits.max_metadata_bytes, options.metadata_policy),
            full_events,
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
            buffers: RowBuffers::default(),
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
    pub(super) fn owned_ancillary(&mut self) -> Option<AncillaryChunk> {
        let chunk = self.ancillary.as_ref()?;
        if chunk.data.capacity() == chunk.data.len() {
            self.ancillary.take()
        } else {
            Some(chunk.clone())
        }
    }
    pub(super) fn take_palette_and_transparency(&mut self) -> (Vec<[u8; 3]>, Option<Transparency>) {
        (core::mem::take(&mut self.palette), self.transparency.take())
    }
    pub fn process(&mut self, input: &[u8]) -> Result<DecodeProgress, Error> {
        self.process_into(input, None)
    }
    pub(super) fn process_into(
        &mut self,
        input: &[u8],
        mut png: Option<&mut Png>,
    ) -> Result<DecodeProgress, Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if matches!(self.event.take(), Some(Event::Metadata)) {
            if let Some(chunk) = self.ancillary.take() {
                self.data = chunk.data;
                self.data.clear();
            }
            self.metadata = None;
        }
        let initial = self.total_in();
        loop {
            self.animation_fault = None;
            let consumed = self.total_in() - initial;
            match self.run(&input[consumed..], png.as_deref_mut()) {
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
        self.buffers = RowBuffers::default();
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
        let palette = self.palette.len();
        // PLTE is complete before the first IDAT and shared by all frames.
        let strict_palette = if h.color == ColorType::Indexed
            && self.options.palette_policy == crate::PalettePolicy::Strict
            && palette < 1usize << h.depth
        {
            Some(palette as u8)
        } else {
            None
        };
        self.rows = Some(ImageRows::new(
            h,
            kind,
            self.options.limits.max_output_bytes,
            core::mem::take(&mut self.buffers),
            self.full_events,
            strict_palette,
        )?);
        self.image_kind = Some(kind);
        self.update_memory();
        Ok(())
    }
    fn end_image(&mut self) -> Result<ImageKind, Error> {
        let rows = self
            .rows
            .take()
            .ok_or(Error::Invalid("missing image data"))?;
        let (kind, buffers) = rows.finish()?;
        if kind != ImageKind::Default || self.default_control {
            self.frames_ended += 1;
        }
        if self
            .animation
            .is_some_and(|(frames, _)| self.frames_ended < frames)
        {
            self.buffers = buffers;
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
                    || (self.options.metadata_policy == MetadataPolicy::Strict
                        && self.validator.has(b"bKGD"))
                {
                    return Err(Error::Invalid("PLTE ordering or duplication"));
                }
                if self.length == 0 || self.length > 768 || !self.length.is_multiple_of(3) {
                    return Err(Error::Invalid("PLTE contents"));
                }
                let entries = self.length / 3;
                reserve(&mut self.palette, entries)?;
                self.palette.resize(entries, [0; 3]);
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
        self.crc = Crc32::with_type(self.kind);
        self.data.clear();
        if !matches!(&self.kind, b"IHDR" | b"PLTE" | b"IDAT" | b"fdAT")
            && !self.discarded_animation_chunk()
        {
            reserve(&mut self.data, self.length)?;
        }
        self.state = State::ChunkBody;
        Ok(())
    }
    fn complete_chunk(&mut self) -> Result<Option<Event>, Error> {
        if self.discarded_animation_chunk() {
            return Ok(None);
        }
        let h = self.header;
        let d = if &self.kind == b"IHDR" {
            &self.fixed[..]
        } else {
            self.data.as_slice()
        };
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
                let h = h.ok_or(Error::Invalid("missing IHDR"))?;
                let value = if self.full_events {
                    self.validator.check::<true>(&chunk, h, self.palette.len())
                } else {
                    self.validator.check::<false>(&chunk, h, self.palette.len())
                }?;
                let Some(value) = value else {
                    self.data = chunk.data;
                    return Ok(None);
                };
                self.ancillary = Some(chunk);
                self.metadata = Some(value);
                Some(Event::Metadata)
            }
        };
        Ok(event)
    }
    #[inline]
    fn read_chunk_header(&mut self, header: [u8; 8], continuing_idat: bool) -> Result<(), Error> {
        self.length = be32(&header) as usize;
        if self.length > 0x7fff_ffff {
            return Err(Error::Invalid("chunk length exceeds 31 bits"));
        }
        if !continuing_idat {
            self.kind = header[4..].try_into().unwrap();
            if !chunk_type_valid(self.kind) {
                return Err(Error::Invalid("chunk type"));
            }
        }
        self.chunks = self.chunks.checked_add(1).ok_or(Error::LimitExceeded)?;
        if self.chunks > self.options.limits.max_chunks {
            return Err(Error::LimitExceeded);
        }
        Ok(())
    }

    fn continue_idat(&mut self) {
        self.left = self.length;
        self.have = 0;
        self.crc = const { Crc32::with_type(*b"IDAT") };
        self.state = State::ChunkBody;
    }

    #[inline(never)]
    fn collect_stored_idats(
        &mut self,
        input: &[u8],
        png: &mut Png,
        stored_remaining: usize,
    ) -> Result<usize, Error> {
        // Stop inside the stored block, before any new block header or Adler
        // trailer can change the ordering of inflate and row validation.
        let capacity = (stored_remaining - 1).min(IDAT_BATCH_BYTES);
        let chunks_left = self.options.limits.max_chunks - self.chunks;
        if chunks_left == 0 {
            return Ok(0);
        }
        let input = &input[..input.len().min(self.input_left)];
        let mut batch = [0; IDAT_BATCH_BYTES];
        let mut written = 0;
        let mut consumed = 0;
        let mut next_body = 0;
        let mut length = self.length;
        let mut count = 0;
        loop {
            let end = next_body + length;
            if written + length > capacity || input.len() - next_body < length + 4 {
                break;
            }
            let body = &input[next_body..end];
            let mut crc = const { Crc32::with_type(*b"IDAT") };
            crc.update(body);
            if crc.finish() != be32(&input[end..end + 4]) {
                break;
            }
            batch[written..written + length].copy_from_slice(body);
            written += length;
            consumed = end + 4;
            count += 1;
            if count > chunks_left {
                break;
            }
            let Some(header) = input[consumed..].first_chunk::<8>() else {
                break;
            };
            length = be32(header) as usize;
            if length > SMALL_IDAT_BYTES || &header[4..] != b"IDAT" {
                break;
            }
            next_body = consumed + 8;
        }
        if count < 2 {
            return Ok(0);
        }

        let mut staged = 0;
        let event = self.feed_image(&batch[..written], &mut staged, Some(png), false)?;
        debug_assert!(!event && staged == written);
        self.chunks += count - 1; // The first IDAT header was already accepted.
        self.input_left -= consumed;
        self.left = 0;
        self.state = State::ChunkHeader;
        Ok(consumed)
    }

    #[inline(never)]
    fn feed_image(
        &mut self,
        input: &[u8],
        pos: &mut usize,
        mut png: Option<&mut Png>,
        frame_data: bool,
    ) -> Result<bool, Error> {
        let rows = self
            .rows
            .as_mut()
            .ok_or(Error::Invalid("missing image decoder"))?;
        if !frame_data
            && let Some(png) = png.as_deref_mut()
            && rows.collect_in_image(input, pos, &mut png.image)?
        {
            if rows.is_done() {
                *pos = input.len();
            }
            return Ok(false);
        }
        while !rows.is_done() {
            let result = rows.feed(&input[*pos..]);
            let buffers_can_grow = rows.buffers_can_grow();
            let (used, event) = result.map_err(|error| {
                if frame_data {
                    self.animation_fault = Some(AnimationFault::Body);
                    // This limit is the frame's exact filtered size.
                    if error == Error::LimitExceeded {
                        return Error::Invalid("APNG frame data length");
                    }
                }
                error
            })?;
            *pos += used;
            if buffers_can_grow {
                self.peak_image_buffers = self.peak_image_buffers.max(rows.memory_bytes());
            }
            if !event {
                break;
            }
            if let Some(png) = png.as_deref_mut() {
                let kind = self
                    .image_kind
                    .ok_or(Error::Invalid("missing image decoder"))?;
                rows.copy_into(row_image(png, kind)?)?;
                rows.advance()?;
            } else {
                return Ok(true);
            }
        }
        if rows.is_done() {
            *pos = input.len();
        }
        Ok(false)
    }

    fn run(&mut self, input: &[u8], mut png: Option<&mut Png>) -> Result<DecodeProgress, Error> {
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
                    let Some(signature) = field(
                        input,
                        &mut pos,
                        self.fixed.first_chunk_mut::<8>().unwrap(),
                        &mut self.have,
                        &mut self.input_left,
                    )?
                    else {
                        break;
                    };
                    if signature != SIGNATURE {
                        return Err(Error::Invalid("signature"));
                    }
                    self.have = 0;
                    self.state = State::ChunkHeader;
                }
                State::ChunkHeader => {
                    let Some(header) = field(
                        input,
                        &mut pos,
                        self.fixed.first_chunk_mut::<8>().unwrap(),
                        &mut self.have,
                        &mut self.input_left,
                    )?
                    else {
                        break;
                    };
                    let header = *header;
                    self.read_chunk_header(header, false)?;
                    let close_default = self.seen_data && !self.idat_closed;
                    if close_default {
                        if &self.kind == b"IDAT" {
                            // The open IDAT stream already has a validated image.
                            self.continue_idat();
                            continue;
                        }
                        self.idat_closed = true;
                    }
                    self.state = State::ChunkStart;
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
                    let frame_data = &self.kind == b"fdAT";
                    if &self.kind == b"IDAT" || (frame_data && self.animation_error.is_none()) {
                        if frame_data && self.prefix_have < 4 {
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
                        let end = pos + self.left.min(input.len() - pos).min(self.input_left);
                        let start = pos;
                        let result = self.feed_image(
                            &input[..end],
                            &mut pos,
                            png.as_deref_mut(),
                            frame_data,
                        );
                        // Commit the whole accepted span together, including
                        // on error so APNG recovery replays only unread bytes.
                        let n = pos - start;
                        if n != 0 {
                            self.crc.update(&input[start..pos]);
                            self.left -= n;
                            self.input_left -= n;
                        }
                        if result? {
                            return Ok(self.emit(Event::Row, pos));
                        }
                    } else {
                        let n = self.left.min(input.len() - pos).min(self.input_left);
                        let bytes = &input[pos..pos + n];
                        if !self.discarded_animation_chunk() {
                            let start = self.length - self.left;
                            match &self.kind {
                                b"IHDR" => self.fixed[start..start + n].copy_from_slice(bytes),
                                b"PLTE" => self.palette.as_flattened_mut()[start..start + n]
                                    .copy_from_slice(bytes),
                                _ => self.data.extend_from_slice(bytes),
                            }
                        }
                        self.crc.update(bytes);
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
                    let Some(crc) = field(
                        input,
                        &mut pos,
                        &mut self.prefix,
                        &mut self.have,
                        &mut self.input_left,
                    )?
                    else {
                        break;
                    };
                    if be32(crc) != self.crc.finish() {
                        if self.options.metadata_policy.discard_bad_crc(&self.kind) {
                            self.validator.charge(self.length)?;
                            self.have = 0;
                            self.data.clear();
                            self.state = State::ChunkHeader;
                            continue;
                        }
                        if animation_chunk(&self.kind) {
                            self.animation_fault = Some(AnimationFault::Complete);
                        }
                        return Err(Error::Checksum);
                    }
                    if matches!(&self.kind, b"IDAT" | b"fdAT") {
                        self.have = 0;
                        self.state = State::ChunkHeader;
                        // The common IDAT continuation needs neither general
                        // chunk dispatch nor a second read of the kind.
                        if &self.kind == b"IDAT"
                            && self.input_left >= 8
                            && let Some(header) = input[pos..].first_chunk::<8>()
                            && &header[4..] == b"IDAT"
                        {
                            pos += 8;
                            self.input_left -= 8;
                            self.have = 8;
                            self.read_chunk_header(*header, true)?;
                            self.continue_idat();
                            if self.length <= SMALL_IDAT_BYTES
                                && input.len() - pos >= self.length + 16
                                && let Some(png) = png.as_deref_mut()
                            {
                                let stored_remaining =
                                    self.rows.as_ref().map_or(0, ImageRows::stored_remaining);
                                if stored_remaining > self.length.max(1) {
                                    pos += self.collect_stored_idats(
                                        &input[pos..],
                                        png,
                                        stored_remaining,
                                    )?;
                                }
                            }
                        }
                        continue;
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
