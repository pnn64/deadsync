use super::{
    Bits, DIST_BASE, DIST_EXTRA, FIXED, FIXED_PRIMARY, Huffman, LENGTH_BASE, LENGTH_EXTRA,
    inflate_sized_fast,
};
use crate::{Error, checksum::Adler32, reserve};

const WINDOW_SIZE: usize = 32 * 1024;

struct WideTables {
    valid: bool,
    literals: [[u32; 512]; 8],
    distances: [u32; 512],
}

/// Contiguous filtered output for the collecting PNG reader. Only the most
/// recent DEFLATE window and space to inflate the next row are needed.
pub(crate) struct InflateWindow {
    bytes: Vec<u8>,
    pos: usize,
    tables: Vec<WideTables>,
}

impl InflateWindow {
    pub(crate) fn new(inflater: &mut Inflater, row_bytes: usize) -> Result<Self, Error> {
        let len = inflater
            .limit
            .min(WINDOW_SIZE.saturating_add(row_bytes.max(16 * 1024)));
        let mut bytes = core::mem::take(&mut inflater.history);
        let pos = inflater.total_out.min(bytes.len());
        if pos == bytes.len() {
            bytes.rotate_left(inflater.history_pos);
        }
        if len > bytes.len() {
            bytes
                .try_reserve_exact(len - bytes.len())
                .map_err(|_| Error::Allocation)?;
        }
        bytes.resize(len, 0);
        Ok(Self {
            bytes,
            pos,
            tables: Vec::new(),
        })
    }

    #[inline(never)]
    pub(crate) fn process(
        &mut self,
        inflater: &mut Inflater,
        input: &[u8],
        row: &mut [u8],
    ) -> Result<InflateProgress, Error> {
        if row.len() > self.bytes.len() - self.pos {
            let slide = self.pos - WINDOW_SIZE;
            self.bytes.copy_within(slide..self.pos, 0);
            self.pos = WINDOW_SIZE;
        }
        let start = self.pos;
        let progress = inflater.process_output::<true>(
            input,
            &mut self.bytes[..start + row.len()],
            start,
            &mut self.tables,
        )?;
        self.pos += progress.written;
        row[..progress.written].copy_from_slice(&self.bytes[start..self.pos]);
        Ok(progress)
    }

    pub(crate) fn into_history(mut self, output_limit: usize) -> Vec<u8> {
        self.bytes.truncate(history_size(output_limit));
        self.bytes
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        self.bytes.capacity() + self.tables.capacity() * core::mem::size_of::<WideTables>()
    }
}

fn history_size(output_limit: usize) -> usize {
    output_limit.clamp(1, WINDOW_SIZE).next_power_of_two()
}

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
    // None borrows fixed tables; Some owns a dynamic table.
    literals: Option<Huffman>,
    distances: Option<Huffman<32>>,
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
        Self::with_history(output_limit, Vec::new())
    }
    pub(crate) fn with_history(output_limit: usize, mut history: Vec<u8>) -> Result<Self, Error> {
        // A distance cannot exceed produced bytes or the output limit. Round
        // the bounded history to a power of two to retain masked ring indices.
        let size = history_size(output_limit);
        if history.len() != size {
            history = Vec::new();
            reserve(&mut history, size)?;
            history.resize(size, 0);
        }
        // Every match is bounded by this stream's produced bytes, so reused
        // history entries cannot be read before they are overwritten.
        Ok(Self {
            state: State::HeaderCmf,
            word: 0,
            bit_count: 0,
            final_block: false,
            window: 32768,
            history,
            history_pos: 0,
            literals: None,
            distances: None,
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
    pub(crate) fn into_history(self) -> Vec<u8> {
        self.history
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
    pub(crate) fn prefer_contiguous_output(&self) -> bool {
        // The direct row path already handles stored bytes and long repeated
        // matches efficiently. Use wider tables once enough coded input has
        // gone through this stream to amortize the setup and staging copy.
        self.total_in >= 256
            && self.total_in >= self.total_out.div_ceil(64)
            && matches!(
                self.state,
                State::Codes | State::Literal(_) | State::Copy { .. }
            )
    }
    pub(crate) fn stored_remaining(&self) -> usize {
        match self.state {
            State::Stored(left) => left,
            _ => 0,
        }
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
        self.process_output::<false>(input, output, 0, &mut Vec::new())
    }
    fn process_output<const CONTIGUOUS: bool>(
        &mut self,
        input: &[u8],
        output: &mut [u8],
        start: usize,
        tables: &mut Vec<WideTables>,
    ) -> Result<InflateProgress, Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let mut bits = Bits {
            input,
            pos: 0,
            word: self.word,
            count: self.bit_count,
            chunks: &[],
            prior: 0,
        };
        // Bound output once so each literal needs only a slot check.
        let available = (output.len() - start).min(self.limit - self.total_out);
        let result =
            self.run::<CONTIGUOUS>(&mut bits, &mut output[..start + available], start, tables);
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
                written: written - start,
                status,
            }),
            Err(error) => {
                self.failure = Some(error.clone());
                Err(error)
            }
        }
    }
    fn remember(&mut self, bytes: &[u8]) {
        self.total_out += bytes.len();
        let size = self.history.len();
        let end = self.history_pos + bytes.len();
        let next = end & (size - 1);
        if end <= size {
            self.history[self.history_pos..end].copy_from_slice(bytes);
        } else {
            // A whole window can overwrite everything older in this copy.
            let bytes = &bytes[bytes.len().saturating_sub(size)..];
            let start = (next + size - bytes.len()) & (size - 1);
            let (tail, head) = bytes.split_at(size - start);
            self.history[start..].copy_from_slice(tail);
            self.history[..head.len()].copy_from_slice(head);
        }
        self.history_pos = next;
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
        let size = self.history.len();
        if distance == 1 {
            let value = self.history[(self.history_pos + size - 1) & (size - 1)];
            output.fill(value);
            let end = self.history_pos + n;
            if end <= size {
                self.history[self.history_pos..end].fill(value);
            } else {
                self.history[self.history_pos..].fill(value);
                self.history[..end - size].fill(value);
            }
            self.history_pos = end & (size - 1);
            self.total_out += n;
            return;
        }
        let start = (self.history_pos + size - distance) & (size - 1);
        if n >= 16 && matches!(distance, 2 | 4 | 8) {
            self.copy_short_period(distance, start, output);
            return;
        }
        let seed = distance.min(n);
        let end = start + seed;
        if end <= size {
            output[..seed].copy_from_slice(&self.history[start..end]);
        } else {
            let first = size - start;
            output[..first].copy_from_slice(&self.history[start..]);
            output[first..seed].copy_from_slice(&self.history[..end - size]);
        }
        let mut filled = seed;
        while filled < n {
            let count = filled.min(n - filled);
            output.copy_within(..count, filled);
            filled += count;
        }
        self.remember(output);
    }
    #[inline(never)]
    fn copy_short_period(&mut self, distance: usize, start: usize, output: &mut [u8]) {
        let mask = self.history.len() - 1;
        let pattern = if distance == 8
            && let Some(bytes) = self.history[start..].first_chunk::<8>()
        {
            *bytes
        } else {
            core::array::from_fn(|i| self.history[(start + (i & (distance - 1))) & mask])
        };
        let (words, tail) = output.as_chunks_mut::<8>();
        words.fill(pattern);
        tail.copy_from_slice(&pattern[..tail.len()]);
        self.remember(output);
    }

    #[inline]
    fn copy_output<const CONTIGUOUS: bool>(
        &mut self,
        output: &mut [u8],
        distance: usize,
        mut pos: usize,
        length: usize,
    ) {
        if CONTIGUOUS {
            let source = pos - distance;
            let end = pos + length;
            if distance == 1 {
                let value = output[source];
                output[pos..end].fill(value);
            } else {
                while pos < end {
                    let n = (pos - source).min(end - pos);
                    output.copy_within(source..source + n, pos);
                    pos += n;
                }
            }
            self.total_out += length;
        } else {
            self.copy_match(distance, &mut output[pos..pos + length]);
        }
    }

    #[inline(never)]
    fn wide_codes(
        &mut self,
        bits: &mut Bits<'_>,
        output: &mut [u8],
        written: &mut usize,
        tables: &mut Vec<WideTables>,
    ) -> Result<bool, Error> {
        let distances = self.distances.as_ref().unwrap_or(&FIXED.1);
        let start = *written;
        let ended = if let Some(literals) = &self.literals {
            if tables.is_empty() {
                tables.try_reserve_exact(1).map_err(|_| Error::Allocation)?;
                tables.push(WideTables {
                    valid: false,
                    literals: [[0; 512]; 8],
                    distances: [0; 512],
                });
            }
            let tables = &mut tables[0];
            if !tables.valid {
                let pairing = literals.pairing();
                tables.literals.fill(literals.pair_prefix(&pairing));
                literals.pair_primary(&pairing, &mut tables.literals);
                tables.distances = distances.distance_primary();
                tables.valid = true;
            }
            inflate_sized_fast::<_, true>(
                bits,
                output,
                written,
                distances,
                tables.literals.as_flattened().try_into().unwrap(),
                &tables.distances,
                self.window,
            )?
        } else {
            let (literals, distance_fast) = &*FIXED_PRIMARY;
            inflate_sized_fast::<_, true>(
                bits,
                output,
                written,
                distances,
                literals,
                distance_fast,
                self.window,
            )?
        };
        self.total_out += *written - start;
        // Keep at most four whole bytes beyond the current bit position.
        // Even if this was the last symbol, the aligned 32-bit Adler trailer
        // consumes those bytes without advancing into any trailing data.
        let unused = usize::from(bits.count.saturating_sub(32) / 8).min(bits.pos);
        bits.pos -= unused;
        bits.count -= 8 * unused as u8;
        bits.word &= (1u64 << bits.count) - 1;
        Ok(ended)
    }
    fn end_block(&mut self) {
        self.state = if self.final_block {
            State::Adler
        } else {
            State::BlockFinal
        };
    }
    fn run<const CONTIGUOUS: bool>(
        &mut self,
        bits: &mut Bits<'_>,
        output: &mut [u8],
        mut written: usize,
        tables: &mut Vec<WideTables>,
    ) -> Result<(usize, StreamStatus), Error> {
        let mut checksummed = written;
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
        'states: loop {
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
                        self.literals = None;
                        self.distances = None;
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
                    if length == 0 {
                        self.end_block();
                    } else {
                        self.state = State::Stored(length);
                    }
                }
                State::Stored(left) => {
                    // A Huffman end code leaves at most 39 bits. The next
                    // block tag and byte alignment leave at most 32, which
                    // are consumed by the stored length and its complement.
                    debug_assert_eq!(bits.count, 0);
                    debug_assert!(left != 0);
                    let available = output.len() - written;
                    let n = left.min(available).min(bits.input.len() - bits.pos);
                    if n != 0 {
                        let bytes = &mut output[written..written + n];
                        bytes.copy_from_slice(&bits.input[bits.pos..bits.pos + n]);
                        bits.pos += n;
                        if CONTIGUOUS || self.final_block {
                            self.total_out += n;
                        } else {
                            self.remember(bytes);
                        }
                        written += n;
                    }
                    if n == left {
                        self.end_block();
                    } else {
                        self.state = State::Stored(left - n);
                        stop!(if n == available {
                            StreamStatus::NeedOutput
                        } else {
                            StreamStatus::NeedInput
                        });
                    }
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
                    // DynamicLengths overwrites the prefix before either tree reads it.
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
                    self.literals = Some(Huffman::new(&self.code_lengths, true, false)?);
                    self.index = 0;
                    self.state = State::DynamicLengths;
                }
                State::DynamicLengths => {
                    let end = self.nl + self.nd;
                    while self.index < end {
                        let (value, base, extra) = match symbol!(self.literals.as_ref().unwrap()) {
                            value @ 0..=15 => {
                                self.lengths[self.index] = value as u8;
                                self.index += 1;
                                continue;
                            }
                            16 if self.index != 0 => (self.lengths[self.index - 1], 3, 2),
                            17 => (0, 3, 3),
                            18 => (0, 11, 7),
                            _ => return Err(Error::Invalid("invalid code length repeat")),
                        };
                        self.state = State::DynamicRepeat { value, base, extra };
                        continue 'states;
                    }
                    if self.lengths[256] == 0 {
                        return Err(Error::Invalid("missing end of block code"));
                    }
                    self.literals = Some(Huffman::new(&self.lengths[..self.nl], false, false)?);
                    self.distances = Some(Huffman::new(&self.lengths[self.nl..end], false, true)?);
                    if CONTIGUOUS && let Some(tables) = tables.first_mut() {
                        tables.valid = false;
                    }
                    self.state = State::Codes;
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
                    if CONTIGUOUS
                        && output.len() - written >= 16
                        && bits.input.len() - bits.pos >= 8
                        && self.wide_codes(bits, output, &mut written, tables)?
                    {
                        self.end_block();
                        break;
                    }
                    match symbol!(self.literals.as_ref().unwrap_or(&FIXED.0)) {
                        value @ 0..=255 => {
                            let Some(slot) = output.get_mut(written) else {
                                if self.total_out == self.limit {
                                    return Err(Error::LimitExceeded);
                                }
                                self.state = State::Literal(value as u8);
                                stop!(StreamStatus::NeedOutput);
                            };
                            // Keep literal runs in this state, without another
                            // dispatch or a one-byte ring-copy operation.
                            *slot = value as u8;
                            if !CONTIGUOUS {
                                self.history[self.history_pos] = value as u8;
                                self.history_pos =
                                    (self.history_pos + 1) & (self.history.len() - 1);
                            }
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
                            let symbol = symbol!(self.distances.as_ref().unwrap_or(&FIXED.1));
                            if symbol >= 30 {
                                return Err(Error::Invalid("reserved distance symbol"));
                            }
                            self.state = State::DistanceExtra { length, symbol };
                            let distance = DIST_BASE[symbol] + read!(DIST_EXTRA[symbol]) as usize;
                            self.begin_copy(length, distance)?;
                            if length > output.len() - written {
                                break;
                            }
                            self.copy_output::<CONTIGUOUS>(output, distance, written, length);
                            written += length;
                            self.state = State::Codes;
                        }
                        _ => return Err(Error::Invalid("reserved literal symbol")),
                    }
                },
                State::Literal(value) => {
                    let Some(slot) = output.get_mut(written) else {
                        if self.total_out == self.limit {
                            return Err(Error::LimitExceeded);
                        }
                        stop!(StreamStatus::NeedOutput);
                    };
                    *slot = value;
                    if !CONTIGUOUS {
                        self.history[self.history_pos] = value;
                        self.history_pos = (self.history_pos + 1) & (self.history.len() - 1);
                    }
                    self.total_out += 1;
                    written += 1;
                    self.state = State::Codes;
                }
                State::LengthExtra(index) => {
                    self.state = State::DistanceCode(
                        LENGTH_BASE[index] + read!(LENGTH_EXTRA[index]) as usize,
                    )
                }
                State::DistanceCode(length) => {
                    let symbol = symbol!(self.distances.as_ref().unwrap_or(&FIXED.1));
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
                    self.copy_output::<CONTIGUOUS>(output, distance, written, n);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_output_keeps_history_and_exact_zlib_end() {
        use std::io::Write;

        const ROW: usize = 1153;
        let mut random = 0x452f_75a3u32;
        let prefix: Vec<_> = (0..24 * 1024)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        let filtered: Vec<_> = (0..ROW * 121)
            .map(|i| {
                if i / prefix.len() == 2 {
                    ((i / 3) & 63) as u8
                } else {
                    prefix[i % prefix.len()]
                }
            })
            .collect();
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
        encoder.write_all(&filtered).unwrap();
        let mut wire = encoder.finish().unwrap();
        let zlib_bytes = wire.len();
        wire.extend([0x97; 64]);

        for fragment in [1, 17, 1024, 8192, wire.len()] {
            for switch_at in [0, ROW * 41] {
                let mut inflater = Inflater::new(filtered.len()).unwrap();
                let mut window = None;
                let mut result = Vec::with_capacity(filtered.len());
                let mut row = [0; ROW];
                let mut read = 0;
                while !inflater.is_done() {
                    let end = wire.len().min(read + fragment);
                    let wanted = (ROW - result.len() % ROW).min(filtered.len() - result.len());
                    if result.len() == switch_at && window.is_none() {
                        window = Some(InflateWindow::new(&mut inflater, ROW).unwrap());
                    }
                    let input = &wire[read..end];
                    let output = &mut row[..wanted];
                    let progress = match window.as_mut() {
                        Some(window) => window.process(&mut inflater, input, output),
                        None => inflater.process(input, output),
                    }
                    .unwrap();
                    assert!(progress.consumed != 0 || progress.written != 0 || inflater.is_done());
                    read += progress.consumed;
                    result.extend_from_slice(&row[..progress.written]);
                }
                assert_eq!(read, zlib_bytes, "fragment={fragment} switch={switch_at}");
                assert_eq!(result, filtered, "fragment={fragment} switch={switch_at}");
                assert!(
                    window.unwrap().pos < result.len(),
                    "the window must have moved"
                );
            }
        }
    }

    #[test]
    fn reused_history_cannot_supply_a_match_before_the_new_stream_prefix() {
        use super::super::{FIXED_CODES, huffman_fixtures::Wire};
        let mut first = Inflater::new(3).unwrap();
        first.remember(b"old");
        let mut next = Inflater::with_history(3, first.into_history()).unwrap();
        let mut wire = Wire::default();
        wire.put(3, 3);
        let (word, bits) = FIXED_CODES[257];
        wire.put(u32::from(word), bits); // Length three.
        wire.put(0, 5); // Distance one.
        let encoded = wire.finish(b"ddd", false);
        let mut output = [0; 3];
        assert_eq!(
            next.process(&encoded, &mut output),
            Err(Error::Invalid("distance outside history window"))
        );
        assert_eq!(output, [0; 3]);
    }

    #[test]
    fn history_writes_preserve_the_ring_after_oversized_copies() {
        let mut inflater = Inflater::new(1 << 18).unwrap();
        let mut history = vec![0; inflater.history.len()];
        let mut position = 0;
        let mut total = 0;
        for (step, length) in [0, 17, 32749, 31, 32768, 65535, 32770, 5]
            .into_iter()
            .enumerate()
        {
            let bytes: Vec<u8> = (0..length)
                .map(|n| (n * 37 + (n >> 8) + step * 93) as u8)
                .collect();
            inflater.remember(&bytes);
            for byte in bytes {
                history[position] = byte;
                position = (position + 1) % history.len();
            }
            total += length;
            assert_eq!(inflater.history, history);
            assert_eq!(inflater.history_pos, position);
            assert_eq!(inflater.total_out(), total);
        }
    }
}
