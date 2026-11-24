//! Native RFC 1950/1951 compression. No dictionaries are permitted in PNG.
use crate::{
    Error,
    checksum::{Adler32, adler32},
    reserve,
};

#[path = "zlib/deflate.rs"]
mod deflate;
#[path = "zlib/inflate.rs"]
mod inflate;
#[path = "zlib/matcher.rs"]
mod matcher;
#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
#[path = "zlib/x86.rs"]
mod x86;
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

#[derive(Clone)]
struct Bits<'a> {
    input: &'a [u8],
    pos: usize,
    word: u64,
    count: u8,
    chunks: &'a [u8],
    prior: usize,
}
impl<'a> Bits<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            pos: 0,
            word: 0,
            count: 0,
            chunks: &[],
            prior: 0,
        }
    }
    fn png_chunks(chunks: &'a [u8]) -> Self {
        Self {
            chunks,
            ..Self::new(&[])
        }
    }
    // The buffered PNG parser has already checked the image-data framing
    // and CRCs. Ancillary chunks can appear between fdAT payloads.
    #[cold]
    #[inline(never)]
    fn advance(&mut self) -> bool {
        while self.chunks.len() >= 12 {
            let len = u32::from_be_bytes(self.chunks[..4].try_into().unwrap()) as usize;
            if len > self.chunks.len() - 12 {
                return false;
            }
            let (chunk, rest) = self.chunks.split_at(12 + len);
            self.chunks = rest;
            let offset = match &chunk[4..8] {
                b"IDAT" => 8,
                b"fdAT" => 12,
                _ => continue,
            };
            self.prior += self.input.len();
            self.input = &chunk[offset..8 + len];
            self.pos = 0;
            if !self.input.is_empty() {
                return true;
            }
        }
        false
    }
    #[inline]
    fn fill(&mut self, n: u8) {
        while self.count < n {
            if self.pos == self.input.len() && !self.advance() {
                break;
            }
            self.word |= u64::from(self.input[self.pos]) << self.count;
            self.pos += 1;
            self.count += 8;
        }
    }
    #[inline]
    fn fill_huffman(&mut self, n: u8) {
        // Prefetch for primary lookups and canonical misses. consumed()
        // subtracts still-buffered whole bytes, preserving the exact prefix
        // boundary even when lookahead reaches the trailer or a suffix.
        // Field and checksum reads retain their conservative byte refill.
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
    #[inline]
    fn fill_wide(&mut self) {
        if self.input.len() - self.pos >= 8 {
            debug_assert!(self.count < 64);
            let bytes = self.input[self.pos..self.pos + 8].try_into().unwrap();
            self.word |= u64::from_le_bytes(bytes) << self.count;
            // Account only for complete bytes fitting in the accumulator.
            // The last loaded byte is reread on the following refill.
            let bytes = (63 - self.count) / 8;
            self.pos += usize::from(bytes);
            self.count |= 56;
        }
    }
    #[inline(always)]
    fn read(&mut self, n: u8) -> Result<u32, Error> {
        self.fill(n);
        if self.count < n {
            return Err(Error::Truncated);
        }
        Ok(self.take(n))
    }
    #[inline(always)]
    fn take(&mut self, n: u8) -> u32 {
        debug_assert!(n <= self.count);
        let value = self.word as u32 & ((1u64 << n) - 1) as u32;
        self.word >>= n;
        self.count -= n;
        value
    }
    fn consumed(&self) -> usize {
        // ceil((pos * 8 - count) / 8), without overflowing the byte cursor.
        self.prior + self.pos - usize::from(self.count / 8)
    }
    fn align(&mut self) {
        let n = self.count % 8;
        self.word >>= n;
        self.count -= n;
    }
}

// A small primary table handles common codes. Canonical arrays handle longer
// codes without a 128 KiB lookup allocation per dynamic block.
struct Huffman<const SYMBOLS: usize = 288> {
    fast: [u16; 512],
    counts: [u16; 16],
    first: [u16; 16],
    offsets: [u16; 16],
    symbols: [u16; SYMBOLS],
    max: u8,
}
impl<const SYMBOLS: usize> Huffman<SYMBOLS> {
    fn new(lengths: &[u8], complete: bool, allow_empty: bool) -> Result<Self, Error> {
        Self::build(lengths, complete, allow_empty).map_err(Error::Invalid)
    }
    const fn build(
        lengths: &[u8],
        complete: bool,
        allow_empty: bool,
    ) -> Result<Self, &'static str> {
        let mut h = Self {
            fast: [0; 512],
            counts: [0; 16],
            first: [0; 16],
            offsets: [0; 16],
            symbols: [0; SYMBOLS],
            max: 0,
        };
        let mut symbol = 0;
        while symbol < lengths.len() {
            let n = lengths[symbol];
            if n > 15 {
                return Err("Huffman code length");
            }
            if n != 0 {
                h.counts[n as usize] += 1;
                if n > h.max {
                    h.max = n;
                }
            }
            symbol += 1;
        }
        if h.max == 0 {
            return if allow_empty {
                Ok(h)
            } else {
                Err("empty Huffman tree")
            };
        }
        let mut left = 1i32;
        let mut code = 0;
        let mut offset = 0;
        let mut n = 1;
        while n < 16 {
            left = left * 2 - h.counts[n] as i32;
            if left < 0 {
                return Err("oversubscribed Huffman tree");
            }
            code = (code + h.counts[n - 1]) << 1;
            h.first[n] = code;
            h.offsets[n] = offset;
            offset += h.counts[n];
            n += 1;
        }
        if left != 0 && (complete || h.max != 1) {
            return Err("incomplete Huffman tree");
        }
        let mut next = h.first;
        // Short trees depend only on the low `max` bits. Build that prefix
        // once, then duplicate it across the nine-bit primary lookup.
        let primary = 1usize << if h.max < 9 { h.max } else { 9 };
        let mut symbol = 0;
        while symbol < lengths.len() {
            let n = lengths[symbol];
            if n == 0 {
                symbol += 1;
                continue;
            }
            let n = n as usize;
            let code = next[n];
            h.symbols[(h.offsets[n] + code - h.first[n]) as usize] = symbol as u16;
            if n <= 9 {
                let rev = (code.reverse_bits() >> (16 - n)) as usize;
                let mut index = rev;
                while index < primary {
                    h.fast[index] = ((symbol as u16) << 4) | n as u16;
                    index += 1 << n;
                }
            }
            next[n] += 1;
            symbol += 1;
        }
        let mut initialized = primary;
        while initialized < 512 {
            let (prefix, rest) = h.fast.split_at_mut(initialized);
            rest.split_at_mut(initialized).0.copy_from_slice(prefix);
            initialized *= 2;
        }
        Ok(h)
    }
    // A small primary lookup can inline at symbol call sites; keep the
    // canonical long-code and error paths in a separate function.
    #[inline(always)]
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

    #[inline]
    fn decode_buffered(&self, bits: &mut Bits<'_>) -> Result<usize, Error> {
        debug_assert!(bits.count >= 15);
        let entry = self.fast[(bits.word & 511) as usize];
        let n = (entry & 15) as u8;
        if n != 0 {
            bits.take(n);
            Ok((entry >> 4) as usize)
        } else {
            self.decode_slow(bits)
        }
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

    fn wide_primary(&self) -> [u16; 4096] {
        let mut fast = [0; 4096];
        for prefix in fast.as_chunks_mut::<512>().0 {
            *prefix = self.fast;
        }
        for n in 10..=12 {
            for offset in 0..self.counts[n] {
                let symbol = self.symbols[(self.offsets[n] + offset) as usize];
                let code = self.first[n] + offset;
                let mut index = (code.reverse_bits() >> (16 - n)) as usize;
                while index < fast.len() {
                    fast[index] = (symbol << 4) | n as u16;
                    index += 1 << n;
                }
            }
        }
        fast
    }
}

/// Literal codes short enough to pair, in canonical order.
struct Pairing {
    /// Literals of each code length; canonical order sorts them first.
    literals: [usize; 12],
    shortest: usize,
    /// Twelve-bit entries, of 4096, that decode a pair.
    slots: usize,
}

fn literal_entry(symbol: u16, n: usize) -> u32 {
    LITERAL | (n as u32) << 20 | u32::from(symbol) << 4 | n as u32
}

impl Huffman {
    /// Twelve-bit entries that pair literals. Other symbols keep their
    /// `wide_primary` entry. A literal entry adds `LITERAL` and holds the
    /// literal in bits 4..12 and its code length in bits 0..4 and 20..24.
    /// Where the code leaves room for a following literal's code, the entry
    /// also decodes that literal: it adds `PAIR` instead, bits 0..4 hold both
    /// lengths and bits 12..20 the second literal.
    ///
    /// These are the entries for codes of at most nine bits, which repeat
    /// across the table. `pair_primary` adds the rest to the repetitions.
    fn pair_prefix(&self, pairing: &Pairing) -> [u32; 512] {
        let mut prefix = [0; 512];
        for (prefix, &entry) in prefix.iter_mut().zip(&self.fast) {
            let literal = 0u32.wrapping_sub(u32::from((1..4096).contains(&entry)));
            *prefix = u32::from(entry) | (LITERAL | u32::from(entry & 15) << 20) & literal;
        }
        self.pairs(pairing, 2..=9, |code, total, entry| {
            fill(&mut prefix, code, total, entry)
        });
        prefix
    }

    fn pair_primary(&self, pairing: &Pairing, wide: &mut [[u32; 512]; 8]) {
        let wide = wide.as_flattened_mut();
        for n in 10..=12 {
            for offset in 0..self.counts[n] {
                let symbol = self.symbols[(self.offsets[n] + offset) as usize];
                let code = (self.first[n] + offset).reverse_bits() >> (16 - n);
                let entry = if symbol < 256 {
                    literal_entry(symbol, n)
                } else {
                    u32::from(symbol) << 4 | n as u32
                };
                fill(wide, code, n, entry);
            }
        }
        self.pairs(pairing, 10..=12, |code, total, entry| {
            fill(wide, code, total, entry)
        });
    }

    fn pairing(&self) -> Pairing {
        let mut literals = [0; 12];
        for (n, literals) in literals.iter_mut().enumerate().skip(1) {
            let start = self.offsets[n] as usize;
            let symbols = &self.symbols[start..start + self.counts[n] as usize];
            *literals = symbols.partition_point(|&symbol| symbol < 256);
        }
        // Both codes of a pair are at least the shortest literal code long.
        let shortest = (1..12).find(|&n| literals[n] != 0).unwrap_or(12);
        let mut slots = 0;
        for n in shortest..=12 - shortest {
            for m in shortest..=12 - n {
                slots += (literals[n] * literals[m]) << (12 - n - m);
            }
        }
        Pairing {
            literals,
            shortest,
            slots,
        }
    }

    /// Each pair of literal codes with a combined length in `totals`.
    fn pairs(
        &self,
        pairing: &Pairing,
        totals: std::ops::RangeInclusive<usize>,
        mut f: impl FnMut(u16, usize, u32),
    ) {
        let (literals, shortest) = (&pairing.literals, pairing.shortest);
        let mut reversed = [0; 288];
        for n in shortest..=12 - shortest {
            let start = self.offsets[n] as usize;
            for (offset, code) in reversed[start..start + literals[n]].iter_mut().enumerate() {
                *code = (self.first[n] + offset as u16).reverse_bits() >> (16 - n);
            }
        }
        for total in (*totals.start()).max(2 * shortest)..=*totals.end() {
            for n in shortest..=total - shortest {
                let (first, second) = (self.offsets[n] as usize, self.offsets[total - n] as usize);
                for a in first..first + literals[n] {
                    let entry =
                        (literal_entry(self.symbols[a], n) ^ LITERAL | PAIR) + (total - n) as u32;
                    for b in second..second + literals[total - n] {
                        let code = reversed[a] | reversed[b] << n;
                        f(code, total, entry | u32::from(self.symbols[b]) << 12);
                    }
                }
            }
        }
    }
}

/// Write `entry` wherever the `n` bits of `code` begin an index.
fn fill(table: &mut [u32], code: u16, n: usize, entry: u32) {
    let mut index = usize::from(code);
    while index < table.len() {
        table[index] = entry;
        index += 1 << n;
    }
}

impl Huffman<32> {
    /// Pack code length, extra-bit count and distance base in a single lookup.
    /// Longer primary misses retain the canonical decoder. Reserved codes have
    /// a zero base, which no valid distance uses. Repeat the shortest prefix to
    /// avoid translating all 512 entries for common small distance trees.
    fn distance_primary(&self) -> [u32; 512] {
        let mut distance_fast = [0u32; 512];
        let prefix = 1usize << self.max.min(9);
        for (index, value) in distance_fast[..prefix].iter_mut().enumerate() {
            let entry = self.fast[index];
            let symbol = usize::from(entry >> 4);
            *value = if entry == 0 {
                0
            } else if symbol >= 30 {
                u32::from(entry & 15)
            } else {
                ((DIST_BASE[symbol] as u32) << 8)
                    | (u32::from(DIST_EXTRA[symbol]) << 4)
                    | u32::from(entry & 15)
            };
        }
        let mut initialized = prefix;
        while initialized < 512 {
            let (prefix, rest) = distance_fast.split_at_mut(initialized);
            rest[..initialized].copy_from_slice(prefix);
            initialized *= 2;
        }
        distance_fast
    }
}

const fn fixed_lengths() -> [u8; 288] {
    let mut lengths = [8; 288];
    let mut n = 144;
    while n < 280 {
        lengths[n] = if n < 256 { 9 } else { 7 };
        n += 1;
    }
    lengths
}

const FIXED: (Huffman, Huffman<32>) = {
    let literals = match Huffman::<288>::build(&fixed_lengths(), false, false) {
        Ok(tree) => tree,
        Err(_) => panic!("invalid fixed literal tree"),
    };
    let distances = match Huffman::<32>::build(&[5; 32], false, false) {
        Ok(tree) => tree,
        Err(_) => panic!("invalid fixed distance tree"),
    };
    (literals, distances)
};

/// The fixed codes' sized-inflate lookups, built once on first use.
static FIXED_PRIMARY: std::sync::LazyLock<([u16; 4096], [u32; 512])> =
    std::sync::LazyLock::new(|| (FIXED.0.wide_primary(), FIXED.1.distance_primary()));

/// A dynamic block pairs literals where its pairing entries, times the
/// compressed bytes left up to `PAIR_INPUT`, reach `PAIR_WORK`: savings grow
/// with both, against a fixed cost to build the entries.
const PAIR_INPUT: usize = 16384;
const PAIR_WORK: usize = 4096 * 1024;

/// Twelve-bit entries of at least `LITERAL` decode `entry >> 30` literals.
const LITERAL: u32 = 1 << 30;
const PAIR: u32 = 2 << 30;

/// A twelve-bit primary table entry: `u16` for `wide_primary`, `u32` for
/// `pair_primary`.
trait Entry: Copy {
    /// The code bits the entry decodes, or 0 for a longer code.
    fn bits(self) -> u8;
    fn literal(self) -> bool;
    /// Store a literal entry's literals at `at`, returning how many. A
    /// surplus byte may follow them, overwritten before it becomes history.
    fn put(self, output: &mut [u8], at: usize) -> usize;
    /// The symbol of an entry that is not a literal.
    fn symbol(self) -> usize;
    /// The equivalent `wide_primary` entry, for its first symbol only.
    fn single(self) -> u16;
}

impl Entry for u16 {
    fn bits(self) -> u8 {
        (self & 15) as u8
    }
    fn literal(self) -> bool {
        self < 4096
    }
    fn put(self, output: &mut [u8], at: usize) -> usize {
        output[at] = (self >> 4) as u8;
        1
    }
    fn symbol(self) -> usize {
        usize::from(self >> 4)
    }
    fn single(self) -> u16 {
        self
    }
}

impl Entry for u32 {
    fn bits(self) -> u8 {
        (self & 15) as u8
    }
    fn literal(self) -> bool {
        self >= LITERAL
    }
    fn put(self, output: &mut [u8], at: usize) -> usize {
        output[at..at + 2].copy_from_slice(&((self >> 4) as u16).to_le_bytes());
        (self >> 30) as usize
    }
    fn symbol(self) -> usize {
        (self >> 4) as usize
    }
    fn single(self) -> u16 {
        if self.literal() {
            (self & 0xff0 | self >> 20 & 15) as u16
        } else {
            self as u16
        }
    }
}

fn dynamic(bits: &mut Bits<'_>) -> Result<(Huffman, Huffman<32>), Error> {
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
    let tree = Huffman::<19>::new(&lengths, true, false)?;
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

/// Grow unsized output toward `limit` to at least `needed` bytes, doubling so
/// that repeated growth stays linear.
fn grow(output: &mut Vec<u8>, needed: usize, limit: usize) -> Result<(), Error> {
    let len = output.len();
    let target = (len * 2).max(len + 4096).max(needed).min(limit);
    output
        .try_reserve_exact(target - len)
        .map_err(|_| Error::Allocation)?;
    output.resize(target, 0);
    Ok(())
}

/// Output for the sized inflater: an exact slice, or a vector that grows
/// toward its limit.
trait Output {
    fn slice(&mut self) -> &mut [u8];
    /// Grow to hold any token at `pos` below `limit`, returning whether it grew.
    fn room(&mut self, pos: usize, limit: usize) -> Result<bool, Error>;
}
impl Output for &mut [u8] {
    #[inline(always)]
    fn slice(&mut self) -> &mut [u8] {
        self
    }
    #[inline(always)]
    fn room(&mut self, _: usize, _: usize) -> Result<bool, Error> {
        Ok(false)
    }
}
impl Output for &mut Vec<u8> {
    #[inline(always)]
    fn slice(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
    fn room(&mut self, pos: usize, limit: usize) -> Result<bool, Error> {
        if self.len() - pos >= 258 || self.len() == limit {
            return Ok(false);
        }
        grow(self, pos + 258, limit)?;
        Ok(true)
    }
}

// An image's filtered size is known before inflation. Using a slice and a
// cursor removes Vec growth and length updates from the symbol loop.
#[inline(always)]
fn sized_length(bits: &mut Bits<'_>, index: usize, buffered: bool) -> Result<usize, Error> {
    match index {
        0..=7 => Ok(index + 3),
        28 => Ok(258),
        _ if buffered => Ok(LENGTH_BASE[index] + bits.take(LENGTH_EXTRA[index]) as usize),
        _ => Ok(LENGTH_BASE[index] + bits.read(LENGTH_EXTRA[index])? as usize),
    }
}

// A full refill holds the maximum 45-bit match token. Keep the accumulator
// local while input and output have enough room for every legal token.
#[inline(always)]
fn inflate_sized_fast<E: Entry>(
    bits: &mut Bits<'_>,
    output: &mut [u8],
    pos: &mut usize,
    dist: &Huffman<32>,
    fast: &[E; 4096],
    distance_fast: &[u32; 512],
    window: usize,
) -> Result<bool, Error> {
    if bits.input.len() < 8 || output.len() < 258 {
        return Ok(false);
    }
    // A local copy stays in registers; byte stores could alias `bits`.
    let input = bits.input;
    let input_end = input.len() - 8;
    let output_end = output.len() - 258;
    let mut cursor = bits.pos;
    let mut word = bits.word;
    let mut count = bits.count;
    let mut produced = *pos;
    let mut ended = false;
    if cursor > input_end || produced > output_end {
        return Ok(false);
    }
    debug_assert!(count < 64);
    let bytes = u64::from_le_bytes(input[cursor..cursor + 8].try_into().unwrap());
    word |= bytes << count;
    cursor += usize::from((63 - count) / 8);
    count |= 56;
    let mut entry = fast[(word & 4095) as usize];
    'decode: loop {
        'token: {
            let n = entry.bits();
            if n == 0 {
                break 'decode;
            }
            word >>= n;
            count -= n;
            if entry.literal() {
                produced += entry.put(output, produced);
                break 'token;
            }
            let symbol = entry.symbol();
            let index = symbol.wrapping_sub(257);
            if index > 28 {
                if symbol == 256 {
                    ended = true;
                    break 'decode;
                }
                return Err(Error::Invalid("reserved literal symbol"));
            }
            let len = match index {
                0..=7 => index + 3,
                28 => 258,
                _ => {
                    let n = LENGTH_EXTRA[index];
                    let extra = (word & ((1u64 << n) - 1)) as usize;
                    word >>= n;
                    count -= n;
                    LENGTH_BASE[index] + extra
                }
            };
            let entry = distance_fast[(word & 511) as usize];
            let n = (entry & 15) as u8;
            let distance = if n != 0 {
                let extra = ((entry >> 4) & 15) as u8;
                let distance =
                    (entry >> 8) as usize + ((word >> n) & ((1u64 << extra) - 1)) as usize;
                word >>= n + extra;
                count -= n + extra;
                if entry >> 8 == 0 {
                    return Err(Error::Invalid("reserved distance symbol"));
                }
                distance
            } else {
                // The accumulator already holds every possible distance field.
                bits.word = word;
                bits.count = count;
                bits.pos = cursor;
                let symbol = dist.decode_slow(bits)?;
                word = bits.word;
                count = bits.count;
                if symbol >= 30 {
                    return Err(Error::Invalid("reserved distance symbol"));
                }
                let n = DIST_EXTRA[symbol];
                let distance = DIST_BASE[symbol] + (word & ((1u64 << n) - 1)) as usize;
                word >>= n;
                count -= n;
                distance
            };
            if distance > produced || distance > window {
                return Err(Error::Invalid("distance outside history window"));
            }
            let start = produced - distance;
            if len <= 8 && distance >= 8 {
                output.copy_within(start..start + 8, produced);
            } else if len == 3 {
                let first = output[start];
                let second = if distance > 1 {
                    output[start + 1]
                } else {
                    first
                };
                let third = if distance > 2 {
                    output[start + 2]
                } else {
                    first
                };
                output[produced..produced + 3].copy_from_slice(&[first, second, third]);
            } else if len <= 16 && distance >= 8 {
                output.copy_within(start..start + 8, produced);
                let next = produced + 8 - distance;
                output.copy_within(next..next + 8, produced + 8);
            } else if distance == 1 {
                let byte = output[start];
                output[produced..produced + len].fill(byte);
            } else {
                let mut left = len;
                while left != 0 {
                    let n = (produced - start).min(left);
                    output.copy_within(start..start + n, produced);
                    produced += n;
                    left -= n;
                }
                break 'token;
            }
            produced += len;
        }
        // A full load supplies 64 physical bits, even when only 56..63 are
        // accounted. A token consumes at most 45 bits (12 for a literal),
        // leaving at least nineteen valid bits here.
        // Load the next primary entry before refilling the accumulator so
        // the two memory reads can proceed independently. The lookup itself
        // consumes no bits and is discarded if the boundary path is needed.
        let next = fast[(word & 4095) as usize];
        if cursor > input_end || produced > output_end {
            break 'decode;
        }
        debug_assert!(count < 64);
        let bytes = u64::from_le_bytes(input[cursor..cursor + 8].try_into().unwrap());
        word |= bytes << count;
        cursor += usize::from((63 - count) / 8);
        count |= 56;
        entry = next;
    }
    bits.pos = cursor;
    bits.word = word;
    bits.count = count;
    *pos = produced;
    Ok(ended)
}

/// Without `FAST`, every symbol takes the slow path, and `written` is exact
/// even when a symbol fails. With it, a failure leaves `written` behind.
#[inline(always)]
fn inflate_sized_codes<O: Output, const FAST: bool>(
    bits: &mut Bits<'_>,
    buffer: O,
    written: &mut usize,
    fixed_codes: bool,
    window: usize,
    limit: usize,
) -> Result<(), Error> {
    if fixed_codes {
        let (fast, distance_fast) = &*FIXED_PRIMARY;
        let tables = (&FIXED.0, &FIXED.1, fast, distance_fast);
        return inflate_symbols::<O, _, FAST>(bits, buffer, written, tables, window, limit);
    }
    let (lit, dist) = dynamic(bits)?;
    let distance_fast = dist.distance_primary();
    let input = (bits.input.len() - bits.pos).min(PAIR_INPUT);
    let pairing = (input * 4096 >= PAIR_WORK)
        .then(|| lit.pairing())
        .filter(|pairing| pairing.slots * input >= PAIR_WORK);
    let Some(pairing) = pairing else {
        let tables = (&lit, &dist, &lit.wide_primary(), &distance_fast);
        return inflate_symbols::<O, _, FAST>(bits, buffer, written, tables, window, limit);
    };
    // Built in place: a returned table would be copied.
    let mut primary = [lit.pair_prefix(&pairing); 8];
    lit.pair_primary(&pairing, &mut primary);
    let fast = primary.as_flattened().try_into().unwrap();
    let tables = (&lit, &dist, fast, &distance_fast);
    inflate_symbols::<O, u32, FAST>(bits, buffer, written, tables, window, limit)
}

#[inline(always)]
fn inflate_symbols<O: Output, E: Entry, const FAST: bool>(
    bits: &mut Bits<'_>,
    mut buffer: O,
    written: &mut usize,
    (lit, dist, fast, distance_fast): (&Huffman, &Huffman<32>, &[E; 4096], &[u32; 512]),
    window: usize,
    limit: usize,
) -> Result<(), Error> {
    let mut pos = *written;
    loop {
        let output = buffer.slice();
        if FAST && inflate_sized_fast(bits, output, &mut pos, dist, fast, distance_fast, window)? {
            break;
        }
        // Unsized output grows toward its limit, leaving the fast loop room
        // for any token.
        if buffer.room(pos, limit)? {
            continue;
        }
        if !FAST {
            *written = pos;
        }
        let output = buffer.slice();
        bits.fill_wide();
        bits.fill_huffman(12);
        let entry = fast[(bits.word & 4095) as usize].single();
        let n = (entry & 15) as u8;
        if bits.count >= 48 && output.len() - pos >= 4 && (1..4096).contains(&entry) {
            // Look ahead before committing stores, keeping table loads ahead
            // of the branches that distinguish literals from matches/EOB.
            let second = fast[((bits.word >> n) & 4095) as usize].single();
            let n2 = (second & 15) as u8;
            let third = fast[((bits.word >> (n + n2)) & 4095) as usize].single();
            let n3 = (third & 15) as u8;
            let fourth = fast[((bits.word >> (n + n2 + n3)) & 4095) as usize].single();
            let (count, consumed) = if !(1..4096).contains(&second) {
                (1, n)
            } else if !(1..4096).contains(&third) {
                (2, n + n2)
            } else if !(1..4096).contains(&fourth) {
                (3, n + n2 + n3)
            } else {
                (4, n + n2 + n3 + (fourth & 15) as u8)
            };
            // As for short matches below, surplus initialized bytes are not
            // produced history and are overwritten before they can be read.
            output[pos..pos + 4].copy_from_slice(&[
                (entry >> 4) as u8,
                (second >> 4) as u8,
                (third >> 4) as u8,
                (fourth >> 4) as u8,
            ]);
            pos += count;
            bits.take(consumed);
            continue;
        }
        let symbol = if n != 0 && n <= bits.count {
            bits.word >>= n;
            bits.count -= n;
            (entry >> 4) as usize
        } else {
            lit.decode_slow(bits)?
        };
        match symbol {
            literal @ 0..=255 => {
                let remaining = output.get_mut(pos..).ok_or(Error::LimitExceeded)?;
                let (first, rest) = remaining.split_first_mut().ok_or(Error::LimitExceeded)?;
                *first = literal as u8;
                pos += 1;
                let mut word = bits.word;
                let mut count = bits.count;
                for byte in rest {
                    if count < 12 {
                        break;
                    }
                    let entry = fast[(word & 4095) as usize].single();
                    if !(1..4096).contains(&entry) {
                        break;
                    }
                    let n = (entry & 15) as u8;
                    *byte = (entry >> 4) as u8;
                    word >>= n;
                    count -= n;
                    pos += 1;
                }
                bits.word = word;
                bits.count = count;
            }
            256 => break,
            symbol @ 257..=285 => {
                let index = symbol - 257;
                let (len, distance) = if bits.count >= 33 {
                    // At most 5 length, 15 distance-code and 13 extra bits.
                    // The wide refill normally makes all of them available.
                    let len = sized_length(bits, index, true)?;
                    let symbol = dist.decode_buffered(bits)?;
                    if symbol >= 30 {
                        return Err(Error::Invalid("reserved distance symbol"));
                    }
                    (
                        len,
                        DIST_BASE[symbol] + bits.take(DIST_EXTRA[symbol]) as usize,
                    )
                } else {
                    let len = sized_length(bits, index, false)?;
                    let symbol = dist.decode(bits)?;
                    if symbol >= 30 {
                        return Err(Error::Invalid("reserved distance symbol"));
                    }
                    (
                        len,
                        DIST_BASE[symbol] + bits.read(DIST_EXTRA[symbol])? as usize,
                    )
                };
                if distance > pos || distance > window {
                    return Err(Error::Invalid("distance outside history window"));
                }
                if len > output.len() - pos {
                    return Err(Error::LimitExceeded);
                }
                let start = pos - distance;
                if distance == 1 {
                    let byte = output[start];
                    output[pos..pos + len].fill(byte);
                } else if len <= 8 && distance >= 8 && output.len() - pos >= 8 {
                    // Extra bytes are initialized but remain outside produced
                    // history. Every future sample overwrites them before use.
                    output.copy_within(start..start + 8, pos);
                } else if len <= 16 && distance >= 8 && output.len() - pos >= 16 {
                    output.copy_within(start..start + 8, pos);
                    // The next word can refer to the eight bytes just written.
                    // Surplus bytes stay outside produced history.
                    let next = pos + 8 - distance;
                    output.copy_within(next..next + 8, pos + 8);
                } else {
                    let mut left = len;
                    while left != 0 {
                        let count = (pos - start).min(left);
                        output.copy_within(start..start + count, pos);
                        pos += count;
                        left -= count;
                    }
                    continue;
                }
                pos += len;
            }
            _ => return Err(Error::Invalid("reserved literal symbol")),
        }
    }
    *written = pos;
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

// Optional metadata can recover from malformed streams. Charge output already
// produced even when inflation fails, so a later chunk cannot reuse that budget.
pub(crate) fn decompress_metadata(input: &[u8], remaining: &mut usize) -> Result<Vec<u8>, Error> {
    if input.len() < 6 {
        return Err(Error::Truncated);
    }
    let mut output = Vec::new();
    let result = decompress_bits_into::<true>(Bits::new(input), *remaining, false, &mut output);
    *remaining -= output.len();
    if result? != input.len() {
        return Err(Error::Invalid("trailing zlib data"));
    }
    Ok(output)
}

/// Decode the first zlib stream and return the number of input bytes consumed.
pub fn decompress_prefix(input: &[u8], limit: usize) -> Result<(Vec<u8>, usize), Error> {
    decompress_impl(input, limit, false)
}

// PNG framing has already checked the exact filtered size against its limits.
// Reserve it once rather than repeatedly growing/copying the image buffer.
pub(crate) fn decompress_prefix_sized(
    input: &[u8],
    size: usize,
) -> Result<(Vec<u8>, usize), Error> {
    decompress_impl(input, size, true)
}

pub(crate) fn decompress_png_chunks(
    input: &[u8],
    payload_len: usize,
    size: usize,
) -> Result<Vec<u8>, Error> {
    if payload_len < 6 {
        return Err(Error::Truncated);
    }
    decompress_bits::<true>(Bits::png_chunks(input), size, true).map(|(output, _)| output)
}

fn decompress_impl(input: &[u8], limit: usize, sized: bool) -> Result<(Vec<u8>, usize), Error> {
    if input.len() < 6 {
        return Err(Error::Truncated);
    }
    decompress_bits::<true>(Bits::new(input), limit, sized)
}

fn decompress_bits<const DISPATCH: bool>(
    bits: Bits<'_>,
    limit: usize,
    sized: bool,
) -> Result<(Vec<u8>, usize), Error> {
    let mut output = Vec::new();
    let used = decompress_bits_into::<DISPATCH>(bits, limit, sized, &mut output)?;
    Ok((output, used))
}

/// `len` zero bytes with exact capacity, from a zeroed allocation. Fresh
/// pages are already zero, so this skips the pass `resize` would make.
#[allow(unsafe_code)]
fn zeroed(len: usize) -> Result<Vec<u8>, Error> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let layout = std::alloc::Layout::array::<u8>(len).map_err(|_| Error::Allocation)?;
    // SAFETY: `layout` has a nonzero size.
    let bytes = unsafe { std::alloc::alloc_zeroed(layout) };
    if bytes.is_null() {
        return Err(Error::Allocation);
    }
    // SAFETY: the global allocator returned `bytes` for `layout`: `len`
    // bytes at alignment 1, all initialized to zero.
    Ok(unsafe { Vec::from_raw_parts(bytes, len, len) })
}

fn decompress_bits_into<const DISPATCH: bool>(
    mut bits: Bits<'_>,
    limit: usize,
    sized: bool,
    output: &mut Vec<u8>,
) -> Result<usize, Error> {
    let cmf = bits.read(8)? as u8;
    let flags = bits.read(8)? as u8;
    if cmf & 15 != 8 || cmf >> 4 > 7 || (u16::from(cmf) * 256 + u16::from(flags)) % 31 != 0 {
        return Err(Error::Invalid("zlib header"));
    }
    if flags & 32 != 0 {
        return Err(Error::Invalid("zlib dictionary not permitted"));
    }
    let window = 1usize << ((cmf >> 4) + 8);
    if sized {
        // Coded blocks write into an initialized slice. A zeroed allocation
        // provides it without a pass over fresh pages, unless the first block
        // is stored: stored blocks append, often never needing the slice.
        bits.fill(3);
        if bits.count >= 3 && bits.word & 6 == 0 {
            output
                .try_reserve_exact(limit)
                .map_err(|_| Error::Allocation)?;
        } else {
            *output = zeroed(limit)?;
        }
    }
    let mut written = 0;
    // Checksum each block's output while it is still cached, rather than
    // rereading the whole output afterwards.
    let mut checksum = Adler32::new();
    let result = inflate_blocks::<DISPATCH>(
        &mut bits,
        limit,
        sized,
        window,
        output,
        &mut written,
        &mut checksum,
    );
    // Every exit, including errors, leaves exactly the produced bytes.
    output.truncate(written);
    result?;
    bits.align();
    if bits.read(32)?.swap_bytes() != checksum.finish() {
        return Err(Error::Checksum);
    }
    Ok(bits.consumed())
}

fn inflate_block<const DISPATCH: bool, O: Output>(
    bits: &mut Bits<'_>,
    output: O,
    written: &mut usize,
    fixed: bool,
    window: usize,
    limit: usize,
) -> Result<(), Error> {
    #[cfg(target_arch = "x86_64")]
    if DISPATCH {
        return x86::inflate(bits, output, written, fixed, window, limit);
    }
    inflate_sized_codes::<O, true>(bits, output, written, fixed, window, limit)
}

#[inline(always)]
fn inflate_blocks<const DISPATCH: bool>(
    bits: &mut Bits<'_>,
    limit: usize,
    sized: bool,
    window: usize,
    output: &mut Vec<u8>,
    written: &mut usize,
    checksum: &mut Adler32,
) -> Result<(), Error> {
    let mut checked = 0;
    // Stored blocks append into the output until a coded block needs an
    // initialized slice: zeroed to its exact size when sized, or grown as
    // needed. The produced prefix remains its history.
    let mut initialized = !output.is_empty();
    loop {
        let last = bits.read(1)? != 0;
        let kind = bits.read(2)?;
        if kind == 0 {
            bits.align();
            let len = bits.read(16)? as usize;
            if len ^ bits.read(16)? as usize != 65535 {
                return Err(Error::Invalid("stored block length"));
            }
            if len > limit.saturating_sub(*written) {
                return Err(Error::LimitExceeded);
            }
            if initialized {
                if output.len() - *written < len {
                    grow(output, *written + len, limit)?;
                }
            } else if !sized {
                // A nonfinal stored block can amortize the next payload's growth.
                let additional = if last {
                    len
                } else {
                    len.saturating_mul(2)
                        .min(limit.saturating_sub(output.len()))
                };
                reserve_output(output, additional, limit)?;
            }
            // Drain bytes prefetched by a preceding Huffman block, then copy
            // the byte-aligned payload directly without bit-reader dispatch.
            let mut left = len;
            while left != 0 && bits.count >= 8 {
                let byte = bits.read(8)? as u8;
                if initialized {
                    output[*written] = byte;
                } else {
                    output.push(byte);
                }
                *written += 1;
                left -= 1;
            }
            if left != 0 {
                // Direct copying consumes any physically prefetched bytes that
                // are no longer accounted in word. Discard their cached bits
                // before a later refill combines them with subsequent input.
                debug_assert_eq!(bits.count, 0);
                bits.word = 0;
            }
            while left != 0 {
                if bits.pos == bits.input.len() && !bits.advance() {
                    return Err(Error::Truncated);
                }
                let count = left.min(bits.input.len() - bits.pos);
                let end = bits.pos + count;
                let bytes = &bits.input[bits.pos..end];
                if initialized {
                    output[*written..*written + count].copy_from_slice(bytes);
                } else {
                    output.extend_from_slice(bytes);
                }
                *written += count;
                bits.pos = end;
                left -= count;
            }
        } else if kind == 1 || kind == 2 {
            if !initialized {
                if sized {
                    output.resize(limit, 0);
                }
                initialized = true;
            }
            if sized {
                inflate_block::<DISPATCH, _>(
                    bits,
                    output.as_mut_slice(),
                    written,
                    kind == 1,
                    window,
                    limit,
                )?;
            } else {
                let (start, produced) = (bits.clone(), *written);
                let result = inflate_block::<DISPATCH, _>(
                    bits,
                    &mut *output,
                    written,
                    kind == 1,
                    window,
                    limit,
                );
                if let Err(error) = result {
                    // Unsized callers charge the length produced before an
                    // error, which the fast loop does not report. Decoding the
                    // block again a symbol at a time stops at the same symbol
                    // with that length.
                    *bits = start;
                    *written = produced;
                    let again = inflate_sized_codes::<_, false>(
                        bits,
                        output,
                        written,
                        kind == 1,
                        window,
                        limit,
                    );
                    return Err(again.err().unwrap_or(error));
                }
            }
        } else {
            return Err(Error::Invalid("reserved block type"));
        }
        checksum.update(&output[checked..*written]);
        checked = *written;
        if last {
            return Ok(());
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Compression {
    Stored,
    #[default]
    Fast,
    /// Zero-run encoding with an analytic residual tree and no match dictionary.
    Rle,
}

// Signed residual magnitudes determine the literal widths. Giving both signs
// of 32..=38 nine bits closes the Kraft sum without an input histogram.
const fn rle_lengths() -> [u8; 286] {
    let mut lengths = [0; 286];
    let mut symbol = 0;
    while symbol < 256 {
        let magnitude = if symbol <= 128 { symbol } else { 256 - symbol };
        lengths[symbol] = match magnitude {
            0 => 2,
            1 => 4,
            2 => 5,
            3 => 6,
            4..=7 => 7,
            8..=15 => 8,
            16..=38 => 9,
            _ => 10,
        };
        symbol += 1;
    }
    lengths[256] = 10;
    symbol = 257;
    while symbol < 285 {
        lengths[symbol] = 8;
        symbol += 1;
    }
    lengths[285] = 5;
    lengths
}

const fn canonical_codes<const N: usize>(lengths: [u8; N]) -> [(u16, u8); N] {
    let mut counts = [0u16; 11];
    let mut symbol = 0;
    while symbol < N {
        let bits = lengths[symbol] as usize;
        if bits != 0 {
            counts[bits] += 1;
        }
        symbol += 1;
    }
    let mut next = [0u16; 11];
    let mut code = 0u16;
    let mut bits = 1;
    while bits <= 10 {
        code = (code + counts[bits - 1]) << 1;
        next[bits] = code;
        bits += 1;
    }
    assert!(code + counts[10] == 1024);
    let mut codes = [(0, 0); N];
    symbol = 0;
    while symbol < N {
        let bits = lengths[symbol];
        if bits != 0 {
            let index = bits as usize;
            codes[symbol] = (next[index].reverse_bits() >> (16 - bits), bits);
            next[index] += 1;
        }
        symbol += 1;
    }
    codes
}

const RLE_LENGTHS: [u8; 286] = rle_lengths();
const RLE_CODES: [(u16, u8); 286] = canonical_codes(RLE_LENGTHS);
const FIXED_CODES: [(u16, u8); 288] = canonical_codes(fixed_lengths());
/// Each match length's fixed code followed by its extra bits, at `length - 3`.
const FIXED_LENGTH_TOKENS: [(u16, u8); 256] = {
    let mut tokens = [(0, 0); 256];
    let mut length = 3;
    while length <= 258 {
        let index = length_code(length);
        let (code, bits) = FIXED_CODES[257 + index];
        let extra = (length - LENGTH_BASE[index]) as u16;
        tokens[length - 3] = (code | extra << bits, bits + LENGTH_EXTRA[index]);
        length += 1;
    }
    tokens
};
/// Bit-reversed five-bit fixed distance codes.
const FIXED_DISTANCE_CODES: [(u16, u8); 32] = canonical_codes([5; 32]);

struct RleHeader {
    words: [u32; 11],
    bits: usize,
}
impl RleHeader {
    const fn write(&mut self, value: u32, count: u8) {
        let mut bit = 0;
        while bit < count {
            self.words[self.bits / 32] |= ((value >> bit) & 1) << (self.bits % 32);
            self.bits += 1;
            bit += 1;
        }
    }
}

const fn rle_header() -> RleHeader {
    // Values 10 and 16 get two bits; eight other used values get four.
    // This complete code-length tree needs 18 entries in DEFLATE order.
    const META_LENGTHS: [u8; 19] = [0, 4, 4, 0, 4, 4, 4, 4, 4, 4, 2, 0, 0, 0, 0, 0, 2, 0, 0];
    const META_CODES: [(u16, u8); 19] = canonical_codes(META_LENGTHS);
    const ORDER: [usize; 18] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1];
    let mut header = RleHeader {
        words: [0; 11],
        bits: 0,
    };
    header.write(29, 5); // 286 literal/length symbols.
    header.write(0, 5); // One distance symbol.
    header.write(14, 4); // 18 code-length symbols.
    let mut at = 0;
    while at < ORDER.len() {
        header.write(META_LENGTHS[ORDER[at]] as u32, 3);
        at += 1;
    }
    let mut lengths = [0u8; 287];
    at = 0;
    while at < 286 {
        lengths[at] = RLE_LENGTHS[at];
        at += 1;
    }
    lengths[286] = 1; // Distance one has the sole one-bit code, zero.
    at = 0;
    while at < lengths.len() {
        let value = lengths[at];
        let mut end = at + 1;
        while end < lengths.len() && lengths[end] == value {
            end += 1;
        }
        let (code, bits) = META_CODES[value as usize];
        header.write(code as u32, bits);
        let mut remaining = end - at - 1;
        while remaining >= 3 {
            let repeat = if remaining > 6 { 6 } else { remaining };
            let (repeat_code, repeat_bits) = META_CODES[16];
            header.write(repeat_code as u32, repeat_bits);
            header.write((repeat - 3) as u32, 2);
            remaining -= repeat;
        }
        while remaining != 0 {
            header.write(code as u32, bits);
            remaining -= 1;
        }
        at = end;
    }
    header
}

// Computed at compile time: 332 header bits after the three block-type bits.
// Eleven words also bound future header growth within the encoder's slack.
const RLE_HEADER: RleHeader = rle_header();

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
    /// Append up to 32 bits, least significant first.
    #[inline]
    fn write(&mut self, value: u32, n: u8) {
        self.put(self.word | u64::from(value) << self.count, self.count + n);
    }
    /// Emit the complete bytes of `count` (< 64) bits and keep the rest. A
    /// fixed-width store replaces a byte loop; all eight bytes are initialized
    /// before the unused suffix is removed.
    #[inline]
    fn put(&mut self, word: u64, count: u8) {
        let complete = usize::from(count / 8);
        let start = self.bytes.len();
        self.bytes.extend_from_slice(&word.to_le_bytes());
        self.bytes.truncate(start + complete);
        self.word = word >> (complete * 8);
        self.count = count % 8;
    }
    fn literal(&mut self, symbol: usize) {
        let (code, n) = FIXED_CODES[symbol];
        self.write(u32::from(code), n);
    }
    fn align(&mut self) {
        if self.count != 0 {
            self.bytes.push(self.word as u8);
        }
        self.word = 0;
        self.count = 0;
    }
    fn stored_header(&mut self, len: usize, final_block: bool) {
        self.write(u32::from(final_block), 3);
        self.align();
        let len = len as u16;
        self.bytes.extend_from_slice(&len.to_le_bytes());
        self.bytes.extend_from_slice(&(!len).to_le_bytes());
    }
    fn stored(&mut self, input: &[u8], final_block: bool) {
        self.stored_header(input.len(), final_block);
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
        self.finish_compressed(checkpoint, input, final_block);
    }
    #[inline]
    fn finish_compressed(&mut self, checkpoint: Checkpoint, input: &[u8], final_block: bool) {
        let compressed = (self.bytes.len() - checkpoint.bytes) * 8 + self.count as usize
            - checkpoint.count as usize;
        let padding = if final_block {
            (8 - self.count as usize % 8) % 8
        } else {
            0
        };
        let stored = 3 + (8 - (checkpoint.count as usize + 3) % 8) % 8 + 32 + input.len() * 8;
        if stored < compressed + padding {
            self.bytes.truncate(checkpoint.bytes);
            self.word = checkpoint.word;
            self.count = checkpoint.count;
            self.stored(input, final_block);
        }
    }

    fn rle_literals(&mut self, literals: &[u8]) {
        debug_assert!(literals.len() <= 4);
        // Seven pending bits plus four ten-bit literals need at most 47 bits.
        let mut word = self.word;
        let mut count = self.count;
        for &literal in literals {
            let (code, bits) = RLE_CODES[usize::from(literal)];
            word |= u64::from(code) << count;
            count += bits;
        }
        self.put(word, count);
    }

    fn start_rle(&mut self, final_block: bool) -> Checkpoint {
        let checkpoint = Checkpoint {
            bytes: self.bytes.len(),
            word: self.word,
            count: self.count,
        };
        self.write(u32::from(final_block) | 4, 3);
        let whole = RLE_HEADER.bits / 32;
        for &word in &RLE_HEADER.words[..whole] {
            self.write(word, 32);
        }
        let tail = (RLE_HEADER.bits % 32) as u8;
        if tail != 0 {
            self.write(RLE_HEADER.words[whole], tail);
        }
        checkpoint
    }

    fn rle_block(&mut self, input: &[u8], final_block: bool, checksum: &mut Adler32) {
        debug_assert!(input.len() <= matcher::WINDOW);
        let checkpoint = self.start_rle(final_block);
        let mut checksum_start = 0;
        let mut pos = 0;
        while pos < input.len() {
            // Batch literals until a profitable zero prefix appears. Short
            // runs and unaligned prefixes remain literals. The lookahead leaves
            // at least eight bytes for the run/tail handling below.
            while let Some(bytes) = input[pos..].first_chunk::<16>() {
                if bytes[..6] == [0; 6] || bytes[4..10] == [0; 6] {
                    break;
                }
                self.rle_literals(&bytes[..4]);
                self.rle_literals(&bytes[4..8]);
                pos += 8;
            }
            if input[pos] != 0 {
                let count = input[pos..]
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(input.len() - pos);
                for literals in input[pos..pos + count].chunks(4) {
                    self.rle_literals(literals);
                }
                pos += count;
                continue;
            }
            let start = pos;
            let words = input[pos..].as_chunks::<8>().0;
            pos += 8 * words.iter().take_while(|word| **word == [0; 8]).count();
            while pos < input.len() && input[pos] == 0 {
                pos += 1;
            }
            if pos - start >= 128 {
                checksum.update(&input[checksum_start..start]);
                checksum.update_zeros(pos - start);
                checksum_start = pos;
            }
            // Seed each run with a literal, so distance one never refers to
            // data before this block and no retained dictionary is needed.
            self.write(RLE_CODES[0].0 as u32, RLE_CODES[0].1);
            let mut remaining = pos - start - 1;
            // A short match costs at least nine bits; up to four two-bit zeros
            // are cheaper as literals, including after a 258-byte match.
            while remaining >= 5 {
                let length = remaining.min(258);
                let index = length_code(length);
                let (code, bits) = RLE_CODES[257 + index];
                let value = u32::from(code) | (((length - LENGTH_BASE[index]) as u32) << bits);
                // The high zero bit is the sole distance code.
                self.write(value, bits + LENGTH_EXTRA[index] + 1);
                remaining -= length;
            }
            if remaining != 0 {
                self.rle_literals(&input[pos - remaining..pos]);
            }
        }
        // Short runs stay in these spans so bulk checksum code handles noise.
        checksum.update(&input[checksum_start..]);
        self.write(RLE_CODES[256].0 as u32, RLE_CODES[256].1);
        self.finish_compressed(checkpoint, input, final_block);
    }
}

#[inline(always)]
const fn length_code(length: usize) -> usize {
    debug_assert!(3 <= length && length <= 258);
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

/// Fixed-code LZ77 for `data[start..end]`; `data[0]` is stream position `base`.
/// Only positions with four bytes inside `data` are hashed or searched, so a
/// caller bounds lookahead by the slice it supplies.
#[inline(always)]
fn fast_block(
    w: &mut Writer,
    matcher: &mut matcher::Matcher,
    data: &[u8],
    base: usize,
    start: usize,
    end: usize,
    final_block: bool,
) {
    let checkpoint = w.start_fixed(final_block);
    let mut pos = start;
    let mut misses = 0usize;
    while pos < end {
        let mut length = 0;
        let mut distance = 0;
        if pos + 3 < data.len() {
            let mut candidate = matcher.insert(&data[pos..], base + pos);
            let current = &data[pos..end.min(pos + 258)];
            let bits = (w.bytes.len() - checkpoint.bytes) * 8 + w.count as usize
                - checkpoint.count as usize;
            let mut searches = matcher::searches(bits, pos - start);
            // Measure every candidate instead of pre-testing single bytes; only
            // a strictly longer match replaces the earliest longest one.
            while let Some(absolute) = candidate {
                let previous = absolute - base;
                let matched = matcher::prefix(&data[previous..previous + current.len()], current);
                if matched > length {
                    length = matched;
                    distance = pos - previous;
                }
                searches -= 1;
                if length == current.len() || searches == 0 {
                    break;
                }
                candidate = matcher.next(absolute, base + pos);
            }
        }
        if length >= 3 {
            misses = 0;
            // The whole token is at most 8 + 5 + 5 + 13 = 31 bits.
            let (code, n) = FIXED_LENGTH_TOKENS[length - 3];
            let index = distance_code(distance);
            let token = u32::from(code)
                | u32::from(FIXED_DISTANCE_CODES[index].0) << n
                | ((distance - DIST_BASE[index]) as u32) << (n + 5);
            w.write(token, n + 5 + DIST_EXTRA[index]);
            matcher.insert_match(data, base, pos, length, distance);
            pos += length;
        } else {
            // Bound work on sustained incompressible input. Search resumes
            // within at most 15 literal bytes of new structure.
            let count = (1 + (misses >> 8)).min(16).min(end - pos);
            for &byte in &data[pos..pos + count] {
                w.literal(byte as usize);
            }
            misses += count;
            pos += count;
        }
    }
    w.finish_fixed(checkpoint, &data[start..end], final_block);
}

const STORED_BLOCK_SIZE: usize = u16::MAX as usize;

/// A zlib header with room for the worst-case encoding of `len` bytes.
fn zlib_output(bytes: &mut Vec<u8>, len: usize, mode: Compression) -> Result<(), Error> {
    let bound = len
        .checked_add(len.div_ceil(if mode == Compression::Rle { 4 } else { 8 }))
        .and_then(|n| n.checked_add(64))
        .ok_or(Error::LimitExceeded)?;
    bytes.clear();
    bytes
        .try_reserve_exact(bound)
        .map_err(|_| Error::Allocation)?;
    bytes.extend_from_slice(&[0x78, 0x01]);
    Ok(())
}

/// Encode zlib with stored blocks, fixed LZ77 codes, or an analytic zero-run tree.
pub fn compress(input: &[u8], mode: Compression) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    zlib_output(&mut bytes, input.len(), mode)?;
    if mode == Compression::Stored {
        if input.is_empty() {
            bytes.extend_from_slice(&[1, 0, 0, 255, 255]);
        }
        let blocks = input.chunks(STORED_BLOCK_SIZE);
        let count = blocks.len();
        for (n, block) in blocks.enumerate() {
            bytes.push(u8::from(n + 1 == count));
            let len = block.len() as u16;
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(&(!len).to_le_bytes());
            bytes.extend_from_slice(block);
        }
    } else if mode == Compression::Rle {
        let mut writer = Writer {
            bytes,
            word: 0,
            count: 0,
        };
        let mut checksum = Adler32::new();
        for start in (0..input.len().max(1)).step_by(matcher::WINDOW) {
            let end = input.len().min(start + matcher::WINDOW);
            writer.rle_block(&input[start..end], end == input.len(), &mut checksum);
        }
        writer.align();
        writer
            .bytes
            .extend_from_slice(&checksum.finish().to_be_bytes());
        return Ok(writer.bytes);
    } else {
        let mut w = Writer {
            bytes,
            word: 0,
            count: 0,
        };
        let mut matcher = matcher::Matcher::new()?;
        for start in (0..input.len().max(1)).step_by(matcher::WINDOW) {
            let end = input.len().min(start + matcher::WINDOW);
            fast_block(
                &mut w,
                &mut matcher,
                input,
                0,
                start,
                end,
                end == input.len(),
            );
        }
        w.align();
        bytes = w.bytes;
    }
    bytes.extend_from_slice(&adler32(input).to_be_bytes());
    Ok(bytes)
}

/// `compress` fed in pieces. Coded blocks keep the same lookahead and
/// history as `compress`. Stored blocks are appended directly because
/// their sizes and the last block are known before input arrives.
pub(crate) struct Compressor {
    mode: Compression,
    w: Writer,
    matcher: Option<matcher::Matcher>,
    /// Fast history, then the pending block and up to three lookahead bytes.
    window: Vec<u8>,
    history: usize,
    /// Stream position of `window[history]`.
    position: usize,
    stored_remaining: usize,
    stored_block_left: usize,
    checksum: Adler32,
}
impl Compressor {
    const LOOKAHEAD: usize = 3;

    pub(crate) fn new(mode: Compression) -> Self {
        Self {
            mode,
            w: Writer {
                bytes: Vec::new(),
                word: 0,
                count: 0,
            },
            matcher: None,
            window: Vec::new(),
            history: 0,
            position: 0,
            stored_remaining: 0,
            stored_block_left: 0,
            checksum: Adler32::new(),
        }
    }
    /// Start an independent stream with `len` bytes of filtered input.
    pub(crate) fn reset(&mut self, len: usize) -> Result<(), Error> {
        self.window.clear();
        if self.mode != Compression::Stored {
            let history = if self.mode == Compression::Fast {
                matcher::WINDOW
            } else {
                0
            };
            let maximum = history + matcher::WINDOW + Self::LOOKAHEAD;
            reserve(&mut self.window, len.min(maximum))?;
        }
        if self.mode == Compression::Fast {
            if let Some(matcher) = &mut self.matcher {
                matcher.reset();
            } else {
                self.matcher = Some(matcher::Matcher::new()?);
            }
        }
        self.history = 0;
        self.position = 0;
        self.stored_remaining = len;
        self.stored_block_left = 0;
        self.checksum = Adler32::new();
        self.w.word = 0;
        self.w.count = 0;
        zlib_output(&mut self.w.bytes, len, self.mode)
    }
    pub(crate) fn push(&mut self, input: &[u8]) {
        self.append(Some(input), input.len());
    }
    /// `push` for `len` zero bytes, whose checksum needs no byte reads.
    pub(crate) fn push_zeros(&mut self, len: usize) {
        self.append(None, len);
    }
    fn append(&mut self, mut input: Option<&[u8]>, mut len: usize) {
        if self.mode == Compression::Stored {
            self.append_stored(input, len);
            return;
        }
        while len != 0 {
            let full = self.history + matcher::WINDOW + Self::LOOKAHEAD;
            let n = (full - self.window.len()).min(len);
            match &mut input {
                Some(bytes) => {
                    self.window.extend_from_slice(&bytes[..n]);
                    if self.mode == Compression::Fast {
                        self.checksum.update(&bytes[..n]);
                    }
                    *bytes = &bytes[n..];
                }
                None => {
                    self.window.resize(self.window.len() + n, 0);
                    if self.mode == Compression::Fast {
                        self.checksum.update_zeros(n);
                    }
                }
            }
            len -= n;
            if self.window.len() == full {
                self.encode(matcher::WINDOW, false);
            }
        }
    }
    fn append_stored(&mut self, mut input: Option<&[u8]>, mut len: usize) {
        debug_assert!(len <= self.stored_remaining);
        while len != 0 {
            if self.stored_block_left == 0 {
                let block = self.stored_remaining.min(STORED_BLOCK_SIZE);
                self.w.stored_header(block, block == self.stored_remaining);
                self.stored_block_left = block;
            }
            let n = len.min(self.stored_block_left);
            if let Some(bytes) = &mut input {
                let (part, tail) = bytes.split_at(n);
                self.w.bytes.extend_from_slice(part);
                self.checksum.update(part);
                *bytes = tail;
            } else {
                self.w.bytes.resize(self.w.bytes.len() + n, 0);
                self.checksum.update_zeros(n);
            }
            self.stored_remaining -= n;
            self.stored_block_left -= n;
            len -= n;
        }
    }
    fn encode(&mut self, len: usize, final_block: bool) {
        let start = self.history;
        let end = start + len;
        if let Some(matcher) = &mut self.matcher {
            let base = self.position - start;
            fast_block(
                &mut self.w,
                matcher,
                &self.window,
                base,
                start,
                end,
                final_block,
            );
        } else {
            let block = &self.window[start..end];
            self.w.rle_block(block, final_block, &mut self.checksum);
        }
        // Fast keeps the block as history; every mode keeps the lookahead.
        let keep = if self.matcher.is_some() { start } else { end };
        self.window.copy_within(keep.., 0);
        self.window.truncate(self.window.len() - keep);
        self.history = end - keep;
        self.position += len;
    }
    pub(crate) fn finish(&mut self) -> &[u8] {
        if self.mode == Compression::Stored {
            debug_assert_eq!(self.stored_remaining, 0);
            if self.w.bytes.len() == 2 {
                self.w.stored_header(0, true);
            }
        } else {
            // One full block and part of its lookahead can remain.
            if self.window.len() - self.history > matcher::WINDOW {
                self.encode(matcher::WINDOW, false);
            }
            self.encode(self.window.len() - self.history, true);
            self.w.align();
        }
        self.w
            .bytes
            .extend_from_slice(&self.checksum.finish().to_be_bytes());
        &self.w.bytes
    }
}

#[cfg(test)]
#[path = "../tests/support/huffman.rs"]
mod huffman_fixtures;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_rows_and_zero_runs_preserve_block_boundaries() {
        let mut compressor = Compressor::new(Compression::Stored);
        for len in [
            0,
            1,
            STORED_BLOCK_SIZE - 1,
            STORED_BLOCK_SIZE,
            STORED_BLOCK_SIZE + 1,
            2 * STORED_BLOCK_SIZE,
            2 * STORED_BLOCK_SIZE + 1,
        ] {
            let mut input = vec![0; len];
            for (i, byte) in input.iter_mut().enumerate() {
                if i % 4096 >= 1024 {
                    *byte = i.wrapping_mul(193) as u8;
                }
            }
            let expected = compress(&input, Compression::Stored).unwrap();
            for chunk in [127, 4096, STORED_BLOCK_SIZE + 5] {
                compressor.reset(len).unwrap();
                for part in input.chunks(chunk) {
                    let zeros = part.iter().take_while(|&&byte| byte == 0).count();
                    compressor.push_zeros(zeros);
                    compressor.push(&part[zeros..]);
                }
                assert_eq!(compressor.finish(), expected, "length {len}, chunk {chunk}");
            }
        }
    }

    #[test]
    fn coded_stream_resets_match_independent_streams() {
        for mode in [Compression::Fast, Compression::Rle] {
            let mut compressor = Compressor::new(mode);
            for (frame, len) in [137, 98325, 4117, 0, 65537].into_iter().enumerate() {
                let input: Vec<u8> = (0..len)
                    .map(|i| {
                        if i % 19 < 5 {
                            0
                        } else {
                            (i * 37 + frame) as u8
                        }
                    })
                    .collect();
                compressor.reset(len).unwrap();
                for part in input.chunks(273) {
                    compressor.push(part);
                }
                assert_eq!(compressor.finish(), compress(&input, mode).unwrap());
            }
        }
    }

    #[test]
    fn wide_code_lookahead_preserves_prefetched_prefixes() {
        use huffman_fixtures::{Codes, Wire, lengths};
        for depth in 12..=15 {
            let mut lens = [0; 257];
            for n in 1..depth {
                lens[n - 1] = n as u8;
            }
            lens[depth - 1] = depth as u8;
            lens[256] = depth as u8;
            let codes = Codes::new(&lens);
            for offset in 0..8 {
                let mut wire = Wire::default();
                lengths(&mut wire, &lens, &[0]);
                let mut raw = vec![0; offset];
                for _ in 0..offset {
                    codes.symbol(&mut wire, 0);
                }
                // Four 12-bit literals leave only 8..15 accounted bits, but
                // their next 12-bit prefix is already physically present.
                // Longer codes exercise transitions to the canonical path.
                for _ in 0..600 {
                    codes.symbol(&mut wire, depth - 1);
                    raw.push((depth - 1) as u8);
                }
                codes.symbol(&mut wire, 256);
                let bytes = wire.finish(&raw, offset & 1 != 0);
                let mut suffix = bytes.clone();
                suffix.extend_from_slice(b"unconsumed lookahead suffix for wide codes");
                for slack in [0, 258, 516] {
                    for input in [&bytes, &suffix] {
                        assert_eq!(
                            decompress_prefix_sized(input, raw.len() + slack).unwrap(),
                            (raw.clone(), bytes.len()),
                            "depth={depth} offset={offset} slack={slack}"
                        );
                        assert_eq!(
                            decompress_bits::<false>(Bits::new(input), raw.len() + slack, true)
                                .unwrap(),
                            (raw.clone(), bytes.len()),
                            "portable depth={depth} offset={offset} slack={slack}"
                        );
                    }
                }
                for fragment in [1, 7, 8, 9, 31, 1024] {
                    let mut framed = Vec::new();
                    for payload in bytes.chunks(fragment) {
                        framed.extend_from_slice(b"\0\0\0\0IDAT\0\0\0\0");
                        framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                        framed.extend_from_slice(b"IDAT");
                        framed.extend_from_slice(payload);
                        framed.extend_from_slice(&[0; 4]);
                    }
                    assert_eq!(
                        decompress_png_chunks(&framed, bytes.len(), raw.len()).unwrap(),
                        raw,
                        "depth={depth} offset={offset} fragment={fragment}"
                    );
                    assert_eq!(
                        decompress_bits::<false>(Bits::png_chunks(&framed), raw.len(), true)
                            .unwrap(),
                        (raw.clone(), bytes.len()),
                        "portable depth={depth} offset={offset} fragment={fragment}"
                    );
                }
                for missing in 1..=4 {
                    assert!(
                        decompress_prefix_sized(&bytes[..bytes.len() - missing], raw.len())
                            .is_err()
                    );
                    assert!(
                        decompress_bits::<false>(
                            Bits::new(&bytes[..bytes.len() - missing]),
                            raw.len(),
                            true
                        )
                        .is_err()
                    );
                }
            }
        }
    }

    #[test]
    fn word_match_copies_preserve_overlapping_history_and_every_output_tail() {
        use huffman_fixtures::{Codes, Wire};
        let mut lengths = [8; 288];
        lengths[144..256].fill(9);
        lengths[256..280].fill(7);
        let literals = Codes::new(&lengths);
        let distances = Codes::new(&[5; 32]);
        let bases = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33];
        let extras = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4];
        for distance in 1..=40 {
            for length in 3..=16 {
                for tail in (0..=17).chain([255, 256, 257, 258, 259, 511]) {
                    let mut raw: Vec<u8> = (0..64).map(|n| (n * 37 + 11) as u8).collect();
                    let mut wire = Wire::default();
                    wire.stored(&raw, false);
                    wire.put(3, 3); // Final fixed-Huffman block.
                    let (symbol, extra) = match length {
                        3..=10 => (length + 254, None),
                        11..=12 => (265, Some(length - 11)),
                        13..=14 => (266, Some(length - 13)),
                        _ => (267, Some(length - 15)),
                    };
                    literals.symbol(&mut wire, symbol);
                    if let Some(extra) = extra {
                        wire.put(extra as u32, 1);
                    }
                    let code = bases.iter().rposition(|&base| base <= distance).unwrap();
                    distances.symbol(&mut wire, code);
                    wire.put((distance - bases[code]) as u32, extras[code]);
                    for _ in 0..length {
                        raw.push(raw[raw.len() - distance]);
                    }
                    for n in 0..tail {
                        let byte = (103 + n * 19) as u8;
                        literals.symbol(&mut wire, byte as usize);
                        raw.push(byte);
                    }
                    literals.symbol(&mut wire, 256);
                    let bytes = wire.finish(&raw, (distance ^ length ^ tail) & 1 != 0);
                    for actual in [
                        decompress_prefix(&bytes, raw.len()),
                        decompress_prefix_sized(&bytes, raw.len()),
                        decompress_bits::<false>(Bits::new(&bytes), raw.len(), true),
                        decompress_prefix_sized(&bytes, raw.len() + 258),
                        decompress_bits::<false>(Bits::new(&bytes), raw.len() + 258, true),
                    ] {
                        assert_eq!(
                            actual.unwrap(),
                            (raw.clone(), bytes.len()),
                            "distance={distance} length={length} tail={tail}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sized_inflation_matches_independent_huffman_fixtures() {
        for case in huffman_fixtures::all_cases() {
            let limit = case.output.as_ref().map_or(1 << 20, Vec::len);
            let expected = decompress_prefix(&case.bytes, limit);
            assert_eq!(
                decompress_prefix_sized(&case.bytes, limit),
                expected,
                "{}",
                case.name
            );
            if case.bytes.len() >= 6 {
                assert_eq!(
                    decompress_bits::<false>(Bits::new(&case.bytes), limit, true),
                    expected,
                    "portable {}",
                    case.name
                );
            }
            if let Some(raw) = &case.output {
                assert_eq!(
                    expected.unwrap(),
                    (raw.clone(), case.bytes.len()),
                    "{}",
                    case.name
                );
                let mut suffix = case.bytes.clone();
                suffix.extend_from_slice(b"unconsumed sized-inflation suffix");
                assert_eq!(
                    decompress_prefix_sized(&suffix, limit).unwrap(),
                    (raw.clone(), case.bytes.len()),
                    "{}",
                    case.name
                );
                assert_eq!(
                    decompress_bits::<false>(Bits::new(&suffix), limit, true).unwrap(),
                    (raw.clone(), case.bytes.len()),
                    "portable suffix {}",
                    case.name
                );
                for slack in [257, 258, 259, 516] {
                    assert_eq!(
                        decompress_prefix_sized(&suffix, limit + slack).unwrap(),
                        (raw.clone(), case.bytes.len()),
                        "prefetched EOB {} slack={slack}",
                        case.name
                    );
                    assert_eq!(
                        decompress_bits::<false>(Bits::new(&suffix), limit + slack, true).unwrap(),
                        (raw.clone(), case.bytes.len()),
                        "portable prefetched EOB {} slack={slack}",
                        case.name
                    );
                }
                if limit != 0 {
                    assert_eq!(
                        decompress_prefix_sized(&case.bytes, limit - 1),
                        Err(Error::LimitExceeded),
                        "{}",
                        case.name
                    );
                    assert_eq!(
                        decompress_bits::<false>(Bits::new(&case.bytes), limit - 1, true),
                        Err(Error::LimitExceeded),
                        "portable limit {}",
                        case.name
                    );
                }
                for missing in 1..=4.min(case.bytes.len()) {
                    assert!(
                        decompress_prefix_sized(&case.bytes[..case.bytes.len() - missing], limit)
                            .is_err(),
                        "{} missing={missing}",
                        case.name
                    );
                    assert!(
                        decompress_bits::<false>(
                            Bits::new(&case.bytes[..case.bytes.len() - missing]),
                            limit,
                            true
                        )
                        .is_err(),
                        "portable {} missing={missing}",
                        case.name
                    );
                }
            }
        }
    }

    #[test]
    fn idat_boundaries_and_empty_chunks_preserve_huffman_payloads() {
        for case in huffman_fixtures::all_cases() {
            let Some(raw) = case.output else { continue };
            for fragment in [1, 7, 16384] {
                let mut wire = Vec::new();
                for bytes in case.bytes.chunks(fragment) {
                    // Empty IDAT before each fragment, including the header
                    // and Adler trailer; CRCs are checked by the PNG parser.
                    wire.extend_from_slice(b"\0\0\0\0IDAT\0\0\0\0");
                    wire.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
                    wire.extend_from_slice(b"IDAT");
                    wire.extend_from_slice(bytes);
                    wire.extend_from_slice(&[0; 4]);
                }
                assert_eq!(
                    decompress_png_chunks(&wire, case.bytes.len(), raw.len()).unwrap(),
                    raw,
                    "{} fragment={fragment}",
                    case.name
                );
                assert_eq!(
                    decompress_bits::<false>(Bits::png_chunks(&wire), raw.len(), true).unwrap(),
                    (raw.clone(), case.bytes.len()),
                    "portable {} fragment={fragment}",
                    case.name
                );
            }
        }
    }

    #[test]
    fn maximum_match_token_after_four_wide_literals_preserves_boundaries() {
        use huffman_fixtures::{Codes, Wire, lengths};

        // Complete trees: ten successive short leaves plus four 12-bit leaves;
        // fourteen successive short leaves plus two 15-bit distance leaves.
        let mut lit = [0; 286];
        for n in 1..=10 {
            lit[n - 1] = n as u8;
        }
        for symbol in [250, 251, 256, 284] {
            lit[symbol] = 12;
        }
        let mut dist = [0; 30];
        for n in 1..=14 {
            dist[n - 1] = n as u8;
        }
        dist[28] = 15;
        dist[29] = 15;
        let literals = Codes::new(&lit);
        let distances = Codes::new(&dist);

        for ones in [false, true] {
            let mut state = 0x71e2_9b35u32;
            let mut raw: Vec<u8> = (0..32768)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect();
            let mut wire = Wire::default();
            wire.stored(&raw, false);
            lengths(&mut wire, &lit, &dist);
            // Four literals use 48 bits, leaving a physically prefetched next
            // prefix. The following match uses 12 + 5 + 15 + 13 = 45 bits.
            // Each 93-bit group advances five bits; eight cover all alignments.
            for _ in 0..8 {
                for literal in [250, 251, 250, 251] {
                    literals.symbol(&mut wire, literal);
                    raw.push(literal as u8);
                }
                literals.symbol(&mut wire, 284);
                wire.put(31, 5); // Preserve accepted base 227 + 31 = 258.
                distances.symbol(&mut wire, 29);
                wire.put(8191, 13); // Distance 24577 + 8191 = 32768.
                for _ in 0..258 {
                    raw.push(raw[raw.len() - 32768]);
                }
            }
            literals.symbol(&mut wire, 256);
            let bytes = wire.finish(&raw, ones);
            let mut suffix = bytes.clone();
            suffix.extend_from_slice(b"unconsumed maximum-match suffix");
            for slack in [0, 258] {
                for input in [&bytes, &suffix] {
                    for actual in [
                        decompress_prefix_sized(input, raw.len() + slack),
                        decompress_bits::<false>(Bits::new(input), raw.len() + slack, true),
                    ] {
                        assert_eq!(
                            actual.unwrap(),
                            (raw.clone(), bytes.len()),
                            "padding={ones} slack={slack} input={}",
                            input.len()
                        );
                    }
                }
            }
            // Keep the stored history in one IDAT, then fragment the Huffman
            // header, tokens and trailer. Empty chunks also exercise transitions.
            let stored_end = 2 + 5 + 32768; // zlib header + stored header + data.
            for fragment in [1, 7, 31] {
                let mut framed = Vec::new();
                for payload in std::iter::once(&bytes[..stored_end])
                    .chain(bytes[stored_end..].chunks(fragment))
                {
                    framed.extend_from_slice(b"\0\0\0\0IDAT\0\0\0\0");
                    framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                    framed.extend_from_slice(b"IDAT");
                    framed.extend_from_slice(payload);
                    framed.extend_from_slice(&[0; 4]);
                }
                assert_eq!(
                    decompress_png_chunks(&framed, bytes.len(), raw.len()).unwrap(),
                    raw,
                    "padding={ones} fragment={fragment}"
                );
                assert_eq!(
                    decompress_bits::<false>(Bits::png_chunks(&framed), raw.len(), true).unwrap(),
                    (raw.clone(), bytes.len()),
                    "portable padding={ones} fragment={fragment}"
                );
            }
            for actual in [
                decompress_prefix_sized(&bytes, raw.len() - 1),
                decompress_bits::<false>(Bits::new(&bytes), raw.len() - 1, true),
            ] {
                assert_eq!(actual, Err(Error::LimitExceeded), "padding={ones}");
            }
        }
    }

    #[test]
    fn metadata_errors_charge_exactly_the_bytes_produced_before_them() {
        use huffman_fixtures::{Codes, Wire};
        let mut lengths = [8; 288];
        lengths[144..256].fill(9);
        lengths[256..280].fill(7);
        let literals = Codes::new(&lengths);
        let distances = Codes::new(&[5; 32]);
        // The faults land in the symbol-at-a-time path, in the fast loop, and
        // after the unsized output has grown.
        for produced in [0, 1, 257, 300, 4095, 9000] {
            for fault in 0..4 {
                let raw: Vec<u8> = (0..produced).map(|n| (n * 7 + n / 5) as u8).collect();
                let mut wire = Wire::default();
                wire.put(3, 3); // Final fixed-Huffman block.
                for &byte in &raw {
                    literals.symbol(&mut wire, usize::from(byte));
                }
                let (expected, limit) = match fault {
                    0 => {
                        literals.symbol(&mut wire, 257);
                        distances.symbol(&mut wire, 29);
                        wire.put(8191, 13); // Distance 32768, before the output.
                        (Error::Invalid("distance outside history window"), 1 << 20)
                    }
                    1 => {
                        literals.symbol(&mut wire, 257);
                        distances.symbol(&mut wire, 30);
                        (Error::Invalid("reserved distance symbol"), 1 << 20)
                    }
                    2 => {
                        literals.symbol(&mut wire, 286);
                        (Error::Invalid("reserved literal symbol"), 1 << 20)
                    }
                    _ => {
                        literals.symbol(&mut wire, 65);
                        (Error::LimitExceeded, produced)
                    }
                };
                // Trailing symbols keep each fault away from the input end.
                for _ in 0..64 {
                    literals.symbol(&mut wire, 0);
                }
                literals.symbol(&mut wire, 256);
                let bytes = wire.finish(&raw, false);
                let mut remaining = limit;
                assert_eq!(
                    decompress_metadata(&bytes, &mut remaining),
                    Err(expected.clone()),
                    "produced={produced} fault={fault}"
                );
                assert_eq!(
                    limit - remaining,
                    produced,
                    "produced={produced} fault={fault}"
                );
                assert_eq!(decompress_prefix(&bytes, limit), Err(expected));
            }
        }
    }

    #[test]
    fn stored_and_mixed_blocks_preserve_output_limits_and_prefixes() {
        use huffman_fixtures::{Codes, Wire};
        let mut lengths = [8; 288];
        lengths[144..256].fill(9);
        lengths[256..280].fill(7);
        let literals = Codes::new(&lengths);
        let distances = Codes::new(&[5; 32]);
        let tail = [
            13, 29, 47, 193, 211, 229, 0, 255, 7, 31, 63, 127, 128, 1, 2, 3, 4,
        ];
        for stored_len in [0, 1, 7, 8, 31, 258, 65535] {
            let prefix: Vec<u8> = (0..stored_len)
                .map(|n| (n * 73 + n / 13 + 19) as u8)
                .collect();
            for mixed in [false, true] {
                let mut wire = Wire::default();
                let mut raw = prefix.clone();
                wire.stored(&prefix, false);
                if mixed {
                    wire.put(2, 3); // Nonfinal fixed-Huffman block.
                    if raw.len() >= 3 {
                        literals.symbol(&mut wire, 285); // Length 258.
                        distances.symbol(&mut wire, 2); // Distance 3.
                        let seed: [u8; 3] = raw[raw.len() - 3..].try_into().unwrap();
                        raw.extend((0..258).map(|n| seed[n % 3]));
                    }
                    for byte in [0, 144, 255, 31, 1] {
                        literals.symbol(&mut wire, byte as usize);
                        raw.push(byte);
                    }
                    literals.symbol(&mut wire, 256);
                }
                let tail = if stored_len == 0 && !mixed {
                    &[][..]
                } else {
                    &tail[..]
                };
                wire.stored(tail, true);
                raw.extend_from_slice(tail);
                let bytes = wire.finish(&raw, false);
                let mut reference = Vec::new();
                std::io::Read::read_to_end(
                    &mut flate2::read::ZlibDecoder::new(bytes.as_slice()),
                    &mut reference,
                )
                .unwrap();
                assert_eq!(reference, raw, "fixture stored={stored_len} mixed={mixed}");
                let mut with_suffix = bytes.clone();
                with_suffix.extend_from_slice(b"unconsumed stored-block suffix");
                for slack in [0, 1, 258] {
                    let limit = raw.len() + slack;
                    for input in [&bytes, &with_suffix] {
                        for actual in [
                            decompress_prefix_sized(input, limit),
                            decompress_bits::<false>(Bits::new(input), limit, true),
                        ] {
                            assert_eq!(
                                actual.unwrap_or_else(|error| {
                                    panic!(
                                        "stored={stored_len} mixed={mixed} slack={slack}: {error:?}"
                                    )
                                }),
                                (raw.clone(), bytes.len()),
                                "stored={stored_len} mixed={mixed} slack={slack}"
                            );
                        }
                    }
                }
                for fragment in [1, 7, 4096] {
                    let mut framed = Vec::new();
                    for payload in bytes.chunks(fragment) {
                        // Framing CRCs are validated by the PNG parser before
                        // it calls this internal inflater.
                        framed.extend_from_slice(b"\0\0\0\0IDAT\0\0\0\0");
                        framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                        framed.extend_from_slice(b"IDAT");
                        framed.extend_from_slice(payload);
                        framed.extend_from_slice(&[0; 4]);
                    }
                    for slack in [0, 258] {
                        let limit = raw.len() + slack;
                        assert_eq!(
                            decompress_png_chunks(&framed, bytes.len(), limit).unwrap(),
                            raw,
                            "stored={stored_len} mixed={mixed} fragment={fragment} slack={slack}"
                        );
                        assert_eq!(
                            decompress_bits::<false>(Bits::png_chunks(&framed), limit, true)
                                .unwrap(),
                            (raw.clone(), bytes.len())
                        );
                    }
                }
                if !raw.is_empty() {
                    assert_eq!(
                        decompress_prefix_sized(&bytes, raw.len() - 1),
                        Err(Error::LimitExceeded)
                    );
                    assert_eq!(
                        decompress_bits::<false>(Bits::new(&bytes), raw.len() - 1, true),
                        Err(Error::LimitExceeded)
                    );
                }
                for missing in [1, 4, 5] {
                    assert_eq!(
                        decompress_prefix_sized(&bytes[..bytes.len() - missing], raw.len()),
                        Err(Error::Truncated)
                    );
                }
                let mut bad = bytes.clone();
                *bad.last_mut().unwrap() ^= 1;
                assert_eq!(
                    decompress_prefix_sized(&bad, raw.len()),
                    Err(Error::Checksum)
                );
                // Metadata keeps its unsized path and charges produced output
                // even when the complete stream fails its checksum.
                let mut remaining = raw.len() + 9;
                assert_eq!(
                    decompress_metadata(&bad, &mut remaining),
                    Err(Error::Checksum)
                );
                assert_eq!(remaining, 9);
            }
        }
    }

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
                    chunks: &[],
                    prior: 0,
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

    /// Canonical codes of `lengths`, bit-reversed as they appear in a stream.
    fn reversed_codes(lengths: &[u8]) -> Vec<u16> {
        let mut sorted: Vec<_> = (0..lengths.len()).filter(|&s| lengths[s] != 0).collect();
        sorted.sort_by_key(|&s| (lengths[s], s));
        let (mut code, mut previous, mut codes) = (0u32, 0, vec![0; lengths.len()]);
        for symbol in sorted {
            code <<= lengths[symbol] - previous;
            previous = lengths[symbol];
            codes[symbol] = (code.reverse_bits() >> (32 - previous)) as u16;
            code += 1;
        }
        codes
    }

    /// Complete code lengths of at most 15 bits from pseudo-random weights.
    fn random_lengths(state: &mut u32, skew: u32) -> Option<Vec<u8>> {
        let mut nodes: Vec<(u64, Vec<usize>)> = (0..288)
            .filter_map(|symbol| {
                *state ^= *state << 13;
                *state ^= *state >> 17;
                *state ^= *state << 5;
                let weight = u64::from(*state % (1 << skew));
                (weight != 0 || symbol == 256).then(|| (weight + 1, vec![symbol]))
            })
            .collect();
        let mut lengths = vec![0u8; 288];
        while nodes.len() > 1 {
            nodes.sort_by_key(|node| std::cmp::Reverse(node.0));
            let (a, b) = (nodes.pop().unwrap(), nodes.pop().unwrap());
            for &symbol in a.1.iter().chain(&b.1) {
                lengths[symbol] += 1;
            }
            nodes.push((a.0 + b.0, [a.1, b.1].concat()));
        }
        (lengths.iter().all(|&n| n <= 15)).then_some(lengths)
    }

    #[test]
    fn twelve_bit_entries_decode_like_canonical_codes() {
        // Fixed codes; a staircase with literals of every length, which pairs
        // at every combined length and misses beyond twelve bits; short
        // match codes among short literals; one lone code; and random trees.
        let mut trees = vec![fixed_lengths().to_vec()];
        let mut staircase = vec![0; 288];
        for n in 1..15 {
            staircase[n - 1] = n as u8;
        }
        (staircase[14], staircase[256]) = (15, 15);
        trees.push(staircase);
        let mut mixed = vec![0; 288];
        mixed[257] = 1;
        for n in 2..=12 {
            mixed[3 * n] = n as u8;
        }
        mixed[256] = 12;
        trees.push(mixed);
        let mut lone = vec![0; 288];
        lone[65] = 1;
        trees.push(lone);
        let mut state = 0x2545_f491;
        for skew in 1..=12 {
            for _ in 0..4 {
                trees.extend(random_lengths(&mut state, skew));
            }
        }
        assert!(trees.len() > 40);
        for lengths in &trees {
            let tree = Huffman::new(lengths, false, false).unwrap();
            let codes = reversed_codes(lengths);
            let mut first = [None; 4096];
            for (symbol, &n) in lengths
                .iter()
                .enumerate()
                .filter(|&(_, &n)| n != 0 && n <= 12)
            {
                for index in (usize::from(codes[symbol])..4096).step_by(1 << n) {
                    first[index] = Some((symbol as u32, u32::from(n)));
                }
            }
            let singles = tree.wide_primary();
            let pairing = tree.pairing();
            let mut pairs = [tree.pair_prefix(&pairing); 8];
            tree.pair_primary(&pairing, &mut pairs);
            let paired = pairs.as_flattened().iter().filter(|&&entry| entry >= PAIR);
            assert_eq!(paired.count(), pairing.slots, "{lengths:?}");
            for (index, (&single, &entry)) in singles.iter().zip(pairs.as_flattened()).enumerate() {
                let Some((symbol, n)) = first[index] else {
                    assert_eq!((single, entry), (0, 0), "{lengths:?} {index}");
                    continue;
                };
                assert_eq!(u32::from(single), symbol << 4 | n, "{lengths:?} {index}");
                assert_eq!(entry.single(), single, "{lengths:?} {index}");
                let expected = if symbol >= 256 {
                    symbol << 4 | n
                } else {
                    // Bits past the first code with zeros above them find
                    // any second code that fits in twelve bits.
                    match first[index >> n].filter(|&(next, m)| next < 256 && n + m <= 12) {
                        Some((next, m)) => PAIR | next << 12 | symbol << 4 | n << 20 | (n + m),
                        None => LITERAL | symbol << 4 | n << 20 | n,
                    }
                };
                assert_eq!(entry, expected, "{lengths:?} {index}");
            }
        }
    }

    #[test]
    fn paired_literals_decode_through_every_output_and_input_tail() {
        use huffman_fixtures::{Codes, Wire, lengths};
        // Literal k has a (k + 1)-bit code. Drawing it with probability
        // 2^-(k + 1) gives long runs of pairable codes, and enough input that
        // the block pairs them.
        let mut lens = [0; 257];
        for n in 1..15 {
            lens[n - 1] = n as u8;
        }
        (lens[14], lens[256]) = (15, 15);
        let codes = Codes::new(&lens);
        let mut wire = Wire::default();
        lengths(&mut wire, &lens, &[0]);
        let mut state = 0x9e37_79b9u32;
        let mut raw = Vec::new();
        for _ in 0..24_000 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let literal = (state.trailing_zeros() as usize).min(14);
            codes.symbol(&mut wire, literal);
            raw.push(literal as u8);
        }
        codes.symbol(&mut wire, 256);
        let bytes = wire.finish(&raw, false);
        // Enough input follows the block header for the block to pair.
        let tree = Huffman::new(&lens, false, false).unwrap();
        assert!(tree.pairing().slots * (bytes.len() - 100) >= PAIR_WORK);
        assert_eq!(
            decompress_prefix_sized(&bytes, raw.len()).unwrap(),
            (raw.clone(), bytes.len())
        );
        assert_eq!(
            decompress_prefix(&bytes, raw.len()).unwrap(),
            (raw.clone(), bytes.len())
        );
        assert_eq!(
            decompress_bits::<false>(Bits::new(&bytes), raw.len(), true).unwrap(),
            (raw.clone(), bytes.len())
        );
        // Limits at every position near the end, where the slow path decodes
        // pair entries one literal at a time, and charges exact progress.
        for limit in (raw.len() - 600..raw.len()).chain([raw.len() / 2, 1, 0]) {
            assert_eq!(
                decompress_prefix_sized(&bytes, limit),
                Err(Error::LimitExceeded),
                "{limit}"
            );
            assert_eq!(
                decompress_prefix(&bytes, limit),
                Err(Error::LimitExceeded),
                "{limit}"
            );
            let mut remaining = limit;
            assert_eq!(
                decompress_metadata(&bytes, &mut remaining),
                Err(Error::LimitExceeded)
            );
            assert_eq!(remaining, 0, "{limit}");
        }
        for cut in (bytes.len() - 40..bytes.len()).chain([bytes.len() / 2]) {
            let truncated = &bytes[..cut];
            assert!(
                decompress_prefix_sized(truncated, raw.len()).is_err(),
                "{cut}"
            );
            assert!(decompress_prefix(truncated, raw.len()).is_err(), "{cut}");
        }
    }
}
