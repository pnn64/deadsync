use crate::{
    CompatibilityStats, Demuxer, Error, Limits, Packet, Page, PageHeader, PageReader,
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
    compatibility: CompatibilityStats,
}

#[inline(never)]
fn group_pages(pages: &mut [IndexedPage], streams: &mut [StreamIndex]) {
    if pages.is_sorted_by_key(|page| page.stream) {
        return;
    }
    // Fresh private ranges provide counts, then stable destinations.
    for page in pages.iter() {
        streams[page.stream].pages.end += 1;
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
    for position in 0..pages.len() {
        while pages[position].landmark != position {
            let destination = pages[position].landmark;
            pages.swap(position, destination);
        }
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
        Self::read_with_events(reader, options, |_| Ok::<(), Error>(()))
    }
    /// Build the same validated index while observing its existing packet
    /// assembly and pages in physical order. This allows a codec to build its
    /// timing/header index in one forward source pass without a second demux.
    /// Callback errors stop construction; no partial index is returned.
    pub fn read_with_events<E: From<Error>>(
        reader: impl Read,
        options: IndexOptions,
        mut observe: impl FnMut(IndexEvent<'_>) -> Result<(), E>,
    ) -> Result<Self, E> {
        let mut reader = PageReader::new(reader);
        let mut demux = Demuxer::with_validation(options.container_limits, options.validation);
        let mut index = Self {
            streams: Vec::new(),
            pages: Vec::new(),
            length: 0,
            compatibility: CompatibilityStats::default(),
        };
        while let Some(located) = reader.next_page()? {
            let page = located.page;
            let header = page.header();
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
            if header.is_bos() {
                reserve(&mut index.streams, 1)?;
                index.streams.push(StreamIndex {
                    serial: header.serial,
                    chain: demux.chain(),
                    bos_offset: located.offset,
                    eos_offset: 0,
                    end_offset: 0,
                    packets: 0,
                    pages: 0..0,
                });
            }
            let stream = &mut index.streams[state.stream];
            reserve(&mut index.pages, 1)?;
            index.pages.push(IndexedPage {
                offset: located.offset,
                length: page.bytes().len() as u32,
                header,
                stream: state.stream,
                first_packet: stream.packets,
                completed_packets: state.completed_packets,
                first_packet_offset: state.first_packet_offset,
                granule_floor: 0,
                landmark: 0,
            });
            stream.packets = state.packets;
            index.length = located
                .offset
                .checked_add(page.bytes().len() as u64)
                .ok_or(Error::LimitExceeded("indexed file length"))?;
            if header.is_eos() {
                stream.eos_offset = located.offset;
                stream.end_offset = index.length;
            }
            observe(IndexEvent::Page {
                page,
                offset: located.offset,
            })?;
        }
        demux.finish()?;
        index.compatibility = demux.compatibility();
        group_pages(&mut index.pages, &mut index.streams);
        let mut position = 0;
        for (stream_id, stream) in index.streams.iter_mut().enumerate() {
            let start = position;
            let mut floor = 0;
            let mut landmark = start;
            while position < index.pages.len() && index.pages[position].stream == stream_id {
                let page = &mut index.pages[position];
                if page.header.granule_position != UNKNOWN_GRANULE_POSITION
                    && (page.completed_packets != 0 || page.length == 27)
                    && page.header.granule_position >= floor
                {
                    floor = page.header.granule_position;
                    landmark = position;
                }
                page.granule_floor = floor;
                page.landmark = landmark;
                position += 1;
            }
            stream.pages = start..position;
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
    pub fn granule_at_or_before(&self, serial: u32, target: u64) -> Option<&IndexedPage> {
        let pages = self.pages(serial)?;
        let end = pages.partition_point(|page| page.granule_floor <= target);
        Some(&self.pages[pages.get(end.checked_sub(1)?)?.landmark])
    }
    pub fn page_at_or_before(&self, serial: u32, offset: u64) -> Option<&IndexedPage> {
        let pages = self.pages(serial)?;
        pages.get(
            pages
                .partition_point(|page| page.offset <= offset)
                .checked_sub(1)?,
        )
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
        let page = pages.get(
            pages
                .partition_point(|p| p.first_packet <= number)
                .checked_sub(1)?,
        )?;
        let offset = if number == page.first_packet {
            page.first_packet_offset
        } else {
            page.offset
        };
        if offset == page.offset {
            return Some(page);
        }
        let result = pages.get(
            pages
                .partition_point(|page| page.offset <= offset)
                .checked_sub(1)?,
        )?;
        (result.offset == offset).then_some(result)
    }
}
