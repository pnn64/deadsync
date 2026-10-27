//! Vorbis I Appendix A mapping on the independent Ogg container.
use crate::{
    CodebookBehavior, Comments, DecodeOptions, Error, Floor0Behavior, Floor1Behavior,
    Identification, Limits, Metadata, PacketDecoder, ParseOptions, PcmBlock, Residue2Layout,
    ResidueBehavior, Setup, reserve,
};
use deadlib_audio_decode_ogg::{
    self as container, Demuxer, Packet, Page, PageDecoder, PageHeader, PageReader,
    UNKNOWN_GRANULE_POSITION,
};
use std::io::Read;

#[derive(Clone, Copy, Debug)]
pub struct OggOptions {
    pub limits: Limits,
    pub container_limits: container::Limits,
    /// Explicit legacy exception for granule zero on unfinished pages.
    pub allow_zero_span_granules: bool,
    /// Explicit reference behavior for unaligned type-two residue ranges.
    pub residue2_layout: Residue2Layout,
    pub floor0_behavior: Floor0Behavior,
    pub floor1_behavior: Floor1Behavior,
    /// Retain custom-float interpretation through setup parsing and resets.
    pub codebook_behavior: CodebookBehavior,
    pub residue_behavior: ResidueBehavior,
    /// Use `ParseOptions::allow_empty_codebooks` for every logical stream.
    pub allow_empty_codebooks: bool,
    /// Use `DecodeOptions::allow_nonfinite` for every logical stream.
    pub allow_nonfinite: bool,
    pub max_active_streams: usize,
    /// Combined retained setup, metadata and synthesis elements across active
    /// streams. Container assembly storage has its own independent limit.
    pub max_stream_memory: usize,
}
impl Default for OggOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            container_limits: container::Limits::default(),
            allow_zero_span_granules: false,
            residue2_layout: Residue2Layout::Interleaved,
            floor0_behavior: Floor0Behavior::Vorbis,
            floor1_behavior: Floor1Behavior::Vorbis,
            codebook_behavior: CodebookBehavior::Vorbis,
            residue_behavior: ResidueBehavior::Vorbis,
            allow_empty_codebooks: false,
            allow_nonfinite: false,
            max_active_streams: 16,
            max_stream_memory: 256 << 20,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamId {
    pub serial: u32,
    pub chain: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StreamInfo {
    pub id: StreamId,
    pub identification: Identification,
    pub metadata: Metadata,
}
pub struct OggPcm<'a> {
    pub stream: StreamId,
    pub sample_rate: u32,
    /// Position relative to the first retained PCM frame in this logical stream.
    pub start_frame: u64,
    /// Original sample timestamp, including any positive broadcast offset.
    pub start_granule: u64,
    /// Cropped, borrowed PCM; its raw start_frame remains the synthesis position.
    pub pcm: PcmBlock<'a>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamEnd {
    pub stream: StreamId,
    pub frames: u64,
    pub start_granule: u64,
    pub end_granule: u64,
}
pub enum OggEvent<'a> {
    /// Emitted once setup succeeds, before any PCM from this stream.
    StreamStart(&'a StreamInfo),
    Pcm(OggPcm<'a>),
    StreamEnd(StreamEnd),
    /// Unknown codecs are physically validated and skipped without codec setup.
    IgnoredStream(StreamId),
    /// Truncated/malformed comment bodies are nonfatal; metadata becomes empty.
    CommentError {
        stream: StreamId,
        error: Error,
    },
}

#[derive(Default)]
pub(crate) struct Timing {
    pub(crate) raw_end: u64,
    pub(crate) origin: Option<i128>,
    pub(crate) frames: u64,
    last_span: u64,
    extended_end_trim: bool,
}
pub(crate) struct Crop {
    pub(crate) skip: usize,
    pub(crate) keep: usize,
    pub(crate) start_frame: u64,
    pub(crate) start_granule: u64,
}
impl Timing {
    pub(crate) fn with_extended_end_trim(allow: bool) -> Self {
        Self {
            extended_end_trim: allow,
            ..Self::default()
        }
    }
    pub(crate) fn advance(
        &mut self,
        raw_start: u64,
        frames: usize,
        has_granule: bool,
        page: PageHeader,
        ends_packet: bool,
    ) -> Result<Option<Crop>, Error> {
        self.raw_end = raw_start
            .checked_add(frames as u64)
            .ok_or(Error::LimitExceeded("PCM frame position"))?;
        self.last_span = frames as u64;
        if frames == 0 {
            return Ok(None);
        }
        if self.origin.is_none() {
            let origin = if has_granule {
                let end = granule(page.granule_position)? as i128;
                // A reduced first audio page that is also EOS describes tail
                // padding; initial and final trim cannot be distinguished there.
                if page.is_eos() && end <= self.raw_end as i128 {
                    0
                } else {
                    end - self.raw_end as i128
                }
            } else {
                0
            };
            if origin != 0 && (!has_granule || !ends_packet) {
                return Err(Error::Invalid(
                    "nonzero initial offset requires the second audio packet to flush its page",
                ));
            }
            if origin < -(frames as i128) {
                return Err(Error::Invalid("initial trim exceeds first overlap span"));
            }
            self.origin = Some(origin);
        }
        let start = self.origin.unwrap() + raw_start as i128;
        let skip = (-start).clamp(0, frames as i128) as usize;
        let keep = if page.is_eos() {
            (granule(page.granule_position)? as i128 - start).clamp(0, frames as i128) as usize
        } else {
            frames
        };
        if has_granule {
            check_granule(self, page, frames as u64)?;
        }
        if keep < skip {
            return Err(Error::Invalid("negative retained PCM interval"));
        }
        let crop = Crop {
            skip,
            keep,
            start_frame: self.frames,
            start_granule: (start + skip as i128) as u64,
        };
        self.frames = self
            .frames
            .checked_add((keep - skip) as u64)
            .ok_or(Error::LimitExceeded("stream frame count"))?;
        Ok(Some(crop))
    }
    pub(crate) fn check_page(&self, page: PageHeader) -> Result<(), Error> {
        if self.origin.is_none() && self.raw_end == 0 {
            if page.granule_position != 0 {
                return Err(Error::Invalid("granule before any complete overlap output"));
            }
            return Ok(());
        }
        check_granule(self, page, self.last_span)
    }
    pub(crate) fn end(&self, id: StreamId, page: PageHeader) -> Result<StreamEnd, Error> {
        let origin = self.origin.unwrap_or(0).max(0) as u64;
        let end = granule(page.granule_position)?;
        let frames = end
            .checked_sub(origin)
            .ok_or(Error::Invalid("final Vorbis sample count"))?;
        if (!self.extended_end_trim && frames != self.frames)
            || end as i128 > self.origin.unwrap_or(0) + self.raw_end as i128
        {
            return Err(Error::Invalid("final Vorbis sample count"));
        }
        Ok(StreamEnd {
            stream: id,
            frames,
            start_granule: origin,
            end_granule: end,
        })
    }
    pub(crate) fn end_padding(&self, page: PageHeader) -> Result<(u64, bool), Error> {
        let padding = self.origin.unwrap_or(0) + self.raw_end as i128
            - granule(page.granule_position)? as i128;
        let padding =
            u64::try_from(padding).map_err(|_| Error::Invalid("negative Vorbis end padding"))?;
        Ok((padding, padding > self.last_span))
    }
}
struct Stream {
    id: StreamId,
    info: Option<StreamInfo>,
    phase: u8,
    decoder: Option<PacketDecoder>,
    setup_page: u64,
    timing: Timing,
    memory: usize,
}

/// Page-driven physical Ogg Vorbis decoder. Multiplexed streams maintain
/// independent headers, sample rates, layouts, timestamps and overlap state.
/// Unknown codecs are skipped. Completed streams release their codec storage.
/// Mapping/transport errors are terminal until reset; callback output remains
/// provisional until successful StreamEnd/finish validation.
pub struct OggDecoder {
    options: OggOptions,
    demux: Demuxer,
    streams: Vec<Stream>,
    memory: usize,
    failed: bool,
}
impl Default for OggDecoder {
    fn default() -> Self {
        Self::new(OggOptions::default())
    }
}
impl OggDecoder {
    pub fn new(options: OggOptions) -> Self {
        Self {
            options,
            demux: Demuxer::with_validation(
                options.container_limits,
                container::ValidationOptions {
                    allow_zero_span_granules: options.allow_zero_span_granules,
                    ..container::ValidationOptions::default()
                },
            ),
            streams: Vec::new(),
            memory: 0,
            failed: false,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.options);
    }
    /// Active stream information; ended streams are delivered through events
    /// and released. Clone StreamInfo in the callback if retention is needed.
    pub fn stream(&self, id: StreamId) -> Option<&StreamInfo> {
        self.streams.iter().find(|s| s.id == id)?.info.as_ref()
    }
    pub fn storage_bytes(&self) -> usize {
        self.memory
    }
    pub fn compatibility(&self) -> container::CompatibilityStats {
        self.demux.compatibility()
    }
    /// Supply pages in physical order with their absolute byte offsets. Events
    /// borrow page/decoder storage and must be consumed before this call returns.
    pub fn push_page(
        &mut self,
        page: Page<'_>,
        offset: u64,
        mut emit: impl FnMut(OggEvent<'_>),
    ) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self.page_inner(page, offset, &mut emit);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn page_inner(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: &mut impl FnMut(OggEvent<'_>),
    ) -> Result<(), Error> {
        let header = page.header();
        let ends_packet = page.lacing().last().is_none_or(|&n| n < 255);
        let mut failure = None;
        let Self {
            demux,
            streams,
            options,
            memory,
            ..
        } = self;
        demux.push_page(page, offset, |packet| {
            if failure.is_some() {
                return;
            }
            let result = Self::packet(
                streams,
                options,
                memory,
                packet,
                header,
                ends_packet,
                page.bytes().len(),
                emit,
            );
            if let Err(error) = result {
                failure = Some(error);
            }
        })?;
        if let Some(error) = failure {
            return Err(error);
        }
        let index = streams
            .iter()
            .position(|s| s.id.serial == header.serial)
            .ok_or(Error::Invalid("missing logical stream context"))?;
        let stream = &streams[index];
        if stream.info.is_some() {
            if stream.phase != 3 {
                if header.granule_position != 0
                    && header.granule_position != UNKNOWN_GRANULE_POSITION
                {
                    return Err(Error::Invalid("nonzero Vorbis header granule"));
                }
            } else if stream.setup_page == offset {
                if header.granule_position != 0 {
                    return Err(Error::Invalid("setup page granule"));
                }
            } else if page.completed_packets() != 0 || header.is_eos() {
                stream.timing.check_page(header)?;
            }
        }
        if header.is_eos() {
            if let Some(info) = stream.info.as_ref() {
                if stream.phase != 3 {
                    return Err(Error::Invalid("EOS before all Vorbis headers"));
                }
                emit(OggEvent::StreamEnd(stream.timing.end(info.id, header)?));
            }
            let ended = streams.swap_remove(index);
            *memory -= ended.memory;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn packet(
        streams: &mut Vec<Stream>,
        options: &OggOptions,
        memory: &mut usize,
        packet: Packet<'_>,
        page: PageHeader,
        ends_packet: bool,
        page_len: usize,
        emit: &mut impl FnMut(OggEvent<'_>),
    ) -> Result<(), Error> {
        let id = StreamId {
            serial: packet.info.serial,
            chain: packet.info.chain,
        };
        if packet.info.first_in_stream {
            if streams.len() >= options.max_active_streams {
                return Err(Error::LimitExceeded("active Ogg streams"));
            }
            reserve(streams, 1)?;
            let info =
                if let Some(identification) = read_identification(packet.data, page, page_len)? {
                    Some(StreamInfo {
                        id,
                        identification,
                        metadata: Metadata::default(),
                    })
                } else {
                    emit(OggEvent::IgnoredStream(id));
                    None
                };
            streams.push(Stream {
                id,
                info,
                phase: 1,
                decoder: None,
                setup_page: 0,
                timing: Timing::default(),
                memory: 0,
            });
            return Ok(());
        }
        let stream = streams
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(Error::Invalid("unknown Ogg stream"))?;
        let Some(info) = &mut stream.info else {
            return Ok(());
        };
        match stream.phase {
            1 => {
                let available = options.max_stream_memory - *memory;
                let (metadata, warning) = read_metadata(packet.data, options.limits, available)?;
                if let Some(error) = warning {
                    emit(OggEvent::CommentError { stream: id, error });
                }
                charge(memory, metadata.storage_bytes(), options)?;
                stream.memory += metadata.storage_bytes();
                info.metadata = metadata;
                stream.phase = 2;
                if page.granule_position != 0 && packet.info.granule_position.is_some() {
                    return Err(Error::Invalid("comment page granule"));
                }
            }
            2 => {
                if packet.info.granule_position != Some(0) || !ends_packet {
                    return Err(Error::Invalid(
                        "setup header must finish its page at granule zero",
                    ));
                }
                let mut limits = options.limits;
                limits.max_setup_memory = limits
                    .max_setup_memory
                    .min(options.max_stream_memory - *memory);
                let setup = Setup::parse_with_options(
                    packet.data,
                    &info.identification,
                    ParseOptions {
                        limits,
                        allow_empty_codebooks: options.allow_empty_codebooks,
                        codebook_behavior: options.codebook_behavior,
                        residue_behavior: options.residue_behavior,
                    },
                )?;
                let setup_memory = setup.storage_bytes();
                let available = options
                    .max_stream_memory
                    .checked_sub(*memory)
                    .and_then(|n| n.checked_sub(setup_memory))
                    .ok_or(Error::LimitExceeded("active stream memory"))?;
                limits.max_decode_memory = limits.max_decode_memory.min(available);
                let decoder = PacketDecoder::with_options(
                    setup,
                    DecodeOptions {
                        limits,
                        residue2_layout: options.residue2_layout,
                        floor0_behavior: options.floor0_behavior,
                        floor1_behavior: options.floor1_behavior,
                        allow_nonfinite: options.allow_nonfinite,
                    },
                )?;
                charge(memory, setup_memory + decoder.storage_bytes(), options)?;
                stream.memory += setup_memory + decoder.storage_bytes();
                stream.decoder = Some(decoder);
                stream.setup_page = packet.info.end_page_offset;
                stream.phase = 3;
                emit(OggEvent::StreamStart(info));
            }
            3 => {
                if packet.info.start_page_offset == stream.setup_page {
                    return Err(Error::Invalid("audio must begin on a fresh page"));
                }
                let decoder = stream.decoder.as_mut().unwrap();
                let Some(block) = decoder.decode(packet.data)? else {
                    return Ok(());
                };
                let Some(crop) = stream.timing.advance(
                    block.start_frame,
                    block.frames,
                    packet.info.granule_position.is_some(),
                    page,
                    ends_packet,
                )?
                else {
                    return Ok(());
                };
                if crop.keep != crop.skip {
                    emit(OggEvent::Pcm(OggPcm {
                        stream: id,
                        sample_rate: info.identification.sample_rate,
                        start_frame: crop.start_frame,
                        start_granule: crop.start_granule,
                        pcm: block.crop(crop.skip, crop.keep),
                    }));
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self.demux.finish().map_err(Error::from).and({
            if self.streams.is_empty() {
                Ok(())
            } else {
                Err(Error::Invalid("unfinished Vorbis streams"))
            }
        });
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
pub(crate) fn read_identification(
    packet: &[u8],
    page: PageHeader,
    page_len: usize,
) -> Result<Option<Identification>, Error> {
    if !packet.starts_with(b"\x01vorbis") {
        return Ok(None);
    }
    if page_len != 58 || page.granule_position != 0 {
        return Err(Error::Invalid(
            "Vorbis identification page must be 58 bytes at granule zero",
        ));
    }
    Identification::parse(packet).map(Some)
}
pub(crate) fn read_metadata(
    packet: &[u8],
    mut limits: Limits,
    available: usize,
) -> Result<(Metadata, Option<Error>), Error> {
    limits.max_comment_bytes = limits.max_comment_bytes.min(available);
    limits.max_comments = limits
        .max_comments
        .min(available / core::mem::size_of::<core::ops::Range<usize>>());
    match Comments::parse(packet, &limits) {
        Ok(comments) => {
            let required = comments
                .comments
                .len()
                .checked_mul(core::mem::size_of::<core::ops::Range<usize>>())
                .and_then(|n| n.checked_add(comments.vendor.len()))
                .and_then(|n| {
                    comments
                        .comments
                        .iter()
                        .try_fold(n, |n, field| n.checked_add(field.len()))
                });
            if required.is_none_or(|n| n > available) {
                return Err(Error::LimitExceeded("active stream memory"));
            }
            Ok((comments.to_owned()?, None))
        }
        Err(error @ (Error::EndOfPacket | Error::Invalid("comment framing bit"))) => {
            Ok((Metadata::default(), Some(error)))
        }
        Err(error) => Err(error),
    }
}
fn charge(memory: &mut usize, amount: usize, options: &OggOptions) -> Result<(), Error> {
    *memory = memory
        .checked_add(amount)
        .filter(|&n| n <= options.max_stream_memory)
        .ok_or(Error::LimitExceeded("active stream memory"))?;
    Ok(())
}
pub(crate) fn granule(value: u64) -> Result<u64, Error> {
    if value > i64::MAX as u64 {
        return Err(Error::Invalid("unknown/negative Vorbis granule"));
    }
    Ok(value)
}
fn check_granule(timing: &Timing, page: PageHeader, span: u64) -> Result<(), Error> {
    let end = granule(page.granule_position)? as i128;
    let expected = timing.origin.unwrap_or(0) + timing.raw_end as i128;
    if page.is_eos() {
        if end > expected || (!timing.extended_end_trim && end < expected - span as i128) {
            return Err(Error::Invalid("EOS granule outside final packet overlap"));
        }
    } else if end != expected {
        return Err(Error::Invalid(
            "Vorbis granule disagrees with decoded sample position",
        ));
    }
    Ok(())
}

/// Incremental byte-driven decoder. Each push consumes at most one Ogg page;
/// drive it until the supplied chunk is consumed. Events borrow decoder storage.
pub struct OggStreamDecoder {
    pages: PageDecoder,
    decoder: OggDecoder,
    failed: bool,
    offset: u64,
}
impl Default for OggStreamDecoder {
    fn default() -> Self {
        Self::new(OggOptions::default())
    }
}
impl OggStreamDecoder {
    pub fn new(options: OggOptions) -> Self {
        Self {
            pages: PageDecoder::new(),
            decoder: OggDecoder::new(options),
            failed: false,
            offset: 0,
        }
    }
    pub fn reset(&mut self) {
        self.pages.reset();
        self.decoder.reset();
        self.failed = false;
        self.offset = 0;
    }
    pub fn compatibility(&self) -> container::CompatibilityStats {
        self.decoder.compatibility()
    }
    pub fn push(&mut self, input: &[u8], emit: impl FnMut(OggEvent<'_>)) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = (|| {
            let progress = self.pages.push(input)?;
            if let Some(page) = progress.page {
                let length = page.bytes().len();
                self.decoder.push_page(page, self.offset, emit)?;
                self.offset += length as u64;
            }
            Ok(progress.consumed)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self
            .pages
            .finish()
            .map_err(Error::from)
            .and_then(|_| self.decoder.finish());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

/// Pull IO adapter. A read processes one page and its bounded synchronous events.
pub struct OggReader<R> {
    pages: PageReader<R>,
    decoder: OggDecoder,
    finished: bool,
    failed: bool,
}
impl<R: Read> OggReader<R> {
    pub fn new(reader: R) -> Self {
        Self::with_options(reader, OggOptions::default())
    }
    pub fn with_options(reader: R, options: OggOptions) -> Self {
        Self {
            pages: PageReader::new(reader),
            decoder: OggDecoder::new(options),
            finished: false,
            failed: false,
        }
    }
    pub fn read_next(&mut self, emit: impl FnMut(OggEvent<'_>)) -> Result<bool, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.finished {
            return Ok(false);
        }
        let result = (|| {
            if let Some(located) = self.pages.next_page()? {
                self.decoder.push_page(located.page, located.offset, emit)?;
                return Ok(true);
            }
            self.decoder.finish()?;
            self.finished = true;
            Ok(false)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn into_inner(self) -> R {
        self.pages.into_inner()
    }
    pub fn compatibility(&self) -> container::CompatibilityStats {
        self.decoder.compatibility()
    }
}
pub fn decode_reader(
    reader: impl Read,
    options: OggOptions,
    mut emit: impl FnMut(OggEvent<'_>),
) -> Result<(), Error> {
    let mut reader = OggReader::with_options(reader, options);
    while reader.read_next(&mut emit)? {}
    Ok(())
}
