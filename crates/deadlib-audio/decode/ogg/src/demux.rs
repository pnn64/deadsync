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

struct Stream {
    serial: u32,
    chain: u32,
    next_sequence: u32,
    ended: bool,
    continuing: bool,
    partial: Vec<u8>,
    packet_offset: u64,
    packets: u64,
    granule: u64,
}

/// Physical-stream validator and demultiplexer, strict by default. Named
/// compatibility exceptions require with_validation. Errors are terminal
/// until reset. Codec-specific header ordering and time interpretation belong
/// to the codec mapping, not this container.
pub struct Demuxer {
    streams: Vec<Stream>,
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
            streams: Vec::new(),
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
    pub fn reset(&mut self) {
        *self = Self::with_validation(self.limits, self.validation);
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
        if self.streams.is_empty() || self.active != 0 {
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
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self.push_inner(page, offset, emit);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn push_inner(
        &mut self,
        page: Page<'_>,
        offset: u64,
        mut emit: impl FnMut(Packet<'_>),
    ) -> Result<(), Error> {
        let header = page.header();
        let complete = page.completed_packets();
        let index = match self.streams.iter().rposition(|s| s.serial == header.serial) {
            Some(index) => {
                let stream = &self.streams[index];
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
                if self.active == 0 && !self.streams.is_empty() {
                    self.chain = self
                        .chain
                        .checked_add(1)
                        .ok_or(Error::LimitExceeded("chain count"))?;
                    self.bos_phase = true;
                }
                if !self.bos_phase {
                    return Err(Error::Invalid("BOS after non-BOS page in a group"));
                }
                if self.streams.len() == self.limits.max_streams {
                    return Err(Error::LimitExceeded("logical streams"));
                }
                reserve(&mut self.streams, 1)?;
                self.streams.push(Stream {
                    serial: header.serial,
                    chain: self.chain,
                    next_sequence: 0,
                    ended: false,
                    continuing: false,
                    partial: Vec::new(),
                    packet_offset: offset,
                    packets: 0,
                    granule: UNKNOWN_GRANULE_POSITION,
                });
                self.active += 1;
                self.streams.len() - 1
            }
        };
        let stream = &mut self.streams[index];
        let previous = (stream.granule != UNKNOWN_GRANULE_POSITION).then_some(stream.granule);
        let (granule, compatibility) =
            validation::granule(page, complete, previous, self.validation)?;
        if let Some(granule) = granule {
            stream.granule = granule;
        }
        // Preflight the entire page before emitting any of its packets.
        let mut length = stream.partial.len();
        let mut pending = self.pending - length;
        let body = page.body().len();
        // Without buffered packet bytes, the body size bounds every packet.
        if length != 0
            || body > self.limits.max_packet_bytes
            || pending
                .checked_add(body)
                .is_none_or(|n| n > self.limits.max_pending_bytes)
        {
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
        if header.is_eos()
            && (length != 0
                || page.lacing().last() == Some(&255)
                || (page.lacing().is_empty() && stream.continuing))
        {
            return Err(Error::Invalid("unfinished packet at EOS"));
        }
        let mut remaining = complete;
        for fragment in page.fragments() {
            if fragment.starts_packet {
                stream.packet_offset = offset;
            }
            if !fragment.starts_packet || !fragment.ends_packet {
                reserve(&mut stream.partial, fragment.data.len())?;
                stream.partial.extend_from_slice(fragment.data);
            }
            stream.continuing = !fragment.ends_packet;
            if !fragment.ends_packet {
                continue;
            }
            remaining -= 1;
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
                start_page_offset: stream.packet_offset,
                end_page_offset: offset,
            };
            let data = if fragment.starts_packet {
                fragment.data
            } else {
                &stream.partial
            };
            emit(Packet { data, info });
            stream.packets = stream
                .packets
                .checked_add(1)
                .ok_or(Error::LimitExceeded("packet count"))?;
            stream.partial.clear();
            if stream.partial.capacity() > MAX_PAGE_SIZE {
                stream.partial = Vec::new();
            }
        }
        pending += stream.partial.len();
        self.pending = pending;
        stream.next_sequence = header.sequence.wrapping_add(1);
        if header.is_eos() {
            stream.ended = true;
            stream.partial = Vec::new();
            self.active -= 1;
        }
        if !header.is_bos() {
            self.bos_phase = false;
        }
        self.compatibility.add(compatibility)?;
        Ok(())
    }
}
