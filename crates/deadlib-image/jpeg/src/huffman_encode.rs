use crate::output::Output;
use crate::{Error, Result};

pub(crate) struct Codebook {
    pub counts: [u8; 16],
    pub symbols: Vec<u8>,
    codes: [u16; 256],
    lengths: [u8; 256],
}

impl Codebook {
    fn new(counts: [u8; 16], symbols: Vec<u8>) -> Result<Self> {
        let mut book = Self {
            counts,
            symbols,
            codes: [0; 256],
            lengths: [0; 256],
        };
        let mut code = 0u32;
        let mut index = 0;
        for n in 1..=16 {
            let count = book.counts[n - 1] as usize;
            if code + count as u32 >= 1 << n {
                return Err(Error::Invalid("encoder Huffman tree"));
            }
            for _ in 0..count {
                let symbol = *book
                    .symbols
                    .get(index)
                    .ok_or(Error::Invalid("encoder Huffman count"))?
                    as usize;
                if book.lengths[symbol] != 0 {
                    return Err(Error::Invalid("duplicate encoder Huffman symbol"));
                }
                book.lengths[symbol] = n as u8;
                book.codes[symbol] = code as u16;
                index += 1;
                code += 1;
            }
            code <<= 1;
        }
        if index != book.symbols.len() || index == 0 {
            return Err(Error::Invalid("encoder Huffman count"));
        }
        Ok(book)
    }

    /// Annex K.2 tree construction, including the dummy all-ones code point
    /// and the K.3 redistribution to the JPEG maximum of sixteen bits.
    pub fn optimized(histogram: &[u64; 256]) -> Result<Self> {
        let mut frequency = [0u64; 257];
        frequency[..256].copy_from_slice(histogram);
        frequency[256] = 1;
        let mut size = [0u16; 257];
        let mut next = [-1i16; 257];
        loop {
            let least = |except: Option<usize>| {
                let mut best = None;
                let mut smallest = u64::MAX;
                for (i, &v) in frequency.iter().enumerate() {
                    if v != 0 && Some(i) != except && v <= smallest {
                        smallest = v;
                        best = Some(i);
                    }
                }
                best
            };
            let Some(mut a) = least(None) else {
                break;
            };
            let Some(mut b) = least(Some(a)) else {
                break;
            };
            frequency[a] = frequency[a]
                .checked_add(frequency[b])
                .ok_or(Error::Invalid("Huffman histogram overflow"))?;
            frequency[b] = 0;
            size[a] += 1;
            while next[a] >= 0 {
                a = next[a] as usize;
                size[a] += 1;
            }
            next[a] = b as i16;
            size[b] += 1;
            while next[b] >= 0 {
                b = next[b] as usize;
                size[b] += 1;
            }
        }
        let mut bits = [0i32; 257];
        for &n in &size {
            if n > 0 {
                bits[n as usize] += 1;
            }
        }
        for i in (17..=256).rev() {
            while bits[i] > 0 {
                let mut j = i - 2;
                while j > 0 && bits[j] == 0 {
                    j -= 1;
                }
                if j == 0 || bits[i] < 2 {
                    return Err(Error::Invalid("Huffman length redistribution"));
                }
                bits[i] -= 2;
                bits[i - 1] += 1;
                bits[j + 1] += 2;
                bits[j] -= 1;
            }
        }
        let last = (1..=16)
            .rev()
            .find(|&n| bits[n] > 0)
            .ok_or(Error::Invalid("empty Huffman histogram"))?;
        bits[last] -= 1;
        let mut symbols = Vec::new();
        symbols
            .try_reserve_exact(256)
            .map_err(|_| Error::Allocation)?;
        for n in 1..=256 {
            for i in 0..256 {
                if size[i] as usize == n && histogram[i] > 0 {
                    symbols.push(i as u8);
                }
            }
        }
        let mut counts = [0u8; 16];
        for i in 0..16 {
            counts[i] =
                u8::try_from(bits[i + 1]).map_err(|_| Error::Invalid("Huffman length count"))?;
        }
        Self::new(counts, symbols)
    }

    /// Valid fixed fallback tables covering all DCT magnitudes and run symbols,
    /// including 12-bit coefficients. Optimized tables are the default.
    pub fn fixed(ac: bool, precision: u8, progressive: bool, differential: bool) -> Result<Self> {
        let mut counts = [0; 16];
        if ac {
            let symbols: Vec<u8> = (0..=255)
                .filter(|&symbol| {
                    let size = symbol & 15;
                    (size > 0 && size <= precision + 2 + u8::from(differential))
                        || symbol == 0
                        || symbol == 0xf0
                        || (progressive && size == 0)
                })
                .collect();
            if symbols.len() == 256 {
                counts[8] = 255;
                counts[9] = 1;
            } else {
                counts[7] = symbols.len() as u8;
            }
            Self::new(counts, symbols)
        } else {
            counts[if precision == 8 { 3 } else { 4 }] = precision + 4;
            Self::new(counts, (0..precision + 4).collect())
        }
    }

    pub fn lossless_fixed() -> Result<Self> {
        let mut counts = [0; 16];
        counts[4] = 17;
        let mut symbols = Vec::new();
        symbols
            .try_reserve_exact(17)
            .map_err(|_| Error::Allocation)?;
        symbols.extend(0..=16);
        Self::new(counts, symbols)
    }

    #[inline]
    pub fn write(&self, out: &mut Output, symbol: u8) -> Result<()> {
        let length = self.lengths[symbol as usize];
        if length == 0 {
            return Err(Error::Invalid("symbol absent from encoder Huffman table"));
        }
        out.write_bits(self.codes[symbol as usize] as u32, length)
    }

    pub fn emit(&self, out: &mut Output, class: u8, id: u8) -> Result<()> {
        let mut segment = [0u8; 273];
        segment[0] = (class << 4) | id;
        segment[1..17].copy_from_slice(&self.counts);
        segment[17..17 + self.symbols.len()].copy_from_slice(&self.symbols);
        out.segment(0xc4, &segment[..17 + self.symbols.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entropy::{Bits, Huffman};
    fn check(hist: [u64; 256]) {
        let book = Codebook::optimized(&hist).unwrap();
        let table = Huffman::new(&book.counts, &book.symbols).unwrap();
        let mut out = Output::new(10000);
        for (i, &frequency) in hist.iter().enumerate() {
            if frequency > 0 {
                book.write(&mut out, i as u8).unwrap();
            }
        }
        out.pad().unwrap();
        let mut bits = Bits::new(&out.bytes);
        for (i, &frequency) in hist.iter().enumerate() {
            if frequency > 0 {
                assert_eq!(table.decode(&mut bits).unwrap(), i as u8);
            }
        }
        bits.finish().unwrap();
        assert!(book.lengths.iter().all(|&n| n <= 16));
    }
    #[test]
    fn length_limited_trees_and_reserved_code() {
        check([1; 256]);
        let mut hist = [0; 256];
        hist[255] = 1;
        check(hist);
        let mut hist = [0; 256];
        let (mut a, mut b) = (1u64, 1u64);
        for v in &mut hist[..70] {
            *v = a;
            (a, b) = (b, a + b);
        }
        check(hist);
        let mut state = 0x81234567u32;
        for _ in 0..100 {
            let mut hist = [0; 256];
            for v in &mut hist {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                *v = (state >> 24) as u64;
            }
            check(hist);
        }
    }
}
