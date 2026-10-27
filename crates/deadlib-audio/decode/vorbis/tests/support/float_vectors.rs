//! Independent packed-codebook vectors for custom-float boundaries.
use deadlib_audio_decode_vorbis::bitpack::BitWriter;
#[derive(Clone, Copy, Debug)]
pub struct Book {
    pub dimensions: u16,
    pub entries: u32,
    pub lookup: u8,
    pub minimum: u32,
    pub delta: u32,
    pub value_bits: u8,
    pub sequence: bool,
    pub sparse: bool,
}
impl Default for Book {
    fn default() -> Self {
        Self {
            dimensions: 1,
            entries: 4,
            lookup: 2,
            minimum: 0,
            delta: (788 << 21) | 1,
            value_bits: 16,
            sequence: false,
            sparse: false,
        }
    }
}
fn put(b: &mut BitWriter, v: u32, n: u32) {
    b.write(v, n).unwrap();
}
impl Book {
    pub fn lookup_count(self) -> usize {
        if self.lookup == 2 {
            return self.entries as usize * self.dimensions as usize;
        }
        let mut count = 1u32;
        while (count + 1)
            .checked_pow(self.dimensions as u32)
            .is_some_and(|n| n <= self.entries)
        {
            count += 1;
        }
        count as usize
    }
    pub fn quant(self, i: usize) -> u32 {
        [0, 1, 2, 3, (1u32 << self.value_bits) - 1, 1, 0, 2][i % 8]
            & ((1u32 << self.value_bits) - 1)
    }
    pub fn write(self, b: &mut BitWriter) {
        assert!(self.entries.is_power_of_two());
        let count = if self.sparse {
            self.entries / 2
        } else {
            self.entries
        };
        assert!(count > 0);
        let length = count.ilog2().max(1);
        for (v, n) in [
            (0x564342, 24),
            (self.dimensions as u32, 16),
            (self.entries, 24),
            (0, 1),
            (self.sparse as u32, 1),
        ] {
            put(b, v, n);
        }
        for i in 0..self.entries {
            let active = !self.sparse || i % 2 == 0;
            if self.sparse {
                put(b, active as u32, 1);
            }
            if active {
                put(b, length - 1, 5);
            }
        }
        put(b, self.lookup as u32, 4);
        put(b, self.minimum, 32);
        put(b, self.delta, 32);
        put(b, self.value_bits as u32 - 1, 4);
        put(b, self.sequence as u32, 1);
        for i in 0..self.lookup_count() {
            put(b, self.quant(i), self.value_bits as u32);
        }
    }
    pub fn packed(self) -> Vec<u8> {
        let mut b = BitWriter::new();
        self.write(&mut b);
        let mut out = b.finish().unwrap();
        out.push(0);
        out
    }
    pub fn setup_prefix(self) -> Vec<u8> {
        let mut b = BitWriter::new();
        put(&mut b, 0, 8);
        self.write(&mut b);
        let mut out = b"\x05vorbis".to_vec();
        out.extend(b.finish().unwrap());
        out.push(0); // The C private reader bounds arrays in whole remaining bytes.
        out
    }
}

/// Locate minima/deltas by walking only the packed book grammar. No native
/// codebook/header parser or native encoder contributes reconstructed values.
pub fn float_offsets(setup: &[u8]) -> Vec<Option<[usize; 2]>> {
    use deadlib_audio_decode_vorbis::bitpack::{BitReader, ilog};
    assert_eq!(&setup[..7], b"\x05vorbis");
    let mut b = BitReader::new(&setup[7..]);
    let count = b.read(8).unwrap() + 1;
    let mut out = Vec::new();
    for _ in 0..count {
        assert_eq!(b.read(24).unwrap(), 0x564342);
        let dim = b.read(16).unwrap();
        let entries = b.read(24).unwrap();
        if b.flag().unwrap() {
            b.read(5).unwrap();
            let mut i = 0;
            while i < entries {
                i += b.read(ilog(entries - i)).unwrap();
            }
        } else {
            let sparse = b.flag().unwrap();
            for _ in 0..entries {
                if !sparse || b.flag().unwrap() {
                    b.read(5).unwrap();
                }
            }
        }
        let lookup = b.read(4).unwrap();
        if lookup == 0 {
            out.push(None);
            continue;
        }
        assert!([1, 2].contains(&lookup));
        let minimum = 56 + b.position();
        b.read(32).unwrap();
        let delta = 56 + b.position();
        b.read(32).unwrap();
        let width = b.read(4).unwrap() + 1;
        b.read(1).unwrap();
        let values = if lookup == 2 {
            entries * dim
        } else {
            let mut count = 1u32;
            while (count + 1).checked_pow(dim).is_some_and(|n| n <= entries) {
                count += 1;
            }
            count
        };
        for _ in 0..values {
            b.read(width).unwrap();
        }
        out.push(Some([minimum, delta]));
    }
    out
}
pub fn replace_floats(headers: &mut [Vec<u8>], books: &[usize], minimum: u32, delta: u32) {
    let offsets = float_offsets(&headers[2]);
    for &book in books {
        for (position, word) in offsets[book].unwrap().into_iter().zip([minimum, delta]) {
            for i in 0..32 {
                let at = position + i;
                let mask = 1u8 << (at % 8);
                let byte = &mut headers[2][at / 8];
                *byte = (*byte & !mask) | (((word >> i) & 1) as u8) << (at % 8);
            }
        }
    }
}
