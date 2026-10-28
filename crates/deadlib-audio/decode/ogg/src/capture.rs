use crate::{
    CompatibilityStats, Error, IndexedPage, Limits, MAX_PAGE_SIZE, Page, PageHeader,
    ValidationOptions, reserve, validation,
};
use core::ops::ControlFlow;

struct PendingPage {
    offset: u64,
    header: PageHeader,
    checksum: u32,
    length: usize,
    segment: usize,
    body: usize,
    remaining: usize,
    compatibility: CompatibilityStats,
}

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
/// Named compatibility exceptions require with_validation/at_page_with_validation.
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
    validation: ValidationOptions,
    compatibility: CompatibilityStats,
    page: Option<PendingPage>,
}
impl PacketCapture {
    pub fn new(serial: u32, limits: Limits) -> Self {
        Self::with_validation(serial, limits, ValidationOptions::default())
    }
    pub fn with_validation(serial: u32, limits: Limits, validation: ValidationOptions) -> Self {
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
            validation,
            compatibility: CompatibilityStats::default(),
            page: None,
        }
    }
    pub fn at_page(page: &IndexedPage, limits: Limits) -> Self {
        Self::at_page_with_validation(page, limits, ValidationOptions::default())
    }
    pub fn at_page_with_validation(
        page: &IndexedPage,
        limits: Limits,
        validation: ValidationOptions,
    ) -> Self {
        let mut capture = Self::with_validation(page.header.serial, limits, validation);
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
        self.compatibility = CompatibilityStats::default();
        self.page = None;
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
    pub fn compatibility(&self) -> CompatibilityStats {
        self.compatibility
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
        mut emit: impl FnMut(CapturedPacket<'_>),
    ) -> Result<(), Error> {
        self.push_page_until(page, offset, |packet| {
            emit(packet);
            ControlFlow::Continue(())
        })?;
        Ok(())
    }
    /// Capture until the callback breaks after a complete packet. Returns true
    /// when the entire page is consumed. If false, resume with the identical
    /// verified page and offset (push_page may also drain its remainder).
    /// Header/continuation/granule and every fragment's size limits are checked
    /// before emitting the first packet. Page completion/compatibility accounting
    /// is committed once, after its last fragment. Other serials are ignored.
    /// Errors are terminal until reset, which also discards any paused cursor.
    pub fn push_page_until(
        &mut self,
        page: Page<'_>,
        offset: u64,
        emit: impl FnMut(CapturedPacket<'_>) -> ControlFlow<()>,
    ) -> Result<bool, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        let result = self.push_inner(page, offset, emit);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn begin_page(&mut self, page: Page<'_>, offset: u64) -> Result<PendingPage, Error> {
        let header = page.header();
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
        let (granule, compatibility) =
            validation::granule(page, complete, self.granule, self.validation).map_err(
                |error| {
                    if error == Error::Invalid("decreasing granule position") {
                        Error::Invalid("decreasing captured granule")
                    } else {
                        error
                    }
                },
            )?;
        if granule.is_some() {
            self.granule = granule;
        }
        let mut skipping = self.skipping;
        let mut length = self.partial.len();
        let mut continuing = self.continuing;
        let body = page.body().len();
        if length == 0
            && body <= self.limits.max_packet_bytes
            && body <= self.limits.max_pending_bytes
        {
            continuing = page.lacing().last().map_or(continuing, |&n| n == 255);
        } else {
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
        }
        if header.is_eos() && continuing {
            return Err(Error::Invalid("unfinished captured packet at EOS"));
        }
        Ok(PendingPage {
            offset,
            header,
            checksum: page.checksum(),
            length: page.bytes().len(),
            segment: 0,
            body: 0,
            remaining: complete,
            compatibility,
        })
    }
    fn push_inner(
        &mut self,
        page: Page<'_>,
        offset: u64,
        mut emit: impl FnMut(CapturedPacket<'_>) -> ControlFlow<()>,
    ) -> Result<bool, Error> {
        if page.header().serial != self.serial {
            return Ok(true);
        }
        let mut cursor = if let Some(cursor) = self.page.take() {
            if cursor.offset != offset
                || cursor.header != page.header()
                || cursor.checksum != page.checksum()
                || cursor.length != page.bytes().len()
            {
                return Err(Error::Invalid(
                    "different page during paused packet capture",
                ));
            }
            cursor
        } else {
            self.begin_page(page, offset)?
        };
        for fragment in page.fragments_from(cursor.segment, cursor.body) {
            cursor.segment += fragment.data.len() / 255 + usize::from(fragment.ends_packet);
            cursor.body += fragment.data.len();
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
            cursor.remaining -= 1;
            let mut stop = false;
            if !self.skipping {
                let data = if fragment.starts_packet {
                    fragment.data
                } else {
                    &self.partial
                };
                stop = emit(CapturedPacket {
                    data,
                    info: CaptureInfo {
                        serial: self.serial,
                        number: self.next_number,
                        first_in_stream: self.next_number == Some(0),
                        last_in_stream: cursor.remaining == 0 && cursor.header.is_eos(),
                        granule_position: (cursor.remaining == 0)
                            .then_some(cursor.header.granule_position),
                        start_page_offset: self.partial_offset,
                        end_page_offset: offset,
                    },
                })
                .is_break();
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
            if stop && cursor.segment != page.lacing().len() {
                self.page = Some(cursor);
                return Ok(false);
            }
        }
        self.started = true;
        self.next_sequence = Some(cursor.header.sequence.wrapping_add(1));
        if cursor.header.is_eos() {
            self.ended = true;
        }
        self.compatibility.add(cursor.compatibility)?;
        Ok(true)
    }
}
