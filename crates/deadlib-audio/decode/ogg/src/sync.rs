//! Buffered transport synchronization, separate from strict page grammar.
use crate::{Error, MAX_PAGE_SIZE, Page, PageHeader, crc, reserve};
use core::ops::Range;
use std::io::{Read, Seek, SeekFrom};

/// A complete, CRC-verified transport page. Synchronization preserves unknown
/// versions and flag bits, as libogg does; call `validated` before supplying it
/// to the strict container APIs.
#[derive(Clone, Copy)]
pub struct SyncedPage<'a> {
    bytes: &'a [u8],
}
impl<'a> SyncedPage<'a> {
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn version(self) -> u8 {
        self.bytes[4]
    }
    pub fn header(self) -> PageHeader {
        Page::verified(self.bytes).header()
    }
    pub fn lacing(self) -> &'a [u8] {
        &self.bytes[27..27 + self.bytes[26] as usize]
    }
    pub fn body(self) -> &'a [u8] {
        &self.bytes[27 + self.bytes[26] as usize..]
    }
    /// Apply the published version/flag rules. Length and CRC have already
    /// been verified by synchronization, so conversion requires no new CRC pass.
    pub fn validated(self) -> Result<Page<'a>, Error> {
        crate::page::header_size(self.bytes)?;
        Ok(Page::verified(self.bytes))
    }
}

pub struct LocatedSyncedPage<'a> {
    pub offset: u64,
    pub page: SyncedPage<'a>,
}
pub enum PageSeek<'a> {
    NeedMore,
    /// Exactly this many bytes were discarded before the next possible `O`.
    Skipped(usize),
    Page(LocatedSyncedPage<'a>),
}
pub enum PageOut<'a> {
    NeedMore,
    /// One notification per loss of synchronization, until a page is returned.
    Hole,
    Page(LocatedSyncedPage<'a>),
}

/// Native equivalent of libogg's buffered page synchronization operations.
/// No version, flag, sequence, continuation or codec policy is applied here.
/// `PageReader` remains the strict pull API; this buffer also exposes exact skip
/// counts, partial captures, cached-only polling and pageout hole suppression.
pub struct SyncBuffer {
    data: Vec<u8>,
    start: usize,
    position: u64,
    current: Option<(u64, Range<usize>)>,
    unsynced: bool,
    limit: usize,
}
impl Default for SyncBuffer {
    fn default() -> Self {
        Self::new()
    }
}
enum Step {
    NeedMore,
    Skipped(usize),
    Page,
}
impl SyncBuffer {
    /// The 1 MiB default is a buffering policy, not a page or packet limit.
    pub fn new() -> Self {
        Self::with_limit(1 << 20)
    }
    pub fn with_limit(max_buffered_bytes: usize) -> Self {
        Self {
            data: Vec::new(),
            start: 0,
            position: 0,
            current: None,
            unsynced: false,
            limit: max_buffered_bytes,
        }
    }
    pub fn position(&self) -> u64 {
        self.position
    }
    pub fn buffered_bytes(&self) -> usize {
        self.data.len() - self.start
    }
    pub fn storage_bytes(&self) -> usize {
        self.data.capacity()
    }
    pub fn is_unsynced(&self) -> bool {
        self.unsynced
    }
    pub fn reset(&mut self) {
        self.reset_at(0);
    }
    pub fn reset_at(&mut self, position: u64) {
        self.data.clear();
        self.start = 0;
        self.position = position;
        self.current = None;
        self.unsynced = false;
    }
    /// Append bytes, compacting previously consumed storage. A rejected append
    /// leaves bytes, position, hole state and the last page unchanged.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let length = self
            .buffered_bytes()
            .checked_add(bytes.len())
            .filter(|&n| n <= self.limit)
            .ok_or(Error::LimitExceeded("sync buffer bytes"))?;
        // Reserve before compaction so allocation failure is transactional.
        if length > self.data.capacity() {
            let target = self
                .data
                .capacity()
                .saturating_mul(2)
                .max(length)
                .min(self.limit);
            self.data
                .try_reserve_exact(target - self.data.len())
                .map_err(|_| Error::Allocation)?;
        }
        if self.start != 0 {
            self.data.copy_within(self.start.., 0);
            self.data.truncate(self.data.len() - self.start);
            self.start = 0;
        }
        self.data.extend_from_slice(bytes);
        self.current = None;
        Ok(())
    }
    pub fn current_page(&self) -> Option<LocatedSyncedPage<'_>> {
        self.current
            .as_ref()
            .map(|(offset, range)| LocatedSyncedPage {
                offset: *offset,
                page: SyncedPage {
                    bytes: &self.data[range.clone()],
                },
            })
    }
    fn advance(&mut self, bytes: usize) -> Result<(), Error> {
        let next = self
            .position
            .checked_add(bytes as u64)
            .ok_or(Error::LimitExceeded("sync byte position"))?;
        self.start += bytes;
        self.position = next;
        Ok(())
    }
    fn step(&mut self) -> Result<Step, Error> {
        let bytes = &self.data[self.start..];
        self.current = None;
        if bytes.len() < 27 {
            return Ok(Step::NeedMore);
        }
        if bytes.starts_with(b"OggS") {
            let header = 27 + bytes[26] as usize;
            if bytes.len() < header {
                return Ok(Step::NeedMore);
            }
            let length = header + bytes[27..header].iter().map(|&v| v as usize).sum::<usize>();
            if bytes.len() < length {
                return Ok(Step::NeedMore);
            }
            let expected = u32::from_le_bytes(bytes[22..26].try_into().unwrap());
            if crc::page_crc(&bytes[..length]) == expected {
                let offset = self.position;
                let range = self.start..self.start + length;
                self.advance(length)?;
                self.current = Some((offset, range));
                self.unsynced = false;
                return Ok(Step::Page);
            }
        }
        let skip = bytes[1..]
            .iter()
            .position(|&v| v == b'O')
            .map_or(bytes.len(), |n| n + 1);
        self.advance(skip)?;
        Ok(Step::Skipped(skip))
    }
    pub fn page_seek(&mut self) -> Result<PageSeek<'_>, Error> {
        Ok(match self.step()? {
            Step::NeedMore => PageSeek::NeedMore,
            Step::Skipped(n) => PageSeek::Skipped(n),
            Step::Page => PageSeek::Page(self.current_page().unwrap()),
        })
    }
    pub fn page_out(&mut self) -> Result<PageOut<'_>, Error> {
        loop {
            match self.step()? {
                Step::NeedMore => return Ok(PageOut::NeedMore),
                Step::Page => return Ok(PageOut::Page(self.current_page().unwrap())),
                Step::Skipped(_) if !self.unsynced => {
                    self.unsynced = true;
                    return Ok(PageOut::Hole);
                }
                Step::Skipped(_) => {}
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SyncReadOptions {
    /// Size requested from Read each time more bytes are needed. The default
    /// matches libvorbisfile; short reads remain visible to synchronization.
    pub read_size: usize,
    pub max_buffered_bytes: usize,
}
impl Default for SyncReadOptions {
    fn default() -> Self {
        Self {
            read_size: 2048,
            max_buffered_bytes: MAX_PAGE_SIZE + 4096,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum SyncBoundary {
    Unbounded,
    Buffered,
    /// Search for a page beginning before position()+n. A page may extend past
    /// the boundary. Zero is the cached-only operation, matching libvorbisfile.
    Bytes(u64),
}
pub enum SyncRead<'a> {
    Page(LocatedSyncedPage<'a>),
    Boundary,
    Eof,
}

/// Recovering pull synchronization with exact cached/bounded search behavior.
/// Incomplete trailing captures return Eof; strict truncation checks belong to
/// PageReader or a packet/container validator. IO errors preserve buffered data.
pub struct SyncReader<R> {
    inner: R,
    sync: SyncBuffer,
    input: Vec<u8>,
    options: SyncReadOptions,
}
impl<R: Read> SyncReader<R> {
    pub fn new(inner: R) -> Self {
        Self::with_options(inner, SyncReadOptions::default()).unwrap()
    }
    pub fn with_options(inner: R, options: SyncReadOptions) -> Result<Self, Error> {
        if options.read_size == 0 || options.read_size > options.max_buffered_bytes {
            return Err(Error::Invalid("sync read buffer size"));
        }
        Ok(Self {
            inner,
            sync: SyncBuffer::with_limit(options.max_buffered_bytes),
            input: Vec::new(),
            options,
        })
    }
    pub fn position(&self) -> u64 {
        self.sync.position()
    }
    pub fn buffered_bytes(&self) -> usize {
        self.sync.buffered_bytes()
    }
    pub fn storage_bytes(&self) -> usize {
        self.sync.storage_bytes() + self.input.capacity()
    }
    pub fn get_ref(&self) -> &R {
        &self.inner
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
    pub fn current_page(&self) -> Option<LocatedSyncedPage<'_>> {
        self.sync.current_page()
    }
    pub fn next_page(&mut self, boundary: SyncBoundary) -> Result<SyncRead<'_>, Error> {
        let limit = match boundary {
            SyncBoundary::Bytes(n) if n != 0 => Some(
                self.position()
                    .checked_add(n)
                    .ok_or(Error::LimitExceeded("sync search boundary"))?,
            ),
            _ => None,
        };
        loop {
            if limit.is_some_and(|n| self.position() >= n) {
                return Ok(SyncRead::Boundary);
            }
            match self.sync.step()? {
                Step::Page => return Ok(SyncRead::Page(self.sync.current_page().unwrap())),
                Step::Skipped(_) => continue,
                Step::NeedMore => {}
            }
            if matches!(boundary, SyncBoundary::Buffered | SyncBoundary::Bytes(0)) {
                return Ok(SyncRead::Boundary);
            }
            if self.sync.buffered_bytes() > self.options.max_buffered_bytes - self.options.read_size
            {
                return Err(Error::LimitExceeded("sync buffer bytes"));
            }
            if self.input.len() != self.options.read_size {
                reserve(&mut self.input, self.options.read_size)?;
                self.input.resize(self.options.read_size, 0);
            }
            let n = self.inner.read(&mut self.input)?;
            if n == 0 {
                return Ok(SyncRead::Eof);
            }
            self.sync.push(&self.input[..n])?;
        }
    }
}
impl<R: Read + Seek> SyncReader<R> {
    /// Retain cached bytes when already at the requested logical position.
    /// Otherwise reset synchronization after a successful underlying seek.
    /// This matches libvorbisfile's seek helper, including repeated seeks.
    pub fn seek(&mut self, offset: u64) -> Result<(), Error> {
        if self.position() != offset {
            self.inner.seek(SeekFrom::Start(offset))?;
            self.sync.reset_at(offset);
        }
        Ok(())
    }
}
