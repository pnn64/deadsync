//! Vorbis I Appendix A mapping on the independent Ogg container.
use crate::{
    Comments, Error, Identification, Limits, Metadata, PacketDecoder, PcmBlock, Setup, reserve,
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
struct Timing {
    audio_packets: u64,
    raw_end: u64,
    origin: Option<i128>,
    frames: u64,
    last_span: u64,
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
            demux: Demuxer::new(options.container_limits),
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
                check_page_end(stream, header)?;
            }
        }
        if header.is_eos() {
            if let Some(info) = stream.info.as_ref() {
                if stream.phase != 3 {
                    return Err(Error::Invalid("EOS before all Vorbis headers"));
                }
                let origin = stream.timing.origin.unwrap_or(0).max(0) as u64;
                let end = granule(header.granule_position)?;
                if end.checked_sub(origin) != Some(stream.timing.frames) {
                    return Err(Error::Invalid("final Vorbis sample count"));
                }
                emit(OggEvent::StreamEnd(StreamEnd {
                    stream: info.id,
                    frames: stream.timing.frames,
                    start_granule: origin,
                    end_granule: end,
                }));
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
            let info = if packet.data.starts_with(b"\x01vorbis") {
                if page_len != 58 || page.granule_position != 0 {
                    return Err(Error::Invalid(
                        "Vorbis identification page must be 58 bytes at granule zero",
                    ));
                }
                Some(StreamInfo {
                    id,
                    identification: Identification::parse(packet.data)?,
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
                let mut limits = options.limits;
                let available = options.max_stream_memory - *memory;
                limits.max_comment_bytes = limits.max_comment_bytes.min(available);
                limits.max_comments = limits
                    .max_comments
                    .min(available / core::mem::size_of::<core::ops::Range<usize>>());
                let metadata = match Comments::parse(packet.data, &limits) {
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
                        comments.to_owned()?
                    }
                    Err(error @ (Error::EndOfPacket | Error::Invalid("comment framing bit"))) => {
                        emit(OggEvent::CommentError { stream: id, error });
                        Metadata::default()
                    }
                    Err(error) => return Err(error),
                };
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
                let setup = Setup::parse(packet.data, &info.identification, &limits)?;
                let setup_memory = setup.storage_bytes();
                let available = options
                    .max_stream_memory
                    .checked_sub(*memory)
                    .and_then(|n| n.checked_sub(setup_memory))
                    .ok_or(Error::LimitExceeded("active stream memory"))?;
                limits.max_decode_memory = limits.max_decode_memory.min(available);
                let decoder = PacketDecoder::with_limits(setup, limits)?;
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
                let timing = &mut stream.timing;
                timing.audio_packets += 1;
                timing.raw_end = block.start_frame + block.frames as u64;
                timing.last_span = block.frames as u64;
                if block.frames == 0 {
                    return Ok(());
                }
                if timing.origin.is_none() {
                    let origin = if packet.info.granule_position.is_some() {
                        let end = granule(page.granule_position)? as i128;
                        // A single audio page can trim the tail. Its shorter
                        // final granule must not be mistaken for initial trim.
                        if page.is_eos() && end <= timing.raw_end as i128 {
                            0
                        } else {
                            end - timing.raw_end as i128
                        }
                    } else {
                        0
                    };
                    if origin != 0 && (packet.info.granule_position.is_none() || !ends_packet) {
                        return Err(Error::Invalid(
                            "nonzero initial offset requires the second audio packet to flush its page",
                        ));
                    }
                    if origin < -(block.frames as i128) {
                        return Err(Error::Invalid("initial trim exceeds first overlap span"));
                    }
                    timing.origin = Some(origin);
                }
                let origin = timing.origin.unwrap();
                let start = origin + block.start_frame as i128;
                let skip = (-start).clamp(0, block.frames as i128) as usize;
                let keep = if page.is_eos() {
                    (granule(page.granule_position)? as i128 - start).clamp(0, block.frames as i128)
                        as usize
                } else {
                    block.frames
                };
                if packet.info.granule_position.is_some() {
                    check_granule(timing, page, block.frames as u64)?;
                }
                if keep < skip {
                    return Err(Error::Invalid("negative retained PCM interval"));
                }
                let start_frame = timing.frames;
                timing.frames = timing
                    .frames
                    .checked_add((keep - skip) as u64)
                    .ok_or(Error::LimitExceeded("stream frame count"))?;
                if keep != skip {
                    emit(OggEvent::Pcm(OggPcm {
                        stream: id,
                        sample_rate: info.identification.sample_rate,
                        start_frame,
                        start_granule: (start + skip as i128) as u64,
                        pcm: block.crop(skip, keep),
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
fn charge(memory: &mut usize, amount: usize, options: &OggOptions) -> Result<(), Error> {
    *memory = memory
        .checked_add(amount)
        .filter(|&n| n <= options.max_stream_memory)
        .ok_or(Error::LimitExceeded("active stream memory"))?;
    Ok(())
}
fn granule(value: u64) -> Result<u64, Error> {
    if value > i64::MAX as u64 {
        return Err(Error::Invalid("unknown/negative Vorbis granule"));
    }
    Ok(value)
}
fn check_granule(timing: &Timing, page: PageHeader, span: u64) -> Result<(), Error> {
    let end = granule(page.granule_position)? as i128;
    let expected = timing.origin.unwrap_or(0) + timing.raw_end as i128;
    if page.is_eos() {
        if end > expected || end < expected - span as i128 {
            return Err(Error::Invalid("EOS granule outside final packet overlap"));
        }
    } else if end != expected {
        return Err(Error::Invalid(
            "Vorbis granule disagrees with decoded sample position",
        ));
    }
    Ok(())
}
fn check_page_end(stream: &Stream, page: PageHeader) -> Result<(), Error> {
    if stream.timing.origin.is_none() && stream.timing.raw_end == 0 {
        if page.granule_position != 0 {
            return Err(Error::Invalid("granule before any complete overlap output"));
        }
        return Ok(());
    }
    check_granule(&stream.timing, page, stream.timing.last_span)
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
