use std::collections::HashMap;

use crate::{
    CompatibilityStats, Error, MAX_PAGE_SIZE, Page, UNKNOWN_GRANULE_POSITION, ValidationOptions,
    reserve, validation,
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// A policy limit, not a format restriction. Ogg packets are unbounded.
    pub max_packet_bytes: usize,
    pub max_pending_bytes: usize,
    /// Total logical streams, including completed chains (serial uniqueness).
    pub max_streams: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_packet_bytes: 64 << 20,
            max_pending_bytes: 128 << 20,
            max_streams: 4096,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacketInfo {
    pub serial: u32,
    pub chain: u32,
    pub number: u64,
    pub first_in_stream: bool,
    /// True only for the last completed packet on a nonempty EOS page.
    /// A nil EOS page has no packet; observe that page's EOS separately.
    pub last_in_stream: bool,
    /// Only the last completed packet on a page receives its granule.
    pub granule_position: Option<u64>,
    pub start_page_offset: u64,
    pub end_page_offset: u64,
}

/// Borrowed for the callback duration. Full in-page packets borrow the input
/// page directly; only packets spanning pages are copied into assembly storage.
#[derive(Clone, Copy, Debug)]
pub struct Packet<'a> {
    pub data: &'a [u8],
    pub info: PacketInfo,
}

struct Stream<P> {
    serial: u32,
    chain: u32,
    next_sequence: u32,
    ended: bool,
    continuing: bool,
    partial: P,
    packet_offset: u64,
    packets: u64,
    granule: u64,
}

// Keep the first stream inline and retain the heap storage once it is needed.
enum Streams<P> {
    Single(Option<Stream<P>>),
    Many {
        streams: Vec<Stream<P>>,
        serials: Option<HashMap<u32, u32>>,
    },
}

impl<P> Streams<P> {
    fn as_slice(&self) -> &[Stream<P>] {
        match self {
            Self::Single(stream) => stream.as_slice(),
            Self::Many { streams, .. } => streams,
        }
    }

    fn find_mut(&mut self, serial: u32) -> Option<(usize, &mut Stream<P>)> {
        match self {
            Self::Single(stream) => stream
                .as_mut()
                .filter(|s| s.serial == serial)
                .map(|s| (0, s)),
            Self::Many { streams, serials } => Self::find_many(streams, serials, serial),
        }
    }

    #[inline(never)]
    fn find_many<'a>(
        streams: &'a mut [Stream<P>],
        serials: &mut Option<HashMap<u32, u32>>,
        serial: u32,
    ) -> Option<(usize, &'a mut Stream<P>)> {
        let (last, earlier) = streams.split_last_mut()?;
        if last.serial == serial {
            return Some((earlier.len(), last));
        }
        // A wide group only needs a table when a lookup goes past the last stream.
        if earlier.len() >= 127 {
            let map = serials.get_or_insert_with(|| Self::index_serials(earlier, last));
            if !map.is_empty() {
                let index = *map.get(&serial)? as usize;
                return Some((index, &mut earlier[index]));
            }
        }
        earlier
            .iter_mut()
            .enumerate()
            .rev()
            .find(|(_, s)| s.serial == serial)
    }

    fn get_mut(&mut self, index: usize) -> &mut Stream<P> {
        match self {
            Self::Single(stream) => {
                debug_assert_eq!(index, 0);
                stream.as_mut().unwrap()
            }
            Self::Many { streams, .. } => &mut streams[index],
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Single(stream) => *stream = None,
            Self::Many { streams, serials } => {
                streams.clear();
                *serials = None;
            }
        }
    }

    fn push(&mut self, stream: Stream<P>) -> Result<usize, Error> {
        match self {
            Self::Single(first) if first.is_none() => {
                *first = Some(stream);
                Ok(0)
            }
            Self::Single(first) => {
                let mut streams = Vec::new();
                reserve(&mut streams, 2)?;
                streams.push(first.take().unwrap());
                streams.push(stream);
                *self = Self::Many {
                    streams,
                    serials: None,
                };
                Ok(1)
            }
            Self::Many { streams, serials } => {
                reserve(streams, 1)?;
                let index = streams.len();
                if let Some(map) = serials {
                    Self::append_serial(map, stream.serial, index as u32);
                }
                streams.push(stream);
                Ok(index)
            }
        }
    }

    #[cold]
    fn append_serial(map: &mut HashMap<u32, u32>, serial: u32, index: u32) {
        if !map.is_empty() {
            if map.try_reserve(1).is_ok() {
                map.insert(serial, index);
            } else {
                map.clear();
            }
        }
    }

    #[cold]
    fn index_serials(earlier: &[Stream<P>], last: &Stream<P>) -> HashMap<u32, u32> {
        let mut map = HashMap::new();
        // An empty map keeps the linear lookup active if allocation fails.
        if map.try_reserve(earlier.len() + 1).is_ok() {
            // Unique u32 serials guarantee that every stream index fits in u32.
            map.extend(
                earlier
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (s.serial, i as u32)),
            );
            map.insert(last.serial, earlier.len() as u32);
        }
        map
    }
}

pub(crate) struct PageState {
    pub stream: usize,
    pub first_packet_offset: u64,
    pub first_packet: u64,
    pub completed_packets: u16,
}

/// Physical-stream validator and demultiplexer, strict by default. Named
/// compatibility exceptions require with_validation. Errors are terminal
/// until reset. Codec-specific header ordering and time interpretation belong
/// to the codec mapping, not this container.
pub struct Demuxer(Demux<Vec<u8>>);

impl Default for Demuxer {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl Demuxer {
    pub fn new(limits: Limits) -> Self {
        Self::with_validation(limits, ValidationOptions::default())
    }
    pub fn with_validation(limits: Limits, validation: ValidationOptions) -> Self {
        Self(Demux::with_validation(limits, validation))
    }
    /// Discard logical streams and terminal state, retaining stream-list capacity.
    pub fn reset(&mut self) {
        self.0.reset();
    }
    pub fn compatibility(&self) -> CompatibilityStats {
        self.0.compatibility()
    }
    pub fn chain(&self) -> u32 {
        self.0.chain()
    }
    pub fn active_streams(&self) -> usize {
        self.0.active
    }
    pub fn pending_bytes(&self) -> usize {
        self.0.pending
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        self.0.finish()
    }
    pub fn push_page(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: impl FnMut(Packet<'_>),
    ) -> Result<(), Error> {
        self.0.push_indexed_page(page, offset, emit).map(|_| ())
    }
}

pub(crate) struct Demux<P> {
    streams: Streams<P>,
    limits: Limits,
    active: usize,
    chain: u32,
    bos_phase: bool,
    pending: usize,
    failed: bool,
    validation: ValidationOptions,
    compatibility: CompatibilityStats,
}

impl<P: Assembly> Demux<P> {
    pub(crate) fn with_validation(limits: Limits, validation: ValidationOptions) -> Self {
        Self {
            streams: Streams::Single(None),
            limits,
            active: 0,
            chain: 0,
            bos_phase: true,
            pending: 0,
            failed: false,
            validation,
            compatibility: CompatibilityStats::default(),
        }
    }
    fn reset(&mut self) {
        self.streams.clear();
        self.active = 0;
        self.chain = 0;
        self.bos_phase = true;
        self.pending = 0;
        self.failed = false;
        self.compatibility = CompatibilityStats::default();
    }
    pub(crate) fn compatibility(&self) -> CompatibilityStats {
        self.compatibility
    }
    pub(crate) fn chain(&self) -> u32 {
        self.chain
    }
    pub(crate) fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.streams.as_slice().is_empty() || self.active != 0 {
            self.failed = true;
            return Err(Error::Truncated);
        }
        Ok(())
    }
    pub(crate) fn push_indexed_page(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: impl FnMut(Packet<'_>),
    ) -> Result<PageState, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self.push_inner(page, offset, emit);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    #[cold]
    #[inline(never)]
    fn start_stream(
        &mut self,
        page: Page<'_>,
        header: crate::PageHeader,
        offset: u64,
        complete: usize,
    ) -> Result<usize, Error> {
        if !header.is_bos() {
            return Err(Error::Invalid("logical stream missing BOS"));
        }
        if header.sequence != 0 {
            return Err(Error::Sequence {
                serial: header.serial,
                expected: 0,
                actual: header.sequence,
            });
        }
        if header.is_continued() || complete != 1 || page.lacing().last() == Some(&255) {
            return Err(Error::Invalid(
                "BOS must contain one complete initial packet",
            ));
        }
        let streams = self.streams.as_slice();
        if self.active == 0 && !streams.is_empty() {
            self.chain = self
                .chain
                .checked_add(1)
                .ok_or(Error::LimitExceeded("chain count"))?;
            self.bos_phase = true;
        }
        if !self.bos_phase {
            return Err(Error::Invalid("BOS after non-BOS page in a group"));
        }
        if streams.len() == self.limits.max_streams {
            return Err(Error::LimitExceeded("logical streams"));
        }
        let index = self.streams.push(Stream {
            serial: header.serial,
            chain: self.chain,
            next_sequence: 0,
            ended: false,
            continuing: false,
            partial: P::default(),
            packet_offset: offset,
            packets: 0,
            granule: UNKNOWN_GRANULE_POSITION,
        })?;
        self.active += 1;
        Ok(index)
    }
    fn push_inner(
        &mut self,
        page: Page<'_>,
        offset: u64,
        mut emit: impl FnMut(Packet<'_>),
    ) -> Result<PageState, Error> {
        let header = page.header();
        let complete = page.completed_packets();
        let (index, stream) = match self.streams.find_mut(header.serial) {
            Some((index, stream)) => {
                if header.is_bos() {
                    return Err(Error::Invalid("duplicate serial/BOS"));
                }
                if stream.ended {
                    return Err(Error::Invalid("page after EOS"));
                }
                if header.sequence != stream.next_sequence {
                    return Err(Error::Sequence {
                        serial: header.serial,
                        expected: stream.next_sequence,
                        actual: header.sequence,
                    });
                }
                if header.is_continued() != stream.continuing {
                    return Err(Error::Invalid("packet continuation flag"));
                }
                (index, stream)
            }
            None => {
                let index = self.start_stream(page, header, offset, complete)?;
                (index, self.streams.get_mut(index))
            }
        };
        let previous = (stream.granule != UNKNOWN_GRANULE_POSITION).then_some(stream.granule);
        let (granule, compatibility) =
            validation::granule(page, header, complete, previous, self.validation)?;
        if let Some(granule) = granule {
            stream.granule = granule;
        }
        // Preflight the entire page before emitting any of its packets.
        let first_packet_offset = if header.is_continued() {
            stream.packet_offset
        } else {
            offset
        };
        let carried = stream.partial.pending();
        let mut length = carried;
        let mut pending = self.pending - length;
        let body = page.body().len();
        // An empty body cannot enlarge a previously validated cached packet.
        if body == 0 {
            if complete != 0 {
                length = 0;
            }
        } else if length != 0
            || body > self.limits.max_packet_bytes
            || pending
                .checked_add(body)
                .is_none_or(|n| n > self.limits.max_pending_bytes)
        {
            if complete == 0 {
                // Without a terminator, the entire body belongs to one packet.
                length = length
                    .checked_add(body)
                    .ok_or(Error::LimitExceeded("packet size"))?;
                if length > self.limits.max_packet_bytes {
                    return Err(Error::LimitExceeded("packet size"));
                }
                if pending
                    .checked_add(length)
                    .is_none_or(|n| n > self.limits.max_pending_bytes)
                {
                    return Err(Error::LimitExceeded("pending packet bytes"));
                }
            } else {
                for fragment in page.fragments() {
                    length = length
                        .checked_add(fragment.data.len())
                        .ok_or(Error::LimitExceeded("packet size"))?;
                    if length > self.limits.max_packet_bytes {
                        return Err(Error::LimitExceeded("packet size"));
                    }
                    // Completed in-page packets do not need assembly allocation.
                    if (!fragment.starts_packet || !fragment.ends_packet)
                        && pending
                            .checked_add(length)
                            .is_none_or(|n| n > self.limits.max_pending_bytes)
                    {
                        return Err(Error::LimitExceeded("pending packet bytes"));
                    }
                    if fragment.ends_packet {
                        length = 0;
                    }
                }
            }
        }
        if header.is_eos()
            && (length != 0
                || page.lacing().last() == Some(&255)
                || (page.lacing().is_empty() && stream.continuing))
        {
            return Err(Error::Invalid("unfinished packet at EOS"));
        }
        let first_packet = stream.packets;
        if let Some(partial) = stream.partial.buffer() {
            let mut remaining = complete;
            for fragment in page.fragments() {
                if !fragment.starts_packet || !fragment.ends_packet {
                    if fragment.starts_packet {
                        stream.packet_offset = offset;
                    }
                    reserve(partial, fragment.data.len())?;
                    partial.extend_from_slice(fragment.data);
                    stream.continuing = !fragment.ends_packet;
                    if stream.continuing {
                        break;
                    }
                }
                remaining -= 1;
                let (data, start_page_offset) = if fragment.starts_packet {
                    (fragment.data, offset)
                } else {
                    (partial.as_slice(), stream.packet_offset)
                };
                let info = PacketInfo {
                    serial: stream.serial,
                    chain: stream.chain,
                    number: stream.packets,
                    first_in_stream: stream.packets == 0,
                    last_in_stream: remaining == 0 && header.is_eos(),
                    granule_position: if remaining == 0 {
                        Some(header.granule_position)
                    } else {
                        None
                    },
                    start_page_offset,
                    end_page_offset: offset,
                };
                emit(Packet { data, info });
                stream.packets = stream
                    .packets
                    .checked_add(1)
                    .ok_or(Error::LimitExceeded("packet count"))?;
                if !partial.is_empty() {
                    partial.clear();
                    if partial.capacity() > MAX_PAGE_SIZE {
                        *partial = Vec::new();
                    }
                }
            }
            pending += partial.len();
        } else {
            let lacing = page.lacing();
            let mut next_length = carried;
            // Nil pages leave a carried packet alone.
            if let Some(&last) = lacing.last() {
                let continuing = last == 255;
                if continuing || stream.continuing {
                    next_length = if !continuing {
                        0
                    } else if complete == 0 {
                        carried + body
                    } else {
                        lacing.iter().rev().take_while(|&&lace| lace == 255).count() * 255
                    };
                    stream.partial.set_len(next_length);
                    stream.continuing = continuing;
                }
                // Only the leading continuation started on an earlier page.
                if !header.is_continued() || complete > 1 || (complete == 1 && continuing) {
                    stream.packet_offset = offset;
                }
            }
            stream.packets = stream
                .packets
                .checked_add(complete as u64)
                .ok_or(Error::LimitExceeded("packet count"))?;
            pending += next_length;
        }
        self.pending = pending;
        stream.next_sequence = header.sequence.wrapping_add(1);
        if header.is_eos() {
            stream.ended = true;
            if let Some(partial) = stream.partial.buffer() {
                *partial = Vec::new();
            }
            self.active -= 1;
        }
        if !header.is_bos() {
            self.bos_phase = false;
        }
        self.compatibility.record(compatibility)?;
        Ok(PageState {
            stream: index,
            first_packet_offset,
            first_packet,
            completed_packets: complete as u16,
        })
    }
}

// Indexing without callbacks keeps only a length. Both representations
// share the same stream and packet validation.
pub(crate) trait Assembly: Default {
    fn pending(&self) -> usize;

    fn buffer(&mut self) -> Option<&mut Vec<u8>>;

    fn set_len(&mut self, _length: usize) {}
}

impl Assembly for Vec<u8> {
    fn pending(&self) -> usize {
        self.len()
    }

    fn buffer(&mut self) -> Option<&mut Vec<u8>> {
        Some(self)
    }
}

impl Assembly for usize {
    fn pending(&self) -> usize {
        *self
    }

    fn buffer(&mut self) -> Option<&mut Vec<u8>> {
        None
    }

    fn set_len(&mut self, length: usize) {
        *self = length;
    }
}
