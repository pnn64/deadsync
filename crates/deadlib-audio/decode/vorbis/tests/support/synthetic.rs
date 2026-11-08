//! Procedural, non-silent setup/packet vectors, independent of the encoder.
use deadlib_audio_decode_vorbis::{
    bitpack::{BitWriter, ilog},
    *,
};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub modes: u8,
    pub first_long: bool,
    pub kind: u8,
    pub dimensions: u8,
    pub classifications: u8,
    pub stages: u8,
    pub empty_first_submap: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            modes: 3,
            first_long: false,
            kind: 1,
            dimensions: 1,
            classifications: 2,
            stages: 1,
            empty_first_submap: false,
        }
    }
}
fn put(bits: &mut BitWriter, fields: &[(u32, u32)]) {
    for &(value, width) in fields {
        bits.write(value, width).unwrap();
    }
}
impl Config {
    pub fn long(self, mode: u8) -> bool {
        (mode & 1 != 0) != self.first_long
    }
    pub fn headers(self) -> Vec<Vec<u8>> {
        let id = Identification {
            channels: 2,
            sample_rate: 48000,
            bitrate_maximum: -1,
            bitrate_nominal: -1,
            bitrate_minimum: -1,
            block_sizes: [64, 256],
        }
        .encode()
        .unwrap();
        let comments = Comments {
            vendor: b"deadlib synthetic setup parity",
            comments: vec![],
        }
        .encode(&Limits::default())
        .unwrap();
        let mut bits = BitWriter::new();
        // Scalar phrasebook: eight three-bit entries, with configurable dimension.
        put(
            &mut bits,
            &[
                (1, 8),
                (0x564342, 24),
                (self.dimensions as u32, 16),
                (8, 24),
                (0, 1),
                (0, 1),
            ],
        );
        for _ in 0..8 {
            bits.write(2, 5).unwrap();
        }
        // VQ book: two one-dimensional entries, values one and three.
        put(
            &mut bits,
            &[
                (0, 4),
                (0x564342, 24),
                (1, 16),
                (2, 24),
                (0, 1),
                (0, 1),
                (0, 5),
                (0, 5),
                (2, 4),
                (0, 32),
                ((788 << 21) | 1, 32),
                (1, 4),
                (0, 1),
                (1, 2),
                (3, 2),
                (0, 6),
                (0, 16), // Time transform.
                (0, 6),
                (1, 16),
                (0, 5),
                (0, 2),
                (6, 4),
            ],
        ); // Floor1, no partitions.
        let residues = if self.empty_first_submap { 2 } else { 1 };
        bits.write(residues - 1, 6).unwrap();
        for residue in 0..residues {
            let stages = if self.empty_first_submap && residue == 0 {
                0
            } else {
                self.stages
            };
            put(
                &mut bits,
                &[
                    (self.kind as u32, 16),
                    (0, 24),
                    (32, 24),
                    (7, 24),
                    (self.classifications as u32 - 1, 6),
                    (0, 8),
                ],
            );
            for _ in 0..self.classifications {
                put(
                    &mut bits,
                    &[(stages as u32 & 7, 3), (1, 1), (stages as u32 >> 3, 5)],
                );
            }
            for _ in 0..self.classifications {
                for stage in 0..8 {
                    if stages & (1 << stage) != 0 {
                        bits.write(1, 8).unwrap();
                    }
                }
            }
        }
        put(&mut bits, &[(0, 6), (0, 16)]); // One mapping, type zero.
        if self.empty_first_submap {
            put(&mut bits, &[(1, 1), (1, 4), (0, 1), (0, 2), (0, 4), (1, 4)]);
        } else {
            put(&mut bits, &[(0, 1), (0, 1), (0, 2)]);
        }
        for residue in 0..residues {
            put(&mut bits, &[(0, 8), (0, 8), (residue, 8)]);
        }
        bits.write(self.modes as u32 - 1, 6).unwrap();
        for mode in 0..self.modes {
            put(
                &mut bits,
                &[(self.long(mode) as u32, 1), (0, 16), (0, 16), (0, 8)],
            );
        }
        bits.write(1, 1).unwrap();
        let mut setup = b"\x05vorbis".to_vec();
        setup.extend(bits.finish().unwrap());
        vec![id, comments, setup]
    }
    /// Replace a particular phrasebook word (word, vector, entry). Entries
    /// outside classifications^dimensions exercise oversized legacy books.
    pub fn audio(
        self,
        mode: u8,
        windows: u32,
        seed: u32,
        replacement: Option<(usize, usize, u32)>,
    ) -> Vec<u8> {
        let mut bits = BitWriter::new();
        put(
            &mut bits,
            &[(0, 1), (mode as u32, ilog(self.modes as u32 - 1))],
        );
        if mode >= self.modes {
            return bits.finish().unwrap();
        }
        if self.long(mode) {
            bits.write(windows, 2).unwrap();
        }
        for channel in 0..2 {
            // Different nonzero, flat gains make misplaced channel data visible.
            let y = 245 - 13 * channel;
            put(&mut bits, &[(1, 1), (y, 8), (y, 8)]);
        }
        let vectors = if self.kind == 2 || self.empty_first_submap {
            1
        } else {
            2
        };
        let cardinality = (self.classifications as u32).pow(self.dimensions as u32);
        let stages = 8 - self.stages.leading_zeros();
        for stage in 0..stages {
            for (word, partition) in (0..4).step_by(self.dimensions as usize).enumerate() {
                if stage == 0 {
                    for vector in 0..vectors {
                        let value = match replacement {
                            Some((w, v, entry)) if w == word && v == vector => entry,
                            _ => (seed + word as u32 + vector as u32) % cardinality,
                        };
                        // Entry-order MSB codewords, carried by an LSB bitpacker.
                        bits.write(value.reverse_bits() >> 29, 3).unwrap();
                        if value >= cardinality {
                            // Reference aborts only this residue, without reading
                            // the following bytes or turning this into bit EOF.
                            bits.write(0x6a39c57b, 32).unwrap();
                            return bits.finish().unwrap();
                        }
                    }
                }
                if self.stages & (1 << stage) != 0 {
                    for p in partition..(partition + self.dimensions as usize).min(4) {
                        for vector in 0..vectors {
                            for bin in 0..8 {
                                bits.write((seed + stage + p as u32 + vector as u32 + bin) & 1, 1)
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
