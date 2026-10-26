use crate::{Error, IndexedPage, Limits, MAX_PAGE_SIZE, Page, UNKNOWN_GRANULE_POSITION, reserve};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureInfo {
    pub serial: u32,
    /// Known for an indexed capture or a capture beginning at BOS. Otherwise
    /// absolute numbering cannot be inferred from a partial logical stream.
    pub number: Option<u64>,
    pub first_in_stream: bool,
    pub last_in_stream: bool,
    pub granule_position: Option<u64>,
    pub start_page_offset: u64,
    pub end_page_offset: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct CapturedPacket<'a> {
    pub data: &'a [u8],
    pub info: CaptureInfo,
}

/// A selected logical stream captured at an arbitrary verified page boundary.
/// Discards only the initial unavailable continuation, then validates sequence,
/// continuation, granules, packet sizes and EOS strictly. Other serials are
/// ignored; physical group/chaining validation is provided by Demuxer/OggIndex.
/// This is a seek/capture API, not silent packet-hole concealment.
pub struct PacketCapture {
    serial: u32,
    limits: Limits,
    next_sequence: Option<u32>,
    next_number: Option<u64>,
    partial: Vec<u8>,
    partial_offset: u64,
    continuing: bool,
    skipping: bool,
    started: bool,
    ended: bool,
    granule: Option<u64>,
    failed: bool,
}
impl PacketCapture {
    pub fn new(serial: u32, limits: Limits) -> Self {
        Self {
            serial,
            limits,
            next_sequence: None,
            next_number: None,
            partial: Vec::new(),
            partial_offset: 0,
            continuing: false,
            skipping: false,
            started: false,
            ended: false,
            granule: None,
            failed: false,
        }
    }
    pub fn at_page(page: &IndexedPage, limits: Limits) -> Self {
        let mut capture = Self::new(page.header.serial, limits);
        capture.reset_at(page);
        capture
    }
    /// Retains packet assembly capacity; a subsequent capture can start at any
    /// verified page of the same serial, with unknown absolute packet numbers.
    pub fn reset(&mut self) {
        self.next_sequence = None;
        self.next_number = None;
        self.partial.clear();
        self.partial_offset = 0;
        self.continuing = false;
        self.skipping = false;
        self.started = false;
        self.ended = false;
        self.granule = None;
        self.failed = false;
    }
    /// Seed a repeated indexed seek, retaining scratch capacity. Supplying a
    /// different stream changes the selected serial. Index/source identity is
    /// the caller's responsibility; use the index from this exact byte source.
    pub fn reset_at(&mut self, page: &IndexedPage) {
        self.reset();
        self.serial = page.header.serial;
        self.next_sequence = Some(page.header.sequence);
        self.next_number = Some(page.first_packet);
    }
    pub fn pending_bytes(&self) -> usize {
        self.partial.len()
    }
    pub fn is_ended(&self) -> bool {
        self.ended
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if !self.ended {
            self.failed = true;
            return Err(Error::Truncated);
        }
        Ok(())
    }
    pub fn push_page(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: impl FnMut(CapturedPacket<'_>),
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
        mut emit: impl FnMut(CapturedPacket<'_>),
    ) -> Result<(), Error> {
        let header = page.header();
        if header.serial != self.serial {
            return Ok(());
        }
        if self.ended {
            return Err(Error::Invalid("page after captured EOS"));
        }
        if let Some(expected) = self.next_sequence
            && header.sequence != expected
        {
            return Err(Error::Sequence {
                serial: self.serial,
                expected,
                actual: header.sequence,
            });
        }
        let complete = page.completed_packets();
        if complete == 0
            && !page.lacing().is_empty()
            && header.granule_position != UNKNOWN_GRANULE_POSITION
        {
            return Err(Error::Invalid("granule on page without a completed packet"));
        }
        if complete != 0 && header.granule_position == UNKNOWN_GRANULE_POSITION {
            return Err(Error::Invalid(
                "unknown granule on page with completed packets",
            ));
        }
        if self.started {
            if header.is_bos() {
                return Err(Error::Invalid("BOS during packet capture"));
            }
            if header.is_continued() != self.continuing {
                return Err(Error::Invalid("captured packet continuation flag"));
            }
        } else {
            if header.is_bos() {
                if header.sequence != 0
                    || header.is_continued()
                    || complete != 1
                    || page.lacing().last() == Some(&255)
                {
                    return Err(Error::Invalid("invalid captured BOS"));
                }
                self.next_number = Some(0);
            }
            self.skipping = header.is_continued();
            self.continuing = self.skipping;
        }
        if header.granule_position != UNKNOWN_GRANULE_POSITION {
            if self.granule.is_some_and(|gp| header.granule_position < gp) {
                return Err(Error::Invalid("decreasing captured granule"));
            }
            self.granule = Some(header.granule_position);
        }
        let mut skipping = self.skipping;
        let mut length = self.partial.len();
        let mut continuing = self.continuing;
        for fragment in page.fragments() {
            continuing = !fragment.ends_packet;
            if !skipping {
                length = length
                    .checked_add(fragment.data.len())
                    .ok_or(Error::LimitExceeded("packet size"))?;
                if length > self.limits.max_packet_bytes {
                    return Err(Error::LimitExceeded("packet size"));
                }
                if (!fragment.starts_packet || !fragment.ends_packet)
                    && length > self.limits.max_pending_bytes
                {
                    return Err(Error::LimitExceeded("pending packet bytes"));
                }
            }
            if fragment.ends_packet {
                length = 0;
                skipping = false;
            }
        }
        if header.is_eos() && continuing {
            return Err(Error::Invalid("unfinished captured packet at EOS"));
        }
        let mut remaining = complete;
        for fragment in page.fragments() {
            if fragment.starts_packet {
                self.partial_offset = offset;
            }
            if !self.skipping && (!fragment.starts_packet || !fragment.ends_packet) {
                reserve(&mut self.partial, fragment.data.len())?;
                self.partial.extend_from_slice(fragment.data);
            }
            self.continuing = !fragment.ends_packet;
            if !fragment.ends_packet {
                continue;
            }
            remaining -= 1;
            if !self.skipping {
                let data = if fragment.starts_packet {
                    fragment.data
                } else {
                    &self.partial
                };
                emit(CapturedPacket {
                    data,
                    info: CaptureInfo {
                        serial: self.serial,
                        number: self.next_number,
                        first_in_stream: self.next_number == Some(0),
                        last_in_stream: remaining == 0 && header.is_eos(),
                        granule_position: (remaining == 0).then_some(header.granule_position),
                        start_page_offset: self.partial_offset,
                        end_page_offset: offset,
                    },
                });
            }
            self.next_number = self
                .next_number
                .map(|n| n.checked_add(1).ok_or(Error::LimitExceeded("packet count")))
                .transpose()?;
            self.skipping = false;
            self.partial.clear();
            if self.partial.capacity() > MAX_PAGE_SIZE {
                self.partial = Vec::new();
            }
        }
        self.started = true;
        self.next_sequence = Some(header.sequence.wrapping_add(1));
        if header.is_eos() {
            self.ended = true;
            self.partial = Vec::new();
        }
        Ok(())
    }
}
