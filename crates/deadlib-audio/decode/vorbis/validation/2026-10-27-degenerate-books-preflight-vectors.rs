//! Independent wire vectors for scalar dimensions and zero-active-entry books.
use deadlib_audio_decode_vorbis::{
    bitpack::{BitWriter, ilog},
    *,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Use {
    Unused,
    FloorMaster,
    FloorSubclass,
    FloorZero,
    ResidueClass,
    ResidueVector,
}
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub usage: Use,
    pub dimensions: u16,
    pub entries: u32,
    pub lookup: u8,
    pub empty: bool,
    pub ordered_zero: bool,
    pub kind: u8,
    pub channels: u8,
    pub coupling: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            usage: Use::Unused,
            dimensions: 1,
            entries: 4,
            lookup: 0,
            empty: true,
            ordered_zero: false,
            kind: 1,
            channels: 2,
            coupling: false,
        }
    }
}
#[derive(Clone, Copy)]
struct Book {
    dimensions: u16,
    entries: u32,
    lookup: u8,
    empty: bool,
    ordered_zero: bool,
}
fn put(bits: &mut BitWriter, fields: &[(u32, u32)]) {
    for &(value, width) in fields {
        bits.write(value, width).unwrap();
    }
}
impl Book {
    fn write(self, bits: &mut BitWriter) {
        put(
            bits,
            &[
                (0x564342, 24),
                (self.dimensions as u32, 16),
                (self.entries, 24),
            ],
        );
        if self.ordered_zero && self.entries == 0 {
            put(bits, &[(1, 1), (0, 5)]);
        } else {
            put(bits, &[(0, 1), (self.empty as u32, 1)]);
            for _ in 0..self.entries {
                if self.empty {
                    bits.write(0, 1).unwrap();
                } else {
                    bits.write(self.width() - 1, 5).unwrap();
                }
            }
        }
        bits.write(self.lookup as u32, 4).unwrap();
        if self.lookup != 0 {
            put(bits, &[(0, 32), ((786 << 21) | 1, 32), (2, 4), (0, 1)]);
            let count = if self.lookup == 1 {
                if self.dimensions == 0 {
                    0
                } else {
                    let mut n = 0u32;
                    while (n + 1)
                        .checked_pow(self.dimensions as u32)
                        .is_some_and(|v| v <= self.entries)
                    {
                        n += 1;
                    }
                    n
                }
            } else {
                self.entries * self.dimensions as u32
            };
            for i in 0..count {
                bits.write(i % 7 + 1, 3).unwrap();
            }
        }
    }
    fn width(self) -> u32 {
        ilog(self.entries.saturating_sub(1)).max(1)
    }
    fn scalar(self, bits: &mut BitWriter, entry: u32) {
        assert!(!self.empty && self.entries != 0);
        if self.entries == 1 {
            bits.write(entry & 1, 1).unwrap();
            return;
        }
        let width = self.width();
        bits.write((entry % self.entries).reverse_bits() >> (32 - width), width)
            .unwrap();
    }
}
impl Config {
    pub fn needs_empty_policy(self) -> bool {
        self.empty || self.entries == 0
    }
    fn target(self) -> usize {
        match self.usage {
            Use::Unused => 4,
            Use::FloorZero | Use::ResidueVector => 3,
            _ => 2,
        }
    }
    fn books(self) -> [Book; 5] {
        let scalar = Book {
            dimensions: 1,
            entries: 4,
            lookup: 0,
            empty: false,
            ordered_zero: false,
        };
        let mut books = [
            scalar,
            Book {
                dimensions: 2,
                entries: 2,
                lookup: 2,
                ..scalar
            },
            scalar,
            Book {
                dimensions: 2,
                lookup: 2,
                ..scalar
            },
            scalar,
        ];
        books[self.target()] = Book {
            dimensions: self.dimensions,
            entries: self.entries,
            lookup: self.lookup,
            empty: self.needs_empty_policy(),
            ordered_zero: self.ordered_zero,
        };
        books
    }
    fn residue(self, submap: usize) -> (u8, usize, usize) {
        let kind = (self.kind + submap as u8) % 3;
        let channels = if submap == 0 {
            1
        } else {
            self.channels as usize - 1
        };
        let partition = 12 * if kind == 2 { channels } else { 1 };
        (kind, partition, partition * 2 + 3)
    }
    pub fn headers(self) -> Vec<Vec<u8>> {
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
        let comments = Comments {
            vendor: b"deadlib codebook compatibility",
            comments: vec![],
        }
        .encode(&Limits::default())
        .unwrap();
        let mut bits = BitWriter::new();
        bits.write(4, 8).unwrap();
        for book in self.books() {
            book.write(&mut bits);
        }
        put(&mut bits, &[(0, 6), (0, 16), (1, 6)]); // Time, two floors.
        if self.usage == Use::FloorZero {
            put(
                &mut bits,
                &[
                    (0, 16),
                    (1, 8),
                    (48000, 16),
                    (64, 16),
                    (6, 6),
                    (60, 8),
                    (0, 4),
                    (3, 8),
                ],
            );
        } else if matches!(self.usage, Use::FloorMaster | Use::FloorSubclass) {
            put(&mut bits, &[(1, 16), (1, 5), (0, 4), (0, 3)]);
            if self.usage == Use::FloorMaster {
                put(&mut bits, &[(1, 2), (2, 8), (1, 8), (0, 8)]);
            } else {
                put(&mut bits, &[(0, 2), (3, 8)]);
            }
            put(&mut bits, &[(0, 2), (7, 4), (32, 7)]);
        } else {
            put(&mut bits, &[(1, 16), (0, 5), (0, 2), (7, 4)]);
        }
        put(&mut bits, &[(1, 16), (0, 5), (0, 2), (7, 4), (1, 6)]); // Floor B, two residues.
        for submap in 0..2 {
            let (kind, partition, end) = self.residue(submap);
            let class = if submap == 0 && self.usage == Use::ResidueClass {
                2
            } else {
                0
            };
            put(
                &mut bits,
                &[
                    (kind as u32, 16),
                    (0, 24),
                    (end as u32, 24),
                    (partition as u32 - 1, 24),
                    (0, 6),
                    (class, 8),
                    (7, 3),
                    (0, 1),
                    (1, 8),
                    (
                        if submap == 0 && self.usage == Use::ResidueVector {
                            3
                        } else {
                            1
                        },
                        8,
                    ),
                    (1, 8),
                ],
            );
        }
        put(
            &mut bits,
            &[(0, 6), (0, 16), (1, 1), (1, 4), (self.coupling as u32, 1)],
        );
        if self.coupling {
            put(
                &mut bits,
                &[
                    (0, 8),
                    (0, ilog(self.channels as u32 - 1)),
                    (1, ilog(self.channels as u32 - 1)),
                ],
            );
        }
        bits.write(0, 2).unwrap();
        for ch in 0..self.channels {
            bits.write(u32::from(ch != 0), 4).unwrap();
        }
        put(
            &mut bits,
            &[(0, 8), (0, 8), (0, 8), (0, 8), (1, 8), (1, 8), (1, 6)],
        );
        for long in [0, 1] {
            put(&mut bits, &[(long, 1), (0, 16), (0, 16), (0, 8)]);
        }
        bits.write(1, 1).unwrap();
        let mut setup = b"\x05vorbis".to_vec();
        setup.extend(bits.finish().unwrap());
        vec![id, comments, setup]
    }
    pub fn audio(self, long: bool, windows: u32, seed: usize) -> Vec<u8> {
        let books = self.books();
        let mut bits = BitWriter::new();
        put(&mut bits, &[(0, 1), (long as u32, 1)]);
        if long {
            bits.write(windows, 2).unwrap();
        }
        let mut nonzero = vec![false; self.channels as usize];
        for (ch, active) in nonzero.iter_mut().enumerate() {
            *active = !(seed + ch).is_multiple_of(4);
            if ch == 0 && self.usage == Use::FloorZero {
                bits.write(
                    if *active {
                        (10 + seed * 7 % 35) as u32
                    } else {
                        0
                    },
                    6,
                )
                .unwrap();
                if *active {
                    bits.write(0, 1).unwrap();
                    if !books[3].empty {
                        books[3].scalar(&mut bits, seed as u32);
                    }
                }
            } else {
                bits.write(*active as u32, 1).unwrap();
                if *active {
                    let y = 230 - (ch % 12) as u32;
                    put(&mut bits, &[(y, 8), (y, 8)]);
                    if ch == 0 && self.usage == Use::FloorMaster {
                        if books[2].empty {
                            *active = false;
                        } else {
                            let choice = seed as u32 % books[2].entries;
                            books[2].scalar(&mut bits, seed as u32);
                            if choice & 1 == 0 {
                                books[0].scalar(&mut bits, seed as u32 % 4);
                            }
                        }
                    } else if ch == 0 && self.usage == Use::FloorSubclass {
                        if books[2].empty {
                            *active = false;
                        } else {
                            books[2].scalar(&mut bits, seed as u32);
                        }
                    }
                }
            }
        }
        if self.coupling && (nonzero[0] || nonzero[1]) {
            nonzero[0] = true;
            nonzero[1] = true;
        }
        let bins = if long { 128 } else { 32 };
        for submap in 0..2 {
            let bundle: Vec<_> = if submap == 0 {
                vec![0]
            } else {
                (1..self.channels as usize).collect()
            };
            if !bundle.iter().any(|&ch| nonzero[ch]) {
                continue;
            }
            let (kind, partition, end) = self.residue(submap);
            let actual = bins * if kind == 2 { bundle.len() } else { 1 };
            let partitions = end.min(actual) / partition;
            let classbook = books[if submap == 0 && self.usage == Use::ResidueClass {
                2
            } else {
                0
            }];
            if classbook.empty {
                continue;
            }
            let vectors: Vec<_> = if kind == 2 {
                vec![0]
            } else {
                bundle.into_iter().filter(|&ch| nonzero[ch]).collect()
            };
            for stage in 0..3 {
                for first in (0..partitions).step_by(classbook.dimensions as usize) {
                    if stage == 0 {
                        for _ in &vectors {
                            classbook.scalar(&mut bits, 0);
                        }
                    }
                    for part in first..(first + classbook.dimensions as usize).min(partitions) {
                        for &vector in &vectors {
                            let book = books[if stage == 1
                                && submap == 0
                                && self.usage == Use::ResidueVector
                            {
                                3
                            } else {
                                1
                            }];
                            if book.empty {
                                continue;
                            }
                            for word in 0..partition / book.dimensions as usize {
                                book.scalar(
                                    &mut bits,
                                    (seed + part + vector + stage + word) as u32,
                                );
                            }
                        }
                    }
                }
            }
        }
        bits.write(0x6a39c57b, 32).unwrap();
        bits.finish().unwrap()
    }
}
