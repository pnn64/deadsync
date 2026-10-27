//! RFC 9639 section 10.1 mapping. The frame codec remains container independent.
#![doc = include_str!("../OGG.md")]
use crate::{
    DecodeOptions, Error, FrameDecoder, FrameHeader, Integrity, Metadata, MetadataBlock, Pcm,
    StreamInfo,
    checksum::{Md5, crc16},
    reserve,
};
use deadlib_audio_decode_ogg::{
    self as container, Demuxer, Packet, Page, PageDecoder, PageHeader, PageReader,
    UNKNOWN_GRANULE_POSITION,
};
use std::{io::Read, ops::Range};

#[derive(Clone, Copy, Debug)]
pub struct OggOptions {
    pub decode: DecodeOptions,
    pub container_limits: container::Limits,
    pub max_active_streams: usize,
    /// Aggregate retained FLAC metadata, in addition to per-stream codec limits.
    pub max_metadata_bytes: usize,
}
impl Default for OggOptions {
    fn default() -> Self {
        Self {
            decode: DecodeOptions::default(),
            container_limits: container::Limits::default(),
            max_active_streams: 16,
            max_metadata_bytes: 128 << 20,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamId {
    pub serial: u32,
    pub chain: u32,
}
#[derive(Debug)]
pub struct OggPcm<'a> {
    pub stream: StreamId,
    /// Relative to the first decoded sample in this logical stream.
    pub start_sample: u64,
    /// Original container position, including a possible live broadcast offset.
    pub start_granule: u64,
    pub header: FrameHeader,
    pub pcm: Pcm<'a>,
}
pub enum OggEvent<'a> {
    StreamStart {
        stream: StreamId,
        metadata: Metadata<'a>,
    },
    Pcm(OggPcm<'a>),
    StreamEnd {
        stream: StreamId,
        samples: u64,
        start_granule: u64,
        integrity: Integrity,
    },
    IgnoredStream(StreamId),
}

pub(crate) fn validation() -> container::ValidationOptions {
    // RFC 9639 requires header-page granules zero even for spanning metadata.
    // Mapping-level checks below restrict this exception to FLAC headers.
    container::ValidationOptions {
        allow_zero_span_granules: true,
        ..Default::default()
    }
}
pub(crate) struct Headers {
    pub id: StreamId,
    pub bytes: Vec<u8>,
    pub info: StreamInfo,
    pub done: bool,
    pub end_page: u64,
    remaining: Option<u16>,
    blocks: usize,
}
impl Headers {
    pub fn identification(packet: Packet<'_>, page: Page<'_>) -> Result<Option<Self>, Error> {
        if !packet.data.starts_with(b"\x7fFLAC") {
            return Ok(None);
        }
        if packet.data.len() != 51
            || page.bytes().len() != 79
            || page.header().granule_position != 0
        {
            return Err(Error::Invalid("Ogg FLAC identification page"));
        }
        if packet.data[5..7] != [1, 0] {
            return Err(Error::Invalid("unsupported Ogg FLAC mapping version"));
        }
        if &packet.data[9..13] != b"fLaC" {
            return Err(Error::Invalid("Ogg FLAC native signature"));
        }
        let (block, last, used) = MetadataBlock::parse(&packet.data[13..])?;
        if block.kind != 0 || used != 38 || last {
            return Err(Error::Invalid("Ogg FLAC STREAMINFO header"));
        }
        let info = StreamInfo::parse(block.data)?;
        let count = u16::from_be_bytes(packet.data[7..9].try_into().unwrap());
        Ok(Some(Self {
            id: StreamId {
                serial: packet.info.serial,
                chain: packet.info.chain,
            },
            bytes: packet.data[9..].to_vec(),
            info,
            done: false,
            end_page: packet.info.end_page_offset,
            remaining: (count != 0).then_some(count),
            blocks: 1,
        }))
    }
    pub fn metadata(
        &mut self,
        packet: Packet<'_>,
        options: OggOptions,
        available: usize,
    ) -> Result<(), Error> {
        if self.done {
            return Err(Error::Invalid("metadata after Ogg FLAC headers"));
        }
        if self.blocks >= options.decode.limits.metadata_blocks {
            return Err(Error::LimitExceeded("metadata blocks"));
        }
        if packet.data.len() > available
            || self.bytes.len() - 4 + packet.data.len() > options.decode.limits.metadata_bytes
        {
            return Err(Error::LimitExceeded("metadata bytes"));
        }
        let (block, last, used) = MetadataBlock::parse(packet.data)?;
        if used != packet.data.len() || block.kind == 0 {
            return Err(Error::Invalid("one metadata block per Ogg packet"));
        }
        if let Some(n) = &mut self.remaining {
            if *n == 0 {
                return Err(Error::Invalid("Ogg FLAC header packet count"));
            }
            *n -= 1;
            if last != (*n == 0) {
                return Err(Error::Invalid("Ogg FLAC header packet count"));
            }
        }
        block.validate()?;
        reserve(&mut self.bytes, packet.data.len())?;
        self.bytes.extend_from_slice(packet.data);
        self.blocks += 1;
        self.done = last;
        self.end_page = packet.info.end_page_offset;
        if last {
            Metadata::parse(&self.bytes, options.decode.limits)?;
        }
        Ok(())
    }
    pub fn audio_header(
        &self,
        packet: Packet<'_>,
        options: OggOptions,
    ) -> Result<FrameHeader, Error> {
        if !self.done {
            return Err(Error::Invalid("audio before Ogg FLAC metadata end"));
        }
        if packet.info.start_page_offset == self.end_page {
            return Err(Error::Invalid("FLAC audio must start on a new Ogg page"));
        }
        if packet.data.len() > options.decode.limits.frame_bytes {
            return Err(Error::LimitExceeded("frame bytes"));
        }
        let header = FrameHeader::parse(packet.data, Some(&self.info))?;
        if packet.data.len() < header.header_bytes + 2 {
            return Err(Error::Truncated);
        }
        if crc16(packet.data) != 0 {
            return Err(Error::Checksum("frame CRC-16"));
        }
        Ok(header)
    }
    pub fn check_page(&self, page: Page<'_>, offset: u64) -> Result<(), Error> {
        let gp = page.header().granule_position;
        if !self.done || self.end_page == offset {
            if gp != 0 && !(page.completed_packets() == 0 && gp == UNKNOWN_GRANULE_POSITION) {
                return Err(Error::Invalid("nonzero Ogg FLAC metadata granule"));
            }
        } else if !page.lacing().is_empty()
            && page.completed_packets() == 0
            && gp != UNKNOWN_GRANULE_POSITION
        {
            return Err(Error::Invalid("granule on unfinished FLAC audio page"));
        }
        Ok(())
    }
}

/// Container timing and frame numbering are independent: a positive granule
/// origin need not equal the first frame's coded number.
pub(crate) struct Sequence {
    pub samples: u64,
    pub origin: Option<u64>,
    pub base: u64,
    first_number: Option<u64>,
    frames: u64,
    fixed_size: usize,
    variable: bool,
    short: bool,
}
impl Sequence {
    pub fn new() -> Self {
        Self {
            samples: 0,
            origin: None,
            base: 0,
            first_number: None,
            frames: 0,
            fixed_size: 0,
            variable: false,
            short: false,
        }
    }
    pub fn advance(
        &mut self,
        h: FrameHeader,
        info: StreamInfo,
        legacy: bool,
    ) -> Result<u64, Error> {
        if self.short {
            return Err(Error::Invalid("short Ogg FLAC block before end"));
        }
        let variable =
            h.variable_block_size || (legacy && info.min_block_size != info.max_block_size);
        let first = match self.first_number {
            Some(first) => {
                if self.variable != h.variable_block_size {
                    return Err(Error::Invalid("Ogg FLAC blocking strategy changed"));
                }
                first
            }
            None => {
                self.first_number = Some(h.number);
                self.variable = h.variable_block_size;
                self.fixed_size = if info.min_block_size == info.max_block_size {
                    info.max_block_size as usize
                } else {
                    h.block_size
                };
                self.base = if variable {
                    h.number
                } else {
                    h.number
                        .checked_mul(self.fixed_size as u64)
                        .ok_or(Error::Invalid("FLAC sample position overflow"))?
                };
                h.number
            }
        };
        if h.number
            != first
                .checked_add(if variable { self.samples } else { self.frames })
                .ok_or(Error::Invalid("FLAC frame number overflow"))?
        {
            return Err(Error::Invalid("non-contiguous Ogg FLAC numbering"));
        }
        if h.block_size > info.max_block_size as usize
            || (!variable && h.block_size > self.fixed_size)
        {
            return Err(Error::Invalid("Ogg FLAC block size"));
        }
        self.short = h.block_size < info.min_block_size as usize
            || (!variable && h.block_size < self.fixed_size);
        let start = self.samples;
        self.samples = self
            .samples
            .checked_add(h.block_size as u64)
            .ok_or(Error::Invalid("FLAC sample position overflow"))?;
        self.frames += 1;
        if info.total_samples != 0 && self.base + self.samples > info.total_samples {
            return Err(Error::Invalid("Ogg FLAC sample total exceeded"));
        }
        Ok(start)
    }
    pub fn page_end(&mut self, page: PageHeader) -> Result<(), Error> {
        if page.granule_position == UNKNOWN_GRANULE_POSITION {
            return Ok(());
        }
        let gp = page.granule_position;
        if self.samples == 0 {
            return if gp == 0 {
                Ok(())
            } else {
                Err(Error::Invalid("granule before Ogg FLAC audio"))
            };
        }
        if gp > i64::MAX as u64 {
            return Err(Error::Invalid("negative Ogg FLAC granule"));
        }
        match self.origin {
            None => {
                self.origin = Some(
                    gp.checked_sub(self.samples)
                        .ok_or(Error::Invalid("Ogg FLAC granule before decoded samples"))?,
                )
            }
            Some(origin) => {
                if origin.checked_add(self.samples) != Some(gp) {
                    return Err(Error::Invalid("Ogg FLAC granule disagrees with samples"));
                }
            }
        }
        Ok(())
    }
    pub fn finish(&self, info: StreamInfo) -> Result<(), Error> {
        if info.total_samples != 0 && self.base + self.samples != info.total_samples {
            return Err(Error::Invalid("Ogg FLAC sample total mismatch"));
        }
        Ok(())
    }
}
struct Pending {
    bytes: Range<usize>,
    start: u64,
}
struct Stream {
    headers: Headers,
    sequence: Sequence,
    decoder: FrameDecoder,
    digest: Option<Md5>,
    pending: Vec<Pending>,
    pending_bytes: Vec<u8>,
}
impl Stream {
    fn emit(
        &mut self,
        data: &[u8],
        start: u64,
        emit: &mut impl FnMut(OggEvent<'_>),
    ) -> Result<(), Error> {
        let frame = self.decoder.decode(data)?;
        if frame.bytes != data.len() {
            return Err(Error::Invalid("one FLAC frame per Ogg packet"));
        }
        if self.sequence.base != 0 {
            self.digest = None;
        }
        if let Some(digest) = &mut self.digest {
            frame.pcm.update_md5(digest, frame.header.bits_per_sample);
        }
        emit(OggEvent::Pcm(OggPcm {
            stream: self.headers.id,
            start_sample: start,
            start_granule: self.sequence.origin.unwrap() + start,
            header: frame.header,
            pcm: frame.pcm,
        }));
        Ok(())
    }
}

pub struct OggDecoder {
    demux: Demuxer,
    streams: Vec<Stream>,
    options: OggOptions,
    metadata_bytes: usize,
    failed: bool,
}
impl OggDecoder {
    pub fn new(options: OggOptions) -> Self {
        Self {
            demux: Demuxer::with_validation(options.container_limits, validation()),
            streams: Vec::new(),
            options,
            metadata_bytes: 0,
            failed: false,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.options);
    }
    pub fn push_page(
        &mut self,
        page: Page<'_>,
        offset: u64,
        mut emit: impl FnMut(OggEvent<'_>),
    ) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("Ogg FLAC decoder is terminal after error"));
        }
        let result = (|| {
            let mut failure = None;
            let streams = &mut self.streams;
            let options = self.options;
            let metadata_bytes = &mut self.metadata_bytes;
            self.demux.push_page(page, offset, |packet| {
                if failure.is_none() {
                    failure =
                        process_packet(streams, metadata_bytes, options, page, packet, &mut emit)
                            .err();
                }
            })?;
            if let Some(error) = failure {
                return Err(error);
            }
            if let Some(i) = self
                .streams
                .iter()
                .position(|s| s.headers.id.serial == page.header().serial)
            {
                let stream = &mut self.streams[i];
                stream.headers.check_page(page, offset)?;
                if stream.headers.done && stream.headers.end_page != offset {
                    stream.sequence.page_end(page.header())?;
                    if !stream.pending.is_empty() {
                        // Only the first completed audio page needs encoded buffering,
                        // to resolve its broadcast origin before emitting any PCM.
                        let bytes = std::mem::take(&mut stream.pending_bytes);
                        let pending = std::mem::take(&mut stream.pending);
                        for packet in pending {
                            stream.emit(&bytes[packet.bytes], packet.start, &mut emit)?;
                        }
                    }
                }
                if page.header().is_eos() {
                    if !stream.headers.done {
                        return Err(Error::Truncated);
                    }
                    stream.sequence.finish(stream.headers.info)?;
                    let integrity = if stream.sequence.base != 0 {
                        Integrity::Partial
                    } else if let Some(digest) = stream.digest.take() {
                        if digest.finalize() != stream.headers.info.md5 {
                            return Err(Error::Checksum("PCM MD5"));
                        }
                        Integrity::Verified
                    } else {
                        Integrity::Unavailable
                    };
                    emit(OggEvent::StreamEnd {
                        stream: stream.headers.id,
                        samples: stream.sequence.samples,
                        start_granule: stream.sequence.origin.unwrap_or(0),
                        integrity,
                    });
                    self.metadata_bytes -= stream.headers.bytes.len();
                    self.streams.remove(i);
                }
            } else if page.completed_packets() == 0
                && !page.lacing().is_empty()
                && page.header().granule_position == 0
            {
                return Err(Error::Invalid("zero span granule outside FLAC metadata"));
            }
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Invalid("Ogg FLAC decoder is terminal after error"));
        }
        let result = self.demux.finish().map_err(Error::from).and({
            if self.streams.is_empty() {
                Ok(())
            } else {
                Err(Error::Truncated)
            }
        });
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
fn process_packet(
    streams: &mut Vec<Stream>,
    metadata_bytes: &mut usize,
    options: OggOptions,
    page: Page<'_>,
    packet: Packet<'_>,
    emit: &mut impl FnMut(OggEvent<'_>),
) -> Result<(), Error> {
    if packet.info.first_in_stream {
        let Some(headers) = Headers::identification(packet, page)? else {
            emit(OggEvent::IgnoredStream(StreamId {
                serial: packet.info.serial,
                chain: packet.info.chain,
            }));
            return Ok(());
        };
        if streams.len() >= options.max_active_streams {
            return Err(Error::LimitExceeded("active Ogg FLAC streams"));
        }
        if options.decode.limits.metadata_blocks == 0
            || options.decode.limits.metadata_bytes < 38
            || headers.bytes.len() > options.max_metadata_bytes.saturating_sub(*metadata_bytes)
        {
            return Err(Error::LimitExceeded("metadata bytes"));
        }
        *metadata_bytes += headers.bytes.len();
        let info = headers.info;
        reserve(streams, 1)?;
        streams.push(Stream {
            headers,
            sequence: Sequence::new(),
            decoder: FrameDecoder::new(Some(info))?,
            digest: (options.decode.verify_md5 && info.md5 != [0; 16]).then(Md5::new),
            pending: Vec::new(),
            pending_bytes: Vec::new(),
        });
        return Ok(());
    }
    let Some(stream) = streams
        .iter_mut()
        .find(|s| s.headers.id.serial == packet.info.serial)
    else {
        return Ok(());
    };
    if !stream.headers.done {
        stream.headers.metadata(
            packet,
            options,
            options.max_metadata_bytes.saturating_sub(*metadata_bytes),
        )?;
        *metadata_bytes += packet.data.len();
        if stream.headers.done {
            emit(OggEvent::StreamStart {
                stream: stream.headers.id,
                metadata: Metadata::parse(&stream.headers.bytes, options.decode.limits)?,
            });
        }
    } else {
        let header = stream.headers.audio_header(packet, options)?;
        let start = stream.sequence.advance(
            header,
            stream.headers.info,
            options.decode.allow_legacy_variable_blocks,
        )?;
        if stream.sequence.origin.is_some() {
            stream.emit(packet.data, start, emit)?;
        } else {
            reserve(&mut stream.pending_bytes, packet.data.len())?;
            let begin = stream.pending_bytes.len();
            stream.pending_bytes.extend_from_slice(packet.data);
            reserve(&mut stream.pending, 1)?;
            stream.pending.push(Pending {
                bytes: begin..stream.pending_bytes.len(),
                start,
            });
        }
    }
    Ok(())
}

pub struct OggReader<R> {
    pages: PageReader<R>,
    decoder: OggDecoder,
    done: bool,
    failed: bool,
}
impl<R: Read> OggReader<R> {
    pub fn new(source: R, options: OggOptions) -> Self {
        Self {
            pages: PageReader::new(source),
            decoder: OggDecoder::new(options),
            done: false,
            failed: false,
        }
    }
    pub fn read_next(&mut self, emit: impl FnMut(OggEvent<'_>)) -> Result<bool, Error> {
        if self.failed {
            return Err(Error::Invalid("Ogg FLAC reader is terminal after error"));
        }
        if self.done {
            return Ok(false);
        }
        let result = (|| match self.pages.next_page()? {
            Some(page) => {
                self.decoder.push_page(page.page, page.offset, emit)?;
                Ok(true)
            }
            None => {
                self.decoder.finish()?;
                self.done = true;
                Ok(false)
            }
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
pub struct OggStreamDecoder {
    pages: PageDecoder,
    decoder: OggDecoder,
    offset: u64,
    failed: bool,
}
impl OggStreamDecoder {
    pub fn new(options: OggOptions) -> Self {
        Self {
            pages: PageDecoder::new(),
            decoder: OggDecoder::new(options),
            offset: 0,
            failed: false,
        }
    }
    /// Consumes at most one page per call. Repeat until the input chunk is consumed.
    pub fn push(&mut self, input: &[u8], emit: impl FnMut(OggEvent<'_>)) -> Result<usize, Error> {
        if self.failed {
            return Err(Error::Invalid(
                "Ogg FLAC stream decoder is terminal after error",
            ));
        }
        let result = (|| {
            let progress = self.pages.push(input)?;
            if let Some(page) = progress.page {
                self.decoder.push_page(page, self.offset, emit)?;
                self.offset += page.bytes().len() as u64;
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
            return Err(Error::Invalid(
                "Ogg FLAC stream decoder is terminal after error",
            ));
        }
        let result = self
            .pages
            .finish()
            .map_err(Error::from)
            .and_then(|()| self.decoder.finish());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn reset(&mut self) {
        self.pages.reset();
        self.decoder.reset();
        self.offset = 0;
        self.failed = false;
    }
}
