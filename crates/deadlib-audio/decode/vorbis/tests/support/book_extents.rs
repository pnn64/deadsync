//! Independent scalar-book declarations around the reference extent boundary.
use deadlib_audio_decode_vorbis::bitpack::BitWriter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Style {
    First,
    Last,
    Empty,
    Complete,
}
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub dimensions: u16,
    pub entries: u32,
    pub style: Style,
    pub lookup: u8,
}
impl Shape {
    pub fn packed(self) -> (Vec<u8>, usize) {
        let mut bits = BitWriter::new();
        for (value, width) in [
            (0x564342, 24),
            (self.dimensions as u32, 16),
            (self.entries, 24),
            (0, 1),
            ((self.style != Style::Complete) as u32, 1),
        ] {
            bits.write(value, width).unwrap();
        }
        for entry in 0..self.entries {
            let active = match self.style {
                Style::First => entry == 0,
                Style::Last => entry + 1 == self.entries,
                Style::Empty => false,
                Style::Complete => true,
            };
            if self.style != Style::Complete {
                bits.write(active as u32, 1).unwrap();
            }
            if active {
                let width = if self.style == Style::Complete {
                    assert!(self.entries.is_power_of_two());
                    self.entries.trailing_zeros().max(1)
                } else {
                    1
                };
                bits.write(width - 1, 5).unwrap();
            }
        }
        bits.write(self.lookup as u32, 4).unwrap();
        if self.lookup != 0 {
            for (value, width) in [(0, 32), ((788 << 21) | 1, 32), (0, 4), (0, 1)] {
                bits.write(value, width).unwrap();
            }
            let count = if self.lookup == 1 {
                1
            } else {
                self.dimensions as u32 * self.entries
            };
            for _ in 0..count {
                bits.write(1, 1).unwrap();
            }
        }
        let length = bits.position();
        bits.write(0x5a83c729, 32).unwrap();
        (bits.finish().unwrap(), length)
    }
}
pub fn matrix() -> Vec<Shape> {
    let mut out = Vec::new();
    for dimensions in [
        0u16, 1, 2, 3, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 511, 512, 1023, 1024,
        2047, 2048, 4095, 4096, 8191, 8192, 16383, 16384, 32767, 32768, 65535,
    ] {
        // Use the standard integer's bit width solely to choose probes. Neither
        // their acceptance nor their consumption is calculated by this builder.
        let width = u16::BITS - dimensions.leading_zeros();
        let entries = if width == 0 {
            vec![0, 1, 255, (1 << 24) - 1]
        } else {
            let edge = 1u32 << (24 - width);
            vec![edge / 2 - 1, edge / 2, edge - 1, edge, edge + 1]
        };
        for entries in entries {
            for style in [Style::First, Style::Last, Style::Empty, Style::Complete] {
                if entries == 0 && style != Style::Empty {
                    continue;
                }
                if style == Style::Complete && (!entries.is_power_of_two() || entries > 256) {
                    continue;
                }
                out.push(Shape {
                    dimensions,
                    entries,
                    style,
                    lookup: 0,
                });
            }
        }
    }
    for lookup in [1, 2] {
        for entries in [128, 256] {
            for style in [Style::First, Style::Empty] {
                out.push(Shape {
                    dimensions: 32768,
                    entries,
                    style,
                    lookup,
                });
            }
        }
    }
    out
}
