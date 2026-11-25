use crate::demux::{Assembly, Demux};
use crate::{
    CompatibilityStats, Error, Limits, Packet, Page, PageHeader, PageReader,
    UNKNOWN_GRANULE_POSITION, ValidationOptions, reserve,
};
use core::ops::Range;
use std::io::{self, Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug)]
pub struct IndexOptions {
    pub container_limits: Limits,
    /// Bounds retained page entries; this is an application policy.
    pub max_pages: usize,
    pub validation: ValidationOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GranuleLocation {
    pub offset: u64,
    pub length: u32,
    pub header: PageHeader,
}

/// Find the last CRC-verified page of a serial with a known granule <= target,
/// without retaining a file index. Bisection uses codec-defined numeric granule
/// order and scans past unknown granules/other serials. No read leaves `range`.
/// Known granules must be nondecreasing within the serial; this search cannot
/// check that global invariant without walking the whole stream.
///
/// Capture recovery is used at probe offsets: this does not validate the whole
/// physical stream or prove that the result is a codec pre-roll point. Use
/// OggIndex for full validation, then PacketCapture plus codec-specific pre-roll.
/// The source cursor after this search is unspecified; seek to the returned
/// offset explicitly before capture. The same applies after an IO error.
pub fn find_granule<R: Read + Seek>(
    reader: &mut R,
    range: Range<u64>,
    serial: u32,
    target: u64,
) -> Result<Option<GranuleLocation>, Error> {
    if range.start > range.end {
        return Err(Error::Invalid("reversed seek range"));
    }
    if range.is_empty() {
        return Ok(None);
    }
    let source = BoundedSource {
        inner: reader,
        start: range.start,
        end: range.end,
        position: range.start,
    };
    let mut pages = PageReader::with_mode(source, crate::SyncMode::Recover);
    fn next<R: Read + Seek>(
        reader: &mut PageReader<R>,
        start: u64,
        end: u64,
        serial: u32,
    ) -> Result<Option<GranuleLocation>, Error> {
        reader.seek(start)?;
        while let Some(page) = reader.next_page()? {
            if page.offset >= end {
                return Ok(None);
            }
            let header = page.page.header();
            if header.serial == serial
                && header.granule_position != UNKNOWN_GRANULE_POSITION
                && (page.page.lacing().iter().any(|&n| n < 255) || page.page.lacing().is_empty())
            {
                return Ok(Some(GranuleLocation {
                    offset: page.offset,
                    length: page.page.bytes().len() as u32,
                    header,
                }));
            }
        }
        Ok(None)
    }
    let Some(first) = next(&mut pages, range.start, range.end, serial)? else {
        return Ok(None);
    };
    if first.header.granule_position > target {
        return Ok(None);
    }
    let mut best = first;
    let mut low = first.offset + first.length as u64;
    let mut high = range.end;
    while low < high {
        let middle = low + (high - low) / 2;
        match next(&mut pages, middle, high, serial)? {
            Some(page) if page.header.granule_position <= target => {
                best = page;
                low = page.offset + page.length as u64;
            }
            _ => high = middle,
        }
    }
    Ok(Some(best))
}

// Reuses one PageReader allocation across bisection probes while fencing reads
// at the caller's physical-file range. PageReader uses absolute seeks.
struct BoundedSource<'a, R> {
    inner: &'a mut R,
    start: u64,
    end: u64,
    position: u64,
}
impl<R: Read> Read for BoundedSource<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = (self.end - self.position).min(out.len() as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        let n = self.inner.read(&mut out[..count])?;
        self.position += n as u64;
        Ok(n)
    }
}
impl<R: Seek> Seek for BoundedSource<'_, R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.end.checked_add_signed(offset),
        }
        .filter(|&position| position >= self.start && position <= self.end)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek outside Ogg search range")
        })?;
        let actual = self.inner.seek(SeekFrom::Start(position))?;
        if actual != position {
            return Err(io::Error::other("seek returned a different position"));
        }
        self.position = actual;
        Ok(actual)
    }
}
impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            container_limits: Limits::default(),
            max_pages: 1 << 20,
            validation: ValidationOptions::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StreamIndex {
    pub serial: u32,
    pub chain: u32,
    pub bos_offset: u64,
    pub eos_offset: u64,
    pub end_offset: u64,
    pub packets: u64,
    pages: Range<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct IndexedPage {
    pub offset: u64,
    pub length: u32,
    pub header: PageHeader,
    /// Index into OggIndex::streams(), in physical BOS order.
    pub stream: usize,
    /// Absolute logical packet number at the start of this page. A leading
    /// continuation belongs to this packet; a nil page may have no packet.
    pub first_packet: u64,
    pub completed_packets: u16,
    /// Starting page of the first packet, including a leading continuation.
    pub first_packet_offset: u64,
    granule_floor: u64,
    landmark: usize,
}

/// CRC- and grammar-validated full physical-file index, with any explicitly
/// enabled compatibility exceptions counted by compatibility(). Entries are stored in
/// flat per-stream ranges, ordered by byte offset within each stream. The
/// granule lookup is logarithmic, including arbitrarily long packet spans.
/// Granules remain codec-defined integers; pre-roll and sample interpretation
/// belong to the codec. Building an index consumes the entire input from BOS.
pub struct OggIndex {
    streams: Vec<StreamIndex>,
    pages: Vec<IndexedPage>,
    length: u64,
    granule_reciprocal: u64,
    offset_scale: u64,
    packet_scale: u64,
    compatibility: CompatibilityStats,
}

#[inline(never)]
fn group_pages(pages: &mut [IndexedPage], streams: &mut [StreamIndex]) {
    // EOS records the physical end of each stream. Its sequence also gives the
    // page count unless the index could contain a wrapped sequence number.
    if pages.len() <= u32::MAX as usize {
        for stream in streams.iter_mut() {
            stream.pages.end = pages[stream.pages.end - 1].header.sequence as usize + 1;
        }
    } else {
        for stream in streams.iter_mut() {
            stream.pages.end = 0;
        }
        for page in pages.iter() {
            streams[page.stream].pages.end += 1;
        }
    }
    let mut position = 0;
    for stream in streams.iter_mut() {
        stream.pages.start = position;
        position += stream.pages.end;
        stream.pages.end = position;
    }
    // Physical order is already offset order within each stream. Stable bucket
    // destinations give the same ordering as (stream, offset), without comparisons.
    for page in pages.iter_mut() {
        let next = &mut streams[page.stream].pages.start;
        page.landmark = *next;
        *next += 1;
    }
    // Carry the displaced record so each destination is written once.
    for position in 0..pages.len() {
        let destination = pages[position].landmark;
        if destination != position {
            let first = pages[position];
            let mut page = core::mem::replace(&mut pages[destination], first);
            while page.landmark != position {
                let next = page.landmark;
                page = core::mem::replace(&mut pages[next], page);
            }
            pages[position] = page;
        }
    }
    let mut start = 0;
    for stream in streams {
        stream.pages.start = start;
        start = stream.pages.end;
    }
}

// Keep the extra arithmetic off the small-index search path.
#[inline(never)]
fn offset_hint(pages: &[IndexedPage], end: u64, offset: u64, scale: u64) -> Option<&IndexedPage> {
    if scale == 0 {
        return multiplexed_offset_hint(pages, end, offset);
    }
    let start = pages.first()?.offset;
    let distance = offset.checked_sub(start)?;
    // The surrounding search already excludes offsets at or beyond end.
    let slot = ((u128::from(distance) * u128::from(scale)) >> 64) as usize;
    let page = pages.get(slot)?;
    // A verified page containing the offset is necessarily the last start before it.
    let covers = |page: &IndexedPage| {
        offset
            .checked_sub(page.offset)
            .is_some_and(|n| n < u64::from(page.length))
    };
    if covers(page) {
        return Some(page);
    }
    let neighbor = if offset < page.offset {
        pages.get(slot.checked_sub(1)?)?
    } else {
        pages.get(slot + 1)?
    };
    covers(neighbor).then_some(neighbor)
}

#[inline(never)]
fn multiplexed_offset_hint(pages: &[IndexedPage], end: u64, offset: u64) -> Option<&IndexedPage> {
    let start = pages.first()?.offset;
    let distance = offset.checked_sub(start)?;
    let slot = (distance.checked_mul(pages.len() as u64)? / (end - start)) as usize;
    let page = pages.get(slot)?;
    // A gap can belong to this stream's preceding page. Check page starts.
    if offset < page.offset {
        let previous = pages.get(slot.checked_sub(1)?)?;
        return (previous.offset <= offset).then_some(previous);
    }
    let next = pages.get(slot + 1);
    if offset < next.map_or(end, |next| next.offset) {
        Some(page)
    } else {
        let after = pages.get(slot + 2).map_or(end, |after| after.offset);
        next.filter(|_| offset < after)
    }
}

fn granule_scale(pages: &[IndexedPage]) -> u64 {
    if pages.len() < 33 {
        return 0;
    }
    let first = pages[0].granule_floor;
    let span = pages[pages.len() - 1].granule_floor - first;
    let step = pages[1].granule_floor - first;
    if step != 0 && step.checked_mul((pages.len() - 1) as u64) == Some(span) {
        return u64::MAX / step;
    }
    // Ignore the possibly shortened final page when estimating the scale.
    let steps = pages.len() - 2;
    let span = pages[steps].granule_floor - first;
    if span < steps as u64 {
        return 0;
    }
    let scale = ((((steps as u128) << 64) - 1) / u128::from(span)) as u64;
    let quarter = steps / 4;
    for page in [quarter, 2 * quarter, 3 * quarter] {
        if granule_hint(pages, pages[page].granule_floor, scale).is_none() {
            return 0;
        }
    }
    scale
}

// Confirm both sides of the estimate, including ties between landmarks.
#[inline(never)]
fn granule_hint(pages: &[IndexedPage], target: u64, reciprocal: u64) -> Option<&IndexedPage> {
    let distance = target.checked_sub(pages[0].granule_floor)?;
    let estimate = ((u128::from(distance) + 1) * u128::from(reciprocal)) >> 64;
    let slot = usize::try_from(estimate).ok()?;
    let page = pages.get(slot)?;
    let next = pages.get(slot + 1)?;
    if page.granule_floor > target {
        let previous = pages.get(slot.checked_sub(1)?)?;
        (previous.granule_floor <= target).then_some(previous)
    } else if target >= next.granule_floor {
        let after = pages.get(slot + 2)?;
        (target < after.granule_floor).then_some(next)
    } else {
        Some(page)
    }
}

/// Borrowed observations during index construction. Packets precede their
/// completing page's Page event; nil pages still produce a Page event. Packet
/// data may borrow either the page or the demultiplexer's span assembly buffer.
/// Observations are provisional until construction succeeds: a later page,
/// callback or end-of-input validation can still fail the physical stream.
pub enum IndexEvent<'a> {
    Packet { page: Page<'a>, packet: Packet<'a> },
    Page { page: Page<'a>, offset: u64 },
}

impl OggIndex {
    pub fn read(reader: impl Read, options: IndexOptions) -> Result<Self, Error> {
        Self::read_inner::<usize, Error>(reader, options, |_| Ok(()))
    }
    /// Build the same validated index while observing its existing packet
    /// assembly and pages in physical order. This allows a codec to build its
    /// timing/header index in one forward source pass without a second demux.
    /// Callback errors stop construction; no partial index is returned.
    pub fn read_with_events<E: From<Error>>(
        reader: impl Read,
        options: IndexOptions,
        observe: impl FnMut(IndexEvent<'_>) -> Result<(), E>,
    ) -> Result<Self, E> {
        Self::read_inner::<Vec<u8>, E>(reader, options, observe)
    }
    fn read_inner<P: Assembly, E: From<Error>>(
        reader: impl Read,
        options: IndexOptions,
        mut observe: impl FnMut(IndexEvent<'_>) -> Result<(), E>,
    ) -> Result<Self, E> {
        let mut reader = PageReader::new(reader);
        let mut demux = Demux::<P>::with_validation(options.container_limits, options.validation);
        let mut index = Self {
            streams: Vec::new(),
            pages: Vec::new(),
            length: 0,
            granule_reciprocal: 0,
            offset_scale: 0,
            packet_scale: 0,
            compatibility: CompatibilityStats::default(),
        };
        let mut has_unknown = false;
        while let Some(located) = reader.next_page()? {
            let page = located.page;
            let mut failure = None;
            let state = demux.push_indexed_page(page, located.offset, |packet| {
                if failure.is_none() {
                    failure = observe(IndexEvent::Packet { page, packet }).err();
                }
            })?;
            if let Some(error) = failure {
                return Err(error);
            }
            if index.pages.len() == options.max_pages {
                return Err(Error::LimitExceeded("index pages").into());
            }
            let header = page.header();
            if header.is_bos() {
                reserve(&mut index.streams, 1)?;
                index.streams.push(StreamIndex {
                    serial: header.serial,
                    chain: demux.chain(),
                    bos_offset: located.offset,
                    eos_offset: 0,
                    end_offset: 0,
                    packets: 0,
                    pages: index.pages.len()..0,
                });
            }
            reserve(&mut index.pages, 1)?;
            let granule = header.granule_position;
            let (granule_floor, landmark) = if granule == UNKNOWN_GRANULE_POSITION {
                has_unknown = true;
                let previous = index
                    .pages
                    .last()
                    .expect("stream starts with a known granule");
                (previous.granule_floor, previous.landmark)
            } else {
                (granule, index.pages.len())
            };
            index.pages.push(IndexedPage {
                offset: located.offset,
                length: page.bytes().len() as u32,
                header,
                stream: state.stream,
                first_packet: state.first_packet,
                completed_packets: state.completed_packets,
                first_packet_offset: state.first_packet_offset,
                granule_floor,
                landmark,
            });
            index.length = located
                .offset
                .checked_add(page.bytes().len() as u64)
                .ok_or(Error::LimitExceeded("indexed file length"))?;
            if header.is_eos() {
                let stream = &mut index.streams[state.stream];
                stream.packets = state.first_packet + u64::from(state.completed_packets);
                stream.eos_offset = located.offset;
                stream.end_offset = index.length;
                stream.pages.end = index.pages.len();
            }
            observe(IndexEvent::Page {
                page,
                offset: located.offset,
            })?;
        }
        demux.finish()?;
        index.compatibility = demux.compatibility();
        let grouped = index
            .streams
            .windows(2)
            .all(|pair| pair[0].pages.end == pair[1].pages.start);
        if index.streams.len() == 1 && index.pages.len() >= 33 {
            let pages = &index.pages;
            // Each indexed page spans multiple bytes, so the 64-bit scale fits.
            let span = pages[pages.len() - 1].offset - pages[1].offset;
            index.offset_scale =
                (((pages.len() - 2) as u128) << 64).div_ceil(u128::from(span)) as u64;
            index.packet_scale = packet_scale(pages, index.streams[0].packets);
        }
        let strict_granules = index.compatibility == CompatibilityStats::default();
        if grouped && strict_granules {
            if index.streams.len() == 1 {
                index.granule_reciprocal = granule_scale(&index.pages);
            }
            return Ok(index);
        }
        if !grouped {
            group_pages(&mut index.pages, &mut index.streams);
        }
        // Grouping assigned each known granule its final page position.
        if strict_granules && !has_unknown {
            return Ok(index);
        }
        for stream in &index.streams {
            let mut floor = 0;
            let mut landmark = stream.pages.start;
            for (position, page) in stream
                .pages
                .clone()
                .zip(&mut index.pages[stream.pages.clone()])
            {
                if page.header.granule_position != UNKNOWN_GRANULE_POSITION
                    && (page.completed_packets != 0 || page.length == 27)
                    && page.header.granule_position >= floor
                {
                    floor = page.header.granule_position;
                    landmark = position;
                }
                page.granule_floor = floor;
                page.landmark = landmark;
            }
        }
        Ok(index)
    }
    pub fn streams(&self) -> &[StreamIndex] {
        &self.streams
    }
    pub fn compatibility(&self) -> CompatibilityStats {
        self.compatibility
    }
    pub fn stream(&self, serial: u32) -> Option<&StreamIndex> {
        let (first, rest) = self.streams.split_first()?;
        if first.serial == serial {
            return Some(first);
        }
        let (last, middle) = rest.split_last()?;
        if last.serial == serial {
            Some(last)
        } else {
            middle.iter().find(|s| s.serial == serial)
        }
    }
    pub fn pages(&self, serial: u32) -> Option<&[IndexedPage]> {
        Some(&self.pages[self.stream(serial)?.pages.clone()])
    }
    pub fn length(&self) -> u64 {
        self.length
    }
    pub fn storage_bytes(&self) -> usize {
        self.pages.capacity() * core::mem::size_of::<IndexedPage>()
            + self.streams.capacity() * core::mem::size_of::<StreamIndex>()
    }
    /// Last known granule <= target. Equal granules choose the latest page,
    /// including a nil EOS. Unknown-granule span pages are never returned.
    /// With explicit compatibility options, ignored zero span granules and a
    /// decreasing EOS are excluded from ordered landmarks. Raw header values
    /// remain available in pages(), including the declared final endpoint.
    #[inline]
    pub fn granule_at_or_before(&self, serial: u32, target: u64) -> Option<&IndexedPage> {
        let pages = self.pages(serial)?;
        let (last, earlier) = pages.split_last()?;
        let page = if last.granule_floor <= target {
            last
        } else if self.granule_reciprocal != 0
            && let Some(page) = granule_hint(pages, target, self.granule_reciprocal)
        {
            page
        } else {
            let end = earlier.partition_point(|page| page.granule_floor <= target);
            &earlier[end.checked_sub(1)?]
        };
        Some(&self.pages[page.landmark])
    }
    #[inline]
    pub fn page_at_or_before(&self, serial: u32, offset: u64) -> Option<&IndexedPage> {
        let pages = self.pages(serial)?;
        let (last, earlier) = pages.split_last()?;
        if last.offset <= offset {
            return Some(last);
        }
        if earlier.len() >= 32
            && let Some(page) = offset_hint(&earlier[1..], last.offset, offset, self.offset_scale)
        {
            return Some(page);
        }
        let end = earlier.partition_point(|page| page.offset <= offset);
        Some(&earlier[end.checked_sub(1)?])
    }
    /// Page on which the requested packet starts, even for a packet spanning
    /// many pages. The returned page may begin with an earlier continuation;
    /// PacketCapture discards that fragment before returning complete packets.
    pub fn packet_start_page(&self, serial: u32, number: u64) -> Option<&IndexedPage> {
        let stream = self.stream(serial)?;
        if number >= stream.packets {
            return None;
        }
        let pages = &self.pages[stream.pages.clone()];
        // Equal totals are a hint. Verify the candidate starts and completes the packet.
        if pages.len() as u64 == stream.packets {
            let page = &pages[number as usize];
            if page.first_packet == number
                && page.completed_packets != 0
                && page.first_packet_offset == page.offset
            {
                return Some(page);
            }
        }
        if number == 0 {
            return pages.first();
        }
        let (last, earlier) = pages.split_last()?;
        let (page, earlier) = if last.first_packet <= number {
            (last, earlier)
        } else {
            let slot = if self.packet_scale != 0
                && let Some(end) = packet_hint(pages, number, self.packet_scale)
            {
                end - 1
            } else {
                packet_floor(earlier, number)
            };
            let (before, from) = earlier.split_at(slot);
            (&from[0], before)
        };
        let offset = page.first_packet_offset;
        if number != page.first_packet || offset == page.offset {
            return Some(page);
        }
        let (previous, before) = earlier.split_last()?;
        if previous.offset == offset {
            return Some(previous);
        }
        packet_origin(before, offset)
    }
}

#[inline]
fn packet_floor(pages: &[IndexedPage], number: u64) -> usize {
    // The first page starts packet zero. Only the upper boundary needs searching.
    debug_assert!(!pages.is_empty() && pages[0].first_packet <= number);
    let mut first = 0;
    let mut size = pages.len();
    while size > 1 {
        let half = size / 2;
        let middle = first + half;
        first =
            core::hint::select_unpredictable(pages[middle].first_packet <= number, middle, first);
        size -= half;
    }
    first
}

#[inline(never)]
fn packet_scale(pages: &[IndexedPage], packets: u64) -> u64 {
    if packets <= 1 || packets == pages.len() as u64 {
        return 0;
    }
    let ratio = (((pages.len() - 1) as u128) << 32) / u128::from(packets - 1);
    let Ok(scale) = u64::try_from(ratio) else {
        return 0;
    };
    if scale == 0 {
        return 0;
    }
    let quarter = pages.len() / 4;
    // Keep interpolation out of searches on files with uneven packet density.
    for page in [quarter, 2 * quarter, 3 * quarter] {
        let number = pages[page].first_packet;
        if number >= packets || packet_hint(pages, number, scale).is_none() {
            return 0;
        }
    }
    scale
}

#[inline(never)]
fn packet_hint(pages: &[IndexedPage], number: u64, scale: u64) -> Option<usize> {
    // Exclude the isolated initial packet and estimate the last page for this packet.
    if number == 0 {
        return None;
    }
    // Probe the page immediately below the estimated end of the packet.
    let scaled = u128::from(number) * u128::from(scale) - 1;
    let slot = 1 + usize::try_from(scaled >> 32).ok()?;
    let page = pages.get(slot)?;
    if page.first_packet <= number
        && pages
            .get(slot + 1)
            .is_none_or(|next| next.first_packet > number)
    {
        Some(slot + 1)
    } else {
        None
    }
}

#[inline(never)]
fn packet_origin(pages: &[IndexedPage], offset: u64) -> Option<&IndexedPage> {
    let (older, nearby) = pages.split_at(pages.len().saturating_sub(3));
    if let Some(page) = nearby.iter().rfind(|page| page.offset == offset) {
        return Some(page);
    }
    let origin = older
        .binary_search_by_key(&offset, |page| page.offset)
        .ok()?;
    Some(&older[origin])
}
