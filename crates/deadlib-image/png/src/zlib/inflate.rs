use super::{Bits, DIST_BASE, DIST_EXTRA, Huffman, LENGTH_BASE, LENGTH_EXTRA, fixed};
use crate::{Error, checksum::Adler32, reserve};

/// Why a streaming codec stopped. Only `Done` proves checksum-verified completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamStatus {
    NeedInput,
    NeedOutput,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InflateProgress {
    /// Bytes accepted from this call's input. Never includes bytes after zlib.
    pub consumed: usize,
    /// Bytes initialized in this call's output buffer.
    pub written: usize,
    pub status: StreamStatus,
}

#[derive(Clone, Copy)]
enum State {
    HeaderCmf,
    HeaderFlags(u8),
    BlockFinal,
    BlockType,
    StoredLength,
    StoredComplement(usize),
    Stored(usize),
    DynamicCounts,
    DynamicCodeLengths,
    DynamicLengths,
    DynamicRepeat { value: u8, base: usize, extra: u8 },
    Codes,
    Literal(u8),
    LengthExtra(usize),
    DistanceCode(usize),
    DistanceExtra { length: usize, symbol: usize },
    Copy { distance: usize, left: usize },
    Adler,
    Done,
}

/// Resumable RFC 1950/1951 decoder with at most 32 KiB of history allocation.
///
/// Retains partial fields, code-length repeats, Huffman symbols and matches;
/// never replays previously consumed input. Output is provisional until `Done`.
/// Pass only unconsumed input on the next call, with fresh output space. An
/// error is terminal and is returned again by subsequent calls.
pub struct Inflater {
    state: State,
    word: u64,
    bit_count: u8,
    final_block: bool,
    window: usize,
    history: Vec<u8>,
    history_pos: usize,
    literals: Huffman,
    distances: Huffman,
    code_lengths: [u8; 19],
    lengths: [u8; 318],
    nl: usize,
    nd: usize,
    nc: usize,
    index: usize,
    checksum: Adler32,
    total_in: usize,
    total_out: usize,
    limit: usize,
    failure: Option<Error>,
}

impl Inflater {
    pub fn new(output_limit: usize) -> Result<Self, Error> {
        let mut history = Vec::new();
        // A distance cannot exceed produced bytes or the output limit. Round
        // the bounded history to a power of two to retain masked ring indices.
        let size = output_limit.clamp(1, 32768).next_power_of_two();
        reserve(&mut history, size)?;
        history.resize(size, 0);
        Ok(Self {
            state: State::HeaderCmf,
            word: 0,
            bit_count: 0,
            final_block: false,
            window: 32768,
            history,
            history_pos: 0,
            literals: Huffman::new(&[], false, true)?,
            distances: Huffman::new(&[], false, true)?,
            code_lengths: [0; 19],
            lengths: [0; 318],
            nl: 0,
            nd: 0,
            nc: 0,
            index: 0,
            checksum: Adler32::new(),
            total_in: 0,
            total_out: 0,
            limit: output_limit,
            failure: None,
        })
    }
    pub fn total_in(&self) -> usize {
        self.total_in
    }
    pub fn total_out(&self) -> usize {
        self.total_out
    }
    /// Decoder-owned memory, excluding the caller's input and output buffers.
    pub fn working_memory_bytes(&self) -> usize {
        core::mem::size_of::<Self>() + self.history.capacity()
    }
    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done) && self.failure.is_none()
    }
    /// Confirm end-of-input after all pending output has been drained.
    pub fn finish(&self) -> Result<(), Error> {
        if let Some(error) = &self.failure {
            Err(error.clone())
        } else if self.is_done() {
            Ok(())
        } else {
            Err(Error::Truncated)
        }
    }
    pub fn process(&mut self, input: &[u8], output: &mut [u8]) -> Result<InflateProgress, Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let mut bits = Bits {
            input,
            pos: 0,
            word: self.word,
            count: self.bit_count,
        };
        let result = self.run(&mut bits, output);
        self.word = bits.word;
        self.bit_count = bits.count;
        self.total_in = match self.total_in.checked_add(bits.pos) {
            Some(n) => n,
            None => {
                self.failure = Some(Error::LimitExceeded);
                return Err(Error::LimitExceeded);
            }
        };
        match result {
            Ok((written, status)) => Ok(InflateProgress {
                consumed: bits.pos,
                written,
                status,
            }),
            Err(error) => {
                self.failure = Some(error.clone());
                Err(error)
            }
        }
    }
    fn remember(&mut self, mut bytes: &[u8]) {
        self.total_out += bytes.len();
        let size = self.history.len();
        while !bytes.is_empty() {
            let n = bytes.len().min(size - self.history_pos);
            self.history[self.history_pos..self.history_pos + n].copy_from_slice(&bytes[..n]);
            self.history_pos = (self.history_pos + n) & (size - 1);
            bytes = &bytes[n..];
        }
    }
    #[inline]
    fn begin_copy(&mut self, length: usize, distance: usize) -> Result<(), Error> {
        if distance > self.total_out || distance > self.window {
            return Err(Error::Invalid("distance outside history window"));
        }
        if length > self.limit - self.total_out {
            return Err(Error::LimitExceeded);
        }
        self.state = State::Copy {
            distance,
            left: length,
        };
        Ok(())
    }
    #[inline]
    fn copy_match(&mut self, distance: usize, output: &mut [u8]) {
        let n = output.len();
        let seed = distance.min(n);
        let size = self.history.len();
        let start = (self.history_pos + size - distance) & (size - 1);
        let first = seed.min(size - start);
        output[..first].copy_from_slice(&self.history[start..start + first]);
        output[first..seed].copy_from_slice(&self.history[..seed - first]);
        let mut filled = seed;
        while filled < n {
            let count = filled.min(n - filled);
            output.copy_within(..count, filled);
            filled += count;
        }
        self.remember(output);
    }
    fn end_block(&mut self) {
        self.state = if self.final_block {
            State::Adler
        } else {
            State::BlockFinal
        };
    }
    fn run(
        &mut self,
        bits: &mut Bits<'_>,
        output: &mut [u8],
    ) -> Result<(usize, StreamStatus), Error> {
        let mut written = 0usize;
        let mut checksummed = 0usize;
        macro_rules! stop {
            ($status:expr) => {{
                self.checksum.update(&output[checksummed..written]);
                return Ok((written, $status));
            }};
        }
        macro_rules! read {
            ($n:expr) => {
                match bits.read($n) {
                    Ok(value) => value,
                    Err(Error::Truncated) => stop!(StreamStatus::NeedInput),
                    Err(error) => return Err(error),
                }
            };
        }
        macro_rules! symbol {
            ($tree:expr) => {
                match $tree.decode(bits) {
                    Ok(value) => value,
                    Err(Error::Truncated) => stop!(StreamStatus::NeedInput),
                    Err(error) => return Err(error),
                }
            };
        }
        loop {
            match self.state {
                State::HeaderCmf => self.state = State::HeaderFlags(read!(8) as u8),
                State::HeaderFlags(cmf) => {
                    let flags = read!(8) as u8;
                    if cmf & 15 != 8
                        || cmf >> 4 > 7
                        || (u16::from(cmf) * 256 + u16::from(flags)) % 31 != 0
                    {
                        return Err(Error::Invalid("zlib header"));
                    }
                    if flags & 32 != 0 {
                        return Err(Error::Invalid("zlib dictionary not permitted"));
                    }
                    self.window = 1usize << ((cmf >> 4) + 8);
                    self.state = State::BlockFinal;
                }
                State::BlockFinal => {
                    self.final_block = read!(1) != 0;
                    self.state = State::BlockType;
                }
                State::BlockType => match read!(2) {
                    0 => {
                        bits.align();
                        self.state = State::StoredLength;
                    }
                    1 => {
                        (self.literals, self.distances) = fixed();
                        self.state = State::Codes;
                    }
                    2 => self.state = State::DynamicCounts,
                    _ => return Err(Error::Invalid("reserved block type")),
                },
                State::StoredLength => self.state = State::StoredComplement(read!(16) as usize),
                State::StoredComplement(length) => {
                    if length ^ read!(16) as usize != 65535 {
                        return Err(Error::Invalid("stored block length"));
                    }
                    if length > self.limit - self.total_out {
                        return Err(Error::LimitExceeded);
                    }
                    self.state = State::Stored(length);
                }
                State::Stored(left) => {
                    if left == 0 {
                        self.end_block();
                        continue;
                    }
                    if written == output.len() {
                        stop!(StreamStatus::NeedOutput);
                    }
                    let n = if bits.count >= 8 {
                        output[written] = read!(8) as u8;
                        1
                    } else {
                        let n = left
                            .min(output.len() - written)
                            .min(bits.input.len() - bits.pos);
                        if n == 0 {
                            stop!(StreamStatus::NeedInput);
                        }
                        output[written..written + n]
                            .copy_from_slice(&bits.input[bits.pos..bits.pos + n]);
                        bits.pos += n;
                        n
                    };
                    if self.final_block {
                        self.total_out += n;
                    } else {
                        self.remember(&output[written..written + n]);
                    }
                    written += n;
                    self.state = State::Stored(left - n);
                }
                State::DynamicCounts => {
                    let fields = read!(14);
                    self.nl = (fields & 31) as usize + 257;
                    self.nd = ((fields >> 5) & 31) as usize + 1;
                    self.nc = (fields >> 10) as usize + 4;
                    if self.nl > 286 {
                        return Err(Error::Invalid("too many literal codes"));
                    }
                    self.code_lengths.fill(0);
                    self.lengths.fill(0);
                    self.index = 0;
                    self.state = State::DynamicCodeLengths;
                }
                State::DynamicCodeLengths => {
                    const ORDER: [usize; 19] = [
                        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
                    ];
                    while self.index < self.nc {
                        self.code_lengths[ORDER[self.index]] = read!(3) as u8;
                        self.index += 1;
                    }
                    self.literals = Huffman::new(&self.code_lengths, true, false)?;
                    self.index = 0;
                    self.state = State::DynamicLengths;
                }
                State::DynamicLengths => {
                    if self.index == self.nl + self.nd {
                        if self.lengths[256] == 0 {
                            return Err(Error::Invalid("missing end of block code"));
                        }
                        self.literals = Huffman::new(&self.lengths[..self.nl], false, false)?;
                        self.distances =
                            Huffman::new(&self.lengths[self.nl..self.nl + self.nd], false, true)?;
                        self.state = State::Codes;
                        continue;
                    }
                    match symbol!(&self.literals) {
                        value @ 0..=15 => {
                            self.lengths[self.index] = value as u8;
                            self.index += 1;
                        }
                        16 if self.index != 0 => {
                            self.state = State::DynamicRepeat {
                                value: self.lengths[self.index - 1],
                                base: 3,
                                extra: 2,
                            }
                        }
                        17 => {
                            self.state = State::DynamicRepeat {
                                value: 0,
                                base: 3,
                                extra: 3,
                            }
                        }
                        18 => {
                            self.state = State::DynamicRepeat {
                                value: 0,
                                base: 11,
                                extra: 7,
                            }
                        }
                        _ => return Err(Error::Invalid("invalid code length repeat")),
                    }
                }
                State::DynamicRepeat { value, base, extra } => {
                    let n = base + read!(extra) as usize;
                    if n > self.nl + self.nd - self.index {
                        return Err(Error::Invalid("code length repeat overflow"));
                    }
                    self.lengths[self.index..self.index + n].fill(value);
                    self.index += n;
                    self.state = State::DynamicLengths;
                }
                State::Codes => loop {
                    match symbol!(&self.literals) {
                        value @ 0..=255 => {
                            if self.total_out == self.limit {
                                return Err(Error::LimitExceeded);
                            }
                            if written == output.len() {
                                self.state = State::Literal(value as u8);
                                stop!(StreamStatus::NeedOutput);
                            }
                            // Keep literal runs in this state, without another
                            // dispatch or a one-byte ring-copy operation.
                            output[written] = value as u8;
                            self.history[self.history_pos] = value as u8;
                            self.history_pos = (self.history_pos + 1) & (self.history.len() - 1);
                            self.total_out += 1;
                            written += 1;
                        }
                        256 => {
                            self.end_block();
                            break;
                        }
                        value @ 257..=285 => {
                            let index = value - 257;
                            // Save each suspension point before reading its
                            // field. Complete tokens avoid the outer dispatch;
                            // fragmented tokens resume in the existing states.
                            self.state = State::LengthExtra(index);
                            let length = LENGTH_BASE[index] + read!(LENGTH_EXTRA[index]) as usize;
                            self.state = State::DistanceCode(length);
                            let symbol = symbol!(&self.distances);
                            if symbol >= 30 {
                                return Err(Error::Invalid("reserved distance symbol"));
                            }
                            self.state = State::DistanceExtra { length, symbol };
                            let distance = DIST_BASE[symbol] + read!(DIST_EXTRA[symbol]) as usize;
                            self.begin_copy(length, distance)?;
                            if length > output.len() - written {
                                break;
                            }
                            self.copy_match(distance, &mut output[written..written + length]);
                            written += length;
                            self.state = State::Codes;
                        }
                        _ => return Err(Error::Invalid("reserved literal symbol")),
                    }
                },
                State::Literal(value) => {
                    if self.total_out == self.limit {
                        return Err(Error::LimitExceeded);
                    }
                    if written == output.len() {
                        stop!(StreamStatus::NeedOutput);
                    }
                    output[written] = value;
                    self.remember(&output[written..written + 1]);
                    written += 1;
                    self.state = State::Codes;
                }
                State::LengthExtra(index) => {
                    self.state = State::DistanceCode(
                        LENGTH_BASE[index] + read!(LENGTH_EXTRA[index]) as usize,
                    )
                }
                State::DistanceCode(length) => {
                    let symbol = symbol!(&self.distances);
                    if symbol >= 30 {
                        return Err(Error::Invalid("reserved distance symbol"));
                    }
                    self.state = State::DistanceExtra { length, symbol };
                }
                State::DistanceExtra { length, symbol } => {
                    let distance = DIST_BASE[symbol] + read!(DIST_EXTRA[symbol]) as usize;
                    self.begin_copy(length, distance)?;
                }
                State::Copy { distance, left } => {
                    if written == output.len() {
                        stop!(StreamStatus::NeedOutput);
                    }
                    let n = left.min(output.len() - written);
                    self.copy_match(distance, &mut output[written..written + n]);
                    written += n;
                    self.state = if n == left {
                        State::Codes
                    } else {
                        State::Copy {
                            distance,
                            left: left - n,
                        }
                    };
                }
                State::Adler => {
                    self.checksum.update(&output[checksummed..written]);
                    checksummed = written;
                    bits.align();
                    let expected = read!(32).swap_bytes();
                    if expected != self.checksum.finish() {
                        return Err(Error::Checksum);
                    }
                    self.state = State::Done;
                }
                State::Done => stop!(StreamStatus::Done),
            }
        }
    }
}
