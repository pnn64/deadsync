//! GIF's little-endian, late-change, 12-bit LZW, implemented locally.
use crate::{Error, reserve};

/// Reusable incremental LZW decoder. The caller must retain the output prefix
/// unchanged until the image ends: dictionary phrases refer to that prefix.
/// Output is bounded before every expansion, including KwKwK and frozen tables.
pub struct Decoder {
    // Output offsets relative to the last CLEAR, packed with 12-bit lengths.
    // The allocation is sized from the output bound and reused across frames.
    dictionary: Vec<u64>,
    origin: usize,
    clear: u16,
    next: u16,
    mask: u16,
    // Zero after EOI. A reset restores the initial code width.
    width: u8,
    // Same layout as a dictionary entry, with zero for an empty history.
    previous: u64,
    bits: u64,
    bit_count: u8,
}
impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}
impl Decoder {
    pub fn new() -> Self {
        Self {
            dictionary: Vec::new(),
            origin: 0,
            clear: 4,
            next: 6,
            mask: 7,
            width: 3,
            previous: 0,
            bits: 0,
            bit_count: 0,
        }
    }
    pub fn reset(&mut self, minimum: u8) -> Result<(), Error> {
        if !(2..=8).contains(&minimum) {
            return Err(Error::Invalid("LZW minimum code size"));
        }
        self.reset_compatible(minimum)
    }
    /// Some GIF writers use a one-bit minimum for monochrome images instead
    /// of the required two. Keep the literal alphabet and control codes intact.
    pub(crate) fn reset_compatible(&mut self, minimum: u8) -> Result<(), Error> {
        if !(1..=8).contains(&minimum) {
            return Err(Error::Invalid("LZW minimum code size"));
        }
        self.clear = 1 << minimum;
        self.clear_table();
        self.bits = 0;
        self.bit_count = 0;
        Ok(())
    }
    fn clear_table(&mut self) {
        self.dictionary.clear();
        self.next = self.clear + 2;
        self.width = self.clear.trailing_zeros() as u8 + 1;
        self.mask = (self.clear << 1) - 1;
        self.previous = 0;
    }
    pub fn ended(&self) -> bool {
        self.width == 0
    }
    pub(crate) fn literal_count(&self) -> usize {
        usize::from(self.clear)
    }
    pub fn update(
        &mut self,
        input: &[u8],
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<(), Error> {
        if let [byte] = input
            && u16::from(self.bit_count) + 8 < u16::from(self.width)
        {
            self.bits |= u64::from(*byte) << self.bit_count;
            self.bit_count += 8;
            return Ok(());
        }
        if self.ended() {
            return Ok(());
        }
        let mut input = input;
        while !input.is_empty() {
            if input.len() >= 4 {
                // A safe unaligned word refill amortizes the byte/width
                // loop. At most 11 bits remain, so 32 more fit in u64.
                self.bits |=
                    u64::from(u32::from_le_bytes(input[..4].try_into().unwrap())) << self.bit_count;
                self.bit_count += 32;
                input = &input[4..];
            } else if let Some((&byte, rest)) = input.split_first() {
                self.bits |= u64::from(byte) << self.bit_count;
                self.bit_count += 8;
                input = rest;
            } else {
                break;
            }
            while self.bit_count >= self.width {
                let code = self.bits as u16 & self.mask;
                self.bits >>= self.width;
                self.bit_count -= self.width;
                let start = output.len();
                let length = if code < self.clear {
                    // Literals need neither control-code checks nor phrase lookup.
                    if start >= limit {
                        return Err(Error::Invalid("LZW output exceeds frame size"));
                    }
                    if start == output.capacity() {
                        reserve(output, 1)?;
                    }
                    output.push(code as u8);
                    1
                } else {
                    if code == self.clear {
                        self.clear_table();
                        continue;
                    }
                    if code == self.clear + 1 {
                        self.width = 0;
                        return Ok(());
                    }
                    let index = usize::from(code - self.clear - 2);
                    let (offset, length) = if let Some(&entry) = self.dictionary.get(index) {
                        (
                            self.origin + (entry >> 12) as usize,
                            (entry & 4095) as usize,
                        )
                    } else if code == self.next && self.next < 4096 {
                        if self.previous == 0 {
                            return Err(Error::Invalid("LZW first code is not a literal"));
                        }
                        (
                            self.origin + (self.previous >> 12) as usize,
                            (self.previous & 4095) as usize + 1,
                        )
                    } else {
                        return Err(Error::Invalid("undefined LZW code"));
                    };
                    if length > limit.saturating_sub(start) {
                        return Err(Error::Invalid("LZW output exceeds frame size"));
                    }
                    let available = start - offset;
                    if length <= 8 && available >= 8 && limit - start >= 8 {
                        // A constant-size copy avoids a variable-size memmove
                        // for short phrases. Both source and temporary output
                        // are initialized and within the caller's bound; drop
                        // the extra suffix before exposing the output again.
                        reserve(output, 8)?;
                        output.extend_from_within(offset..offset + 8);
                        output.truncate(start + length);
                    } else {
                        let count = length.min(available);
                        reserve(output, length)?;
                        output.extend_from_within(offset..offset + count);
                        if count < length {
                            output.push(output[offset]);
                        }
                    }
                    length
                };
                // A full table cannot use the previous phrase until CLEAR.
                if self.next == 4096 {
                    continue;
                }
                if self.previous != 0 {
                    if self.dictionary.len() == self.dictionary.capacity() {
                        let bound = limit
                            .saturating_sub(self.origin)
                            .min((4096 - self.clear - 2) as usize);
                        self.dictionary
                            .try_reserve_exact(bound - self.dictionary.len())
                            .map_err(|_| Error::Allocation)?;
                    }
                    self.dictionary.push(self.previous + 1);
                    self.next += 1;
                    // A one-bit minimum starts with next == 1 << width, so
                    // its first insertion crosses (rather than reaches) it.
                    if self.next > self.mask && self.width < 12 {
                        self.width += 1;
                        self.mask = (self.mask << 1) | 1;
                    }
                } else {
                    self.origin = start;
                }
                // Before the table freezes, even a one-bit minimum keeps
                // phrase lengths at most 4093. Relative offsets fit 24 bits
                // even when a caller's output prefix exceeds 4 GiB.
                self.previous = ((start - self.origin) as u64) << 12 | length as u64;
            }
        }
        Ok(())
    }
}

const FOUR_SYMBOL_SLOTS: usize = 4096 * 4;

const fn direct_table_slots(pixels: usize, literals: usize) -> usize {
    // At most literals² insertions emit one pixel. The other insertions
    // emit at least two, and the final phrase leaves one or more pixels.
    let insertions = (pixels - 1 + literals * literals) / 2;
    (literals + 2 + insertions) * literals
}

/// Reusable flat-table encoder; no production codec dependencies.
pub struct Encoder(());
impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}
impl Encoder {
    pub fn new() -> Self {
        Self(())
    }
    pub fn encode(&mut self, input: &[u8], minimum: u8, output: &mut Vec<u8>) -> Result<(), Error> {
        if !(2..=8).contains(&minimum)
            || (minimum < 8
                && input
                    .chunks(256)
                    .any(|chunk| index_bits(chunk) >> minimum != 0))
        {
            return Err(Error::Invalid("LZW minimum code size or literal range"));
        }
        Self::encode_rows(
            std::iter::once(input),
            input,
            minimum,
            repeated_index(input),
            |bits, count| {
                reserve(output, count)?;
                output.extend_from_slice(&bits.to_le_bytes()[..count]);
                Ok(())
            },
        )
    }

    // The rows contain the validated indices in wire order. Phrases can span rows.
    pub(crate) fn encode_rows<'a>(
        rows: impl Iterator<Item = &'a [u8]>,
        indices: &[u8],
        minimum: u8,
        repeated: Option<u8>,
        mut emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let pixels = indices.len();
        if let Some(pixel) = repeated {
            return Self::encode_repeated(pixel, pixels, minimum, emit);
        }
        if pixels > 1024 {
            if minimum == 2 {
                if pixels <= 2048 {
                    return Self::encode_direct::<2, { direct_table_slots(2048, 4) }>(rows, emit);
                }
                return Self::encode_direct::<2, FOUR_SYMBOL_SLOTS>(rows, emit);
            }
            if minimum == 3 && pixels <= 2048 {
                return Self::encode_direct::<3, { direct_table_slots(2048, 8) }>(rows, emit);
            }
            if pixels <= 1152 {
                return Self::encode_hashed::<4096>(rows, minimum, emit);
            }
            return Self::encode_hashed::<8192>(rows, minimum, emit);
        }
        if pixels <= 2 {
            // CLEAR, up to two literals, and EOI all use the initial width.
            // With at most two pixels, interlacing preserves the input order.
            let clear = 1u64 << minimum;
            let width = minimum + 1;
            let mut bits = clear;
            let mut used = width;
            for &pixel in indices {
                bits |= u64::from(pixel) << used;
                used += width;
            }
            bits |= (clear + 1) << used;
            return emit(bits, usize::from((used + width).div_ceil(8)));
        }
        if pixels == 3 {
            // The iterator preserves GIF's row order even for an interlaced column.
            let mut ordered = rows.flatten().copied();
            let first = u64::from(ordered.next().unwrap());
            let second = u64::from(ordered.next().unwrap());
            let third = u64::from(ordered.next().unwrap());
            let clear = 1u64 << minimum;
            let width = minimum + 1;
            let prefix = clear | (first << width);
            let (bits, count) = if first == second && second == third {
                // The last two literals match the first dictionary phrase.
                (
                    prefix | ((clear + 2) << (2 * width)) | ((clear + 1) << (3 * width)),
                    4 * width,
                )
            } else {
                let bits = prefix
                    | (second << (2 * width))
                    | (third << (3 * width))
                    | ((clear + 1) << (4 * width));
                // Two insertions grow the width at EOI only for a two-bit alphabet.
                (bits, 5 * width + u8::from(minimum == 2))
            };
            return emit(bits, usize::from(count.div_ceil(8)));
        }
        // Each insertion needs a new pixel, so the small tables stay below half full.
        if pixels <= 32 {
            return if pixels <= 8 {
                Self::encode_hashed::<16>(rows, minimum, emit)
            } else if minimum == 2 {
                Self::encode_direct::<2, { direct_table_slots(32, 4) }>(rows, emit)
            } else {
                Self::encode_hashed::<64>(rows, minimum, emit)
            };
        }
        match (minimum, pixels <= 256) {
            (2, true) => Self::encode_direct::<2, { direct_table_slots(256, 4) }>(rows, emit),
            (2, false) => Self::encode_direct::<2, { direct_table_slots(1024, 4) }>(rows, emit),
            (3, true) => Self::encode_direct::<3, { direct_table_slots(256, 8) }>(rows, emit),
            (3, false) => Self::encode_direct::<3, { direct_table_slots(1024, 8) }>(rows, emit),
            (_, true) => Self::encode_hashed::<512>(rows, minimum, emit),
            (_, false) => Self::encode_hashed::<2048>(rows, minimum, emit),
        }
    }

    #[inline(never)]
    fn encode_direct<'a, const MINIMUM: u8, const SLOTS: usize>(
        rows: impl Iterator<Item = &'a [u8]>,
        emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        Self::encode_table(rows, MINIMUM, &mut [0u16; SLOTS], emit)
    }

    #[inline(never)]
    fn encode_hashed<'a, const SLOTS: usize>(
        rows: impl Iterator<Item = &'a [u8]>,
        minimum: u8,
        emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        Self::encode_table(rows, minimum, &mut [0u32; SLOTS], emit)
    }

    fn encode_repeated(
        pixel: u8,
        mut remaining: usize,
        minimum: u8,
        emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let clear = 1u16 << minimum;
        let mut bits = BitWriter {
            emit,
            bits: 0,
            count: 0,
        };
        bits.code(clear, minimum + 1)?;
        // Each insertion extends the longest run by one. The first run
        // is a literal, then length n takes dictionary code CLEAR + n.
        let code = |length| {
            if length == 1 {
                u16::from(pixel)
            } else {
                clear + length
            }
        };
        let mut longest = 1;
        while remaining > usize::from(longest) {
            bits.code(code(longest), (clear + longest).ilog2() as u8 + 1)?;
            remaining -= usize::from(longest);
            if clear + longest == 4095 {
                bits.code(clear, 12)?;
                longest = 1;
            } else {
                longest += 1;
            }
        }
        bits.code(code(remaining as u16), (clear + longest).ilog2() as u8 + 1)?;
        bits.code(clear + 1, (clear + longest + 1).min(4095).ilog2() as u8 + 1)?;
        bits.finish()
    }

    fn encode_table<'a, Entry, const SLOTS: usize>(
        rows: impl Iterator<Item = &'a [u8]>,
        minimum: u8,
        table: &mut [Entry; SLOTS],
        emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error>
    where
        Entry: Copy + Default + Into<u32> + TryFrom<u32>,
    {
        let mask = SLOTS - 1;
        let shift = u32::BITS - SLOTS.ilog2();
        let clear = 1u16 << minimum;
        let mut next = clear + 2;
        let initial_width = minimum + 1;
        let mut width = initial_width;
        let mut limit = clear << 1;
        let mut bits = BitWriter {
            emit,
            bits: 0,
            count: 0,
        };
        bits.code(clear, width)?;
        let mut pending = None;
        for row in rows {
            let mut symbols = row.iter().copied();
            let Some(mut prefix) = pending.or_else(|| symbols.next().map(u16::from)) else {
                continue;
            };
            'symbols: for symbol in symbols {
                let (slot, tag) = if size_of::<Entry>() == size_of::<u16>() {
                    let slot = (usize::from(prefix) << minimum) | usize::from(symbol);
                    let code = table[slot].into() as u16;
                    if code != 0 {
                        prefix = code;
                        continue 'symbols;
                    }
                    (slot, 0)
                } else {
                    let key = (u32::from(prefix) << 8) | u32::from(symbol);
                    let tag = key << 12;
                    let hash = key.wrapping_mul(0x9e3779b1);
                    let mut slot = (hash >> shift) as usize;
                    let step = if SLOTS <= 4096 {
                        1
                    } else {
                        // An odd stride visits every slot in a power-of-two table.
                        (hash as usize & mask) | 1
                    };
                    loop {
                        let entry = table[slot].into();
                        if entry == 0 {
                            break (slot, tag);
                        }
                        // A matching tag leaves just the 12-bit code.
                        let code = entry ^ tag;
                        if code < 4096 {
                            prefix = code as u16;
                            continue 'symbols;
                        }
                        slot = (slot + step) & mask;
                    }
                };
                // The decoder adds this phrase after reading the next code.
                bits.code(prefix, width)?;
                prefix = symbol.into();
                if next == limit {
                    if limit == 4096 {
                        bits.code(clear, 12)?;
                        table.fill(Entry::default());
                        next = clear + 2;
                        width = initial_width;
                        limit = clear << 1;
                        continue 'symbols;
                    }
                    width += 1;
                    limit <<= 1;
                }
                // The assigned code is never zero, even when the key is.
                table[slot] = Entry::try_from(tag | u32::from(next)).ok().unwrap();
                next += 1;
            }
            pending = Some(prefix);
        }
        if let Some(prefix) = pending {
            bits.code(prefix, width)?;
        }
        // The final prefix has caught the decoder up to the encoder's table.
        let end_width = width + u8::from(next == limit && limit < 4096);
        bits.code(clear + 1, end_width)?;
        bits.finish()
    }
}

// The highest bit set in any index determines the required alphabet width.
pub(crate) fn index_bits(indices: &[u8]) -> u8 {
    indices.iter().fold(0, |bits, &index| bits | index)
}

pub(crate) fn repeated_index(indices: &[u8]) -> Option<u8> {
    if indices.len() < 32 || indices[0] != indices[1] {
        return None;
    }
    let pixel = indices[0];
    let (chunks, tail) = indices.as_chunks::<16>();
    (chunks.iter().all(|chunk| *chunk == [pixel; 16]) && tail.iter().all(|&index| index == pixel))
        .then_some(pixel)
}

struct BitWriter<F> {
    emit: F,
    bits: u64,
    count: u8,
}
impl<F: FnMut(u64, usize) -> Result<(), Error>> BitWriter<F> {
    fn code(&mut self, code: u16, width: u8) -> Result<(), Error> {
        self.bits |= u64::from(code) << self.count;
        self.count += width;
        // At most 47 buffered bits plus a 12-bit code fit in u64. Flush six
        // bytes at a time to amortize fallible growth across more codes.
        if self.count >= 48 {
            (self.emit)(self.bits, 6)?;
            self.bits >>= 48;
            self.count -= 48;
        }
        Ok(())
    }
    fn finish(mut self) -> Result<(), Error> {
        if self.count != 0 {
            (self.emit)(self.bits, usize::from(self.count.div_ceil(8)))?;
        }
        Ok(())
    }
}

pub fn decode(input: &[u8], minimum: u8, expected: usize) -> Result<Vec<u8>, Error> {
    let mut decoder = Decoder::new();
    decoder.reset(minimum)?;
    let mut output = Vec::new();
    reserve(&mut output, expected)?;
    decoder.update(input, &mut output, expected)?;
    if !decoder.ended() || output.len() != expected {
        return Err(Error::Invalid("incomplete LZW image"));
    }
    Ok(output)
}
pub fn encode(input: &[u8], minimum: u8) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    Encoder::new().encode(input, minimum, &mut output)?;
    Ok(output)
}
