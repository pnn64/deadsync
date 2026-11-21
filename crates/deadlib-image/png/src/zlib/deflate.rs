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
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.finishing && !input.is_empty() {
            let error = Error::Invalid("input after zlib finish");
            self.failure = Some(error.clone());
            return Err(error);
        }
        let result = self.run(input, output, finish);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }
    fn run(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        finish: bool,
    ) -> Result<DeflateProgress, Error> {
        let (mut consumed, mut written) = (0, 0);
        loop {
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
            if self.finishing {
                self.done = true;
                return Ok(DeflateProgress {
                    consumed,
                    written,
                    status: StreamStatus::Done,
                });
            }
            let n = (self.history + BLOCK - self.window.len()).min(input.len() - consumed);
            self.total_in = self.total_in.checked_add(n).ok_or(Error::LimitExceeded)?;
            self.window
                .extend_from_slice(&input[consumed..consumed + n]);
            if self.mode != Compression::Rle {
                self.checksum.update(&input[consumed..consumed + n]);
            }
            consumed += n;
            let final_block = finish && consumed == input.len();
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
