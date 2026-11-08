//! HDR known answers with independent chunk, stored-DEFLATE and CRC framing.
use super::chunks;
use deadlib_image_png::{ColorType, Image, Metadata, Samples};

pub const FORMATS: [(u8, u8); 15] = [
    (0, 1),
    (0, 2),
    (0, 4),
    (0, 8),
    (0, 16),
    (2, 8),
    (2, 16),
    (3, 1),
    (3, 2),
    (3, 4),
    (3, 8),
    (4, 8),
    (4, 16),
    (6, 8),
    (6, 16),
];

#[derive(Clone, Debug)]
pub struct Hdr {
    pub chromaticities: [u16; 8],
    pub luminance: [u32; 2],
    pub light: [u32; 2],
}
impl Default for Hdr {
    fn default() -> Self {
        Self {
            chromaticities: [35400, 14600, 8500, 39850, 6550, 2300, 15635, 16450],
            luminance: [40_000_000, 5],
            light: [10_000_000, 2_500_000],
        }
    }
}
impl Hdr {
    pub fn payloads(&self) -> [([u8; 4], Vec<u8>); 3] {
        let mut display = Vec::new();
        for v in self.chromaticities {
            display.extend(v.to_be_bytes());
        }
        for v in self.luminance {
            display.extend(v.to_be_bytes());
        }
        let light = self.light.into_iter().flat_map(u32::to_be_bytes).collect();
        [
            (*b"cICP", vec![9, 16, 0, 1]),
            (*b"mDCV", display),
            (*b"cLLI", light),
        ]
    }
    pub fn metadata(&self) -> [Metadata; 3] {
        [
            Metadata::Cicp([9, 16, 0, 1]),
            Metadata::MasteringDisplay {
                chromaticities: self.chromaticities,
                luminance: self.luminance,
            },
            Metadata::ContentLight(self.light),
        ]
    }
    pub fn valid_wire_integers(&self) -> bool {
        self.luminance
            .into_iter()
            .chain(self.light)
            .all(|n| n <= 0x7fff_ffff)
    }
    pub fn fixture(&self, color: u8, depth: u8, interlaced: bool) -> Vec<u8> {
        let extra: Vec<_> = self
            .payloads()
            .into_iter()
            .map(|(k, d)| chunks::chunk(k, &d))
            .collect();
        fixture(color, depth, interlaced, &extra)
    }
}

pub fn pixel(color: u8, depth: u8) -> Image {
    let color = match color {
        0 => ColorType::Grayscale,
        2 => ColorType::Rgb,
        3 => ColorType::Indexed,
        4 => ColorType::GrayscaleAlpha,
        6 => ColorType::Rgba,
        _ => unreachable!(),
    };
    let n = color.channels();
    let samples = if depth == 16 {
        Samples::U16([0x0001, 0x2345, 0x89ab, 0xfedc][..n].to_vec())
    } else if depth < 8 || color == ColorType::Indexed {
        Samples::U8(vec![1; n])
    } else {
        Samples::U8([1, 35, 137, 254][..n].to_vec())
    };
    Image {
        width: 1,
        height: 1,
        color_type: color,
        bit_depth: depth,
        samples,
    }
}
pub fn packed(color: u8, depth: u8) -> Vec<u8> {
    let image = pixel(color, depth);
    match image.samples {
        Samples::U16(v) => v.into_iter().flat_map(u16::to_be_bytes).collect(),
        Samples::U8(_) if depth < 8 => vec![1 << (8 - depth)],
        Samples::U8(v) => v,
    }
}
pub fn core(color: u8, depth: u8, interlaced: bool) -> Vec<Vec<u8>> {
    let header = chunks::chunk(
        *b"IHDR",
        &[
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            1,
            depth,
            color,
            0,
            0,
            u8::from(interlaced),
        ],
    );
    let mut out = vec![header];
    if color == 3 {
        out.push(chunks::chunk(*b"PLTE", &[0, 0, 0, 255, 255, 255]));
    }
    // A 1x1 Adam7 image has exactly one nonempty pass and the same filtered row.
    let mut row = vec![0];
    row.extend(packed(color, depth));
    out.push(chunks::chunk(*b"IDAT", &chunks::stored(&row)));
    out.push(chunks::chunk(*b"IEND", &[]));
    out
}
pub fn fixture(color: u8, depth: u8, interlaced: bool, extra: &[Vec<u8>]) -> Vec<u8> {
    let mut c = core(color, depth, interlaced);
    c.splice(1..1, extra.iter().cloned());
    chunks::file(&c)
}

/// Mutate every byte of both HDR payloads, keeping independent expectations.
pub fn byte_cases() -> impl Iterator<Item = (String, Hdr)> {
    (0..32).flat_map(|slot| {
        (0u8..=255).map(move |byte| {
            let mut h = Hdr::default();
            if slot < 16 {
                let v = &mut h.chromaticities[slot / 2];
                let mut bytes = v.to_be_bytes();
                bytes[slot % 2] = byte;
                *v = u16::from_be_bytes(bytes);
            } else {
                let v = if slot < 24 {
                    &mut h.luminance[(slot - 16) / 4]
                } else {
                    &mut h.light[(slot - 24) / 4]
                };
                let mut bytes = v.to_be_bytes();
                bytes[slot % 4] = byte;
                *v = u32::from_be_bytes(bytes);
            }
            (format!("byte-{slot}-{byte}"), h)
        })
    })
}
pub const BOUNDARIES: [u32; 15] = [
    0,
    1,
    255,
    256,
    9999,
    10000,
    49999,
    50000,
    100_000_000,
    0x0102_0304,
    0x7fff_fffe,
    0x7fff_ffff,
    0x8000_0000,
    0x8000_0001,
    0xffff_ffff,
];
pub fn boundary_cases() -> impl Iterator<Item = (String, Hdr)> {
    (0..4).flat_map(|slot| {
        BOUNDARIES.into_iter().map(move |v| {
            let mut h = Hdr::default();
            if slot < 2 {
                h.luminance[slot] = v;
            } else {
                h.light[slot - 2] = v;
            }
            (format!("integer-{slot}-{v}"), h)
        })
    })
}
