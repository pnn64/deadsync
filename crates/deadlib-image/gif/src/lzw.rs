//! GIF's little-endian, late-change, 12-bit LZW, implemented locally.
use crate::{Error, reserve};

/// Reusable incremental LZW decoder. The caller must retain the output prefix
/// unchanged until the image ends: dictionary phrases refer to that prefix.
/// Output is bounded before every expansion, including KwKwK and frozen tables.
pub struct Decoder {
    offsets: [usize; 4096],
    lengths: [u16; 4096],
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
            offsets: [0; 4096],
            lengths: [0; 4096],
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
        self.minimum = minimum;
        self.clear = 1 << minimum;
        self.clear_table();
        self.bits = 0;
        self.bit_count = 0;
        self.ended = false;
        Ok(())
    }
    fn clear_table(&mut self) {
        self.next = self.clear + 2;
        self.width = self.minimum + 1;
        self.previous = None;
    }
    pub fn ended(&self) -> bool {
        self.ended
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
                if code == self.clear {
                    self.clear_table();
                    continue;
                }
                if code == self.clear + 1 {
                    self.ended = true;
                    break;
                }
                let start = output.len();
                let (length, source, first) = if code < self.clear {
                    (1, None, code as u8)
                } else if code < self.next {
                    let offset = self.offsets[code as usize];
                    (
                        self.lengths[code as usize] as usize,
                        Some(offset),
                        output[offset],
                    )
                } else if code == self.next && self.next < 4096 {
                    let (offset, len) = self
                        .previous
                        .ok_or(Error::Invalid("LZW first code is not a literal"))?;
                    (len + 1, Some(offset), output[offset])
                } else {
                    return Err(Error::Invalid("undefined LZW code"));
                };
                if length > limit.saturating_sub(start) {
                    return Err(Error::Invalid("LZW output exceeds frame size"));
                }
                reserve(output, length)?;
                if let Some(offset) = source {
                    let count = length.min(start - offset);
                    output.extend_from_within(offset..offset + count);
                    if count < length {
                        output.push(first);
                    }
                } else {
                    output.push(first);
                }
                if let Some((offset, previous_len)) = self.previous
                    && self.next < 4096
                {
                    self.offsets[self.next as usize] = offset;
                    self.lengths[self.next as usize] = (previous_len + 1) as u16;
                    self.next += 1;
                    if self.next == 1 << self.width && self.width < 12 {
                        self.width += 1;
                    }
                }
                self.previous = Some((start, length));
            }
        }
        Ok(())
    }
}

/// Reusable flat hash-table encoder; no production codec dependencies.
pub struct Encoder {
    keys: [u32; 8192],
    values: [u16; 8192],
}
impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}
impl Encoder {
    pub fn new() -> Self {
        Self {
            keys: [u32::MAX; 8192],
            values: [0; 8192],
        }
    }
    pub fn encode(&mut self, input: &[u8], minimum: u8, output: &mut Vec<u8>) -> Result<(), Error> {
        if !(2..=8).contains(&minimum) || input.iter().any(|&v| u16::from(v) >= 1 << minimum) {
            return Err(Error::Invalid("LZW minimum code size or literal range"));
        }
        self.keys.fill(u32::MAX);
        let clear = 1u16 << minimum;
        let mut next = clear + 2;
        let mut bits = BitWriter {
            output,
            bits: 0,
            count: 0,
            minimum,
            width: minimum + 1,
            next: clear + 2,
            previous: false,
        };
        bits.code(clear)?;
        if let Some((&first, rest)) = input.split_first() {
            let mut prefix = u16::from(first);
            for &symbol in rest {
                let key = (u32::from(prefix) << 8) | u32::from(symbol);
                let mut slot = (key.wrapping_mul(0x9e3779b1) >> 19) as usize;
                while self.keys[slot] != key && self.keys[slot] != u32::MAX {
                    slot = (slot + 1) & 8191;
                }
                if self.keys[slot] == key {
                    prefix = self.values[slot];
                    continue;
                }
                bits.code(prefix)?;
                if next < 4096 {
                    self.keys[slot] = key;
                    self.values[slot] = next;
                    next += 1;
                } else {
                    bits.code(clear)?;
                    self.keys.fill(u32::MAX);
                    next = clear + 2;
                }
                prefix = symbol.into();
            }
            bits.code(prefix)?;
        }
        bits.code(clear + 1)?;
        if bits.count != 0 {
            reserve(bits.output, 1)?;
            bits.output.push(bits.bits as u8);
        }
        Ok(())
    }
}

struct BitWriter<'a> {
    output: &'a mut Vec<u8>,
    bits: u64,
    count: u8,
    minimum: u8,
    width: u8,
    next: u16,
    previous: bool,
}
impl BitWriter<'_> {
    fn code(&mut self, code: u16) -> Result<(), Error> {
        self.bits |= u64::from(code) << self.count;
        self.count += self.width;
        reserve(self.output, 2)?;
        while self.count >= 8 {
            self.output.push(self.bits as u8);
            self.bits >>= 8;
            self.count -= 8;
        }
        let clear = 1 << self.minimum;
        if code == clear {
            self.width = self.minimum + 1;
            self.next = clear + 2;
            self.previous = false;
        } else if code != clear + 1 {
            if self.previous && self.next < 4096 {
                self.next += 1;
                if self.next == 1 << self.width && self.width < 12 {
                    self.width += 1;
                }
            }
            self.previous = true;
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
