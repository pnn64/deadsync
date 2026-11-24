use super::{Compression, StreamStatus, Writer, fast_block, matcher};
use crate::{Error, checksum::Adler32, reserve};

const BLOCK: usize = 32768;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeflateProgress {
    pub consumed: usize,
    pub written: usize,
    pub status: StreamStatus,
}

/// Bounded zlib encoder with 32 KiB input blocks. Fast retains LZ77 history;
/// Stored and Rle need no match dictionary.
///
/// `finish=true` marks the supplied input as the end of the stream. Resubmit any
/// unconsumed suffix with `finish=true` and drain until `Done`. Intermediate
/// calls may buffer a partial block. No flush padding is inserted between calls,
/// and the compressor never retains more than one block plus its history.
pub struct Deflater {
    mode: Compression,
    writer: Writer,
    pending: usize,
    /// The previous Fast block (`history` bytes) followed by the current block,
    /// so matches read one contiguous slice across the block boundary.
    window: Vec<u8>,
    history: usize,
    matcher: Option<matcher::Matcher>,
    encoded: usize,
    total_in: usize,
    total_out: usize,
    checksum: Adler32,
    finishing: bool,
    done: bool,
    failure: Option<Error>,
}
impl Deflater {
    pub fn new(mode: Compression) -> Result<Self, Error> {
        let mut bytes = Vec::new();
        let bits = if mode == Compression::Rle { 10 } else { 9 };
        reserve(&mut bytes, BLOCK * bits / 8 + 64)?;
        bytes.extend_from_slice(&[0x78, 0x01]);
        let mut window = Vec::new();
        let mut matcher = None;
        if mode == Compression::Fast {
            reserve(&mut window, BLOCK * 2)?;
            matcher = Some(matcher::Matcher::new()?);
        } else {
            reserve(&mut window, BLOCK)?;
        }
        Ok(Self {
            mode,
            writer: Writer {
                bytes,
                word: 0,
                count: 0,
            },
            pending: 0,
            window,
            history: 0,
            matcher,
            encoded: 0,
            total_in: 0,
            total_out: 0,
            checksum: Adler32::new(),
            finishing: false,
            done: false,
            failure: None,
        })
    }
    /// Begin an independent zlib stream after the previous one has drained.
    pub(crate) fn reset(&mut self) {
        debug_assert!(self.is_done());
        self.writer.bytes.clear();
        self.writer.bytes.extend_from_slice(&[0x78, 0x01]);
        self.writer.word = 0;
        self.writer.count = 0;
        self.pending = 0;
        self.window.clear();
        self.history = 0;
        if let Some(matcher) = &mut self.matcher {
            matcher.reset();
        }
        self.encoded = 0;
        self.total_in = 0;
        self.total_out = 0;
        self.checksum = Adler32::new();
        self.finishing = false;
        self.done = false;
    }
    pub fn total_in(&self) -> usize {
        self.total_in
    }
    pub fn total_out(&self) -> usize {
        self.total_out
    }
    pub fn is_done(&self) -> bool {
        self.done && self.failure.is_none()
    }
    pub fn working_memory_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.writer.bytes.capacity()
            + self.window.capacity()
            + self.matcher.as_ref().map_or(0, matcher::Matcher::memory)
    }
    pub fn process(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        finish: bool,
    ) -> Result<DeflateProgress, Error> {
        self.process_input(Some(input), input.len(), output, finish)
    }
    /// `process` for `len` zero input bytes, whose checksum needs no reads.
    pub(crate) fn process_zeros(
        &mut self,
        len: usize,
        output: &mut [u8],
    ) -> Result<DeflateProgress, Error> {
        self.process_input(None, len, output, false)
    }
    fn process_input(
        &mut self,
        input: Option<&[u8]>,
        len: usize,
        output: &mut [u8],
        finish: bool,
    ) -> Result<DeflateProgress, Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.finishing && len != 0 {
            let error = Error::Invalid("input after zlib finish");
            self.failure = Some(error.clone());
            return Err(error);
        }
        let result = self.run(input, len, output, finish);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn run(
        &mut self,
        input: Option<&[u8]>,
        len: usize,
        output: &mut [u8],
        finish: bool,
    ) -> Result<DeflateProgress, Error> {
        let (mut consumed, mut written) = (0, 0);
        loop {
            if !self.writer.bytes.is_empty() {
                let n = (self.writer.bytes.len() - self.pending).min(output.len() - written);
                output[written..written + n]
                    .copy_from_slice(&self.writer.bytes[self.pending..self.pending + n]);
                self.pending += n;
                written += n;
                self.total_out = self.total_out.checked_add(n).ok_or(Error::LimitExceeded)?;
                if self.pending != self.writer.bytes.len() {
                    return Ok(DeflateProgress {
                        consumed,
                        written,
                        status: StreamStatus::NeedOutput,
                    });
                }
                self.writer.bytes.clear();
                self.pending = 0;
            }
            if self.finishing {
                self.done = true;
                return Ok(DeflateProgress {
                    consumed,
                    written,
                    status: StreamStatus::Done,
                });
            }
            let n = (self.history + BLOCK - self.window.len()).min(len - consumed);
            self.total_in = self.total_in.checked_add(n).ok_or(Error::LimitExceeded)?;
            if let Some(input) = input {
                let bytes = &input[consumed..consumed + n];
                self.window.extend_from_slice(bytes);
                if self.mode != Compression::Rle {
                    self.checksum.update(bytes);
                }
            } else {
                self.window.resize(self.window.len() + n, 0);
                if self.mode != Compression::Rle {
                    self.checksum.update_zeros(n);
                }
            }
            consumed += n;
            let final_block = finish && consumed == len;
            if self.window.len() == self.history + BLOCK || final_block {
                self.encode_block(final_block);
            } else {
                return Ok(DeflateProgress {
                    consumed,
                    written,
                    status: StreamStatus::NeedInput,
                });
            }
        }
    }
    fn encode_block(&mut self, final_block: bool) {
        let start = self.history;
        let end = self.window.len();
        let block = &self.window[start..];
        if self.mode == Compression::Stored {
            self.writer.stored(block, final_block);
        } else if self.mode == Compression::Rle {
            self.writer
                .rle_block(block, final_block, &mut self.checksum);
        } else {
            let matcher = self.matcher.as_mut().unwrap();
            // Hash the previous block's three trailing positions once their
            // lookahead is available. Absolute positions retain full history.
            for back in (1..=3).rev() {
                if self.encoded >= back && end - start >= 4 - back {
                    matcher.insert(&self.window[start - back..], self.encoded - back);
                }
            }
            let base = self.encoded - start;
            fast_block(
                &mut self.writer,
                matcher,
                &self.window,
                base,
                start,
                end,
                final_block,
            );
            // The current block becomes the next block's history.
            self.window.copy_within(start.., 0);
            self.history = end - start;
        }
        self.encoded += end - start;
        self.window.truncate(self.history);
        if final_block {
            self.writer.align();
            self.writer
                .bytes
                .extend_from_slice(&self.checksum.finish().to_be_bytes());
            self.finishing = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive one stream. `None` parts are zero runs fed through
    /// `process_zeros`; the last part must carry bytes to finish the stream.
    fn stream(
        deflater: &mut Deflater,
        parts: &[(Option<&[u8]>, usize)],
        capacity: usize,
    ) -> Vec<u8> {
        let mut buffer = vec![0; capacity];
        let mut encoded = Vec::new();
        for (n, &(mut input, mut len)) in parts.iter().enumerate() {
            let finish = n + 1 == parts.len();
            loop {
                let progress = match input {
                    Some(bytes) => deflater.process(bytes, &mut buffer, finish),
                    None => deflater.process_zeros(len, &mut buffer),
                }
                .unwrap();
                encoded.extend_from_slice(&buffer[..progress.written]);
                if let Some(bytes) = &mut input {
                    *bytes = &bytes[progress.consumed..];
                }
                len -= progress.consumed;
                match progress.status {
                    StreamStatus::NeedOutput => {}
                    StreamStatus::NeedInput if !finish && len == 0 => break,
                    StreamStatus::Done if finish && len == 0 => break,
                    status => panic!("unexpected {status:?}"),
                }
            }
        }
        encoded
    }

    #[test]
    fn zero_runs_encode_like_zero_bytes() {
        let bytes: Vec<u8> = (0..70_000u32).map(|n| (n * 37 % 251) as u8 | 1).collect();
        let zeros = vec![0; 70_000];
        for mode in [Compression::Fast, Compression::Rle, Compression::Stored] {
            for runs in [
                [0, 1, 5],
                [3, 32768, 7],
                [32765, 70_000, 2],
                [1, 40_000, 65_000],
            ] {
                let mut with_runs = Vec::new();
                let mut materialized = Vec::new();
                for (n, &run) in runs.iter().enumerate() {
                    let text = &bytes[n * 9_000..n * 9_000 + 1 + n * 4_000];
                    with_runs.push((Some(text), text.len()));
                    materialized.push((Some(text), text.len()));
                    with_runs.push((None, run));
                    materialized.push((Some(&zeros[..run]), run));
                }
                with_runs.push((Some(&bytes[..3]), 3));
                materialized.push((Some(&bytes[..3]), 3));
                for capacity in [1, 7, 65_536] {
                    assert_eq!(
                        stream(&mut Deflater::new(mode).unwrap(), &with_runs, capacity),
                        stream(&mut Deflater::new(mode).unwrap(), &materialized, capacity),
                        "{mode:?} {runs:?} {capacity}"
                    );
                }
            }
        }
    }

    #[test]
    fn reset_matches_fresh_deflation_across_frames() {
        let data: Vec<_> = (0..BLOCK * 3)
            .map(|n| if n % 257 < 17 { 0 } else { (n * 37) as u8 })
            .collect();
        for mode in [Compression::Fast, Compression::Rle, Compression::Stored] {
            let mut reused = Deflater::new(mode).unwrap();
            for len in [31, BLOCK * 2 + 7, 0, BLOCK + 19, 137] {
                let mut fresh = Deflater::new(mode).unwrap();
                let parts = [(Some(&data[..len]), len)];
                let encoded = stream(&mut reused, &parts, 19);
                assert_eq!(encoded, stream(&mut fresh, &parts, 19));
                assert_eq!(reused.total_in(), len);
                assert_eq!(reused.total_out(), encoded.len());
                assert_eq!(reused.working_memory_bytes(), fresh.working_memory_bytes());
                reused.reset();
            }
        }
    }
}
