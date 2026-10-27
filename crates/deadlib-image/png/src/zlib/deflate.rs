use super::{
    Compression, DIST_BASE, DIST_EXTRA, LENGTH_BASE, LENGTH_EXTRA, StreamStatus, Writer,
    distance_code, length_code, matcher,
};
use crate::{Error, checksum::Adler32, reserve};

const BLOCK: usize = 32768;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeflateProgress {
    pub consumed: usize,
    pub written: usize,
    pub status: StreamStatus,
}

/// Bounded zlib encoder with 32 KiB input blocks and cross-block LZ77 history.
///
/// `finish=true` marks the supplied input as the end of the stream. Resubmit any
/// unconsumed suffix with `finish=true` and drain until `Done`. Intermediate
/// calls may buffer a partial block. No flush padding is inserted between calls,
/// and the compressor never retains more than one block plus its history.
pub struct Deflater {
    mode: Compression,
    writer: Writer,
    pending: usize,
    block: Vec<u8>,
    history: Vec<u8>,
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
        reserve(&mut bytes, BLOCK * 9 / 8 + 64)?;
        bytes.extend_from_slice(&[0x78, 0x01]);
        let mut block = Vec::new();
        reserve(&mut block, BLOCK)?;
        let mut history = Vec::new();
        let mut matcher = None;
        if mode == Compression::Fast {
            reserve(&mut history, BLOCK)?;
            history.resize(BLOCK, 0);
            matcher = Some(matcher::Matcher::new()?);
        }
        Ok(Self {
            mode,
            writer: Writer {
                bytes,
                word: 0,
                count: 0,
            },
            pending: 0,
            block,
            history,
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
            + self.block.capacity()
            + self.history.capacity()
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
            let n = (BLOCK - self.block.len()).min(input.len() - consumed);
            self.total_in = self.total_in.checked_add(n).ok_or(Error::LimitExceeded)?;
            self.block.extend_from_slice(&input[consumed..consumed + n]);
            self.checksum.update(&input[consumed..consumed + n]);
            consumed += n;
            let final_block = finish && consumed == input.len();
            if self.block.len() == BLOCK || final_block {
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
    fn value(&self, index: usize) -> u8 {
        if index >= self.encoded {
            self.block[index - self.encoded]
        } else {
            self.history[index & (BLOCK - 1)]
        }
    }
    fn encode_block(&mut self, final_block: bool) {
        if self.mode == Compression::Stored {
            self.writer.stored(&self.block, final_block);
        } else {
            let checkpoint = self.writer.start_fixed(final_block);
            // Hash the previous block's three trailing positions once their
            // lookahead is available. Absolute positions retain full history.
            for back in (1..=3).rev() {
                if self.encoded >= back && self.block.len() >= 4 - back {
                    let index = self.encoded - back;
                    let bytes = [
                        self.value(index),
                        self.value(index + 1),
                        self.value(index + 2),
                        self.value(index + 3),
                    ];
                    self.matcher.as_mut().unwrap().insert(&bytes, index);
                }
            }
            let mut pos = 0;
            let mut misses = 0usize;
            while pos < self.block.len() {
                let global = self.encoded + pos;
                let mut length = 0;
                let mut distance = 0;
                if pos + 3 < self.block.len() {
                    let mut candidate = self
                        .matcher
                        .as_mut()
                        .unwrap()
                        .insert(&self.block[pos..], global);
                    let bits = (self.writer.bytes.len() - checkpoint.bytes) * 8
                        + self.writer.count as usize
                        - checkpoint.count as usize;
                    let searches = matcher::searches(bits, pos);
                    for attempt in 0..searches {
                        let Some(previous) = candidate else {
                            break;
                        };
                        let max = (self.block.len() - pos).min(258);
                        if length < max
                            && self.value(previous + length) == self.block[pos + length]
                            && self.value(previous) == self.block[pos]
                        {
                            let matched = self.match_length(previous, pos, max);
                            if matched > length {
                                length = matched;
                                distance = global - previous;
                            }
                            if length == max {
                                break;
                            }
                        }
                        if attempt + 1 == searches {
                            break;
                        }
                        candidate = self.matcher.as_ref().unwrap().next(previous, global);
                    }
                }
                let count = if length >= 3 {
                    misses = 0;
                    let index = length_code(length);
                    self.writer.literal(257 + index);
                    self.writer
                        .write((length - LENGTH_BASE[index]) as u32, LENGTH_EXTRA[index]);
                    let index = distance_code(distance);
                    self.writer.write((index as u32).reverse_bits() >> 27, 5);
                    self.writer
                        .write((distance - DIST_BASE[index]) as u32, DIST_EXTRA[index]);
                    length
                } else {
                    let count = (1 + (misses >> 8)).min(16).min(self.block.len() - pos);
                    for &byte in &self.block[pos..pos + count] {
                        self.writer.literal(byte as usize);
                    }
                    misses += count;
                    count
                };
                if length >= 3 {
                    for next in matcher::positions(pos, count) {
                        if next + 3 < self.block.len() {
                            self.matcher
                                .as_mut()
                                .unwrap()
                                .insert(&self.block[next..], self.encoded + next);
                        }
                    }
                }
                pos += count;
            }
            // Current-block references read from `block`; keep the previous
            // block intact until all matches have been selected.
            let at = self.encoded & (BLOCK - 1);
            let first = self.block.len().min(BLOCK - at);
            self.history[at..at + first].copy_from_slice(&self.block[..first]);
            self.history[..self.block.len() - first].copy_from_slice(&self.block[first..]);
            self.writer
                .finish_fixed(checkpoint, &self.block, final_block);
        }
        self.encoded += self.block.len();
        self.block.clear();
        if final_block {
            self.writer.align();
            self.writer
                .bytes
                .extend_from_slice(&self.checksum.finish().to_be_bytes());
            self.finishing = true;
        }
    }
    #[inline(always)]
    fn match_length(&self, previous: usize, pos: usize, max: usize) -> usize {
        let mut length = 0;
        while length < max {
            let source = previous + length;
            let bytes = if source >= self.encoded {
                &self.block[source - self.encoded..]
            } else {
                let ring = source & (BLOCK - 1);
                let count = (self.encoded - source).min(BLOCK - ring);
                &self.history[ring..ring + count]
            };
            let count = bytes.len().min(max - length);
            let equal = matcher::prefix(
                &bytes[..count],
                &self.block[pos + length..pos + length + count],
            );
            length += equal;
            if equal != count {
                break;
            }
        }
        length
    }
}
