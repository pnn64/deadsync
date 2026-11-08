//! Literal PNG Third Edition scalar field cases, without production encoding.
use super::chunks as wire;
use deadlib_image_png::{ColorType as C, Image, Metadata as M, Samples, Transparency};

pub const FORMATS: [(C, u8); 15] = [
    (C::Grayscale, 1),
    (C::Grayscale, 2),
    (C::Grayscale, 4),
    (C::Grayscale, 8),
    (C::Grayscale, 16),
    (C::Rgb, 8),
    (C::Rgb, 16),
    (C::Indexed, 1),
    (C::Indexed, 2),
    (C::Indexed, 4),
    (C::Indexed, 8),
    (C::GrayscaleAlpha, 8),
    (C::GrayscaleAlpha, 16),
    (C::Rgba, 8),
    (C::Rgba, 16),
];
pub const EDGES: [u32; 10] = [
    0,
    1,
    255,
    256,
    65535,
    65536,
    0x7fff_fffe,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
];
const CHRM: [u32; 8] = [31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000];

#[derive(Clone, Copy)]
pub struct Format {
    pub color: C,
    pub depth: u8,
    pub interlaced: bool,
    pub palette: Option<usize>,
}
impl Format {
    pub fn channels(self) -> usize {
        match self.color {
            C::Grayscale | C::Indexed => 1,
            C::Rgb => 3,
            C::GrayscaleAlpha => 2,
            C::Rgba => 4,
        }
    }
    pub fn image(self) -> Image {
        Image {
            width: 1,
            height: 1,
            color_type: self.color,
            bit_depth: self.depth,
            samples: if self.depth == 16 {
                Samples::U16(vec![0; self.channels()])
            } else {
                Samples::U8(vec![0; self.channels()])
            },
        }
    }
}
#[derive(Clone)]
pub struct Case {
    pub name: String,
    pub format: Format,
    pub kind: [u8; 4],
    pub data: Vec<u8>,
    pub valid: bool,
    pub metadata: Option<M>,
    pub transparency: Option<Transparency>,
    pub seed: bool,
    pub oracle: bool,
}
pub fn palette(count: usize) -> Vec<u8> {
    (0..count * 3).map(|n| (n * 71 + 17) as u8).collect()
}
impl Case {
    pub fn palette(&self) -> Vec<[u8; 3]> {
        let bytes = if self.kind == *b"PLTE" {
            self.data.clone()
        } else {
            self.format.palette.map_or_else(Vec::new, palette)
        };
        bytes.as_chunks::<3>().0.to_vec()
    }
    pub fn wire(&self) -> Vec<u8> {
        let f = self.format;
        let h = vec![
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            1,
            f.depth,
            f.color as u8,
            0,
            0,
            u8::from(f.interlaced),
        ];
        let mut chunks = vec![wire::chunk(*b"IHDR", &h)];
        let early = matches!(&self.kind, b"gAMA" | b"cHRM" | b"sBIT" | b"sRGB");
        let field = wire::chunk(self.kind, &self.data);
        if early {
            chunks.push(field.clone());
        }
        if let Some(count) = f.palette {
            chunks.push(wire::chunk(*b"PLTE", &palette(count)));
        }
        if !early {
            chunks.push(field);
        }
        let raw = vec![0; 1 + (f.channels() * usize::from(f.depth)).div_ceil(8)];
        chunks.push(wire::chunk(*b"IDAT", &wire::stored(&raw)));
        chunks.push(wire::chunk(*b"IEND", &[]));
        wire::file(&chunks)
    }
    pub fn c_records(&self) -> Vec<([u8; 4], Vec<u32>)> {
        let mut records = Vec::new();
        let palette = self.palette();
        if !palette.is_empty() {
            records.push((
                *b"PLTE",
                palette.iter().flatten().map(|&v| u32::from(v)).collect(),
            ));
        }
        if let Some(value) = &self.transparency {
            records.push((
                *b"tRNS",
                match value {
                    Transparency::Grayscale(v) => vec![u32::from(*v)],
                    Transparency::Rgb(v) => v.iter().map(|&v| u32::from(v)).collect(),
                    Transparency::Palette(v) => v.iter().map(|&v| u32::from(v)).collect(),
                },
            ));
        }
        if let Some(m) = &self.metadata {
            let values = match m {
                M::Gamma(v) => vec![*v],
                M::Chromaticities(v) => v.to_vec(),
                M::SignificantBits(v) => v.iter().map(|&v| u32::from(v)).collect(),
                M::Srgb(v) | M::BackgroundIndex(v) => vec![u32::from(*v)],
                M::BackgroundGray(v) => vec![u32::from(*v)],
                M::BackgroundRgb(v) => v.iter().map(|&v| u32::from(v)).collect(),
                M::Histogram(v) => v.iter().map(|&v| u32::from(v)).collect(),
                M::PhysicalDimensions { x, y, unit } => vec![*x, *y, u32::from(*unit)],
                M::Time(v) => v.iter().map(|&v| u32::from(v)).collect(),
                _ => unreachable!(),
            };
            records.push((self.kind, values));
        }
        records.sort_by_key(|r| r.0);
        records
    }
}
fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}
fn case(
    f: Format,
    kind: [u8; 4],
    data: Vec<u8>,
    valid: bool,
    metadata: Option<M>,
    suffix: impl std::fmt::Display,
) -> Case {
    Case {
        name: format!(
            "scalar-{}-{}-{}-{}-{suffix}",
            std::str::from_utf8(&kind).unwrap(),
            f.color as u8,
            f.depth,
            u8::from(f.interlaced)
        ),
        format: f,
        kind,
        data,
        valid,
        metadata,
        transparency: None,
        seed: false,
        oracle: false,
    }
}
pub fn visit(mut emit: impl FnMut(Case)) {
    for (color, depth) in FORMATS {
        for interlaced in [false, true] {
            let f = Format {
                color,
                depth,
                interlaced,
                palette: (color == C::Indexed).then_some(1),
            };
            // Every payload length surrounding each exact scalar layout.
            let significant = if color == C::Indexed { 3 } else { f.channels() };
            let background = match color {
                C::Indexed => 1,
                C::Grayscale | C::GrayscaleAlpha => 2,
                _ => 6,
            };
            for (kind, base, metadata) in [
                (*b"gAMA", words(&[45455]), M::Gamma(45455)),
                (*b"cHRM", words(&CHRM), M::Chromaticities(CHRM)),
                (*b"sRGB", vec![0], M::Srgb(0)),
                (
                    *b"sBIT",
                    vec![1; significant],
                    M::SignificantBits(vec![1; significant]),
                ),
                (
                    *b"pHYs",
                    [words(&[1, 1]), vec![0]].concat(),
                    M::PhysicalDimensions {
                        x: 1,
                        y: 1,
                        unit: 0,
                    },
                ),
                (
                    *b"tIME",
                    vec![7, 234, 10, 27, 12, 34, 56],
                    M::Time([2026, 10, 27, 12, 34, 56]),
                ),
                (
                    *b"bKGD",
                    vec![0; background],
                    match color {
                        C::Indexed => M::BackgroundIndex(0),
                        C::Grayscale | C::GrayscaleAlpha => M::BackgroundGray(0),
                        _ => M::BackgroundRgb([0; 3]),
                    },
                ),
            ] {
                for len in 0..=base.len() + 3 {
                    let mut data = base.clone();
                    data.resize(len, 0);
                    let mut c = case(
                        f,
                        kind,
                        data,
                        len == base.len(),
                        Some(metadata.clone()),
                        format!("length-{len}"),
                    );
                    c.seed = len.abs_diff(base.len()) <= 1;
                    c.oracle = c.valid;
                    emit(c);
                }
            }
            for position in 0..significant {
                for value in 0..=255u8 {
                    let mut data = vec![1; significant];
                    data[position] = value;
                    let maximum = if color == C::Indexed { 8 } else { depth };
                    let mut c = case(
                        f,
                        *b"sBIT",
                        data.clone(),
                        value != 0 && value <= maximum,
                        Some(M::SignificantBits(data)),
                        format!("byte-{position}-{value}"),
                    );
                    c.seed = [0, 1, maximum, maximum + 1, 255].contains(&value);
                    emit(c);
                }
            }
            for value in 0..=255u8 {
                let mut c = case(
                    f,
                    *b"sRGB",
                    vec![value],
                    value <= 3,
                    Some(M::Srgb(value)),
                    format!("intent-{value}"),
                );
                c.seed = value <= 4 || value == 255;
                c.oracle = value <= 3;
                emit(c);
                let mut c = case(
                    f,
                    *b"pHYs",
                    [words(&[1, 1]), vec![value]].concat(),
                    value <= 1,
                    Some(M::PhysicalDimensions {
                        x: 1,
                        y: 1,
                        unit: value,
                    }),
                    format!("unit-{value}"),
                );
                c.seed = value <= 2 || value == 255;
                emit(c);
            }
            for (position, minimum, maximum) in
                [(2, 1, 12), (3, 1, 31), (4, 0, 23), (5, 0, 59), (6, 0, 60)]
            {
                for value in 0..=255u8 {
                    let mut data = vec![7, 234, 10, 27, 12, 34, 56];
                    data[position] = value;
                    let expected = M::Time([
                        2026,
                        u16::from(data[2]),
                        u16::from(data[3]),
                        u16::from(data[4]),
                        u16::from(data[5]),
                        u16::from(data[6]),
                    ]);
                    let mut c = case(
                        f,
                        *b"tIME",
                        data,
                        (minimum..=maximum).contains(&value),
                        Some(expected),
                        format!("byte-{position}-{value}"),
                    );
                    c.seed = [0, minimum, maximum, maximum + 1, 255].contains(&value);
                    emit(c);
                }
            }
            for (kind, values) in [
                (*b"gAMA", vec![45455]),
                (*b"cHRM", CHRM.to_vec()),
                (*b"pHYs", vec![1, 1]),
            ] {
                for position in 0..values.len() {
                    for value in EDGES {
                        let mut integers = values.clone();
                        integers[position] = value;
                        let metadata = match &kind {
                            b"gAMA" => M::Gamma(value),
                            b"cHRM" => M::Chromaticities(integers.clone().try_into().unwrap()),
                            _ => M::PhysicalDimensions {
                                x: integers[0],
                                y: integers[1],
                                unit: 1,
                            },
                        };
                        let mut data = words(&integers);
                        if kind == *b"pHYs" {
                            data.push(1);
                        }
                        let mut c = case(
                            f,
                            kind,
                            data,
                            value <= 0x7fff_ffff && (kind != *b"gAMA" || value != 0),
                            Some(metadata),
                            format!("integer-{position}-{value}"),
                        );
                        c.seed = true;
                        emit(c);
                    }
                }
            }
            for kind in [*b"bKGD", *b"tRNS"] {
                if color == C::Indexed {
                    if kind == *b"bKGD" {
                        for count in [1, 1usize << depth] {
                            for value in 0..=255u8 {
                                let mut f = f;
                                f.palette = Some(count);
                                let mut c = case(
                                    f,
                                    kind,
                                    vec![value],
                                    usize::from(value) < count,
                                    Some(M::BackgroundIndex(value)),
                                    format!("palette-{count}-index-{value}"),
                                );
                                c.seed = [0, 1, 255].contains(&value)
                                    || usize::from(value).abs_diff(count) <= 1;
                                emit(c);
                            }
                        }
                    }
                    continue;
                }
                if kind == *b"tRNS" && matches!(color, C::GrayscaleAlpha | C::Rgba) {
                    continue;
                }
                let components = if matches!(color, C::Grayscale | C::GrayscaleAlpha) {
                    1
                } else {
                    3
                };
                for position in 0..components * 2 {
                    for value in 0..=255u8 {
                        let mut data = vec![0; components * 2];
                        data[position] = value;
                        let integers: Vec<_> = data
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|v| u16::from_be_bytes([v[0], v[1]]))
                            .collect();
                        let mask = ((1u32 << depth) - 1) as u16;
                        let metadata = (kind == *b"bKGD").then(|| {
                            if components == 1 {
                                M::BackgroundGray(integers[0] & mask)
                            } else {
                                M::BackgroundRgb([
                                    integers[0] & mask,
                                    integers[1] & mask,
                                    integers[2] & mask,
                                ])
                            }
                        });
                        let mut c = case(
                            f,
                            kind,
                            data,
                            true,
                            metadata,
                            format!("byte-{position}-{value}"),
                        );
                        if kind == *b"tRNS" {
                            c.transparency = Some(if components == 1 {
                                Transparency::Grayscale(integers[0])
                            } else {
                                Transparency::Rgb(integers.try_into().unwrap())
                            });
                        }
                        c.seed = [0, 1, 127, 128, 255].contains(&value);
                        emit(c);
                    }
                }
            }
            // PLTE lengths include all remainders and the 256-entry boundary.
            for len in 0usize..=771 {
                let mut no_palette = f;
                no_palette.palette = None;
                let mut data = palette(len.div_ceil(3));
                data.truncate(len);
                let valid = !matches!(color, C::Grayscale | C::GrayscaleAlpha)
                    && len != 0
                    && len % 3 == 0
                    && len <= 768
                    && (color != C::Indexed || len / 3 <= 1usize << depth);
                let mut c = case(
                    no_palette,
                    *b"PLTE",
                    data,
                    valid,
                    None,
                    format!("length-{len}"),
                );
                c.seed = [0, 1, 2, 3, 6, 767, 768, 769, 770, 771].contains(&len)
                    || (color == C::Indexed && len.abs_diff(3usize << depth) <= 1);
                c.oracle = valid && [3, 6, 768].contains(&len);
                emit(c);
            }
            let palettes = if color == C::Indexed {
                vec![1, 1usize << depth]
            } else {
                vec![0]
            };
            for count in palettes {
                let mut f = f;
                f.palette = (count != 0).then_some(count);
                let exact = match color {
                    C::Grayscale => 2,
                    C::Rgb => 6,
                    _ => count,
                };
                let end = if color == C::Indexed { count + 3 } else { 9 };
                for len in 0..=end {
                    let data = vec![0; len];
                    let valid = match color {
                        C::Grayscale | C::Rgb => len == exact,
                        C::Indexed => len <= count,
                        _ => false,
                    };
                    let mut c = case(
                        f,
                        *b"tRNS",
                        data.clone(),
                        valid,
                        None,
                        format!("palette-{count}-length-{len}"),
                    );
                    c.transparency = Some(match color {
                        C::Grayscale => Transparency::Grayscale(0),
                        C::Rgb => Transparency::Rgb([0; 3]),
                        _ => Transparency::Palette(data),
                    });
                    c.seed = len.abs_diff(exact) <= 1 || len <= 1;
                    c.oracle = valid && len != 0 && len == exact;
                    emit(c);
                }
            }
            let palettes = match color {
                C::Grayscale | C::GrayscaleAlpha => vec![0],
                C::Indexed => vec![1, 1usize << depth],
                _ => vec![1, 256],
            };
            for count in palettes {
                let mut f = f;
                f.palette = (count != 0).then_some(count);
                for len in 0..=2 * count + 3 {
                    let data = vec![1; len];
                    let mut c = case(
                        f,
                        *b"hIST",
                        data,
                        count != 0 && len == 2 * count,
                        Some(M::Histogram(vec![257; count])),
                        format!("palette-{count}-length-{len}"),
                    );
                    c.seed = len.abs_diff(2 * count) <= 1;
                    c.oracle = c.valid;
                    emit(c);
                }
            }
        }
    }
    // Complete timestamp year domain, including zero; calendar interpretation is
    // separate from the field ranges in PNG Table 26.
    let f = Format {
        color: C::Rgb,
        depth: 8,
        interlaced: false,
        palette: None,
    };
    for year in 0..=65535u16 {
        let mut data = year.to_be_bytes().to_vec();
        data.extend([2, 31, 23, 59, 60]);
        let mut c = case(
            f,
            *b"tIME",
            data,
            true,
            Some(M::Time([year, 2, 31, 23, 59, 60])),
            format!("year-{year}"),
        );
        c.seed = [0, 1, 255, 256, 65534, 65535].contains(&year);
        c.oracle = c.seed;
        emit(c);
    }
}
pub fn seeds() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    visit(|case| {
        if case.seed {
            out.push((case.name.clone(), case.wire()));
        }
    });
    out
}
