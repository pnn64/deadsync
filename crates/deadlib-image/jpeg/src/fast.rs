//! Sequential 8-bit Huffman decoding for bounded pixel output.
//!
//! Acceptance matches [`crate::entropy::Bits`] exactly. Refills are eager, so
//! a marker or the end of input appends virtual zero bits instead of failing
//! immediately; consuming any of them is a sticky truncation error reported by
//! [`Reader::check`], [`Reader::restart`] and [`Reader::finish`].
use crate::entropy::{Huffman, LOOK_BITS};
use crate::{Error, Result, ZIGZAG};

const FAST: u32 = LOOK_BITS as u32;

// Copy lets cold paths take the state by value: no reference escapes, so
// the hot loop keeps `buffer` and `count` in registers.
#[derive(Clone, Copy)]
pub(crate) struct Reader<'a> {
    input: &'a [u8],
    position: usize,
    /// Valid bits are left-aligned. The unused suffix may contain the high
    /// bits of the next unread byte after a word refill.
    buffer: u64,
    count: u32,
    /// Trailing virtual zero bits included in `count`.
    virtual_bits: u32,
    overrun: bool,
    strict: bool,
}

#[inline(always)]
fn has_ff(word: u64) -> bool {
    let inverted = !word;
    inverted.wrapping_sub(0x0101_0101_0101_0101) & !inverted & 0x8080_8080_8080_8080 != 0
}

#[inline(always)]
fn extend(value: u32, size: u32) -> i32 {
    let value = value as i32;
    if value < 1 << (size - 1) {
        value + 1 - (1 << size)
    } else {
        value
    }
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a [u8], strict: bool) -> Self {
        Self {
            input,
            position: 0,
            buffer: 0,
            count: 0,
            virtual_bits: 0,
            overrun: false,
            strict,
        }
    }

    /// Guarantee at least 32 buffered (possibly virtual) bits.
    #[inline(always)]
    fn fill(&mut self) {
        if self.count < 32 {
            self.refill();
        }
    }

    #[inline(always)]
    fn refill(&mut self) {
        if let Some(bytes) = self.input.get(self.position..self.position + 8) {
            let word = u64::from_be_bytes(bytes.try_into().unwrap());
            if !has_ff(word) {
                let n = (64 - self.count) / 8;
                // Any bits below the new count belong to the same next
                // byte that a later word or byte refill will merge here.
                self.buffer |= word >> self.count;
                self.position += n as usize;
                self.count += 8 * n;
                return;
            }
        }
        *self = self.refill_bytes();
    }

    #[cold]
    #[inline(never)]
    fn refill_bytes(mut self) -> Self {
        while self.count <= 56 {
            let Some(&byte) = self.input.get(self.position) else {
                self.pad();
                break;
            };
            if byte == 0xff {
                if self.input.get(self.position + 1) != Some(&0) {
                    self.pad();
                    break;
                }
                self.position += 1;
            }
            self.position += 1;
            self.buffer |= u64::from(byte) << (56 - self.count);
            self.count += 8;
        }
        self
    }

    fn pad(&mut self) {
        self.overrun |= self.virtual_bits > self.count;
        self.virtual_bits = self.virtual_bits.min(self.count) + (64 - self.count);
        self.count = 64;
    }

    #[inline(always)]
    fn peek(&self, n: u32) -> u32 {
        (self.buffer >> (64 - n)) as u32
    }

    #[inline(always)]
    fn consume(&mut self, n: u32) {
        self.buffer <<= n;
        self.count -= n;
    }

    #[inline(always)]
    fn receive(&mut self, size: u32) -> i32 {
        let value = self.peek(size);
        self.consume(size);
        extend(value, size)
    }

    /// Report a read beyond the real entropy data.
    #[inline]
    pub fn check(&self) -> Result<()> {
        if self.overrun || self.virtual_bits > self.count {
            return Err(Error::Truncated);
        }
        Ok(())
    }

    fn align(&mut self) -> Result<()> {
        self.check()?;
        let real = self.count - self.virtual_bits;
        if self.strict && (real > 7 || (real > 0 && self.peek(real) != (1 << real) - 1)) {
            return Err(Error::Invalid("entropy padding"));
        }
        self.buffer = 0;
        self.count = 0;
        self.virtual_bits = 0;
        if !self.strict {
            crate::entropy::skip_padding(self.input, &mut self.position)?;
        }
        Ok(())
    }

    pub fn restart(&mut self, expected: u8) -> Result<()> {
        self.align()?;
        if self.input.get(self.position) != Some(&0xff) {
            return Err(Error::Invalid("missing restart marker"));
        }
        while self.input.get(self.position) == Some(&0xff) {
            self.position += 1;
        }
        if self.input.get(self.position) != Some(&(0xd0 + expected)) {
            return Err(Error::Invalid("restart sequence"));
        }
        self.position += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.align()?;
        if self.position != self.input.len() {
            return Err(Error::Invalid("excess entropy data"));
        }
        Ok(())
    }
}

/// Decode one symbol; the caller guarantees at least 16 buffered bits.
#[inline(always)]
fn symbol(table: &Huffman, reader: &mut Reader<'_>) -> Result<u32> {
    let entry = table.lookup[reader.peek(FAST) as usize];
    if entry != 0 {
        reader.consume(u32::from(entry >> 8));
        return Ok(u32::from(entry & 0xff));
    }
    let (symbol, length) = long_symbol(table, reader.peek(16) as i32)?;
    reader.consume(length);
    Ok(symbol)
}

// A prefix miss means the code is longer than FAST bits or unassigned:
// canonical codes give no shorter match, as in the bitwise decoder.
#[cold]
#[inline(never)]
fn long_symbol(table: &Huffman, peek: i32) -> Result<(u32, u32)> {
    for length in FAST + 1..=16 {
        let code = peek >> (16 - length);
        if code <= table.max_code[length as usize] {
            let index = code + table.offsets[length as usize];
            let value = table
                .values
                .get(index as usize)
                .copied()
                .ok_or(Error::Invalid("Huffman code"))?;
            return Ok((u32::from(value), length));
        }
    }
    Err(Error::Invalid("unassigned Huffman code"))
}

/// Decode one sequential block. Returns the largest zigzag index written, so
/// zero identifies a DC-only block. `block` must start zeroed. Magnitude
/// categories reach precision + 3 bits, so a symbol never exceeds the 32
/// bits that `fill` guarantees.
#[inline(always)]
pub(crate) fn block(
    block: &mut [i16; 64],
    reader: &mut Reader<'_>,
    dc: &Huffman,
    ac: &Huffman,
    predictor: &mut i32,
    precision: u8,
    differential: bool,
) -> Result<usize> {
    reader.fill();
    let size = symbol(dc, reader)?;
    if size > u32::from(precision) + 3 {
        return Err(Error::Invalid("DC magnitude category"));
    }
    if size != 0 {
        *predictor += reader.receive(size);
    }
    if *predictor < -(1 << (precision + 3)) || *predictor >= 1 << (precision + 3) {
        return Err(Error::Invalid("DC coefficient range"));
    }
    block[0] = *predictor as i16;
    let mut last = 0;
    let mut k = 1;
    while k < 64 {
        reader.fill();
        let fast = ac.ac[reader.peek(FAST) as usize];
        if fast != 0 {
            k += (fast >> 4 & 15) as usize;
            if k >= 64 {
                return Err(Error::Invalid("AC run or magnitude"));
            }
            reader.consume((fast & 15) as u32);
            block[ZIGZAG[k]] = fast >> 8;
            last = k;
            k += 1;
            continue;
        }
        let symbol = symbol(ac, reader)?;
        let (run, size) = ((symbol >> 4) as usize, symbol & 15);
        if size == 0 {
            if run == 0 {
                break;
            }
            if run != 15 {
                return Err(Error::Invalid("sequential AC run symbol"));
            }
            k += 16;
            if k > 64 {
                return Err(Error::Invalid("AC zero run exceeds block"));
            }
        } else {
            k += run;
            if k >= 64 || size > u32::from(precision) + 2 + u32::from(differential) {
                return Err(Error::Invalid("AC run or magnitude"));
            }
            block[ZIGZAG[k]] = reader.receive(size) as i16;
            last = k;
            k += 1;
        }
    }
    Ok(last)
}
