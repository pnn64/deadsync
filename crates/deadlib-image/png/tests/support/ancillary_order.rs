//! Literal PNG Table 7 representatives; no native serializer builds the wire.
use super::chunks::{chunk, file, header, stored};
use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, ColorType, Image, Png, Samples, Transparency,
};

pub const NAMES: [[u8; 4]; 18] = [
    *b"cHRM", *b"gAMA", *b"iCCP", *b"sBIT", *b"sRGB", *b"cICP", *b"mDCV", *b"cLLI", *b"bKGD",
    *b"hIST", *b"tRNS", *b"pHYs", *b"sPLT", *b"eXIf", *b"tIME", *b"tEXt", *b"zTXt", *b"iTXt",
];
// Slots: before IHDR, before PLTE, before IDAT, between IDATs, after IDAT,
// after IEND. Without a PLTE, slots 1 and 2 describe the same legal interval.
pub const PAIR_SLOTS: [usize; 3] = [1, 2, 4];

#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub color: u8,
    pub depth: u8,
    pub palette: bool,
}
impl Context {
    pub fn channels(self) -> usize {
        match self.color {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            6 => 4,
            _ => unreachable!(),
        }
    }
    pub fn packed(self) -> Vec<u8> {
        match self.depth {
            16 => [0, 1].repeat(self.channels()),
            8 => vec![1; self.channels()],
            n => vec![1 << (8 - n)],
        }
    }
    pub fn pixel(self) -> Png {
        let mut png = Png::new(Image {
            width: 1,
            height: 1,
            color_type: ColorType::try_from(self.color).unwrap(),
            bit_depth: self.depth,
            samples: if self.depth == 16 {
                Samples::U16(vec![1; self.channels()])
            } else {
                Samples::U8(vec![1; self.channels()])
            },
        });
        if self.palette {
            png.palette = vec![[0, 0, 0], [255, 255, 255]];
        }
        png
    }
}

pub fn contexts() -> Vec<Context> {
    let mut out = Vec::new();
    for (color, depths) in [
        (0, &[1, 2, 4, 8, 16][..]),
        (2, &[8, 16][..]),
        (3, &[1, 2, 4, 8][..]),
        (4, &[8, 16][..]),
        (6, &[8, 16][..]),
    ] {
        for &depth in depths {
            out.push(Context {
                color,
                depth,
                palette: color == 3,
            });
            if color == 2 || color == 6 {
                out.push(Context {
                    color,
                    depth,
                    palette: true,
                });
            }
        }
    }
    assert_eq!(out.len(), 19);
    out
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: [u8; 4],
    pub data: Vec<u8>,
    pub slot: usize,
}
pub fn entry(c: Context, kind: [u8; 4], slot: usize) -> Entry {
    let data = match &kind {
        b"cHRM" => [31270u32, 32900, 64000, 33000, 30000, 60000, 15000, 6000]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        b"gAMA" => 45455u32.to_be_bytes().to_vec(),
        b"iCCP" => {
            let profile: &[u8] = if c.color == 0 || c.color == 4 {
                include_bytes!("../fixtures/icc/sGrey-v4.icc")
            } else {
                include_bytes!("../fixtures/icc/sRGB-v4.icc")
            };
            let mut data = b"Profile\0\0".to_vec();
            data.extend(stored(profile));
            data
        }
        b"sBIT" => vec![1; if c.color == 3 { 3 } else { c.channels() }],
        b"sRGB" => vec![0],
        b"cICP" => vec![9, 16, 0, 1],
        b"mDCV" => [35400u16, 14600, 8500, 39850, 6550, 2300, 15635, 16450]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .chain([40_000_000u32, 5].into_iter().flat_map(u32::to_be_bytes))
            .collect(),
        b"cLLI" => [10_000_000u32, 2_500_000]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        b"bKGD" if c.color == 3 => vec![1],
        b"bKGD" if c.color == 0 || c.color == 4 => vec![0, 1],
        b"bKGD" => vec![0, 1, 0, 1, 0, 1],
        b"hIST" => vec![0, 1, 0, 1],
        b"tRNS" if c.color == 0 => vec![0, 1],
        b"tRNS" if c.color == 3 => vec![0, 255],
        b"tRNS" => vec![0, 1, 0, 1, 0, 1],
        b"pHYs" => vec![0, 0, 0, 1, 0, 0, 0, 1, 1],
        b"sPLT" => b"Palette\0\x08\x01\x02\x03\xff\0\x01".to_vec(),
        b"eXIf" => b"II\x2a\0\x08\0\0\0\0\0\0\0\0\0".to_vec(),
        b"tIME" => vec![7, 234, 10, 27, 23, 59, 60],
        b"tEXt" => b"Comment\0Exact text".to_vec(),
        b"zTXt" => {
            let mut data = b"Comment\0\0".to_vec();
            data.extend(stored(b"Exact text"));
            data
        }
        b"iTXt" => b"Comment\0\0\0en\0Title\0Exact text".to_vec(),
        _ => unreachable!(),
    };
    Entry { kind, data, slot }
}

// Independent table/relationship model, evaluated before invoking the codec.
pub fn valid(c: Context, entries: &[Entry]) -> bool {
    for (index, e) in entries.iter().enumerate() {
        if matches!(e.slot, 0 | 3 | 5) {
            return false;
        }
        let before_palette = NAMES[..8].contains(&e.kind);
        let before_data = before_palette || NAMES[8..14].contains(&e.kind);
        if (before_data && e.slot == 4) || (before_palette && c.palette && e.slot == 2) {
            return false;
        }
        if (e.kind == *b"hIST" && !c.palette)
            || (e.kind == *b"tRNS" && !matches!(c.color, 0 | 2 | 3))
            || (NAMES[8..11].contains(&e.kind) && c.palette && e.slot == 1)
        {
            return false;
        }
        if e.kind == *b"mDCV" && !entries.iter().any(|v| v.kind == *b"cICP") {
            return false;
        }
        for prior in &entries[..index] {
            if e.kind == prior.kind
                && !matches!(&e.kind, b"tEXt" | b"zTXt" | b"iTXt")
                && (e.kind != *b"sPLT"
                    || e.data.split(|&b| b == 0).next() == prior.data.split(|&b| b == 0).next())
            {
                return false;
            }
        }
    }
    true
}

pub fn companion(c: Context, entries: &mut Vec<Entry>) {
    if entries.iter().any(|v| v.kind == *b"mDCV") && !entries.iter().any(|v| v.kind == *b"cICP") {
        entries.insert(0, entry(c, *b"cICP", 1));
    }
}

pub fn fixture(c: Context, entries: &[Entry], split: usize) -> Vec<u8> {
    let mut raw = vec![0];
    raw.extend(c.packed());
    let zlib = stored(&raw);
    let split = split.min(zlib.len());
    let mut parts = Vec::new();
    for slot in 0..6 {
        parts.extend(
            entries
                .iter()
                .filter(|e| e.slot == slot)
                .map(|e| chunk(e.kind, &e.data)),
        );
        match slot {
            0 => parts.push(header(c.color, c.depth)),
            1 if c.palette => parts.push(chunk(*b"PLTE", &[0, 0, 0, 255, 255, 255])),
            2 => parts.push(chunk(*b"IDAT", &zlib[..split])),
            3 => parts.push(chunk(*b"IDAT", &zlib[split..])),
            4 => parts.push(chunk(*b"IEND", &[])),
            _ => (),
        }
    }
    file(&parts)
}

pub fn expected(c: Context, entries: &[Entry]) -> Png {
    let mut png = c.pixel();
    let mut ordered = entries.to_vec();
    ordered.sort_by_key(|e| e.slot);
    for e in ordered {
        if e.kind == *b"tRNS" {
            png.transparency = Some(match c.color {
                0 => Transparency::Grayscale(1),
                2 => Transparency::Rgb([1; 3]),
                3 => Transparency::Palette(vec![0, 255]),
                _ => unreachable!(),
            });
        } else {
            png.ancillary.push(AncillaryChunk {
                kind: e.kind,
                data: e.data,
                position: if e.slot == 4 {
                    ChunkPosition::AfterData
                } else if c.palette && e.slot == 2 {
                    ChunkPosition::BeforeData
                } else {
                    ChunkPosition::BeforePalette
                },
            });
        }
    }
    png
}
