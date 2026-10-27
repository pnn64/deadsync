//! Indexed, sample-accurate seeking with one-packet overlap pre-roll.
use crate::{
    Error, Limits, Metadata, OggEvent, OggPcm, PacketDecoder, Setup, StreamEnd, StreamId,
    StreamInfo,
    ogg::{Timing, granule, read_identification, read_metadata},
};
use core::ops::ControlFlow;
use deadlib_audio_decode_ogg::{
    self as container, IndexedPage, OggIndex, PacketCapture, Page, PageReader,
    UNKNOWN_GRANULE_POSITION,
};
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug)]
pub struct SeekOptions {
    pub limits: Limits,
    pub container_limits: container::Limits,
    pub allow_zero_span_granules: bool,
    /// Explicit legacy endpoint policy. The index knows the final granule in
    /// advance and can trim across earlier packets/pages, including a decreasing
    /// EOS granule. Streaming APIs deliberately do not offer this exception.
    pub trim_to_final_granule: bool,
    /// Retain two mode bits per packet to select the nearest overlap pre-roll
    /// packet without synthesizing the earlier packets in its page interval.
    /// Disabling this saves that index storage and uses page landmarks alone;
    /// both strategies return sample-accurate PCM. Enabled by default.
    pub packet_preroll: bool,
    pub max_index_pages: usize,
    /// Retained Vorbis metadata, setup packets and seek entries. The native
    /// container index has its own page-count bound, separately from this budget.
    pub max_index_memory: usize,
    /// Setup and synthesis elements of the single selected decoder.
    pub max_stream_memory: usize,
}
impl Default for SeekOptions {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            container_limits: container::Limits::default(),
            allow_zero_span_granules: false,
            trim_to_final_granule: false,
            packet_preroll: true,
            max_index_pages: 1 << 20,
            max_index_memory: 128 << 20,
            max_stream_memory: 256 << 20,
        }
    }
}
impl SeekOptions {
    fn validation(self) -> container::ValidationOptions {
        container::ValidationOptions {
            allow_zero_span_granules: self.allow_zero_span_granules,
            allow_decreasing_eos_granule: self.trim_to_final_granule,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SeekMark {
    granule: u64,
    packet: u64,
    raw_end: u64,
}

pub struct SeekStream {
    pub info: StreamInfo,
    pub end: StreamEnd,
    pub comment_error: Option<Error>,
    pub end_padding_frames: u64,
    /// True when the declared endpoint needs the explicit cross-packet policy.
    pub extended_end_trim: bool,
    origin: i128,
    setup_packet: Vec<u8>,
    marks: Vec<SeekMark>,
    packet_modes: Vec<u8>,
    first_audio: Option<u64>,
}
impl SeekStream {
    fn packet_block_size(&self, number: u64) -> Option<usize> {
        let byte = *self.packet_modes.get(usize::try_from(number / 4).ok()?)?;
        match (byte >> ((number & 3) * 2)) & 3 {
            1 => Some(self.info.identification.block_sizes[0]),
            2 => Some(self.info.identification.block_sizes[1]),
            _ => None,
        }
    }
    fn refine_preroll(
        &self,
        mut packet: u64,
        mut raw_end: u64,
        target: i128,
    ) -> Result<(u64, u64), Error> {
        let mut previous = self
            .packet_block_size(packet)
            .ok_or(Error::Invalid("missing indexed pre-roll mode"))?;
        let mut next = packet
            .checked_add(1)
            .ok_or(Error::LimitExceeded("packet count"))?;
        while usize::try_from(next / 4).is_ok_and(|byte| byte < self.packet_modes.len()) {
            if let Some(size) = self.packet_block_size(next) {
                let end = raw_end
                    .checked_add(((previous + size) / 4) as u64)
                    .ok_or(Error::LimitExceeded("raw frame count"))?;
                if end as i128 > target {
                    break;
                }
                packet = next;
                raw_end = end;
                previous = size;
            }
            next = next
                .checked_add(1)
                .ok_or(Error::LimitExceeded("packet count"))?;
        }
        Ok((packet, raw_end))
    }
}

/// Container-validated index with full Vorbis headers and mode/granule timing.
/// Construction scans packet prefixes, not floors, residues or PCM. Malformed
/// audio payloads can still fail when synthesized. Unknown codecs are omitted.
/// The source must remain byte-for-byte identical while this index is reused.
pub struct VorbisIndex {
    container: OggIndex,
    streams: Vec<SeekStream>,
    memory: usize,
    options: SeekOptions,
}
impl VorbisIndex {
    pub fn read<R: Read + Seek>(reader: &mut R, options: SeekOptions) -> Result<Self, Error> {
        reader
            .seek(SeekFrom::Start(0))
            .map_err(container::Error::from)?;
        let mut builder = IndexBuilder {
            streams: Vec::new(),
            active: Vec::new(),
            memory: 0,
            options,
        };
        let container = OggIndex::read_with_events(
            &mut *reader,
            container::IndexOptions {
                container_limits: options.container_limits,
                max_pages: options.max_index_pages,
                validation: options.validation(),
            },
            |event| match event {
                container::IndexEvent::Packet { page, packet } => builder.packet(page, packet),
                container::IndexEvent::Page { page, offset } => builder.page(page, offset),
            },
        )?;
        Ok(Self {
            container,
            streams: builder.streams,
            memory: builder.memory,
            options,
        })
    }
    pub fn streams(&self) -> &[SeekStream] {
        &self.streams
    }
    pub fn stream(&self, id: StreamId) -> Option<&SeekStream> {
        self.streams.iter().find(|s| s.info.id == id)
    }
    pub fn container(&self) -> &OggIndex {
        &self.container
    }
    /// Retained container and Vorbis index elements, excluding allocator overhead.
    pub fn storage_bytes(&self) -> usize {
        self.container.storage_bytes() + self.memory
    }
}

// Only the small mode vector survives setup parsing. Full codebooks/floors and
// residues are released immediately, keeping one parsed setup at a time even
// when physical streams interleave their headers and audio.
struct BuildingStream {
    stream: usize,
    modes: Vec<crate::Mode>,
    setup_page: Option<u64>,
    previous_size: usize,
    last_audio: Option<u64>,
    timing: Timing,
}
struct IndexBuilder {
    streams: Vec<SeekStream>,
    active: Vec<BuildingStream>,
    memory: usize,
    options: SeekOptions,
}
impl IndexBuilder {
    fn packet(&mut self, page: Page<'_>, packet: container::Packet<'_>) -> Result<(), Error> {
        let options = self.options;
        let header = page.header();
        let ends_packet = page.lacing().last().is_none_or(|&n| n < 255);
        if packet.info.first_in_stream {
            let Some(identification) =
                read_identification(packet.data, header, page.bytes().len())?
            else {
                return Ok(());
            };
            let id = StreamId {
                serial: packet.info.serial,
                chain: packet.info.chain,
            };
            reserve_index(
                &mut self.streams,
                1,
                &mut self.memory,
                options.max_index_memory,
            )?;
            crate::reserve(&mut self.active, 1)?;
            self.active.push(BuildingStream {
                stream: self.streams.len(),
                modes: Vec::new(),
                setup_page: None,
                previous_size: 0,
                last_audio: None,
                timing: Timing::with_extended_end_trim(options.trim_to_final_granule),
            });
            self.streams.push(SeekStream {
                info: StreamInfo {
                    id,
                    identification,
                    metadata: Metadata::default(),
                },
                end: StreamEnd {
                    stream: id,
                    frames: 0,
                    start_granule: 0,
                    end_granule: 0,
                },
                comment_error: None,
                end_padding_frames: 0,
                extended_end_trim: false,
                origin: 0,
                setup_packet: Vec::new(),
                marks: Vec::new(),
                packet_modes: Vec::new(),
                first_audio: None,
            });
            return Ok(());
        }
        let Some(state) = self
            .active
            .iter_mut()
            .find(|state| self.streams[state.stream].info.id.serial == packet.info.serial)
        else {
            return Ok(());
        };
        let stream = &mut self.streams[state.stream];
        match packet.info.number {
            1 => {
                let (metadata, error) = read_metadata(
                    packet.data,
                    options.limits,
                    options.max_index_memory - self.memory,
                )?;
                charge(
                    &mut self.memory,
                    metadata.storage_bytes(),
                    options.max_index_memory,
                )?;
                stream.info.metadata = metadata;
                stream.comment_error = error;
                if packet.info.granule_position.is_some_and(|gp| gp != 0) {
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
                limits.max_setup_memory = limits.max_setup_memory.min(options.max_stream_memory);
                let setup = Setup::parse(packet.data, &stream.info.identification, &limits)?;
                check_decoder_memory(&setup, options)?;
                reserve_index(
                    &mut stream.setup_packet,
                    packet.data.len(),
                    &mut self.memory,
                    options.max_index_memory,
                )?;
                stream.setup_packet.extend_from_slice(packet.data);
                state.modes = setup.into_modes();
                state.setup_page = Some(packet.info.end_page_offset);
            }
            _ => {
                if state.modes.is_empty() {
                    return Err(Error::Invalid("audio before Vorbis setup"));
                }
                if Some(packet.info.start_page_offset) == state.setup_page {
                    return Err(Error::Invalid("audio must begin on a fresh page"));
                }
                let Some((audio, _)) = crate::decoder::packet_header_from_modes(
                    &state.modes,
                    stream.info.identification.block_sizes,
                    &options.limits,
                    packet.data,
                )?
                else {
                    return Ok(());
                };
                let frames = if state.previous_size == 0 {
                    0
                } else {
                    (state.previous_size + audio.block_size) / 4
                };
                state.timing.advance(
                    state.timing.raw_end,
                    frames,
                    packet.info.granule_position.is_some(),
                    header,
                    ends_packet,
                )?;
                state.previous_size = audio.block_size;
                let number = packet.info.number;
                if options.packet_preroll {
                    let byte = usize::try_from(number / 4)
                        .ok()
                        .and_then(|n| n.checked_add(1))
                        .ok_or(Error::LimitExceeded("packet-mode index"))?;
                    let extra = byte.saturating_sub(stream.packet_modes.len());
                    reserve_index(
                        &mut stream.packet_modes,
                        extra,
                        &mut self.memory,
                        options.max_index_memory,
                    )?;
                    stream.packet_modes.resize(byte, 0);
                    let code =
                        1 + u8::from(audio.block_size == stream.info.identification.block_sizes[1]);
                    stream.packet_modes[byte - 1] |= code << ((number & 3) * 2);
                }
                stream.first_audio.get_or_insert(number);
                state.last_audio = Some(number);
            }
        }
        Ok(())
    }
    fn page(&mut self, page: Page<'_>, offset: u64) -> Result<(), Error> {
        let header = page.header();
        let Some(active) = self
            .active
            .iter()
            .position(|state| self.streams[state.stream].info.id.serial == header.serial)
        else {
            return Ok(());
        };
        let state = &mut self.active[active];
        let stream = &mut self.streams[state.stream];
        if state.modes.is_empty() {
            if header.granule_position != 0 && header.granule_position != UNKNOWN_GRANULE_POSITION {
                return Err(Error::Invalid("nonzero Vorbis header granule"));
            }
        } else if Some(offset) == state.setup_page {
            if header.granule_position != 0 {
                return Err(Error::Invalid("setup page granule"));
            }
        } else if page.completed_packets() != 0 || header.is_eos() {
            state.timing.check_page(header)?;
            if let Some(packet) = state.last_audio
                && stream
                    .marks
                    .last()
                    .is_none_or(|mark| header.granule_position >= mark.granule)
            {
                reserve_index(
                    &mut stream.marks,
                    1,
                    &mut self.memory,
                    self.options.max_index_memory,
                )?;
                stream.marks.push(SeekMark {
                    granule: granule(header.granule_position)?,
                    packet,
                    raw_end: state.timing.raw_end,
                });
            }
        }
        if header.is_eos() {
            if state.modes.is_empty() {
                return Err(Error::Invalid("EOS before all Vorbis headers"));
            }
            stream.end = state.timing.end(stream.info.id, header)?;
            (stream.end_padding_frames, stream.extended_end_trim) =
                state.timing.end_padding(header)?;
            stream.origin = state.timing.origin.unwrap_or(0);
            self.active.swap_remove(active);
        }
        Ok(())
    }
}

fn check_decoder_memory(setup: &Setup, options: SeekOptions) -> Result<(), Error> {
    let decode = PacketDecoder::required_storage(setup);
    if decode > options.limits.max_decode_memory {
        return Err(Error::LimitExceeded("decode memory"));
    }
    if setup
        .storage_bytes()
        .checked_add(decode)
        .is_none_or(|n| n > options.max_stream_memory)
    {
        return Err(Error::LimitExceeded("selected stream memory"));
    }
    Ok(())
}
fn charge(memory: &mut usize, amount: usize, limit: usize) -> Result<(), Error> {
    *memory = memory
        .checked_add(amount)
        .filter(|&n| n <= limit)
        .ok_or(Error::LimitExceeded("Vorbis seek index memory"))?;
    Ok(())
}
fn reserve_index<T>(
    values: &mut Vec<T>,
    count: usize,
    memory: &mut usize,
    limit: usize,
) -> Result<(), Error> {
    let required = values
        .len()
        .checked_add(count)
        .ok_or(Error::LimitExceeded("Vorbis seek index memory"))?;
    if required <= values.capacity() {
        return Ok(());
    }
    let old = values.capacity();
    let maximum = old + (limit - *memory) / core::mem::size_of::<T>();
    if required > maximum {
        return Err(Error::LimitExceeded("Vorbis seek index memory"));
    }
    let target = old.saturating_mul(2).max(4).max(required).min(maximum);
    values
        .try_reserve_exact(target - values.len())
        .map_err(|_| Error::Allocation)?;
    charge(
        memory,
        (values.capacity() - old) * core::mem::size_of::<T>(),
        limit,
    )
}
fn read_page<'a, R: Read + Seek>(
    reader: &'a mut PageReader<R>,
    entry: &IndexedPage,
) -> Result<Page<'a>, Error> {
    if reader.position() != entry.offset {
        reader.seek(entry.offset)?;
    }
    let page = reader
        .next_page()?
        .ok_or(Error::Container(container::Error::Truncated))?;
    if page.offset != entry.offset
        || page.page.header() != entry.header
        || page.page.bytes().len() != entry.length as usize
    {
        return Err(Error::Invalid("source changed after Ogg indexing"));
    }
    Ok(page.page)
}

/// Seekable native Vorbis decoder. Select each logical stream independently;
/// no implicit resampling or combined timestamp across incompatible chains.
/// Repeated seeks in the same stream retain setup, tables and synthesis scratch.
/// A seek selects one pre-roll packet; synthesis/discard happens in read_next.
pub struct OggSeekReader<R> {
    pages: PageReader<R>,
    index: VorbisIndex,
    selected: Option<usize>,
    decoder: Option<PacketDecoder>,
    capture: PacketCapture,
    next_page: usize,
    page_pending: bool,
    prime_packet: u64,
    primed: bool,
    last_span: u64,
    position: u64,
    announce: bool,
    finished: bool,
    end_seek: bool,
    failed: bool,
}
impl<R: Read + Seek> OggSeekReader<R> {
    pub fn new(reader: R) -> Result<Self, Error> {
        Self::with_options(reader, SeekOptions::default())
    }
    pub fn with_options(mut reader: R, options: SeekOptions) -> Result<Self, Error> {
        let index = VorbisIndex::read(&mut reader, options)?;
        Self::with_index(reader, index)
    }
    /// The supplied source must be the immutable source used to build index.
    pub fn with_index(reader: R, index: VorbisIndex) -> Result<Self, Error> {
        let id = index
            .streams
            .first()
            .ok_or(Error::Invalid("no Vorbis streams"))?
            .info
            .id;
        let limits = index.options.container_limits;
        let validation = index.options.validation();
        let mut out = Self {
            pages: PageReader::new(reader),
            index,
            selected: None,
            decoder: None,
            capture: PacketCapture::with_validation(id.serial, limits, validation),
            next_page: 0,
            page_pending: false,
            prime_packet: 0,
            primed: false,
            last_span: 0,
            position: 0,
            announce: true,
            finished: false,
            end_seek: false,
            failed: false,
        };
        out.seek(id, 0)?;
        Ok(out)
    }
    pub fn index(&self) -> &VorbisIndex {
        &self.index
    }
    pub fn selected(&self) -> &SeekStream {
        &self.index.streams[self.selected.unwrap()]
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn decoder_storage_bytes(&self) -> usize {
        self.decoder
            .as_ref()
            .map_or(0, |d| d.setup().storage_bytes() + d.storage_bytes())
    }
    /// Seek by retained frame within a logical stream, allowing its exact end.
    /// Invalid arguments leave the cursor usable. A successful seek also resets
    /// a terminal read error; IO/setup errors during seek remain terminal.
    pub fn seek(&mut self, id: StreamId, frame: u64) -> Result<(), Error> {
        let stream_id = self
            .index
            .streams
            .iter()
            .position(|s| s.info.id == id)
            .ok_or(Error::Invalid("unknown Vorbis seek stream"))?;
        let stream = &self.index.streams[stream_id];
        if frame > stream.end.frames {
            return Err(Error::Invalid("seek beyond Vorbis end"));
        }
        let result = (|| {
            if self.selected != Some(stream_id) || self.decoder.is_none() {
                self.decoder = None;
                let mut limits = self.index.options.limits;
                limits.max_setup_memory = limits
                    .max_setup_memory
                    .min(self.index.options.max_stream_memory);
                let setup =
                    Setup::parse(&stream.setup_packet, &stream.info.identification, &limits)?;
                check_decoder_memory(&setup, self.index.options)?;
                self.decoder = Some(PacketDecoder::with_limits(setup, limits)?);
            }
            self.selected = Some(stream_id);
            let target = stream
                .end
                .start_granule
                .checked_add(frame)
                .ok_or(Error::LimitExceeded("seek granule"))?;
            let before = stream.marks.partition_point(|mark| mark.granule <= target);
            let mark = before.checked_sub(1).map(|n| stream.marks[n]);
            let (mut packet, mut base) = mark
                .map_or((stream.first_audio.unwrap_or(3), 0), |mark| {
                    (mark.packet, mark.raw_end)
                });
            if self.index.options.packet_preroll && frame < stream.end.frames {
                (packet, base) =
                    stream.refine_preroll(packet, base, target as i128 - stream.origin)?;
            }
            self.prime_packet = packet;
            self.primed = false;
            self.last_span = 0;
            self.position = frame;
            self.announce = true;
            self.finished = false;
            self.end_seek = frame == stream.end.frames;
            self.page_pending = false;
            self.decoder.as_mut().unwrap().reset_at(base);
            if frame < stream.end.frames {
                let start = self
                    .index
                    .container
                    .packet_start_page(id.serial, packet)
                    .ok_or(Error::Invalid("missing seek pre-roll packet"))?;
                let entries = self.index.container.pages(id.serial).unwrap();
                self.next_page = entries.partition_point(|page| page.offset < start.offset);
                self.capture.reset_at(start);
                self.pages.seek(start.offset)?;
            }
            Ok(())
        })();
        self.failed = result.is_err();
        result
    }
    /// Process at most one selected-stream page. All output begins at the exact
    /// requested retained frame. Start/end events are repeated after each seek;
    /// StreamEnd describes the whole logical stream, not only the sought suffix.
    pub fn read_next(&mut self, emit: impl FnMut(OggEvent<'_>)) -> Result<bool, Error> {
        self.read_step(false, emit)
    }
    /// Process at most one selected-stream page and synthesize at most one
    /// packet at/after seek pre-roll. Returns true on progress, including a
    /// priming/discarded packet or a page fragment that emits no PCM; false only
    /// after StreamEnd. PCM is one complete overlap block, possibly trimmed at
    /// the requested start/end. Consume that block before the next call.
    ///
    /// The verified page stays buffered without copying, rereading or repeated
    /// CRC work. Calls may alternate with read_next, which drains the remaining
    /// page. Page granule and EOS validation occur when its final fragment is
    /// consumed; output remains provisional until those checks complete.
    pub fn read_packet(&mut self, emit: impl FnMut(OggEvent<'_>)) -> Result<bool, Error> {
        self.read_step(true, emit)
    }
    fn read_step(
        &mut self,
        one_packet: bool,
        mut emit: impl FnMut(OggEvent<'_>),
    ) -> Result<bool, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.finished {
            return Ok(false);
        }
        let result = self.read_inner(one_packet, &mut emit);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn read_inner(
        &mut self,
        one_packet: bool,
        emit: &mut impl FnMut(OggEvent<'_>),
    ) -> Result<bool, Error> {
        let stream = &self.index.streams[self.selected.unwrap()];
        if self.announce {
            if let Some(error) = &stream.comment_error {
                emit(OggEvent::CommentError {
                    stream: stream.info.id,
                    error: error.clone(),
                });
            }
            emit(OggEvent::StreamStart(&stream.info));
            self.announce = false;
        }
        if self.end_seek {
            emit(OggEvent::StreamEnd(stream.end));
            self.finished = true;
            return Ok(true);
        }
        let entries = self.index.container.pages(stream.info.id.serial).unwrap();
        let entry = entries
            .get(self.next_page)
            .ok_or(Error::Container(container::Error::Truncated))?;
        let page = if self.page_pending {
            self.pages
                .current_page()
                .ok_or(Error::Invalid("missing paused seek page"))?
                .page
        } else {
            read_page(&mut self.pages, entry)?
        };
        let mut failure = None;
        let decoder = self.decoder.as_mut().unwrap();
        let complete = self.capture.push_page_until(page, entry.offset, |packet| {
            if failure.is_some() || packet.info.number.unwrap() < self.prime_packet {
                return ControlFlow::Continue(());
            }
            let result = (|| {
                let Some(block) = decoder.decode(packet.data)? else {
                    return Ok(());
                };
                if !self.primed {
                    if packet.info.number != Some(self.prime_packet) || block.frames != 0 {
                        return Err(Error::Invalid("seek pre-roll packet mismatch"));
                    }
                    self.primed = true;
                    return Ok(());
                }
                self.last_span = block.frames as u64;
                let start = stream.origin + block.start_frame as i128;
                let target = stream.end.start_granule as i128 + self.position as i128;
                let skip = (target - start).clamp(0, block.frames as i128) as usize;
                let keep = (stream.end.end_granule as i128 - start).clamp(0, block.frames as i128)
                    as usize;
                if skip > keep {
                    return Err(Error::Invalid("seek trim interval"));
                }
                if skip != keep {
                    let granule = start + skip as i128;
                    if granule != target {
                        return Err(Error::Invalid("seek PCM discontinuity"));
                    }
                    let position = self.position;
                    self.position += (keep - skip) as u64;
                    emit(OggEvent::Pcm(OggPcm {
                        stream: stream.info.id,
                        sample_rate: stream.info.identification.sample_rate,
                        start_frame: position,
                        start_granule: granule as u64,
                        pcm: block.crop(skip, keep),
                    }));
                }
                Ok(())
            })();
            if let Err(error) = result {
                failure = Some(error);
            }
            if one_packet {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })?;
        if let Some(error) = failure {
            return Err(error);
        }
        self.page_pending = !complete;
        if !complete {
            return Ok(true);
        }
        self.next_page += 1;
        if self.primed && (page.completed_packets() != 0 || page.header().is_eos()) {
            let expected = stream.origin + decoder.position() as i128;
            let actual = granule(page.header().granule_position)? as i128;
            if page.header().is_eos() {
                if actual > expected
                    || (!stream.extended_end_trim && actual < expected - self.last_span as i128)
                {
                    return Err(Error::Invalid("seek EOS granule"));
                }
            } else if actual != expected {
                return Err(Error::Invalid("seek page granule"));
            }
        }
        if page.header().is_eos() {
            self.capture.finish()?;
            if self.position != stream.end.frames {
                return Err(Error::Invalid("seek final sample count"));
            }
            emit(OggEvent::StreamEnd(stream.end));
            self.finished = true;
        }
        Ok(true)
    }
    pub fn into_inner(self) -> R {
        self.pages.into_inner()
    }
    /// Retain the validated index when moving the source to another reader.
    pub fn into_parts(self) -> (R, VorbisIndex) {
        (self.pages.into_inner(), self.index)
    }
}
