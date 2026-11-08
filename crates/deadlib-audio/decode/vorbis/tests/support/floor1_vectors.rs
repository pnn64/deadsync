//! Independent floor-one wire vectors for guard and EOF comparisons.
use super::codec::{
    bitpack::{BitWriter, ilog},
    *,
};
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub channels: u8,
    pub multiplier: u8,
    pub range_bits: u8,
    pub posts: u8,
    pub high: u32,
    pub subclasses: u8,
    pub absent: bool,
    pub empty: bool,
    pub coupling: bool,
    pub kind: u8,
    pub submaps: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            channels: 3,
            multiplier: 4,
            range_bits: 7,
            posts: 1,
            high: 64,
            subclasses: 0,
            absent: false,
            empty: false,
            coupling: true,
            kind: 1,
            submaps: 2,
        }
    }
}
fn put(b: &mut BitWriter, fields: &[(u32, u32)]) {
    for &(v, n) in fields {
        b.write(v, n).unwrap();
    }
}
impl Config {
    fn values(self) -> Vec<u32> {
        if self.high >= 3 {
            vec![0, 1, self.high - 1, self.high]
        } else {
            vec![0, self.high]
        }
    }
    fn partitions(self) -> Vec<u8> {
        let mut left = self.posts;
        let mut out = Vec::new();
        while left > 0 {
            let n = left.min(8);
            out.push(n);
            left -= n;
        }
        out
    }
    fn book(self, b: &mut BitWriter, index: u8) {
        let (entries, used, width, lookup) = match index {
            0 => (8, (0..8).collect::<Vec<_>>(), 3, 0),
            1 => (
                self.high + 1,
                self.values(),
                if self.high >= 3 { 2 } else { 1 },
                0,
            ),
            2 => (2, vec![0, 1], 1, 0),
            3 => (2, vec![0, 1], 1, 2),
            4 => (4, if self.empty { vec![] } else { vec![0, 1, 2, 3] }, 2, 0),
            _ => unreachable!(),
        };
        let sparse = index == 1 || index == 4;
        put(
            b,
            &[
                (0x564342, 24),
                (1, 16),
                (entries, 24),
                (0, 1),
                (sparse as u32, 1),
            ],
        );
        for entry in 0..entries {
            let present = used.contains(&entry);
            if sparse {
                b.write(present as u32, 1).unwrap();
            }
            if present {
                b.write(width - 1, 5).unwrap();
            }
        }
        b.write(lookup, 4).unwrap();
        if lookup != 0 {
            put(
                b,
                &[
                    (0x80000000 | (788 << 21) | 1, 32),
                    ((787 << 21) | 1, 32),
                    (1, 4),
                    (0, 1),
                    (0, 2),
                    (3, 2),
                ],
            );
        }
    }
    pub fn headers(self) -> Vec<Vec<u8>> {
        assert!(
            self.high > 0
                && self.high < 1 << 20
                && self.posts <= 63
                && (self.posts as u32) < (1 << self.range_bits)
        );
        let id = Identification {
            channels: self.channels,
            sample_rate: 48000,
            bitrate_maximum: -1,
            bitrate_nominal: -1,
            bitrate_minimum: -1,
            block_sizes: [64, 256],
        }
        .encode()
        .unwrap();
        let comment = Comments {
            vendor: b"deadlib floor one guard parity",
            comments: vec![],
        }
        .encode(&Limits::default())
        .unwrap();
        let mut b = BitWriter::new();
        b.write(4, 8).unwrap();
        for i in 0..5 {
            self.book(&mut b, i);
        }
        put(&mut b, &[(0, 6), (0, 16), (0, 6), (1, 16)]);
        let parts = self.partitions();
        b.write(parts.len() as u32, 5).unwrap();
        for i in 0..parts.len() {
            b.write(i as u32, 4).unwrap();
        }
        for &dim in &parts {
            put(&mut b, &[(dim as u32 - 1, 3), (self.subclasses as u32, 2)]);
            if self.subclasses > 0 {
                b.write(0, 8).unwrap();
            }
            for slot in 0..1 << self.subclasses {
                let book = if self.absent || self.subclasses > 0 && slot % 3 == 1 {
                    0
                } else if self.empty {
                    5
                } else {
                    2
                };
                b.write(book, 8).unwrap();
            }
        }
        put(
            &mut b,
            &[(self.multiplier as u32 - 1, 2), (self.range_bits as u32, 4)],
        );
        for x in self.positions() {
            b.write(x, self.range_bits as u32).unwrap();
        }
        b.write(self.submaps as u32 - 1, 6).unwrap();
        for s in 0..self.submaps {
            put(
                &mut b,
                &[
                    (((self.kind + s) % 3) as u32, 16),
                    (1, 24),
                    (0xffffff, 24),
                    (11, 24),
                    (0, 6),
                    (2, 8),
                    (1, 3),
                    (0, 1),
                    (3, 8),
                ],
            );
        }
        put(&mut b, &[(0, 6), (0, 16)]);
        if self.submaps > 1 {
            put(&mut b, &[(1, 1), (self.submaps as u32 - 1, 4)]);
        } else {
            b.write(0, 1).unwrap();
        }
        b.write(self.coupling as u32, 1).unwrap();
        if self.coupling {
            b.write(self.channels as u32 - 2, 8).unwrap();
            for ch in 1..self.channels {
                b.write(ch as u32 - 1, ilog(self.channels as u32 - 1))
                    .unwrap();
                b.write(ch as u32, ilog(self.channels as u32 - 1)).unwrap();
            }
        }
        b.write(0, 2).unwrap();
        if self.submaps > 1 {
            for ch in 0..self.channels {
                b.write(self.mux(ch as usize) as u32, 4).unwrap();
            }
        }
        for s in 0..self.submaps {
            put(&mut b, &[(0, 8), (0, 8), (s as u32, 8)]);
        }
        b.write(1, 6).unwrap();
        for long in [0, 1] {
            put(&mut b, &[(long, 1), (0, 16), (0, 16), (0, 8)]);
        }
        b.write(1, 1).unwrap();
        let mut setup = b"\x05vorbis".to_vec();
        setup.extend(b.finish().unwrap());
        vec![id, comment, setup]
    }
    fn positions(self) -> Vec<u32> {
        let end = 1u32 << self.range_bits;
        let n = self.posts as usize;
        // Insertion order deliberately depends on later non-neighbor points.
        let mut sorted: Vec<_> = (1..=n).map(|i| i as u32 * end / (n as u32 + 1)).collect();
        let mut out = Vec::new();
        while !sorted.is_empty() {
            let i = sorted.len() / 2;
            out.push(sorted.remove(i));
        }
        out
    }
    pub fn floor_packet(
        self,
        prefix: u32,
        y0: u32,
        y1: u32,
        selection: u32,
        seed: usize,
    ) -> Vec<u8> {
        let mut b = BitWriter::new();
        b.write(0, prefix).unwrap();
        self.write_floor(&mut b, y0, y1, selection, seed);
        b.finish().unwrap()
    }
    fn write_floor(self, b: &mut BitWriter, y0: u32, y1: u32, selection: u32, seed: usize) {
        let width = ilog([256, 128, 86, 64][self.multiplier as usize - 1] - 1);
        put(b, &[(1, 1), (y0, width), (y1, width)]);
        let mut at = 0;
        for dim in self.partitions() {
            let mut value = if self.subclasses > 0 {
                b.write(selection.reverse_bits() >> 29, 3).unwrap();
                selection
            } else {
                0
            };
            for _ in 0..dim {
                let slot = value & ((1 << self.subclasses) - 1);
                value >>= self.subclasses;
                if !self.absent && !(self.subclasses > 0 && slot % 3 == 1) {
                    if self.empty {
                        return;
                    }
                    let count = self.values().len();
                    let word = (seed + at) % count;
                    let bits = if count == 4 { 2 } else { 1 };
                    b.write((word as u32).reverse_bits() >> (32 - bits), bits)
                        .unwrap();
                }
                at += 1;
            }
        }
    }
    fn mux(self, ch: usize) -> usize {
        (self.channels as usize - 1 - ch) % self.submaps as usize
    }
    pub fn audio(self, long: bool, windows: u32, seed: usize) -> Vec<u8> {
        assert!(!self.empty);
        let bins = if long { 128 } else { 32 };
        let mut b = BitWriter::new();
        put(&mut b, &[(0, 1), (long as u32, 1)]);
        if long {
            b.write(windows, 2).unwrap();
        }
        let width = ilog([256, 128, 86, 64][self.multiplier as usize - 1] - 1);
        let max: u32 = (1 << width) - 1;
        let mut active = vec![false; self.channels as usize];
        for (ch, nz) in active.iter_mut().enumerate() {
            *nz = !(seed + ch).is_multiple_of(4);
            if *nz {
                let ys = [0, 1, max / 2, max.saturating_sub(1), max];
                self.write_floor(
                    &mut b,
                    ys[(ch + seed) % 5],
                    ys[(ch + seed + 2) % 5],
                    ((seed + ch) % 8) as u32,
                    seed + ch,
                );
            } else {
                b.write(0, 1).unwrap();
            }
        }
        if self.coupling {
            for ch in 1..self.channels as usize {
                if active[ch - 1] || active[ch] {
                    active[ch - 1] = true;
                    active[ch] = true;
                }
            }
        }
        for s in 0..self.submaps as usize {
            let bundle: Vec<_> = (0..self.channels as usize)
                .filter(|&ch| self.mux(ch) == s)
                .collect();
            if !bundle.iter().any(|&ch| active[ch]) {
                continue;
            }
            let kind = (self.kind as usize + s) % 3;
            let actual = bins * if kind == 2 { bundle.len() } else { 1 };
            let parts = (actual - 1) / 12;
            let vectors = if kind == 2 {
                1
            } else {
                bundle.iter().filter(|&&ch| active[ch]).count()
            };
            for part in 0..parts {
                for _ in 0..vectors {
                    b.write(0, 1).unwrap();
                }
                let size = if kind == 2 {
                    let offset = 1 + part * 12;
                    let ch = bundle.len();
                    (offset + 12) / ch * ch - offset / ch * ch
                } else {
                    12
                };
                for v in 0..vectors {
                    for i in 0..size {
                        b.write(((seed + part + v + i) & 1) as u32, 1).unwrap();
                    }
                }
            }
        }
        b.write(0x6a39c57b, 32).unwrap();
        b.finish().unwrap()
    }
}
