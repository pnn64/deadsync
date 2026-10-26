use super::{
    Compression, DIST_BASE, DIST_EXTRA, LENGTH_BASE, LENGTH_EXTRA, StreamStatus, Writer, hash3,
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
    heads: Vec<usize>,
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
        let mut heads = Vec::new();
        if mode == Compression::Fast {
            reserve(&mut history, BLOCK)?;
            history.resize(BLOCK, 0);
            reserve(&mut heads, 65536)?;
            heads.resize(65536, usize::MAX);
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
            heads,
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
            + self.heads.capacity() * core::mem::size_of::<usize>()
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
            self.writer.write(u32::from(final_block), 3);
            self.writer.bytes.push(self.writer.word as u8);
            self.writer.word = 0;
            self.writer.count = 0;
            let len = self.block.len() as u16;
            self.writer.bytes.extend_from_slice(&len.to_le_bytes());
            self.writer.bytes.extend_from_slice(&(!len).to_le_bytes());
            self.writer.bytes.extend_from_slice(&self.block);
        } else {
            self.writer.write(u32::from(final_block) | 2, 3);
            // Hash the previous block's two trailing positions once their
            // lookahead is available. Absolute positions retain full history.
            for back in (1..=2).rev() {
                if self.encoded >= back && self.block.len() >= 3 - back {
                    let index = self.encoded - back;
                    let bytes = [
                        self.value(index),
                        self.value(index + 1),
                        self.value(index + 2),
                    ];
                    self.heads[hash3(&bytes)] = index;
                }
            }
            let mut pos = 0;
            while pos < self.block.len() {
                let global = self.encoded + pos;
                let mut length = 0;
                let mut distance = 0;
                if pos + 2 < self.block.len() {
                    let hash = hash3(&self.block[pos..]);
                    let previous = self.heads[hash];
                    self.heads[hash] = global;
                    if previous != usize::MAX && previous < global && global - previous <= BLOCK {
                        let max = (self.block.len() - pos).min(258);
                        while length < max
                            && self.value(previous + length) == self.block[pos + length]
                        {
                            length += 1;
                        }
                        distance = global - previous;
                    }
                }
                let count = if length >= 3 {
                    let index = LENGTH_BASE.iter().rposition(|&n| n <= length).unwrap();
                    self.writer.literal(257 + index);
                    self.writer
                        .write((length - LENGTH_BASE[index]) as u32, LENGTH_EXTRA[index]);
                    let index = DIST_BASE.iter().rposition(|&n| n <= distance).unwrap();
                    self.writer.write((index as u32).reverse_bits() >> 27, 5);
                    self.writer
                        .write((distance - DIST_BASE[index]) as u32, DIST_EXTRA[index]);
                    length
                } else {
                    self.writer.literal(self.block[pos] as usize);
                    1
                };
                for next in pos..pos + count {
                    if next != pos && next + 2 < self.block.len() {
                        self.heads[hash3(&self.block[next..])] = self.encoded + next;
                    }
                    self.history[(self.encoded + next) & (BLOCK - 1)] = self.block[next];
                }
                pos += count;
            }
            self.writer.literal(256);
        }
        self.encoded += self.block.len();
        self.block.clear();
        if final_block {
            if self.writer.count != 0 {
                self.writer.bytes.push(self.writer.word as u8);
                self.writer.word = 0;
                self.writer.count = 0;
            }
            self.writer
                .bytes
                .extend_from_slice(&self.checksum.finish().to_be_bytes());
            self.finishing = true;
        }
    }
}
