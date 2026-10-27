use crate::{Error, MAX_PAGE_SIZE, Page, page, reserve};
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SyncMode {
    /// Every byte must belong to the next CRC-verified page.
    #[default]
    Strict,
    /// Scan through noise/corrupt pages to the next CRC-verified capture.
    /// This is transport recovery, not a legacy codec compatibility policy.
    Recover,
}

pub struct LocatedPage<'a> {
    pub offset: u64,
    pub page: Page<'a>,
}

/// Pull reader with at most one maximum-sized page of buffered input.
/// Wrap unbuffered files/sockets in `BufReader` to reduce small IO calls.
pub struct PageReader<R> {
    inner: R,
    buffer: Vec<u8>,
    start: usize,
    offset: u64,
    returned: usize,
    mode: SyncMode,
    discarded: u64,
    eof: bool,
    failed: bool,
}
impl<R: Read> PageReader<R> {
    pub fn new(inner: R) -> Self {
        Self::with_mode(inner, SyncMode::Strict)
    }
    pub fn with_mode(inner: R, mode: SyncMode) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            start: 0,
            offset: 0,
            returned: 0,
            mode,
            discarded: 0,
            eof: false,
            failed: false,
        }
    }
    /// Absolute byte position immediately after the last returned page, or
    /// the current capture candidate if no page has been returned yet.
    pub fn position(&self) -> u64 {
        self.offset + self.returned as u64
    }
    pub fn discarded_bytes(&self) -> u64 {
        self.discarded
    }
    /// Borrow the last returned, CRC-verified page again without IO or copying.
    /// None before a successful next_page, after EOF, or after a successful seek.
    /// This permits a packet consumer to pause within a buffered page.
    pub fn current_page(&self) -> Option<LocatedPage<'_>> {
        (self.returned != 0).then(|| LocatedPage {
            offset: self.offset,
            page: Page::verified(&self.buffer[self.start..self.start + self.returned]),
        })
    }
    /// The underlying reader can be ahead of `position()` after recovery.
    pub fn into_inner(self) -> R {
        self.inner
    }

    fn discard(&mut self, count: usize) {
        self.start += count;
        self.offset += count as u64;
    }
    fn fill(&mut self, required: usize) -> Result<bool, Error> {
        debug_assert!(required <= MAX_PAGE_SIZE);
        while self.buffer.len() - self.start < required && !self.eof {
            self.read_more(required)?;
        }
        Ok(self.buffer.len() - self.start >= required)
    }
    fn read_more(&mut self, target: usize) -> Result<(), Error> {
        debug_assert!(target <= MAX_PAGE_SIZE && self.buffer.len() - self.start < target);
        if self.start != 0 {
            self.buffer.copy_within(self.start.., 0);
            self.buffer.truncate(self.buffer.len() - self.start);
            self.start = 0;
        }
        let old = self.buffer.len();
        reserve(&mut self.buffer, target - old)?;
        self.buffer.resize(target, 0);
        let result = self.inner.read(&mut self.buffer[old..]);
        match result {
            Ok(0) => {
                self.buffer.truncate(old);
                self.eof = true;
            }
            Ok(n) => self.buffer.truncate(old + n),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => self.buffer.truncate(old),
            Err(e) => {
                self.buffer.truncate(old);
                return Err(e.into());
            }
        }
        Ok(())
    }

    // Return only lengths here, so the recovery loop can mutate the buffer
    // without retaining a borrow for the returned page's lifetime.
    fn next_length(&mut self) -> Result<Option<usize>, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        self.discard(self.returned);
        self.returned = 0;
        loop {
            let result = self.candidate();
            match result {
                Ok(None) => return Ok(None),
                Ok(Some(len)) => return Ok(Some(len)),
                Err(error) => {
                    if self.mode == SyncMode::Strict
                        || matches!(error, Error::Io(_) | Error::Allocation)
                    {
                        self.failed = true;
                        return Err(error);
                    }
                    // Retain all but the first byte: a false capture's body
                    // may contain the actual next page (also at EOF).
                    self.discard(1);
                    self.discarded += 1;
                    if let Err(error) = self.scan_capture() {
                        self.failed = true;
                        return Err(error);
                    }
                }
            }
        }
    }
    // Scan noise in batches instead of moving a 27-byte header window and
    // issuing one underlying read per discarded byte. Keep three trailing
    // bytes so a capture split across reads remains visible. This also scans
    // the body of a failed CRC candidate before reading past that candidate.
    fn scan_capture(&mut self) -> Result<(), Error> {
        loop {
            let available = &self.buffer[self.start..];
            if let Some(skip) = available.windows(4).position(|bytes| bytes == b"OggS") {
                self.discard(skip);
                self.discarded += skip as u64;
                return Ok(());
            }
            let skip = if self.eof {
                available.len()
            } else {
                available.len().saturating_sub(3)
            };
            self.discard(skip);
            self.discarded += skip as u64;
            if self.eof {
                return Ok(());
            }
            self.read_more(4096)?;
        }
    }
    fn candidate(&mut self) -> Result<Option<usize>, Error> {
        if !self.fill(27)? {
            if self.buffer.len() == self.start {
                return Ok(None);
            }
            return Err(Error::Truncated);
        }
        let header = page::header_size(&self.buffer[self.start..])?;
        if !self.fill(header)? {
            return Err(Error::Truncated);
        }
        let len = page::page_size(&self.buffer[self.start..])?;
        if !self.fill(len)? {
            return Err(Error::Truncated);
        }
        Page::parse(&self.buffer[self.start..self.start + len])?;
        Ok(Some(len))
    }
    pub fn next_page(&mut self) -> Result<Option<LocatedPage<'_>>, Error> {
        let Some(len) = self.next_length()? else {
            return Ok(None);
        };
        self.returned = len;
        // Already verified in candidate(); construction is crate-private so
        // public callers cannot obtain an unverified page.
        let page = Page::verified(&self.buffer[self.start..self.start + len]);
        Ok(Some(LocatedPage {
            offset: self.offset,
            page,
        }))
    }
}
impl<R: Read + Seek> PageReader<R> {
    /// Reset transport state at an absolute offset. Use recovery mode when
    /// the offset is not a known page boundary. Codec overlap is caller-owned.
    pub fn seek(&mut self, offset: u64) -> Result<(), Error> {
        self.inner.seek(SeekFrom::Start(offset))?;
        self.buffer.clear();
        self.start = 0;
        self.offset = offset;
        self.returned = 0;
        self.discarded = 0;
        self.eof = false;
        self.failed = false;
        Ok(())
    }
}

/// One page at most is consumed per push; the page borrows decoder storage.
pub struct PageProgress<'a> {
    pub consumed: usize,
    pub page: Option<Page<'a>>,
}

/// Incremental strict parser. Keeps at most one page; never buffers the rest
/// of a caller's chunk. Errors are terminal until `reset`.
#[derive(Default)]
pub struct PageDecoder {
    buffer: Vec<u8>,
    ready: bool,
    failed: bool,
}
impl PageDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.ready = false;
        self.failed = false;
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if !self.buffer.is_empty() && !self.ready {
            self.failed = true;
            return Err(Error::Truncated);
        }
        Ok(())
    }
    pub fn push(&mut self, input: &[u8]) -> Result<PageProgress<'_>, Error> {
        if self.failed {
            return Err(Error::Failed);
        }
        if self.ready {
            self.buffer.clear();
            self.ready = false;
        }
        let result = self.fill_page(input);
        match result {
            Ok((consumed, true)) => {
                self.ready = true;
                Ok(PageProgress {
                    consumed,
                    page: Some(Page::verified(&self.buffer)),
                })
            }
            Ok((consumed, false)) => Ok(PageProgress {
                consumed,
                page: None,
            }),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
    fn fill_page(&mut self, input: &[u8]) -> Result<(usize, bool), Error> {
        let mut consumed = 0;
        loop {
            let target = if self.buffer.len() < 27 {
                27
            } else {
                let header = page::header_size(&self.buffer)?;
                if self.buffer.len() < header {
                    header
                } else {
                    page::page_size(&self.buffer)?
                }
            };
            let take = (target - self.buffer.len()).min(input.len() - consumed);
            reserve(&mut self.buffer, take)?;
            self.buffer
                .extend_from_slice(&input[consumed..consumed + take]);
            consumed += take;
            if self.buffer.len() < target {
                return Ok((consumed, false));
            }
            if self.buffer.len() >= 27 {
                let header = page::header_size(&self.buffer)?;
                if self.buffer.len() >= header
                    && self.buffer.len() == page::page_size(&self.buffer)?
                {
                    Page::parse(&self.buffer)?;
                    return Ok((consumed, true));
                }
            }
            if consumed == input.len() {
                return Ok((consumed, false));
            }
        }
    }
}
