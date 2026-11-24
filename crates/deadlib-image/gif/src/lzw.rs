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
    width: u8,
    minimum: u8,
    previous: Option<(usize, usize)>,
    bits: u64,
    bit_count: u8,
    ended: bool,
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
            width: 3,
            minimum: 2,
            previous: None,
            bits: 0,
            bit_count: 0,
            ended: false,
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
        self.minimum = minimum;
        self.clear = 1 << minimum;
        self.clear_table();
        self.bits = 0;
        self.bit_count = 0;
        self.ended = false;
        Ok(())
    }
    fn clear_table(&mut self) {
        self.dictionary.clear();
        self.next = self.clear + 2;
        self.width = self.minimum + 1;
        self.previous = None;
    }
    pub fn ended(&self) -> bool {
        self.ended
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
        let mut input = input;
        while !input.is_empty() && !self.ended {
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
                let code = (self.bits & ((1 << self.width) - 1)) as u16;
                self.bits >>= self.width;
                self.bit_count -= self.width;
                let start = output.len();
                let length = if code < self.clear {
                    // Literals need neither control-code checks nor phrase lookup.
                    if start >= limit {
                        return Err(Error::Invalid("LZW output exceeds frame size"));
                    }
                    reserve(output, 1)?;
                    output.push(code as u8);
                    1
                } else {
                    if code == self.clear {
                        self.clear_table();
                        continue;
                    }
                    if code == self.clear + 1 {
                        self.ended = true;
                        break;
                    }
                    let index = usize::from(code - self.clear - 2);
                    let (offset, length) = if let Some(&entry) = self.dictionary.get(index) {
                        (
                            self.origin + (entry >> 12) as usize,
                            (entry & 4095) as usize,
                        )
                    } else if code == self.next && self.next < 4096 {
                        let (offset, len) = self
                            .previous
                            .ok_or(Error::Invalid("LZW first code is not a literal"))?;
                        (offset, len + 1)
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
                if let Some((offset, previous_len)) = self.previous {
                    if self.dictionary.len() == self.dictionary.capacity() {
                        let bound = limit
                            .saturating_sub(self.origin)
                            .min((4096 - self.clear - 2) as usize);
                        self.dictionary
                            .try_reserve_exact(bound - self.dictionary.len())
                            .map_err(|_| Error::Allocation)?;
                    }
                    // Before the table freezes, even a one-bit minimum keeps
                    // phrase lengths at most 4093. Thus relative offsets fit
                    // 24 bits even when a caller's output prefix exceeds 4 GiB.
                    // A frozen table adds no entries; CLEAR starts a new epoch.
                    let entry = ((offset - self.origin) as u64) << 12 | (previous_len + 1) as u64;
                    self.dictionary.push(entry);
                    self.next += 1;
                    // A one-bit minimum starts with next == 1 << width, so
                    // its first insertion crosses (rather than reaches) it.
                    if self.next >= 1 << self.width && self.width < 12 {
                        self.width += 1;
                    }
                } else {
                    self.origin = start;
                }
                self.previous = Some((start, length));
            }
        }
        Ok(())
    }
}

/// Reusable flat hash-table encoder; no production codec dependencies.
pub struct Encoder {
    table: Option<[u32; 8192]>,
}
impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}
impl Encoder {
    pub fn new() -> Self {
        Self { table: None }
    }
    pub fn encode(&mut self, input: &[u8], minimum: u8, output: &mut Vec<u8>) -> Result<(), Error> {
        if !(2..=8).contains(&minimum)
            || (minimum < 8
                && input.chunks(256).any(|chunk| {
                    // A high bit in any literal survives the OR reduction.
                    chunk.iter().fold(0, |bits, &pixel| bits | pixel) >> minimum != 0
                }))
        {
            return Err(Error::Invalid("LZW minimum code size or literal range"));
        }
        self.encode_rows(std::iter::once(input), input, minimum, |bits, count| {
            reserve(output, count)?;
            output.extend_from_slice(&bits.to_le_bytes()[..count]);
            Ok(())
        })
    }

    // The rows contain the validated indices in wire order. Phrases can span rows.
    pub(crate) fn encode_rows<'a>(
        &mut self,
        rows: impl Iterator<Item = &'a [u8]>,
        indices: &[u8],
        minimum: u8,
        mut emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let pixels = indices.len();
        if pixels > 1024 {
            let pixel = indices[0];
            if indices[1] == pixel {
                let (chunks, tail) = indices.as_chunks::<16>();
                if chunks.iter().all(|chunk| *chunk == [pixel; 16])
                    && tail.iter().all(|&index| index == pixel)
                {
                    return Self::encode_repeated(pixel, pixels, minimum, emit);
                }
            }
            let table = match &mut self.table {
                Some(table) => {
                    table.fill(0);
                    table
                }
                empty => empty.insert([0; 8192]),
            };
            return Self::encode_table(rows, minimum, table, emit);
        }
        if pixels <= 1 {
            // CLEAR, at most one literal, and EOI all use the initial width.
            let clear = 1u64 << minimum;
            let width = minimum + 1;
            let mut bits = clear;
            let mut used = width;
            if let Some(&pixel) = indices.first() {
                bits |= u64::from(pixel) << used;
                used += width;
            }
            bits |= (clear + 1) << used;
            return emit(bits, usize::from((used + width).div_ceil(8)));
        }
        // Each insertion needs a new pixel, so both tables stay below half full.
        if pixels <= 256 {
            return Self::encode_table(rows, minimum, &mut [0; 512], emit);
        }
        Self::encode_table(rows, minimum, &mut [0; 2048], emit)
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

    fn encode_table<'a, const SLOTS: usize>(
        rows: impl Iterator<Item = &'a [u8]>,
        minimum: u8,
        table: &mut [u32; SLOTS],
        emit: impl FnMut(u64, usize) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mask = SLOTS - 1;
        let shift = u32::BITS - SLOTS.ilog2();
        let clear = 1u16 << minimum;
        let mut next = clear + 2;
        let mut bits = BitWriter {
            emit,
            bits: 0,
            count: 0,
        };
        bits.code(clear, minimum + 1)?;
        let mut pending = None;
        for row in rows {
            let mut symbols = row.iter().copied();
            let Some(mut prefix) = pending.or_else(|| symbols.next().map(u16::from)) else {
                continue;
            };
            'symbols: for symbol in symbols {
                let key = (u32::from(prefix) << 8) | u32::from(symbol);
                let tag = key << 12;
                let hash = key.wrapping_mul(0x9e3779b1);
                let mut slot = (hash >> shift) as usize;
                let step = if SLOTS <= 2048 {
                    1
                } else {
                    // An odd stride visits every slot in a power-of-two table.
                    (hash as usize & mask) | 1
                };
                loop {
                    let entry = table[slot];
                    if entry == 0 {
                        break;
                    }
                    // A matching tag leaves just the 12-bit code.
                    let code = entry ^ tag;
                    if code < 4096 {
                        prefix = code as u16;
                        continue 'symbols;
                    }
                    slot = (slot + step) & mask;
                }
                // The decoder adds this phrase after reading the next code.
                bits.code(prefix, (next - 1).ilog2() as u8 + 1)?;
                if next < 4096 {
                    // The assigned code is never zero, even when the key is.
                    table[slot] = tag | u32::from(next);
                    next += 1;
                } else {
                    bits.code(clear, 12)?;
                    table.fill(0);
                    next = clear + 2;
                }
                prefix = symbol.into();
            }
            pending = Some(prefix);
        }
        if let Some(prefix) = pending {
            bits.code(prefix, (next - 1).ilog2() as u8 + 1)?;
        }
        // The final prefix has caught the decoder up to the encoder's table.
        bits.code(clear + 1, next.min(4095).ilog2() as u8 + 1)?;
        bits.finish()
    }
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
