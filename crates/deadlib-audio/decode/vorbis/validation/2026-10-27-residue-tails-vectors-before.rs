//! Procedural residue geometry, mixed cascades and channel/submap packet order.
use deadlib_audio_decode_vorbis::{
    bitpack::{BitWriter, ilog},
    *,
};

const CASCADES: [u8; 4] = [0, 0x85, 0x7e, 0xff];
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub channels: u8,
    pub submaps: u8,
    pub kind: u8,
    pub class_dimensions: u8,
    pub begin: u32,
    pub clipped_end: bool,
    pub coupling: bool,
    /// Bit zero offsets type-two begin by one scalar; bit one leaves its
    /// partition size independent of channel count. Other types ignore this.
    pub alignment: u8,
}
fn put(bits: &mut BitWriter, fields: &[(u32, u32)]) {
    for &(value, width) in fields {
        bits.write(value, width).unwrap();
    }
}
impl Geometry {
    fn mux(self, channel: usize) -> usize {
        // Interleaved, reversed assignment makes each submap a noncontiguous
        // channel bundle and permits unoccupied submaps when submaps > channels.
        (self.channels as usize - 1 - channel) % self.submaps as usize
    }
    fn bundle(self, submap: usize) -> Vec<usize> {
        (0..self.channels as usize)
            .filter(|&ch| self.mux(ch) == submap)
            .collect()
    }
    fn residue(self, submap: usize) -> (u8, usize, usize, usize) {
        let kind = (self.kind as usize + submap) as u8 % 3;
        let channels = self.bundle(submap).len();
        let scale = if kind == 2 { channels.max(1) } else { 1 };
        let begin = self.begin as usize * scale + usize::from(kind == 2 && self.alignment & 1 != 0);
        let partition = 12
            * if kind == 2 && self.alignment & 2 != 0 {
                1
            } else {
                scale
            };
        let end = if self.clipped_end {
            0xffffff
        } else {
            begin + partition * 2 + 5
        };
        (kind, begin, end, partition)
    }
    fn pairs(self) -> Vec<[usize; 2]> {
        if self.coupling {
            // Cross-submap dependencies and propagation through silent floors.
            (1..self.channels as usize).map(|ch| [ch - 1, ch]).collect()
        } else {
            vec![]
        }
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
            vendor: b"deadlib residue geometry",
            comments: vec![],
        }
        .encode(&Limits::default())
        .unwrap();
        let mut bits = BitWriter::new();
        // Classbook digits encode four classification values. Every entry is
        // used; a word spans one, two or three partitions.
        let entries = 1u32 << (self.class_dimensions * 2);
        put(
            &mut bits,
            &[
                (4, 8),
                (0x564342, 24),
                (self.class_dimensions as u32, 16),
                (entries, 24),
                (0, 1),
                (0, 1),
            ],
        );
        for _ in 0..entries {
            bits.write(self.class_dimensions as u32 * 2 - 1, 5).unwrap();
        }
        bits.write(0, 4).unwrap();
        // Four distinct VQ books with dimensions 1..4, two entries each. Their
        // exactly representable signed values also expose interleave order.
        for dim in 1..=4 {
            put(
                &mut bits,
                &[
                    (0x564342, 24),
                    (dim, 16),
                    (2, 24),
                    (0, 1),
                    (0, 1),
                    (0, 5),
                    (0, 5),
                    (2, 4),
                    (0x80000000 | (788 << 21) | 1, 32),
                    ((786 << 21) | 1, 32),
                    (2, 4),
                    (0, 1),
                ],
            );
            for value in 0..dim * 2 {
                bits.write(value, 3).unwrap();
            }
        }
        put(
            &mut bits,
            &[
                (0, 6),
                (0, 16), // Time transform.
                (0, 6),
                (1, 16),
                (0, 5),
                (0, 2),
                (7, 4), // Floor1, no partitions.
                (self.submaps as u32 - 1, 6),
            ],
        );
        for submap in 0..self.submaps as usize {
            let (kind, begin, end, partition) = self.residue(submap);
            put(
                &mut bits,
                &[
                    (kind as u32, 16),
                    (begin as u32, 24),
                    (end as u32, 24),
                    (partition as u32 - 1, 24),
                    (3, 6),
                    (0, 8),
                ],
            );
            for cascade in CASCADES {
                put(
                    &mut bits,
                    &[(cascade as u32 & 7, 3), (1, 1), (cascade as u32 >> 3, 5)],
                );
            }
            for (class, cascade) in CASCADES.into_iter().enumerate() {
                for stage in 0..8 {
                    if cascade & (1 << stage) != 0 {
                        bits.write(((stage + class) % 4 + 1) as u32, 8).unwrap();
                    }
                }
            }
        }
        put(&mut bits, &[(0, 6), (0, 16)]); // Mapping zero.
        if self.submaps > 1 {
            put(&mut bits, &[(1, 1), (self.submaps as u32 - 1, 4)]);
        } else {
            bits.write(0, 1).unwrap();
        }
        let pairs = self.pairs();
        bits.write(u32::from(!pairs.is_empty()), 1).unwrap();
        if !pairs.is_empty() {
            bits.write(pairs.len() as u32 - 1, 8).unwrap();
            for pair in pairs {
                for ch in pair {
                    bits.write(ch as u32, ilog(self.channels as u32 - 1))
                        .unwrap();
                }
            }
        }
        bits.write(0, 2).unwrap();
        if self.submaps > 1 {
            for ch in 0..self.channels as usize {
                bits.write(self.mux(ch) as u32, 4).unwrap();
            }
        }
        for submap in 0..self.submaps {
            put(&mut bits, &[(0, 8), (0, 8), (submap as u32, 8)]);
        }
        bits.write(1, 6).unwrap();
        for long in [0, 1] {
            put(&mut bits, &[(long, 1), (0, 16), (0, 16), (0, 8)]);
        }
        bits.write(1, 1).unwrap();
        let mut setup = b"\x05vorbis".to_vec();
        setup.extend(bits.finish().unwrap());
        vec![id, comments, setup]
    }
    pub fn audio(self, long: bool, windows: u32, seed: usize) -> Vec<u8> {
        let bins = if long { 128 } else { 32 };
        let mut bits = BitWriter::new();
        put(&mut bits, &[(0, 1), (long as u32, 1)]);
        if long {
            bits.write(windows, 2).unwrap();
        }
        let mut nonzero = vec![false; self.channels as usize];
        for (ch, active) in nonzero.iter_mut().enumerate() {
            *active = !(ch + seed).is_multiple_of(3);
            bits.write(*active as u32, 1).unwrap();
            if *active {
                let y = 215 + ((ch + seed) % 31) as u32;
                put(&mut bits, &[(y, 8), (y, 8)]);
            }
        }
        for [magnitude, angle] in self.pairs() {
            if nonzero[magnitude] || nonzero[angle] {
                nonzero[magnitude] = true;
                nonzero[angle] = true;
            }
        }
        for submap in 0..self.submaps as usize {
            let bundle = self.bundle(submap);
            let (kind, begin, end, partition) = self.residue(submap);
            if !bundle.iter().any(|&ch| nonzero[ch]) {
                continue;
            }
            let actual = bins * if kind == 2 { bundle.len() } else { 1 };
            let begin = begin.min(actual);
            let partitions = (end.min(actual) - begin) / partition;
            let vectors: Vec<_> = if kind == 2 {
                vec![0]
            } else {
                bundle.iter().copied().filter(|&ch| nonzero[ch]).collect()
            };
            let class = |part: usize, vector: usize| (seed + part + vector + submap) % 4;
            for stage in 0..8 {
                for first in (0..partitions).step_by(self.class_dimensions as usize) {
                    if stage == 0 {
                        for &vector in &vectors {
                            let mut word = 0u32;
                            for offset in 0..self.class_dimensions as usize {
                                word = word * 4 + class(first + offset, vector) as u32;
                            }
                            let width = self.class_dimensions as u32 * 2;
                            bits.write(word.reverse_bits() >> (32 - width), width)
                                .unwrap();
                        }
                    }
                    for part in first..(first + self.class_dimensions as usize).min(partitions) {
                        for &vector in &vectors {
                            let class = class(part, vector);
                            if CASCADES[class] & (1 << stage) == 0 {
                                continue;
                            }
                            let dim = (stage + class) % 4 + 1;
                            for word in 0..partition / dim {
                                bits.write(
                                    ((seed + submap + stage + part + vector + word) & 1) as u32,
                                    1,
                                )
                                .unwrap();
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
