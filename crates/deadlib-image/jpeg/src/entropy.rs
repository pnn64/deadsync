use crate::{Error, Result};

const LOOK_BITS: u8 = 9;

/// Canonical JPEG Huffman decoder: a flat prefix lookup and bounded fallback.
#[derive(Clone)]
pub(crate) struct Huffman {
    lookup: [u16; 1 << LOOK_BITS],
    max_code: [i32; 17],
    offsets: [i32; 17],
    values: [u8; 256],
}

impl Huffman {
    pub fn new(counts: &[u8; 16], symbols: &[u8]) -> Result<Self> {
        let count: usize = counts.iter().map(|&v| v as usize).sum();
        if count == 0 || count > 256 || count != symbols.len() {
            return Err(Error::Invalid("Huffman symbol count"));
        }
        let mut table = Self {
            lookup: [0; 1 << LOOK_BITS],
            max_code: [-1; 17],
            offsets: [0; 17],
            values: [0; 256],
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
                    table.lookup[start..start + (1 << shift)]
                        .fill(((length as u16) << 8) | symbols[index + j] as u16);
                }
            }
            index += n as usize;
            code = (code + n) << 1;
        }
        Ok(table)
    }

    #[inline]
    pub fn decode(&self, bits: &mut Bits<'_>) -> Result<u8> {
        if let Some(prefix) = bits.peek(LOOK_BITS)? {
            let entry = self.lookup[prefix as usize];
            if entry != 0 {
                bits.drop((entry >> 8) as u8);
                return Ok(entry as u8);
            }
        }
        let mut code = 0i32;
        for length in 1..=16 {
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
}

impl<'a> Bits<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            position: 0,
            buffer: 0,
            count: 0,
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
        if self.count > 7 || (self.buffer & ((1 << self.count) - 1)) != (1 << self.count) - 1 {
            return Err(Error::Invalid("entropy padding"));
        }
        self.count = 0;
        self.buffer = 0;
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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(Huffman::new(&[0; 16], &[]).is_err());
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
