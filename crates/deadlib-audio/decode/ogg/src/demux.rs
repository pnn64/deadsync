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

enum PartialPacket {
    Bytes(Vec<u8>),
    Length(usize),
}

impl PartialPacket {
    fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Length(length) => *length,
        }
    }
}

struct Stream {
    serial: u32,
    chain: u32,
    next_sequence: u32,
    ended: bool,
    continuing: bool,
    partial: PartialPacket,
    packet_offset: u64,
    packets: u64,
    granule: u64,
}

// Keep the first stream inline and retain the heap storage once it is needed.
enum Streams {
    Single(Option<Stream>),
    Many(Vec<Stream>),
}

impl Streams {
    fn as_slice(&self) -> &[Stream] {
        match self {
            Self::Single(stream) => stream.as_slice(),
            Self::Many(streams) => streams,
        }
    }

    fn find(&self, serial: u32) -> Option<(usize, &Stream)> {
        match self {
            Self::Single(stream) => stream
                .as_ref()
                .filter(|s| s.serial == serial)
                .map(|s| (0, s)),
            Self::Many(streams) => streams
                .iter()
                .enumerate()
                .rev()
                .find(|(_, s)| s.serial == serial),
        }
    }

    fn get_mut(&mut self, index: usize) -> &mut Stream {
        match self {
            Self::Single(stream) => {
                debug_assert_eq!(index, 0);
                stream.as_mut().unwrap()
            }
            Self::Many(streams) => &mut streams[index],
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Single(stream) => *stream = None,
            Self::Many(streams) => streams.clear(),
        }
    }

    fn push(&mut self, stream: Stream) -> Result<usize, Error> {
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
                *self = Self::Many(streams);
                Ok(1)
            }
            Self::Many(streams) => {
                reserve(streams, 1)?;
                let index = streams.len();
                streams.push(stream);
                Ok(index)
            }
        }
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
pub struct Demuxer {
    streams: Streams,
    limits: Limits,
    active: usize,
    chain: u32,
    bos_phase: bool,
    pending: usize,
    failed: bool,
    validation: ValidationOptions,
    compatibility: CompatibilityStats,
}
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
    /// Discard logical streams and terminal state, retaining stream-list capacity.
    pub fn reset(&mut self) {
        self.streams.clear();
        self.active = 0;
        self.chain = 0;
        self.bos_phase = true;
        self.pending = 0;
        self.failed = false;
        self.compatibility = CompatibilityStats::default();
    }
    pub fn compatibility(&self) -> CompatibilityStats {
        self.compatibility
    }
    pub fn chain(&self) -> u32 {
        self.chain
    }
    pub fn active_streams(&self) -> usize {
        self.active
    }
    pub fn pending_bytes(&self) -> usize {
        self.pending
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.streams.as_slice().is_empty() || self.active != 0 {
            self.failed = true;
            return Err(Error::Truncated);
        }
        Ok(())
    }
    pub fn push_page(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: impl FnMut(Packet<'_>),
    ) -> Result<(), Error> {
        self.push_indexed_page::<true>(page, offset, emit)
            .map(|_| ())
    }
    pub(crate) fn push_indexed_page<const OBSERVE: bool>(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: impl FnMut(Packet<'_>),
    ) -> Result<PageState, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self.push_inner::<OBSERVE>(page, offset, emit);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_inner<const OBSERVE: bool>(
        &mut self,
        page: Page<'_>,
        offset: u64,
        mut emit: impl FnMut(Packet<'_>),
    ) -> Result<PageState, Error> {
        let header = page.header();
        let complete = page.completed_packets();
        let index = match self.streams.find(header.serial) {
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
                index
            }
            None => {
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
                    partial: if OBSERVE {
                        PartialPacket::Bytes(Vec::new())
                    } else {
                        PartialPacket::Length(0)
                    },
                    packet_offset: offset,
                    packets: 0,
                    granule: UNKNOWN_GRANULE_POSITION,
                })?;
                self.active += 1;
                index
            }
        };
        let stream = self.streams.get_mut(index);
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
        let carried = stream.partial.len();
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
        if OBSERVE {
            let PartialPacket::Bytes(partial) = &mut stream.partial else {
                unreachable!("observers require packet bytes");
            };
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
        } else {
            let lacing = page.lacing();
            // Nil pages leave a carried packet alone.
            if let Some(&last) = lacing.last() {
                let continuing = last == 255;
                if continuing || stream.continuing {
                    let PartialPacket::Length(length) = &mut stream.partial else {
                        unreachable!("an index without observers only needs lengths");
                    };
                    *length = if !continuing {
                        0
                    } else if complete == 0 {
                        carried + body
                    } else {
                        lacing.iter().rev().take_while(|&&lace| lace == 255).count() * 255
                    };
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
        }
        pending += stream.partial.len();
        self.pending = pending;
        stream.next_sequence = header.sequence.wrapping_add(1);
        if header.is_eos() {
            stream.ended = true;
            if let PartialPacket::Bytes(partial) = &mut stream.partial {
                *partial = Vec::new();
            }
            self.active -= 1;
        }
        if !header.is_bos() {
            self.bos_phase = false;
        }
        self.compatibility.add(compatibility)?;
        Ok(PageState {
            stream: index,
            first_packet_offset,
            first_packet,
            completed_packets: complete as u16,
        })
    }
}
