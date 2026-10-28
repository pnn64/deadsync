//! Native RFC 1950/1951 compression. No dictionaries are permitted in PNG.
use crate::{Error, checksum::adler32, reserve};

mod deflate;
mod inflate;
mod matcher;
pub use deflate::{DeflateProgress, Deflater};
pub use inflate::{InflateProgress, Inflater, StreamStatus};

const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [usize; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

struct Bits<'a> {
    input: &'a [u8],
    pos: usize,
    word: u64,
    count: u8,
}
impl<'a> Bits<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            word: 0,
            count: 0,
        }
    }
    fn fill(&mut self, n: u8) {
        while self.count < n && self.pos < self.input.len() {
            self.word |= u64::from(self.input[self.pos]) << self.count;
            self.pos += 1;
            self.count += 8;
        }
    }
    fn fill_huffman(&mut self, n: u8) {
        // Only Huffman lookahead may refill this widely. Fast lookahead starts
        // with at most 8 bits; a slow miss starts with at most 14 and consumes
        // at least 10. After EOB and alignment, at most four bytes remain: the
        // zlib Adler trailer. A streaming caller's suffix is never accepted.
        // General field/checksum reads retain their conservative byte refill.
        if self.count < n {
            if self.input.len() - self.pos >= 4 {
                let bytes = self.input[self.pos..self.pos + 4].try_into().unwrap();
                self.word |= u64::from(u32::from_le_bytes(bytes)) << self.count;
                self.pos += 4;
                self.count += 32;
            } else {
                self.fill(n);
            }
        }
    }
    fn read(&mut self, n: u8) -> Result<u32, Error> {
        self.fill(n);
        if self.count < n {
            return Err(Error::Truncated);
        }
        let value = self.word as u32 & ((1u64 << n) - 1) as u32;
        self.word >>= n;
        self.count -= n;
        Ok(value)
    }
    fn consumed(&self) -> usize {
        // ceil((pos * 8 - count) / 8), without overflowing the byte cursor.
        self.pos - usize::from(self.count / 8)
    }
    fn align(&mut self) {
        let n = self.count % 8;
        self.word >>= n;
        self.count -= n;
    }
}

// A small primary table handles common codes. Canonical arrays handle longer
// codes without a 128 KiB lookup allocation per dynamic block.
struct Huffman {
    fast: [u16; 512],
    counts: [u16; 16],
    first: [u16; 16],
    offsets: [u16; 16],
    symbols: [u16; 288],
    max: u8,
}
impl Huffman {
    fn new(lengths: &[u8], complete: bool, allow_empty: bool) -> Result<Self, Error> {
        let mut h = Self {
            fast: [0; 512],
            counts: [0; 16],
            first: [0; 16],
            offsets: [0; 16],
            symbols: [0; 288],
            max: 0,
        };
        for &n in lengths {
            if n > 15 {
                return Err(Error::Invalid("Huffman code length"));
            }
            if n != 0 {
                h.counts[n as usize] += 1;
                h.max = h.max.max(n);
            }
        }
        if h.max == 0 {
            return if allow_empty {
                Ok(h)
            } else {
                Err(Error::Invalid("empty Huffman tree"))
            };
        }
        let mut left = 1i32;
        let mut code = 0;
        let mut offset = 0;
        for n in 1..16 {
            left = left * 2 - i32::from(h.counts[n]);
            if left < 0 {
                return Err(Error::Invalid("oversubscribed Huffman tree"));
            }
            code = (code + h.counts[n - 1]) << 1;
            h.first[n] = code;
            h.offsets[n] = offset;
            offset += h.counts[n];
        }
        if left != 0 && (complete || h.max != 1) {
            return Err(Error::Invalid("incomplete Huffman tree"));
        }
        let mut next = h.first;
        for (symbol, &n) in lengths.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let n = n as usize;
            let code = next[n];
            h.symbols[(h.offsets[n] + code - h.first[n]) as usize] = symbol as u16;
            if n <= 9 {
                let rev = (code.reverse_bits() >> (16 - n)) as usize;
                for index in (rev..512).step_by(1 << n) {
                    h.fast[index] = ((symbol as u16) << 4) | n as u16;
                }
            }
            next[n] += 1;
        }
        Ok(h)
    }
    // A small primary lookup can inline at symbol call sites; keep the
    // canonical long-code and error paths in a separate function.
    #[inline]
    fn decode(&self, bits: &mut Bits<'_>) -> Result<usize, Error> {
        bits.fill_huffman(9);
        let entry = self.fast[(bits.word & 511) as usize];
        let n = (entry & 15) as u8;
        if n != 0 && n <= bits.count {
            bits.word >>= n;
            bits.count -= n;
            return Ok((entry >> 4) as usize);
        }
        self.decode_slow(bits)
    }

    #[inline(never)]
    fn decode_slow(&self, bits: &mut Bits<'_>) -> Result<usize, Error> {
        bits.fill_huffman(self.max);
        // A primary-table miss cannot match a code of length 1 through 9.
        // Reverse once, then compare only the remaining canonical lengths.
        let prefix = (bits.word as u16).reverse_bits();
        for n in 10..=self.max.min(bits.count) as usize {
            let code = prefix >> (16 - n);
            if code >= self.first[n] && code - self.first[n] < self.counts[n] {
                bits.word >>= n;
                bits.count -= n as u8;
                return Ok(
                    self.symbols[(self.offsets[n] + code - self.first[n]) as usize] as usize,
                );
            }
        }
        if bits.count < self.max {
            Err(Error::Truncated)
        } else {
            Err(Error::Invalid("invalid Huffman code"))
        }
    }
}

fn fixed() -> (Huffman, Huffman) {
    let mut lengths = [8; 288];
    lengths[144..256].fill(9);
    lengths[256..280].fill(7);
    (
        Huffman::new(&lengths, false, false).unwrap(),
        Huffman::new(&[5; 32], false, false).unwrap(),
    )
}

fn dynamic(bits: &mut Bits<'_>) -> Result<(Huffman, Huffman), Error> {
    let nl = bits.read(5)? as usize + 257;
    let nd = bits.read(5)? as usize + 1;
    let nc = bits.read(4)? as usize + 4;
    if nl > 286 {
        return Err(Error::Invalid("too many literal codes"));
    }
    let order = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let mut lengths = [0; 19];
    for &index in &order[..nc] {
        lengths[index] = bits.read(3)? as u8;
    }
    let tree = Huffman::new(&lengths, true, false)?;
    let mut lengths = [0; 318];
    let mut pos = 0;
    while pos < nl + nd {
        let symbol = tree.decode(bits)?;
        let (value, count) = match symbol {
            0..=15 => (symbol as u8, 1),
            16 if pos != 0 => (lengths[pos - 1], bits.read(2)? as usize + 3),
            17 => (0, bits.read(3)? as usize + 3),
            18 => (0, bits.read(7)? as usize + 11),
            _ => return Err(Error::Invalid("invalid code length repeat")),
        };
        if pos + count > nl + nd {
            return Err(Error::Invalid("code length repeat overflow"));
        }
        lengths[pos..pos + count].fill(value);
        pos += count;
    }
    if lengths[256] == 0 {
        return Err(Error::Invalid("missing end of block code"));
    }
    Ok((
        Huffman::new(&lengths[..nl], false, false)?,
        Huffman::new(&lengths[nl..nl + nd], false, true)?,
    ))
}

fn reserve_output(output: &mut Vec<u8>, additional: usize, limit: usize) -> Result<(), Error> {
    if additional > output.capacity() - output.len() {
        let needed = output
            .len()
            .checked_add(additional)
            .filter(|&n| n <= limit)
            .ok_or(Error::LimitExceeded)?;
        // Grow only as output is produced, capping requests at its limit.
        let capacity = output
            .capacity()
            .saturating_mul(2)
            .max(8)
            .max(needed)
            .min(limit);
        output
            .try_reserve_exact(capacity - output.len())
            .map_err(|_| Error::Allocation)?;
    }
    Ok(())
}

// Keep the Huffman tables and coded-block loop out of stored-block decoding.
#[inline(never)]
fn inflate_codes(
    bits: &mut Bits<'_>,
    output: &mut Vec<u8>,
    fixed_codes: bool,
    window: usize,
    limit: usize,
) -> Result<(), Error> {
    let (lit, dist) = if fixed_codes { fixed() } else { dynamic(bits)? };
    loop {
        match lit.decode(bits)? {
            literal @ 0..=255 => {
                if output.len() == limit {
                    return Err(Error::LimitExceeded);
                }
                if output.len() == output.capacity() {
                    reserve_output(output, 1, limit)?;
                }
                output.push(literal as u8);
                let mut room = (limit - output.len()).min(output.capacity() - output.len());
                let mut word = bits.word;
                let mut count = bits.count;
                // Append complete primary literals from prefetched
                // bits, without a refill or capacity growth here.
                while room != 0 && count >= 9 {
                    let entry = lit.fast[(word & 511) as usize];
                    if !(1..4096).contains(&entry) {
                        break;
                    }
                    let n = (entry & 15) as u8;
                    output.push((entry >> 4) as u8);
                    word >>= n;
                    count -= n;
                    room -= 1;
                }
                bits.word = word;
                bits.count = count;
            }
            256 => break,
            symbol @ 257..=285 => {
                let index = symbol - 257;
                let len = LENGTH_BASE[index] + bits.read(LENGTH_EXTRA[index])? as usize;
                let symbol = dist.decode(bits)?;
                if symbol >= 30 {
                    return Err(Error::Invalid("reserved distance symbol"));
                }
                let distance = DIST_BASE[symbol] + bits.read(DIST_EXTRA[symbol])? as usize;
                if distance > output.len() || distance > window {
                    return Err(Error::Invalid("distance outside history window"));
                }
                if len > limit.saturating_sub(output.len()) {
                    return Err(Error::LimitExceeded);
                }
                reserve_output(output, len, limit)?;
                let start = output.len() - distance;
                if distance == 1 {
                    output.resize(output.len() + len, output[start]);
                } else {
                    let mut left = len;
                    while left != 0 {
                        let count = (output.len() - start).min(left);
                        output.extend_from_within(start..start + count);
                        left -= count;
                    }
                }
            }
            _ => return Err(Error::Invalid("reserved literal symbol")),
        }
    }
    Ok(())
}

/// Decode exactly one zlib stream, rejecting trailing data. `limit` bounds output.
pub fn decompress(input: &[u8], limit: usize) -> Result<Vec<u8>, Error> {
    let (output, used) = decompress_prefix(input, limit)?;
    if used != input.len() {
        return Err(Error::Invalid("trailing zlib data"));
    }
    Ok(output)
}

/// Decode the first zlib stream and return the number of input bytes consumed.
pub fn decompress_prefix(input: &[u8], limit: usize) -> Result<(Vec<u8>, usize), Error> {
    if input.len() < 6 {
        return Err(Error::Truncated);
    }
    let cmf = input[0];
    if cmf & 15 != 8 || cmf >> 4 > 7 || (u16::from(cmf) * 256 + u16::from(input[1])) % 31 != 0 {
        return Err(Error::Invalid("zlib header"));
    }
    if input[1] & 32 != 0 {
        return Err(Error::Invalid("zlib dictionary not permitted"));
    }
    let window = 1usize << ((cmf >> 4) + 8);
    let mut bits = Bits::new(&input[2..]);
    let mut output = Vec::new();
    loop {
        let last = bits.read(1)? != 0;
        let kind = bits.read(2)?;
        if kind == 0 {
            bits.align();
            let len = bits.read(16)? as usize;
            if len ^ bits.read(16)? as usize != 65535 {
                return Err(Error::Invalid("stored block length"));
            }
            if len > limit.saturating_sub(output.len()) {
                return Err(Error::LimitExceeded);
            }
            // A nonfinal stored block can amortize the next payload's growth.
            let additional = if last {
                len
            } else {
                len.saturating_mul(2)
                    .min(limit.saturating_sub(output.len()))
            };
            reserve_output(&mut output, additional, limit)?;
            // Drain bytes prefetched by a preceding Huffman block, then copy
            // the byte-aligned payload directly without bit-reader dispatch.
            let mut left = len;
            while left != 0 && bits.count >= 8 {
                output.push(bits.read(8)? as u8);
                left -= 1;
            }
            let end = bits.pos.checked_add(left).ok_or(Error::LimitExceeded)?;
            output.extend_from_slice(bits.input.get(bits.pos..end).ok_or(Error::Truncated)?);
            bits.pos = end;
        } else if kind == 1 || kind == 2 {
            inflate_codes(&mut bits, &mut output, kind == 1, window, limit)?;
        } else {
            return Err(Error::Invalid("reserved block type"));
        }
        if last {
            break;
        }
    }
    let used = 2 + bits.consumed();
    let checksum = input.get(used..used + 4).ok_or(Error::Truncated)?;
    if u32::from_be_bytes(checksum.try_into().unwrap()) != adler32(&output) {
        return Err(Error::Checksum);
    }
    Ok((output, used + 4))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Compression {
    Stored,
    #[default]
    Fast,
}

struct Writer {
    bytes: Vec<u8>,
    word: u64,
    count: u8,
}
#[derive(Clone, Copy)]
struct Checkpoint {
    bytes: usize,
    word: u64,
    count: u8,
}
impl Writer {
    fn write(&mut self, value: u32, n: u8) {
        self.word |= u64::from(value) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.bytes.push(self.word as u8);
            self.word >>= 8;
            self.count -= 8;
        }
    }
    fn literal(&mut self, symbol: usize) {
        let (code, n) = match symbol {
            0..=143 => (0x30 + symbol, 8),
            144..=255 => (0x190 + symbol - 144, 9),
            256..=279 => (symbol - 256, 7),
            _ => (0xc0 + symbol - 280, 8),
        };
        self.write((code as u32).reverse_bits() >> (32 - n), n);
    }
    fn align(&mut self) {
        if self.count != 0 {
            self.bytes.push(self.word as u8);
        }
        self.word = 0;
        self.count = 0;
    }
    fn stored(&mut self, input: &[u8], final_block: bool) {
        self.write(u32::from(final_block), 3);
        self.align();
        let len = input.len() as u16;
        self.bytes.extend_from_slice(&len.to_le_bytes());
        self.bytes.extend_from_slice(&(!len).to_le_bytes());
        self.bytes.extend_from_slice(input);
    }
    fn start_fixed(&mut self, final_block: bool) -> Checkpoint {
        let checkpoint = Checkpoint {
            bytes: self.bytes.len(),
            word: self.word,
            count: self.count,
        };
        self.write(u32::from(final_block) | 2, 3);
        checkpoint
    }
    fn finish_fixed(&mut self, checkpoint: Checkpoint, input: &[u8], final_block: bool) {
        self.literal(256);
        let fixed = (self.bytes.len() - checkpoint.bytes) * 8 + self.count as usize
            - checkpoint.count as usize;
        let fixed_padding = if final_block {
            (8 - self.count as usize % 8) % 8
        } else {
            0
        };
        let stored = 3 + (8 - (checkpoint.count as usize + 3) % 8) % 8 + 32 + input.len() * 8;
        if stored < fixed + fixed_padding {
            self.bytes.truncate(checkpoint.bytes);
            self.word = checkpoint.word;
            self.count = checkpoint.count;
            self.stored(input, final_block);
        }
    }
}

#[inline(always)]
fn length_code(length: usize) -> usize {
    debug_assert!((3..=258).contains(&length));
    if length <= 10 {
        length - 3
    } else if length == 258 {
        28
    } else {
        let value = length - 3;
        let high = value.ilog2() as usize;
        (high - 1) * 4 + ((value >> (high - 2)) & 3)
    }
}
#[inline(always)]
fn distance_code(distance: usize) -> usize {
    debug_assert!((1..=32768).contains(&distance));
    let value = distance - 1;
    if value < 4 {
        value
    } else {
        let high = value.ilog2() as usize;
        high * 2 + ((value >> (high - 1)) & 1)
    }
}

/// Encode a standards-compliant zlib stream with stored or fixed Huffman LZ77 blocks.
pub fn compress(input: &[u8], mode: Compression) -> Result<Vec<u8>, Error> {
    let bound = input
        .len()
        .checked_add(input.len().div_ceil(8))
        .and_then(|n| n.checked_add(64))
        .ok_or(Error::LimitExceeded)?;
    let mut bytes = Vec::new();
    reserve(&mut bytes, bound)?;
    bytes.extend_from_slice(&[0x78, 0x01]);
    if mode == Compression::Stored {
        if input.is_empty() {
            bytes.extend_from_slice(&[1, 0, 0, 255, 255]);
        }
        let blocks = input.chunks(65535);
        let count = blocks.len();
        for (n, block) in blocks.enumerate() {
            bytes.push(u8::from(n + 1 == count));
            let len = block.len() as u16;
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(&(!len).to_le_bytes());
            bytes.extend_from_slice(block);
        }
    } else {
        let mut w = Writer {
            bytes,
            word: 0,
            count: 0,
        };
        let mut matcher = matcher::Matcher::new()?;
        for block_start in (0..input.len().max(1)).step_by(matcher::WINDOW) {
            let end = input.len().min(block_start + matcher::WINDOW);
            let final_block = end == input.len();
            let checkpoint = w.start_fixed(final_block);
            let mut pos = block_start;
            let mut misses = 0usize;
            while pos < end {
                let mut length = 0;
                let mut distance = 0;
                if pos + 3 < input.len() {
                    let mut candidate = matcher.insert(&input[pos..], pos);
                    let max = (end - pos).min(258);
                    let bits = (w.bytes.len() - checkpoint.bytes) * 8 + w.count as usize
                        - checkpoint.count as usize;
                    let searches = matcher::searches(bits, pos - block_start);
                    for attempt in 0..searches {
                        let Some(previous) = candidate else {
                            break;
                        };
                        if length < max
                            && input[previous + length] == input[pos + length]
                            && input[previous] == input[pos]
                        {
                            let matched = matcher::prefix(
                                &input[previous..previous + max],
                                &input[pos..pos + max],
                            );
                            if matched > length {
                                length = matched;
                                distance = pos - previous;
                            }
                            if length == max {
                                break;
                            }
                        }
                        if attempt + 1 == searches {
                            break;
                        }
                        candidate = matcher.next(previous, pos);
                    }
                }
                if length >= 3 {
                    misses = 0;
                    let index = length_code(length);
                    w.literal(257 + index);
                    w.write((length - LENGTH_BASE[index]) as u32, LENGTH_EXTRA[index]);
                    let index = distance_code(distance);
                    w.write((index as u32).reverse_bits() >> 27, 5);
                    w.write((distance - DIST_BASE[index]) as u32, DIST_EXTRA[index]);
                    for next in matcher::positions(pos, length) {
                        if next + 3 < input.len() {
                            matcher.insert(&input[next..], next);
                        }
                    }
                    pos += length;
                } else {
                    // Bound work on sustained incompressible input. Search
                    // resumes within at most 15 literal bytes of new structure.
                    let count = (1 + (misses >> 8)).min(16).min(end - pos);
                    for &byte in &input[pos..pos + count] {
                        w.literal(byte as usize);
                    }
                    misses += count;
                    pos += count;
                }
            }
            w.finish_fixed(checkpoint, &input[block_start..end], final_block);
        }
        w.align();
        bytes = w.bytes;
    }
    bytes.extend_from_slice(&adler32(input).to_be_bytes());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_prefix_byte_count_avoids_large_position_overflow() {
        for pos in [
            0,
            1,
            2,
            7,
            8,
            9,
            usize::MAX / 8,
            usize::MAX / 8 + 1,
            usize::MAX - 7,
            usize::MAX,
        ] {
            for count in 0..=64u8 {
                if pos as u128 * 8 < u128::from(count) {
                    continue;
                }
                let bits = Bits {
                    input: &[],
                    pos,
                    word: 0,
                    count,
                };
                let expected = (pos as u128 * 8 - u128::from(count)).div_ceil(8);
                assert_eq!(bits.consumed() as u128, expected, "pos={pos} count={count}");
            }
        }
    }

    #[test]
    fn every_length_and_distance_code_matches_rfc1951_tables() {
        for length in 3..=258 {
            assert_eq!(
                length_code(length),
                LENGTH_BASE
                    .iter()
                    .rposition(|&base| base <= length)
                    .unwrap(),
                "length={length}"
            );
        }
        for distance in 1..=32768 {
            assert_eq!(
                distance_code(distance),
                DIST_BASE
                    .iter()
                    .rposition(|&base| base <= distance)
                    .unwrap(),
                "distance={distance}"
            );
        }
    }
}
