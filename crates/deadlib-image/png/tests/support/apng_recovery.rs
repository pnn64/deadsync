//! Independent APNG wire graphs and native known answers.
use super::chunks as wire;
use deadlib_image_png::{ColorType, Image, Samples};

#[derive(Clone)]
pub struct Chunk {
    pub kind: [u8; 4],
    pub data: Vec<u8>,
    pub bad_crc: bool,
}
impl Chunk {
    pub fn new(kind: [u8; 4], data: Vec<u8>) -> Self {
        Self {
            kind,
            data,
            bad_crc: false,
        }
    }
}
pub fn file(chunks: &[Chunk]) -> Vec<u8> {
    let mut result = b"\x89PNG\r\n\x1a\n".to_vec();
    for c in chunks {
        let mut bytes = wire::chunk(c.kind, &c.data);
        if c.bad_crc {
            *bytes.last_mut().unwrap() ^= 1;
        }
        result.extend(bytes);
    }
    result
}
pub fn is_animation(kind: &[u8; 4]) -> bool {
    matches!(kind, b"acTL" | b"fcTL" | b"fdAT")
}
pub fn image(width: u32, height: u32, color: ColorType, depth: u8, salt: usize) -> Image {
    let n = width as usize * height as usize * color.channels();
    let samples = if depth == 16 {
        Samples::U16((0..n).map(|n| (n * 37139 + salt) as u16).collect())
    } else {
        Samples::U8(
            (0..n)
                .map(|n| ((n * 19 + salt) % (1usize << depth)) as u8)
                .collect(),
        )
    };
    Image {
        width,
        height,
        color_type: color,
        bit_depth: depth,
        samples,
    }
}
pub fn filtered(image: &Image, interlaced: bool) -> Vec<u8> {
    let plain = [(0, 0, 1, 1)];
    let adam7 = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let mut result = Vec::new();
    let channels = image.color_type.channels();
    let depth = usize::from(image.bit_depth);
    for &(x, y, dx, dy) in if interlaced { &adam7[..] } else { &plain[..] } {
        let xs: Vec<_> = (x..image.width as usize).step_by(dx).collect();
        if xs.is_empty() {
            continue;
        }
        for row in (y..image.height as usize).step_by(dy) {
            result.push(0);
            let mut values = Vec::new();
            for &col in &xs {
                for channel in 0..channels {
                    let index = (row * image.width as usize + col) * channels + channel;
                    values.push(match &image.samples {
                        Samples::U8(s) => u16::from(s[index]),
                        Samples::U16(s) => s[index],
                    });
                }
            }
            if depth < 8 {
                let mut packed = vec![0; (values.len() * depth).div_ceil(8)];
                for (n, v) in values.into_iter().enumerate() {
                    packed[n * depth / 8] |= (v as u8) << (8 - depth - n * depth % 8);
                }
                result.extend(packed);
            } else if depth == 8 {
                result.extend(values.into_iter().map(|v| v as u8));
            } else {
                for v in values {
                    result.extend(v.to_be_bytes());
                }
            }
        }
    }
    result
}
fn control(sequence: u32, width: u32, height: u32, x: u32, y: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for n in [sequence, width, height, x, y] {
        data.extend(n.to_be_bytes());
    }
    data.extend([0, 7, 0, 0, 0, 0]);
    data
}
fn frame_data(sequence: u32, raw: &[u8]) -> Vec<u8> {
    let mut data = sequence.to_be_bytes().to_vec();
    data.extend(wire::stored(raw));
    data
}

pub struct Fixture {
    pub chunks: Vec<Chunk>,
    pub image: Image,
    pub frames: Vec<Image>,
    pub palette: Vec<[u8; 3]>,
    pub default_is_frame: bool,
    pub interlaced: bool,
}
impl Fixture {
    pub fn new(color: ColorType, depth: u8, interlaced: bool, default_is_frame: bool) -> Self {
        let image = image(3, 2, color, depth, 5);
        let frame0 = self::image(1, 1, color, depth, 7);
        let frame1 = self::image(1, 1, color, depth, 11);
        let mut h = Vec::new();
        h.extend(3u32.to_be_bytes());
        h.extend(2u32.to_be_bytes());
        h.extend([depth, color as u8, 0, 0, u8::from(interlaced)]);
        let mut chunks = vec![
            Chunk::new(*b"IHDR", h),
            Chunk::new(*b"acTL", [2u32.to_be_bytes(), 3u32.to_be_bytes()].concat()),
        ];
        let palette: Vec<[u8; 3]> = if color == ColorType::Indexed {
            (0..1usize << depth)
                .map(|n| [n as u8, (n * 71) as u8, (n * 113) as u8])
                .collect()
        } else {
            Vec::new()
        };
        if !palette.is_empty() {
            chunks.push(Chunk::new(
                *b"PLTE",
                palette.iter().flatten().copied().collect(),
            ));
        }
        chunks.push(Chunk::new(*b"tEXt", b"Stage\0before".to_vec()));
        if default_is_frame {
            chunks.push(Chunk::new(*b"fcTL", control(0, 3, 2, 0, 0)));
        }
        chunks.push(Chunk::new(
            *b"IDAT",
            wire::stored(&filtered(&image, interlaced)),
        ));
        if default_is_frame {
            chunks.push(Chunk::new(*b"fcTL", control(1, 1, 1, 1, 1)));
            chunks.push(Chunk::new(
                *b"fdAT",
                frame_data(2, &filtered(&frame1, interlaced)),
            ));
        } else {
            chunks.push(Chunk::new(*b"fcTL", control(0, 1, 1, 0, 0)));
            chunks.push(Chunk::new(
                *b"fdAT",
                frame_data(1, &filtered(&frame0, interlaced)),
            ));
            chunks.push(Chunk::new(*b"fcTL", control(2, 1, 1, 1, 1)));
            chunks.push(Chunk::new(
                *b"fdAT",
                frame_data(3, &filtered(&frame1, interlaced)),
            ));
        }
        chunks.push(Chunk::new(*b"tEXt", b"Stage\0after".to_vec()));
        chunks.push(Chunk::new(*b"IEND", Vec::new()));
        let frames = if default_is_frame {
            vec![image.clone(), frame1]
        } else {
            vec![frame0, frame1]
        };
        Self {
            chunks,
            image,
            frames,
            palette,
            default_is_frame,
            interlaced,
        }
    }
    pub fn wire(&self) -> Vec<u8> {
        file(&self.chunks)
    }
    pub fn static_wire(&self) -> Vec<u8> {
        file(
            &self
                .chunks
                .iter()
                .filter(|c| !is_animation(&c.kind))
                .cloned()
                .collect::<Vec<_>>(),
        )
    }
    pub fn cases(&self) -> Vec<(String, Vec<u8>)> {
        let mut cases = Vec::new();
        let index = |kind: &[u8; 4]| self.chunks.iter().position(|c| &c.kind == kind).unwrap();
        let ac = index(b"acTL");
        let fc = index(b"fcTL");
        let fd = index(b"fdAT");
        let mut edit = |name: &str, at: usize, data: Vec<u8>| {
            let mut chunks = self.chunks.clone();
            chunks[at].data = data;
            cases.push((name.to_owned(), file(&chunks)));
        };
        for size in [0, 7, 9] {
            edit(&format!("actl-length-{size}"), ac, vec![0; size]);
        }
        for (offset, values) in [
            (0, &[0, 1, 3, 0x8000_0000][..]),
            (4, &[0x8000_0000, 0xffff_ffff][..]),
        ] {
            for &n in values {
                let mut d = self.chunks[ac].data.clone();
                d[offset..offset + 4].copy_from_slice(&u32::to_be_bytes(n));
                edit(&format!("actl-{offset}-{n}"), ac, d);
            }
        }
        for size in [0, 25, 27] {
            edit(&format!("fctl-length-{size}"), fc, vec![0; size]);
        }
        for (offset, values) in [
            (0, &[1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff][..]),
            (4, &[0, 4, 0xffff_ffff][..]),
            (8, &[0, 3, 0xffff_ffff][..]),
            (12, &[3, 0xffff_ffff][..]),
            (16, &[2, 0xffff_ffff][..]),
        ] {
            for &n in values {
                let mut d = self.chunks[fc].data.clone();
                d[offset..offset + 4].copy_from_slice(&u32::to_be_bytes(n));
                edit(&format!("fctl-{offset}-{n}"), fc, d);
            }
        }
        for (offset, n) in [(24, 3), (24, 255), (25, 2), (25, 255)] {
            let mut d = self.chunks[fc].data.clone();
            d[offset] = n;
            edit(&format!("fctl-{offset}-{n}"), fc, d);
        }
        if self.default_is_frame {
            for offset in [4, 8] {
                let mut d = self.chunks[fc].data.clone();
                d[offset..offset + 4].copy_from_slice(&1u32.to_be_bytes());
                edit(&format!("default-frame-shrunken-{offset}"), fc, d);
            }
        }
        for size in [0, 3, 4] {
            let mut d = self.chunks[fd].data.clone();
            d.truncate(size);
            edit(&format!("fdat-length-{size}"), fd, d);
        }
        for n in [0, 999, 0x8000_0000] {
            let mut d = self.chunks[fd].data.clone();
            d[..4].copy_from_slice(&u32::to_be_bytes(n));
            edit(&format!("fdat-sequence-{n}"), fd, d);
        }
        let mut d = self.chunks[fd].data.clone();
        d[4] = 0;
        edit("frame-zlib-header", fd, d);
        let mut d = self.chunks[fd].data.clone();
        *d.last_mut().unwrap() ^= 1;
        edit("frame-adler", fd, d);
        let mut raw = filtered(
            if self.default_is_frame {
                &self.frames[1]
            } else {
                &self.frames[0]
            },
            self.interlaced,
        );
        raw[0] = 5;
        edit(
            "frame-filter",
            fd,
            frame_data(if self.default_is_frame { 2 } else { 1 }, &raw),
        );
        let mut raw = filtered(
            if self.default_is_frame {
                &self.frames[1]
            } else {
                &self.frames[0]
            },
            self.interlaced,
        );
        raw.push(0);
        edit(
            "frame-excess-output",
            fd,
            frame_data(if self.default_is_frame { 2 } else { 1 }, &raw),
        );
        for (name, at) in [
            ("missing-actl", ac),
            ("missing-fctl", fc),
            (
                "missing-fdat",
                self.chunks
                    .iter()
                    .rposition(|c| &c.kind == b"fdAT")
                    .unwrap(),
            ),
        ] {
            let mut chunks = self.chunks.clone();
            chunks.remove(at);
            cases.push((name.to_owned(), file(&chunks)));
        }
        let mut chunks = self.chunks.clone();
        chunks.insert(ac, self.chunks[ac].clone());
        cases.push(("duplicate-actl".into(), file(&chunks)));
        let mut chunks = self.chunks.clone();
        let c = chunks.remove(ac);
        let id = chunks.iter().position(|c| &c.kind == b"IDAT").unwrap();
        chunks.insert(id + 1, c);
        cases.push(("late-actl".into(), file(&chunks)));
        for at in [ac, fc, fd] {
            let mut chunks = self.chunks.clone();
            chunks[at].bad_crc = true;
            cases.push((
                format!("crc-{}", String::from_utf8_lossy(&chunks[at].kind)),
                file(&chunks),
            ));
        }
        let mut chunks = self.chunks.clone();
        chunks[ac].data.clear();
        for c in &mut chunks {
            if is_animation(&c.kind) {
                c.bad_crc = true;
            }
        }
        cases.push(("multiple-animation-faults".into(), file(&chunks)));
        cases
    }
}
pub fn formats() -> Vec<(ColorType, u8)> {
    let mut formats = Vec::new();
    for (color, depths) in [
        (ColorType::Grayscale, &[1, 2, 4, 8, 16][..]),
        (ColorType::Rgb, &[8, 16][..]),
        (ColorType::Indexed, &[1, 2, 4, 8][..]),
        (ColorType::GrayscaleAlpha, &[8, 16][..]),
        (ColorType::Rgba, &[8, 16][..]),
    ] {
        for &depth in depths {
            formats.push((color, depth));
        }
    }
    formats
}
