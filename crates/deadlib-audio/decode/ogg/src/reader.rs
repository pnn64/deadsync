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
    // The initialized suffix is reusable read scratch; only bytes before filled are input.
    filled: usize,
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
            filled: 0,
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
        if self.filled - self.start >= required {
            return Ok(true);
        }
        if self.eof {
            return Ok(false);
        }
        self.read_more(required, true)?;
        Ok(self.filled >= required)
    }
    fn read_more(&mut self, target: usize, complete: bool) -> Result<(), Error> {
        debug_assert!(target <= MAX_PAGE_SIZE && self.filled - self.start < target);
        if self.start != 0 {
            if self.start != self.filled {
                self.buffer.copy_within(self.start..self.filled, 0);
            }
            self.filled -= self.start;
            self.start = 0;
        }
        let mut filled = self.filled;
        if target > self.buffer.len() {
            let additional = target - self.buffer.len();
            reserve(&mut self.buffer, additional)?;
            self.buffer.resize(target, 0);
        }
        let result = loop {
            match self.inner.read(&mut self.buffer[filled..target]) {
                Ok(0) => self.eof = true,
                Ok(n) => filled += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => (),
                Err(e) => break Err(e),
            }
            if !complete || filled >= target || self.eof {
                break Ok(());
            }
        };
        self.filled = filled.min(target);
        result.map_err(Error::from)
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
                Ok(length) => return Ok(length),
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
            let available = &self.buffer[self.start..self.filled];
            if let Some(skip) = capture_pattern(available) {
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
            self.read_more(4096, false)?;
        }
    }
    fn candidate(&mut self) -> Result<Option<usize>, Error> {
        if !self.fill(27)? {
            if self.filled == self.start {
                return Ok(None);
            }
            return Err(Error::Truncated);
        }
        let header = page::header_size(&self.buffer[self.start..self.filled])?;
        if !self.fill(header)? {
            return Err(Error::Truncated);
        }
        let len = header + page::body_size(&self.buffer[self.start + 27..self.start + header]);
        if !self.fill(len)? {
            return Err(Error::Truncated);
        }
        page::check_crc(&self.buffer[self.start..self.start + len])?;
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

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(unsafe_code)]
fn capture_pattern(bytes: &[u8]) -> Option<usize> {
    // SAFETY: SSE2 is enabled for this build target.
    unsafe { capture_pattern_sse2(bytes) }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[target_feature(enable = "sse2")]
fn capture_pattern_sse2(bytes: &[u8]) -> Option<usize> {
    use core::arch::x86_64::{_mm_cmpeq_epi8, _mm_movemask_epi8, _mm_set_epi64x, _mm_set1_epi8};

    let mut rest = bytes;
    while let Some(word) = rest.first_chunk::<16>() {
        let bits = u128::from_ne_bytes(*word);
        let lanes = _mm_set_epi64x((bits >> 64) as i64, bits as i64);
        let o = _mm_movemask_epi8(_mm_cmpeq_epi8(lanes, _mm_set1_epi8(b'O' as i8)));
        if o != 0 {
            let g = _mm_movemask_epi8(_mm_cmpeq_epi8(lanes, _mm_set1_epi8(b'g' as i8)));
            let s = _mm_movemask_epi8(_mm_cmpeq_epi8(lanes, _mm_set1_epi8(b'S' as i8)));
            let matches = o & (g >> 1) & (g >> 2) & (s >> 3);
            if matches != 0 {
                return Some(bytes.len() - rest.len() + matches.trailing_zeros() as usize);
            }
        }
        // Retain three bytes so the next load sees any pattern crossing this one.
        rest = &rest[13..];
    }
    rest.windows(4)
        .position(|word| word == b"OggS")
        .map(|i| bytes.len() - rest.len() + i)
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
fn capture_pattern(bytes: &[u8]) -> Option<usize> {
    let zero_bytes =
        |word: u64| word.wrapping_sub(0x0101_0101_0101_0101) & !word & 0x8080_8080_8080_8080;
    let mut rest = bytes;
    while let Some(window) = rest.first_chunk::<11>() {
        let o = u64::from_le_bytes(window[..8].try_into().unwrap()) ^ u64::from_le_bytes([b'O'; 8]);
        if zero_bytes(o) != 0 {
            // The extra three bytes let all eight lanes test the full pattern.
            let g1 = u64::from_le_bytes(window[1..9].try_into().unwrap());
            let g2 = u64::from_le_bytes(window[2..10].try_into().unwrap());
            let s = u64::from_le_bytes(window[3..11].try_into().unwrap());
            let g = u64::from_le_bytes([b'g'; 8]);
            let mismatches = o | (g1 ^ g) | (g2 ^ g) | (s ^ u64::from_le_bytes([b'S'; 8]));
            let matches = zero_bytes(mismatches);
            if matches != 0 {
                // Borrow propagation can mark later lanes; the first is exact.
                return Some(bytes.len() - rest.len() + matches.trailing_zeros() as usize / 8);
            }
        }
        rest = &rest[8..];
    }
    rest.windows(4)
        .position(|word| word == b"OggS")
        .map(|i| bytes.len() - rest.len() + i)
}

impl<R: Read + Seek> PageReader<R> {
    /// Reset transport state at an absolute offset. Use recovery mode when
    /// the offset is not a known page boundary. Codec overlap is caller-owned.
    pub fn seek(&mut self, offset: u64) -> Result<(), Error> {
        self.inner.seek(SeekFrom::Start(offset))?;
        self.filled = 0;
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
    length: usize,
    ready: bool,
    failed: bool,
}
impl PageDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.length = 0;
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
        if self.failed | self.ready {
            if self.failed {
                return Err(Error::Failed);
            }
            self.buffer.clear();
            self.length = 0;
            self.ready = false;
        }
        // Every available prefix was validated by the preceding push.
        if input.is_empty() {
            return Ok(PageProgress {
                consumed: 0,
                page: None,
            });
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
        // A partial body chunk needs no header planning or input split.
        if self.length != 0 && input.len() < self.length - self.buffer.len() {
            // Skip intermediate growth on shorter pages. Large pages grow with input.
            if input.len() > self.buffer.capacity() - self.buffer.len()
                && (self.length > 32 * 1024
                    || self
                        .buffer
                        .try_reserve(self.length - self.buffer.len())
                        .is_err())
            {
                reserve(&mut self.buffer, input.len())?;
            }
            self.buffer.extend_from_slice(input);
            return Ok((input.len(), false));
        }
        // With a complete header, copy the available page prefix in one step.
        if self.buffer.is_empty()
            && let Ok(length) = page::page_size(input)
        {
            let take = input.len().min(length);
            let complete = take == length;
            if complete || length <= 32 * 1024 {
                match reserve(&mut self.buffer, take) {
                    Ok(()) => {
                        self.buffer.extend_from_slice(&input[..take]);
                        self.length = length;
                        if complete {
                            page::check_crc(&self.buffer)?;
                        }
                        return Ok((take, complete));
                    }
                    Err(error) if complete => return Err(error),
                    // Smaller incremental allocations may still succeed.
                    Err(_) => {}
                }
            }
        }
        let mut consumed = 0;
        loop {
            let target = if self.length != 0 {
                self.length
            } else if self.buffer.len() < 27 {
                27
            } else {
                let header = page::header_size(&self.buffer)?;
                if self.buffer.len() < header {
                    header
                } else {
                    self.length = header + page::body_size(&self.buffer[27..header]);
                    self.length
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
            if self.length != 0 {
                page::check_crc(&self.buffer)?;
                return Ok((consumed, true));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::capture_pattern;

    #[test]
    fn capture_search_matches_scalar_with_every_byte_and_word_boundary() {
        for fill in 0..=255 {
            for length in 0..80 {
                let bytes = vec![fill; length];
                assert_eq!(
                    capture_pattern(&bytes),
                    bytes.windows(4).position(|b| b == b"OggS")
                );
                for offset in 0..length.saturating_sub(3) {
                    let mut bytes = bytes.clone();
                    bytes[offset..offset + 4].copy_from_slice(b"OggS");
                    assert_eq!(
                        capture_pattern(&bytes),
                        bytes.windows(4).position(|b| b == b"OggS")
                    );
                }
            }
        }
        for pattern in [b"OgOg".as_slice(), b"OggOg", b"OggSOggS", b"NOfgOgg"] {
            let bytes = pattern.repeat(50);
            for start in 0..16 {
                for end in start..bytes.len() {
                    let bytes = &bytes[start..end];
                    assert_eq!(
                        capture_pattern(bytes),
                        bytes.windows(4).position(|b| b == b"OggS")
                    );
                }
            }
        }
    }
}
