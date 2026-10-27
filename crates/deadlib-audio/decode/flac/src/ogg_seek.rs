//! Validated physical-file indexing, followed by sample-exact packet capture.
use crate::{
    Error, FrameDecoder, FrameHeader, Integrity, Metadata, OggOptions, OggPcm, StreamId,
    StreamInfo,
    checksum::Md5,
    ogg::{Headers, Sequence, validation},
    reserve,
};
use deadlib_audio_decode_ogg::{
    IndexEvent, IndexOptions, OggIndex, PacketCapture, PageReader, UNKNOWN_GRANULE_POSITION,
};
use std::{
    io::{Read, Seek, SeekFrom},
    ops::ControlFlow,
};

#[derive(Clone, Copy, Debug)]
pub struct OggSeekOptions {
    pub mapping: OggOptions,
    pub max_pages: usize,
    pub max_frames: usize,
}
impl Default for OggSeekOptions {
    fn default() -> Self {
        Self {
            mapping: OggOptions::default(),
            max_pages: 1 << 20,
            max_frames: 4 << 20,
        }
    }
}
#[derive(Clone, Copy)]
struct Location {
    packet: u64,
    start: u64,
    samples: u16,
}
/// Metadata and timing of one FLAC logical stream in physical BOS order.
pub struct OggSeekStream {
    pub id: StreamId,
    pub stream_info: StreamInfo,
    pub samples: u64,
    pub start_granule: u64,
    pub partial: bool,
    metadata: Vec<u8>,
    frames: Vec<Location>,
}
impl OggSeekStream {
    pub fn metadata(&self) -> Metadata<'_> {
        // Already validated on construction, including configured limits.
        Metadata::parse(
            &self.metadata,
            crate::Limits {
                metadata_bytes: self.metadata.len(),
                metadata_blocks: usize::MAX,
                ..Default::default()
            },
        )
        .expect("validated indexed metadata")
    }
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}
struct Builder {
    headers: Headers,
    sequence: Sequence,
    frames: Vec<Location>,
    ended: bool,
}

/// Reads a seekable source from byte zero. Index construction verifies every Ogg
/// page, FLAC header, frame CRC, frame number, granule and declared sample count.
/// Subframes and PCM MD5 are validated when audio is decoded. The index stores
/// pages and frame locations, never decoded PCM or the complete source bytes.
/// A source modified externally after construction invalidates the index.
pub struct OggSeekReader<R> {
    pages: PageReader<R>,
    index: OggIndex,
    streams: Vec<OggSeekStream>,
    options: OggSeekOptions,
    selected: usize,
    next: usize,
    skip: usize,
    capture: PacketCapture,
    pending_page: bool,
    decoder: FrameDecoder,
    digest: Option<Md5>,
    integrity: Integrity,
    failed: bool,
}
impl<R: Read + Seek> OggSeekReader<R> {
    pub fn new(mut source: R, options: OggSeekOptions) -> Result<Self, Error> {
        source.seek(SeekFrom::Start(0))?;
        let mapping = options.mapping;
        let mut builders: Vec<Builder> = Vec::new();
        let mut metadata_bytes = 0;
        let mut frame_count = 0;
        let mut active = 0;
        let index = OggIndex::read_with_events(
            &mut source,
            IndexOptions {
                container_limits: mapping.container_limits,
                max_pages: options.max_pages,
                validation: validation(),
            },
            |event| -> Result<(), Error> {
                match event {
                    IndexEvent::Packet { page, packet } => {
                        if packet.info.first_in_stream {
                            if let Some(headers) = Headers::identification(packet, page)? {
                                if active >= mapping.max_active_streams {
                                    return Err(Error::LimitExceeded("active Ogg FLAC streams"));
                                }
                                if mapping.decode.limits.metadata_blocks == 0
                                    || mapping.decode.limits.metadata_bytes < 38
                                    || headers.bytes.len()
                                        > mapping.max_metadata_bytes.saturating_sub(metadata_bytes)
                                {
                                    return Err(Error::LimitExceeded("indexed metadata bytes"));
                                }
                                metadata_bytes += headers.bytes.len();
                                active += 1;
                                reserve(&mut builders, 1)?;
                                builders.push(Builder {
                                    headers,
                                    sequence: Sequence::new(),
                                    frames: Vec::new(),
                                    ended: false,
                                });
                            }
                        } else if let Some(b) = builders
                            .iter_mut()
                            .find(|b| b.headers.id.serial == packet.info.serial)
                        {
                            if !b.headers.done {
                                b.headers.metadata(
                                    packet,
                                    mapping,
                                    mapping.max_metadata_bytes.saturating_sub(metadata_bytes),
                                )?;
                                metadata_bytes += packet.data.len();
                            } else {
                                if frame_count >= options.max_frames {
                                    return Err(Error::LimitExceeded("indexed FLAC frames"));
                                }
                                let h = b.headers.audio_header(packet, mapping)?;
                                let start = b.sequence.advance(
                                    h,
                                    b.headers.info,
                                    mapping.decode.allow_legacy_variable_blocks,
                                )?;
                                reserve(&mut b.frames, 1)?;
                                b.frames.push(Location {
                                    packet: packet.info.number,
                                    start,
                                    samples: h.block_size as u16,
                                });
                                frame_count += 1;
                            }
                        }
                    }
                    IndexEvent::Page { page, offset } => {
                        if let Some(b) = builders
                            .iter_mut()
                            .find(|b| b.headers.id.serial == page.header().serial)
                        {
                            b.headers.check_page(page, offset)?;
                            if b.headers.done && b.headers.end_page != offset {
                                b.sequence.page_end(page.header())?;
                            }
                            if page.header().is_eos() {
                                if !b.headers.done {
                                    return Err(Error::Truncated);
                                }
                                b.sequence.finish(b.headers.info)?;
                                b.ended = true;
                                active -= 1;
                            }
                        } else if page.completed_packets() == 0
                            && !page.lacing().is_empty()
                            && page.header().granule_position != UNKNOWN_GRANULE_POSITION
                        {
                            return Err(Error::Invalid("zero span granule outside FLAC metadata"));
                        }
                    }
                }
                Ok(())
            },
        )?;
        if builders.is_empty() {
            return Err(Error::Invalid("no Ogg FLAC streams"));
        }
        let mut streams = Vec::new();
        reserve(&mut streams, builders.len())?;
        for b in builders {
            if !b.ended {
                return Err(Error::Truncated);
            }
            streams.push(OggSeekStream {
                id: b.headers.id,
                stream_info: b.headers.info,
                samples: b.sequence.samples,
                start_granule: b.sequence.origin.unwrap_or(0),
                partial: b.sequence.base != 0,
                metadata: b.headers.bytes,
                frames: b.frames,
            });
        }
        let id = streams[0].id;
        let mut result = Self {
            pages: PageReader::new(source),
            index,
            streams,
            options,
            selected: 0,
            next: 0,
            skip: 0,
            capture: PacketCapture::new(id.serial, mapping.container_limits),
            pending_page: false,
            decoder: FrameDecoder::default(),
            digest: None,
            integrity: Integrity::Unavailable,
            failed: false,
        };
        result.seek(id, 0)?;
        Ok(result)
    }
    pub fn streams(&self) -> &[OggSeekStream] {
        &self.streams
    }
    pub fn container_index(&self) -> &OggIndex {
        &self.index
    }
    pub fn selected_stream(&self) -> &OggSeekStream {
        &self.streams[self.selected]
    }
    pub fn integrity(&self) -> Integrity {
        self.integrity
    }
    /// Seek relative to the first available sample. Seeking to EOF is valid.
    /// Seeking to zero restarts full-stream MD5 verification when available.
    pub fn seek(&mut self, stream: StreamId, sample: u64) -> Result<(), Error> {
        let selected = self
            .streams
            .iter()
            .position(|s| s.id == stream)
            .ok_or(Error::Invalid("unknown Ogg FLAC stream"))?;
        let s = &self.streams[selected];
        if sample > s.samples {
            return Err(Error::Invalid("seek beyond Ogg FLAC samples"));
        }
        let next = if sample == s.samples {
            s.frames.len()
        } else {
            s.frames.partition_point(|f| f.start <= sample) - 1
        };
        self.failed = true;
        self.skip = 0;
        if let Some(frame) = s.frames.get(next) {
            let page = self
                .index
                .packet_start_page(s.id.serial, frame.packet)
                .ok_or(Error::Invalid("missing indexed FLAC packet"))?;
            self.pages.seek(page.offset)?;
            self.capture.reset_at(page);
            self.skip = (sample - frame.start) as usize;
        }
        self.decoder.set_stream_info(Some(s.stream_info))?;
        self.digest = (sample == 0
            && !s.partial
            && self.options.mapping.decode.verify_md5
            && s.stream_info.md5 != [0; 16])
            .then(Md5::new);
        self.integrity = if self.digest.is_some() {
            Integrity::Pending
        } else if sample != 0 || s.partial {
            Integrity::Partial
        } else {
            Integrity::Unavailable
        };
        self.selected = selected;
        self.next = next;
        self.pending_page = false;
        self.failed = false;
        Ok(())
    }
    pub fn read_next(&mut self) -> Result<Option<OggPcm<'_>>, Error> {
        if self.failed {
            return Err(Error::Invalid(
                "Ogg FLAC seek reader is terminal after error",
            ));
        }
        let result = self.read_frame();
        let header = match result {
            Ok(Some(header)) => header,
            Ok(None) => return Ok(None),
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        let s = &self.streams[self.selected];
        let start = s.frames[self.next - 1].start + self.skip as u64;
        let pcm = self.decoder.pcm(header).trim_start(self.skip);
        self.skip = 0;
        Ok(Some(OggPcm {
            stream: s.id,
            start_sample: start,
            start_granule: s.start_granule + start,
            header,
            pcm,
        }))
    }
    fn read_frame(&mut self) -> Result<Option<FrameHeader>, Error> {
        let s = &self.streams[self.selected];
        let Some(target) = s.frames.get(self.next) else {
            if let Some(digest) = self.digest.take() {
                if digest.finalize() != s.stream_info.md5 {
                    return Err(Error::Checksum("PCM MD5"));
                }
                self.integrity = Integrity::Verified;
            }
            return Ok(None);
        };
        loop {
            let page = if self.pending_page {
                self.pages.current_page()
            } else {
                self.pages.next_page()?
            }
            .ok_or(Error::Truncated)?;
            let mut found = None;
            let mut failure = None;
            self.pending_page =
                !self
                    .capture
                    .push_page_until(page.page, page.offset, |packet| {
                        if let Some(number) = packet.info.number {
                            if number < target.packet {
                                return ControlFlow::Continue(());
                            }
                            let decoded = (|| {
                                if number != target.packet {
                                    return Err(Error::Invalid("indexed packet number changed"));
                                }
                                let frame = self.decoder.decode(packet.data)?;
                                if frame.bytes != packet.data.len()
                                    || frame.header.block_size != target.samples as usize
                                {
                                    return Err(Error::Invalid("indexed FLAC frame changed"));
                                }
                                if let Some(digest) = &mut self.digest {
                                    frame.pcm.update_md5(digest, frame.header.bits_per_sample);
                                }
                                Ok(frame.header)
                            })();
                            match decoded {
                                Ok(header) => found = Some(header),
                                Err(error) => failure = Some(error),
                            }
                        } else {
                            failure = Some(Error::Invalid("missing indexed packet number"));
                        }
                        ControlFlow::Break(())
                    })?;
            if let Some(error) = failure {
                return Err(error);
            }
            if let Some(header) = found {
                self.next += 1;
                return Ok(Some(header));
            }
        }
    }
    pub fn into_inner(self) -> R {
        self.pages.into_inner()
    }
}
