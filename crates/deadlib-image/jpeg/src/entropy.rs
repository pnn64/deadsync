use crate::{Error, Result};

pub(crate) const LOOK_BITS: u8 = 9;

/// Canonical JPEG Huffman decoder: a flat prefix lookup and bounded fallback.
#[derive(Clone)]
pub(crate) struct Huffman {
    counts: [u8; 16],
    symbol_count: u16,
    pub(crate) lookup: [u16; 1 << LOOK_BITS],
    pub(crate) max_code: [i32; 17],
    pub(crate) offsets: [i32; 17],
    pub(crate) values: [u8; 256],
    /// Sequential AC lookup that also decodes the magnitude when code and
    /// magnitude bits fit the prefix: `value << 8 | run << 4 | length`,
    /// zero otherwise. Magnitudes are limited to seven bits to fit i16.
    pub(crate) ac: [i16; 1 << LOOK_BITS],
}

impl Huffman {
    pub fn new(counts: &[u8; 16], symbols: &[u8]) -> Result<Self> {
        let count: usize = counts.iter().map(|&v| v as usize).sum();
        // B.2.4.2 permits all Li to be zero. Such a destination may remain
        // unused; attempting to decode a symbol still fails as unassigned.
        if count > 256 || count != symbols.len() {
            return Err(Error::Invalid("Huffman symbol count"));
        }
        let mut table = Self {
            counts: *counts,
            symbol_count: count as u16,
            lookup: [0; 1 << LOOK_BITS],
            max_code: [-1; 17],
            offsets: [0; 17],
            values: [0; 256],
            ac: [0; 1 << LOOK_BITS],
        };
        table.values[..count].copy_from_slice(symbols);
        let mut code = 0u32;
        let mut index = 0usize;
        for length in 1..=16 {
            let n = counts[length - 1] as u32;
            // The all-ones code is reserved by T.81 C.2.2, including at length 16.
            if code + n >= 1 << length {
                return Err(Error::Invalid("overfull Huffman tree or all-ones code"));
            }
            if n > 0 {
                table.max_code[length] = (code + n - 1) as i32;
                table.offsets[length] = index as i32 - code as i32;
            }
            if length <= LOOK_BITS as usize {
                let shift = LOOK_BITS as usize - length;
                for j in 0..n as usize {
                    let start = ((code as usize) + j) << shift;
                    let symbol = symbols[index + j];
                    table.lookup[start..start + (1 << shift)]
                        .fill(((length as u16) << 8) | symbol as u16);
                    table.fill_ac(start, length, symbol);
                }
            }
            index += n as usize;
            code = (code + n) << 1;
        }
        Ok(table)
    }

    /// Fill the combined AC entries under one code whose prefix range
    /// starts at `start`: each magnitude pattern covers a contiguous range.
    fn fill_ac(&mut self, start: usize, length: usize, symbol: u8) {
        let (run, size) = (i32::from(symbol >> 4), usize::from(symbol & 15));
        if size == 0 || size > 7 || length + size > LOOK_BITS as usize {
            return;
        }
        let free = LOOK_BITS as usize - length - size;
        for bits in 0..1i32 << size {
            let value = if bits < 1 << (size - 1) {
                bits + 1 - (1 << size)
            } else {
                bits
            };
            let entry = (value << 8 | run << 4 | (length + size) as i32) as i16;
            let at = start + ((bits as usize) << free);
            self.ac[at..at + (1 << free)].fill(entry);
        }
    }

    pub(crate) fn definition(&self) -> (&[u8; 16], &[u8]) {
        (&self.counts, &self.values[..self.symbol_count as usize])
    }

    #[inline]
    pub fn decode(&self, bits: &mut Bits<'_>) -> Result<u8> {
        let prefix = bits.peek(LOOK_BITS)?;
        self.decode_prefix(bits, prefix)
    }

    /// Decode using the prefix already fetched by the caller.
    #[inline]
    pub(crate) fn decode_prefix(&self, bits: &mut Bits<'_>, prefix: Option<u32>) -> Result<u8> {
        let (mut code, start) = if let Some(prefix) = prefix {
            let entry = self.lookup[prefix as usize];
            if entry != 0 {
                bits.drop((entry >> 8) as u8);
                return Ok(entry as u8);
            }
            // A missed prefix cannot match any code up to LOOK_BITS.
            bits.drop(LOOK_BITS);
            (prefix as i32, LOOK_BITS as usize + 1)
        } else {
            (0, 1)
        };
        for length in start..=16 {
            code = (code << 1) | bits.read(1)? as i32;
            if self.max_code[length] >= code {
                let index = code + self.offsets[length];
                return self
                    .values
                    .get(index as usize)
                    .copied()
                    .ok_or(Error::Invalid("Huffman code"));
            }
        }
        Err(Error::Invalid("unassigned Huffman code"))
    }
}

pub(crate) struct Bits<'a> {
    input: &'a [u8],
    position: usize,
    buffer: u64,
    count: u8,
    strict: bool,
}

impl<'a> Bits<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            position: 0,
            buffer: 0,
            count: 0,
            strict: true,
        }
    }

    pub fn with_strict(input: &'a [u8], strict: bool) -> Self {
        Self {
            strict,
            ..Self::new(input)
        }
    }

    #[inline]
    pub fn peek(&mut self, n: u8) -> Result<Option<u32>> {
        while self.count < n {
            let Some(&byte) = self.input.get(self.position) else {
                return Ok(None);
            };
            if byte == 0xff {
                let next = *self.input.get(self.position + 1).ok_or(Error::Truncated)?;
                if next != 0 {
                    return Ok(None);
                }
                self.position += 1;
            }
            self.position += 1;
            self.buffer = (self.buffer << 8) | byte as u64;
            self.count += 8;
        }
        Ok(Some(
            ((self.buffer >> (self.count - n)) & ((1u64 << n) - 1)) as u32,
        ))
    }

    #[inline]
    pub fn drop(&mut self, n: u8) {
        self.count -= n;
    }

    #[inline]
    pub fn read(&mut self, n: u8) -> Result<u32> {
        let value = self.peek(n)?.ok_or(Error::Truncated)?;
        self.drop(n);
        Ok(value)
    }

    #[inline]
    pub fn receive(&mut self, n: u8) -> Result<i32> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.read(n)? as i32;
        Ok(if v < 1 << (n - 1) {
            v + 1 - (1 << n)
        } else {
            v
        })
    }

    fn align(&mut self) -> Result<()> {
        if self.strict
            && (self.count > 7 || (self.buffer & ((1 << self.count) - 1)) != (1 << self.count) - 1)
        {
            return Err(Error::Invalid("entropy padding"));
        }
        self.count = 0;
        self.buffer = 0;
        if !self.strict {
            skip_padding(self.input, &mut self.position)?;
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

/// Discard surplus entropy only after the caller has decoded a complete unit.
/// Stop at the first marker: restart validation must never resynchronize past
/// an unexpected marker. Stuffed FF bytes are data, not marker boundaries.
pub(crate) fn skip_padding(input: &[u8], position: &mut usize) -> Result<()> {
    while let Some(&byte) = input.get(*position) {
        if byte == 0xff {
            match input.get(*position + 1) {
                Some(0) => *position += 2,
                Some(_) => break,
                None => return Err(Error::Truncated),
            }
        } else {
            *position += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_refills_match_an_independent_bit_reader_with_stuffing() {
        let mut state = 0x12345678u32;
        let raw: Vec<_> = (0..4096)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if i % 17 == 0 { 255 } else { state as u8 }
            })
            .collect();
        let stuffed: Vec<_> = raw
            .iter()
            .flat_map(|&v| if v == 255 { vec![255, 0] } else { vec![v] })
            .collect();
        for start in 0..16 {
            let mut bits = Bits::new(&stuffed);
            let mut at = 0;
            let mut turn = start;
            while at < raw.len() * 8 {
                let n = (1 + turn % 16).min(raw.len() * 8 - at);
                let mut expected = 0u32;
                for bit in at..at + n {
                    expected = expected << 1 | u32::from((raw[bit / 8] >> (7 - bit % 8)) & 1);
                }
                assert_eq!(bits.read(n as u8).unwrap(), expected);
                at += n;
                turn += 7;
            }
            bits.finish().unwrap();
        }
    }
    #[test]
    fn all_code_lengths_fast_and_slow() {
        let counts = [1; 16];
        let symbols: Vec<_> = (0..16).collect();
        let table = Huffman::new(&counts, &symbols).unwrap();
        // Codes are 0,10,110,...,1111111111111110.
        for length in 1..=16u8 {
            let code = ((1u32 << length) - 2) << (16 - length);
            let b = [(code >> 8) as u8, code as u8];
            let stuffed: Vec<_> = b
                .into_iter()
                .flat_map(|v| if v == 255 { vec![255, 0] } else { vec![v] })
                .collect();
            let mut bits = Bits::new(&stuffed);
            assert_eq!(table.decode(&mut bits).unwrap(), length - 1);
        }
        assert!(Huffman::new(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], &[0, 1]).is_err());
        let empty = Huffman::new(&[0; 16], &[]).unwrap();
        assert_eq!(empty.definition(), (&[0; 16], &[][..]));
        assert!(empty.decode(&mut Bits::new(&[0, 0])).is_err());
    }

    #[test]
    fn magnitude_sign_padding_and_restart() {
        for n in 1..=15 {
            for value in [0, (1 << (n - 1)) - 1, 1 << (n - 1), (1 << n) - 1] {
                let word = (value as u32) << (16 - n);
                let b = [(word >> 8) as u8, word as u8];
                let data: Vec<_> = b
                    .into_iter()
                    .flat_map(|v| if v == 255 { vec![255, 0] } else { vec![v] })
                    .collect();
                let want = if value < 1 << (n - 1) {
                    value + 1 - (1 << n)
                } else {
                    value
                };
                assert_eq!(Bits::new(&data).receive(n).unwrap(), want);
            }
        }
        let mut b = Bits::new(&[0x7f, 0xff, 0xff, 0xd0, 0xff, 0]);
        assert_eq!(b.read(1).unwrap(), 0);
        b.restart(0).unwrap();
        assert_eq!(b.read(8).unwrap(), 255);
        b.finish().unwrap();
        let mut b = Bits::new(&[0x00]);
        b.read(1).unwrap();
        assert!(b.finish().is_err());
    }
}
