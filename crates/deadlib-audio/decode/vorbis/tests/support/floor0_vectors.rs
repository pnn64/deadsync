//! Procedural floor-zero headers and packets, independent of the encoder.
use super::codec::{
    bitpack::{BitWriter, ilog},
    *,
};
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub order: u8,
    pub rate: u16,
    pub bark: u16,
    pub width: u8,
    pub offset: u8,
    pub books: u8,
    pub dimensions: u8,
    pub long: usize,
    pub channels: u8,
    pub coupling: bool,
    pub empty: bool,
    pub sequence: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            order: 1,
            rate: 48000,
            bark: 64,
            width: 6,
            offset: 30,
            books: 1,
            dimensions: 2,
            long: 256,
            channels: 2,
            coupling: false,
            empty: false,
            sequence: false,
        }
    }
}
fn put(b: &mut BitWriter, fields: &[(u32, u32)]) {
    for &(v, w) in fields {
        b.write(v, w).unwrap();
    }
}
fn scalar(b: &mut BitWriter, value: usize) {
    b.write((value as u32 % 4).reverse_bits() >> 30, 2).unwrap();
}
fn book(
    b: &mut BitWriter,
    dim: u8,
    entries: u32,
    lookup: bool,
    empty: bool,
    sequence: bool,
    lsp: bool,
) {
    put(
        b,
        &[
            (0x564342, 24),
            (dim as u32, 16),
            (entries, 24),
            (0, 1),
            (empty as u32, 1),
        ],
    );
    for _ in 0..entries {
        if empty {
            b.write(0, 1).unwrap();
        } else {
            b.write(if entries == 1 { 0 } else { 1 }, 5).unwrap();
        }
    }
    b.write(if lookup { 2 } else { 0 }, 4).unwrap();
    if lookup {
        put(
            b,
            &[
                ((787 << 21) | 1 | if lsp { 0 } else { 0x80000000 }, 32),
                ((786 << 21) | 1, 32),
                (2, 4),
                (sequence as u32, 1),
            ],
        );
        for i in 0..entries as usize * dim as usize {
            b.write(
                if lsp {
                    (i % 8) as u32
                } else {
                    [0, 1, 2, 3, 4, 0, 1, 4][i % 8]
                },
                3,
            )
            .unwrap();
        }
    }
}
impl Config {
    pub fn headers(self) -> Vec<Vec<u8>> {
        let id = Identification {
            channels: self.channels,
            sample_rate: 48000,
            bitrate_maximum: -1,
            bitrate_nominal: -1,
            bitrate_minimum: -1,
            block_sizes: [64, self.long],
        }
        .encode()
        .unwrap();
        let comments = Comments {
            vendor: b"deadlib floor-zero parameters",
            comments: vec![],
        }
        .encode(&Limits::default())
        .unwrap();
        let mut b = BitWriter::new();
        b.write(2, 8).unwrap();
        book(&mut b, 2, 1, false, false, false, false);
        book(
            &mut b,
            self.dimensions,
            4,
            true,
            self.empty,
            self.sequence,
            true,
        );
        book(&mut b, 2, 4, true, false, false, false);
        put(
            &mut b,
            &[
                (0, 6),
                (0, 16),
                (1, 6),
                (0, 16),
                (self.order as u32, 8),
                (self.rate as u32, 16),
                (self.bark as u32, 16),
                (self.width as u32, 6),
                (self.offset as u32, 8),
                (self.books as u32 - 1, 4),
            ],
        );
        for _ in 0..self.books {
            b.write(1, 8).unwrap();
        }
        put(&mut b, &[(1, 16), (0, 5), (0, 2), (13, 4), (1, 6)]);
        for _ in 0..2 {
            put(
                &mut b,
                &[
                    (1, 16),
                    (0, 24),
                    ((self.long / 2) as u32, 24),
                    (7, 24),
                    (0, 6),
                    (0, 8),
                    (1, 3),
                    (0, 1),
                    (2, 8),
                ],
            );
        }
        put(
            &mut b,
            &[(0, 6), (0, 16), (1, 1), (1, 4), (self.coupling as u32, 1)],
        );
        if self.coupling {
            put(
                &mut b,
                &[
                    (0, 8),
                    (0, ilog(self.channels as u32 - 1)),
                    (1, ilog(self.channels as u32 - 1)),
                ],
            );
        }
        b.write(0, 2).unwrap();
        for ch in 0..self.channels {
            b.write(u32::from(ch != 0), 4).unwrap();
        }
        put(
            &mut b,
            &[(0, 8), (0, 8), (0, 8), (0, 8), (1, 8), (1, 8), (1, 6)],
        );
        for long in [0, 1] {
            put(&mut b, &[(long, 1), (0, 16), (0, 16), (0, 8)]);
        }
        b.write(1, 1).unwrap();
        let mut setup = b"\x05vorbis".to_vec();
        setup.extend(b.finish().unwrap());
        vec![id, comments, setup]
    }
    pub fn maximum(self) -> u64 {
        (1u64 << self.width) - 1
    }
    pub fn amplitude(self, seed: usize) -> u64 {
        let m = self.maximum();
        if m == 0 {
            return 0;
        }
        match seed % 7 {
            0 => 0,
            1 => m / 4,
            2 => m / 2,
            3 => m,
            4 => 1,
            5 => 1u64 << (self.width - 1),
            _ => m - 1,
        }
    }
    fn write_floor(self, b: &mut BitWriter, amp: u64, selector: u32, seed: usize) -> bool {
        assert!(amp <= self.maximum());
        b.write(amp as u32, (self.width as u32).min(32)).unwrap();
        if self.width > 32 {
            b.write((amp >> 32) as u32, self.width as u32 - 32).unwrap();
        }
        if amp == 0 {
            return false;
        }
        b.write(selector, ilog(self.books as u32)).unwrap();
        if selector >= self.books as u32 {
            return false;
        }
        if !self.empty {
            for word in 0..(self.order as usize).div_ceil(self.dimensions as usize) {
                scalar(b, seed + word);
            }
        }
        true
    }
    pub fn floor_packet(self, prefix: u8, amp: u64, selector: u32, seed: usize) -> Vec<u8> {
        let mut b = BitWriter::new();
        b.write(0x55 & ((1 << prefix) - 1), prefix as u32).unwrap();
        self.write_floor(&mut b, amp, selector, seed);
        b.write(0x39c57b6a, 32).unwrap();
        b.finish().unwrap()
    }
    pub fn audio(self, long: bool, windows: u32, amp: u64, selector: u32, seed: usize) -> Vec<u8> {
        let mut b = BitWriter::new();
        put(&mut b, &[(0, 1), (long as u32, 1)]);
        if long {
            b.write(windows, 2).unwrap();
        }
        let mut nonzero = vec![false; self.channels as usize];
        nonzero[0] = self.write_floor(&mut b, amp, selector, seed);
        for (ch, active) in nonzero.iter_mut().enumerate().skip(1) {
            *active = !(seed + ch).is_multiple_of(5);
            b.write(*active as u32, 1).unwrap();
            if *active {
                put(
                    &mut b,
                    &[(255 - ch as u32 % 16, 8), (255 - ch as u32 % 16, 8)],
                );
            }
        }
        if self.coupling && (nonzero[0] || nonzero[1]) {
            nonzero[0] = true;
            nonzero[1] = true;
        }
        let parts = (if long { self.long / 2 } else { 32 }) / 8;
        for submap in 0..2 {
            let bundle: Vec<_> = (0..self.channels as usize)
                .filter(|&ch| (ch != 0) == (submap != 0) && nonzero[ch])
                .collect();
            for first in (0..parts).step_by(2) {
                for &ch in &bundle {
                    b.write(((seed + first + ch) & 1) as u32, 1).unwrap();
                }
                for part in first..(first + 2).min(parts) {
                    for &ch in &bundle {
                        for word in 0..4 {
                            scalar(&mut b, seed + part + ch + word);
                        }
                    }
                }
            }
        }
        b.write(0x6a39c57b, 32).unwrap();
        b.finish().unwrap()
    }
    pub fn sequence(self) -> Vec<Vec<u8>> {
        [
            (false, 0, 1),
            (true, 3, 2),
            (true, 0, 3),
            (false, 0, 5),
            (true, 0, 4),
        ]
        .into_iter()
        .map(|(long, w, seed)| self.audio(long, w, self.amplitude(seed), 0, seed))
        .collect()
    }
}

pub fn pcm_configurations() -> Vec<Config> {
    let mut out: Vec<_> = (0..=63)
        .map(|width| Config {
            width,
            ..Config::default()
        })
        .collect();
    for rate in [
        1, 2, 3, 7, 11, 25, 63, 1000, 8000, 11025, 16000, 22050, 32000, 44100, 48000, 65534, 65535,
    ] {
        for bark in [
            1, 2, 3, 7, 31, 32, 63, 64, 127, 128, 255, 256, 1023, 4095, 16383, 65535,
        ] {
            for long in [64, 256, 2048, 8192] {
                out.push(Config {
                    rate,
                    bark,
                    long,
                    ..Config::default()
                });
            }
        }
    }
    for order in [1, 2, 3, 7, 8, 31, 64, 127, 254, 255] {
        for dimensions in [1, 2, 3, 4, 8] {
            for sequence in [false, true] {
                out.push(Config {
                    order,
                    dimensions,
                    sequence,
                    ..Config::default()
                });
            }
        }
    }
    for offset in [0, 1, 60, 255] {
        for order in [1, 2, 7, 31, 255] {
            for width in [6, 31, 32, 33, 63] {
                out.push(Config {
                    offset,
                    order,
                    width,
                    dimensions: 3,
                    ..Config::default()
                });
            }
        }
    }
    for width in [0, 6, 31, 32, 33, 63] {
        for order in [1, 2, 31, 255] {
            out.push(Config {
                width,
                order,
                empty: true,
                rate: 65535,
                bark: 65535,
                ..Config::default()
            });
        }
    }
    for channels in [1, 2, 3, 8] {
        for coupling in [false, true] {
            if coupling && channels == 1 {
                continue;
            }
            for width in [6, 31, 32, 33, 63] {
                out.push(Config {
                    channels,
                    coupling,
                    width,
                    books: 16,
                    order: 7,
                    dimensions: 3,
                    ..Config::default()
                });
            }
        }
    }
    out
}
