//! Bounded native animation descriptions, literal PNG framing and rational oracle.
//! This module never calls the native encoder or compositor to build an answer.
use super::{apng_recovery::filtered, chunks};
use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, Frame, FrameControl, Image, Png, Samples,
    Transparency,
};

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

struct Input<'a> {
    data: &'a [u8],
    index: usize,
}
impl Input<'_> {
    fn byte(&mut self) -> u8 {
        let value = self.data[self.index % self.data.len()];
        self.index += 1;
        value
    }
    fn word(&mut self) -> u16 {
        u16::from_be_bytes([self.byte(), self.byte()])
    }
}

#[derive(Clone)]
pub struct Scenario {
    pub png: Png,
    pub split: usize,
    pub control_before_animation: bool,
}

impl Scenario {
    /// 16..4096 input bytes; allocation/work bounds are independent of integers
    /// present in the input: at most 9x7 pixels, eight frames, 16 palette entries.
    pub fn from_bytes(data: &[u8]) -> Self {
        assert!(!data.is_empty());
        let (color, depth) = FORMATS[usize::from(data[0]) % FORMATS.len()];
        let mut input = Input { data, index: 1 };
        let width = 1 + u32::from(input.byte() % 9);
        let height = 1 + u32::from(input.byte() % 7);
        let flags = input.byte();
        let default_is_frame = flags & 1 != 0;
        let interlaced = flags & 2 != 0;
        let count = 1 + usize::from(input.byte() % 8);
        let make_image = |w: u32, h: u32, input: &mut Input<'_>| {
            let channels = match color {
                0 | 3 => 1,
                2 => 3,
                4 => 2,
                6 => 4,
                _ => unreachable!(),
            };
            let mask = (1u32 << depth) - 1;
            let mut values = Vec::new();
            for _ in 0..w as usize * h as usize * channels {
                let raw = input.word();
                let value = match raw & 15 {
                    0 => 0,
                    1 => mask,
                    2 => 1,
                    3 => mask.saturating_sub(1),
                    4 => mask / 2,
                    5 => mask / 2 + 1,
                    _ => u32::from(raw) & mask,
                };
                // A flag permits invalid palette indices for 13.1 recovery;
                // regular fixtures stay within their small complete palette.
                values.push(if color == 3 && flags & 4 == 0 {
                    (value % 2) as u16
                } else {
                    value as u16
                });
            }
            Image {
                width: w,
                height: h,
                color_type: ColorType::try_from(color).unwrap(),
                bit_depth: depth,
                samples: if depth == 16 {
                    Samples::U16(values)
                } else {
                    Samples::U8(values.into_iter().map(|v| v as u8).collect())
                },
            }
        };
        let mut png = Png::new(make_image(width, height, &mut input));
        png.interlaced = interlaced;
        if color == 3 {
            png.palette = vec![
                [input.byte(), input.byte(), input.byte()],
                [input.byte(), input.byte(), input.byte()],
            ];
            // Short transparency tables exercise the default opaque entries.
            png.transparency = Some(Transparency::Palette(
                [0u8, 1, 127, 128, 254, 255][usize::from(input.byte()) % 6..]
                    .iter()
                    .copied()
                    .take(usize::from(input.byte() % 3))
                    .collect(),
            ));
        } else if flags & 8 != 0 && matches!(color, 0 | 2) {
            let mask = ((1u32 << depth) - 1) as u16;
            let mut key = || input.word() & mask;
            png.transparency = Some(if color == 0 {
                Transparency::Grayscale(key())
            } else {
                Transparency::Rgb([key(), key(), key()])
            });
        }
        let mut frames = Vec::new();
        for index in 0..count {
            let (w, h, x, y) = if index == 0 && default_is_frame {
                (width, height, 0, 0)
            } else {
                let w = 1 + u32::from(input.byte()) % width;
                let h = 1 + u32::from(input.byte()) % height;
                let x = u32::from(input.byte()) % (width - w + 1);
                let y = u32::from(input.byte()) % (height - h + 1);
                (w, h, x, y)
            };
            let control = FrameControl {
                width: w,
                height: h,
                x_offset: x,
                y_offset: y,
                delay_num: input.word(),
                delay_den: input.word(),
                dispose: [DisposeOp::None, DisposeOp::Background, DisposeOp::Previous]
                    [usize::from(input.byte()) % 3],
                blend: if input.byte() & 1 == 0 {
                    BlendOp::Source
                } else {
                    BlendOp::Over
                },
            };
            frames.push(Frame {
                control,
                image: if index == 0 && default_is_frame {
                    None
                } else {
                    Some(make_image(w, h, &mut input))
                },
            });
        }
        png.animation = Some(Animation {
            plays: u32::from(input.word()),
            default_is_frame,
            frames,
        });
        Self {
            png,
            split: 1 + usize::from(input.byte() % 31),
            control_before_animation: flags & 16 != 0,
        }
    }

    pub fn wire(&self) -> Vec<u8> {
        let png = &self.png;
        let animation = png.animation.as_ref().unwrap();
        let mut header = [
            png.image.width.to_be_bytes(),
            png.image.height.to_be_bytes(),
        ]
        .concat();
        header.extend([
            png.image.bit_depth,
            png.image.color_type as u8,
            0,
            0,
            u8::from(png.interlaced),
        ]);
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend(chunks::chunk(*b"IHDR", &header));
        if !png.palette.is_empty() {
            out.extend(chunks::chunk(
                *b"PLTE",
                &png.palette.iter().flatten().copied().collect::<Vec<_>>(),
            ));
        }
        if let Some(t) = &png.transparency {
            let raw = match t {
                Transparency::Grayscale(v) => v.to_be_bytes().to_vec(),
                Transparency::Rgb(v) => v.iter().flat_map(|v| v.to_be_bytes()).collect(),
                Transparency::Palette(v) => v.clone(),
            };
            out.extend(chunks::chunk(*b"tRNS", &raw));
        }
        let control = |c: FrameControl, sequence: u32| {
            let mut data = [sequence, c.width, c.height, c.x_offset, c.y_offset]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect::<Vec<_>>();
            data.extend(c.delay_num.to_be_bytes());
            data.extend(c.delay_den.to_be_bytes());
            data.extend([
                match c.dispose {
                    DisposeOp::None => 0,
                    DisposeOp::Background => 1,
                    DisposeOp::Previous => 2,
                },
                match c.blend {
                    BlendOp::Source => 0,
                    BlendOp::Over => 1,
                },
            ]);
            chunks::chunk(*b"fcTL", &data)
        };
        if animation.default_is_frame && self.control_before_animation {
            out.extend(control(animation.frames[0].control, 0));
        }
        out.extend(chunks::chunk(
            *b"acTL",
            &[
                u32::try_from(animation.frames.len()).unwrap().to_be_bytes(),
                animation.plays.to_be_bytes(),
            ]
            .concat(),
        ));
        if animation.default_is_frame && !self.control_before_animation {
            out.extend(control(animation.frames[0].control, 0));
        }
        for data in chunks::stored(&filtered(&png.image, png.interlaced)).chunks(self.split) {
            out.extend(chunks::chunk(*b"IDAT", data));
        }
        let mut sequence = u32::from(animation.default_is_frame);
        for frame in &animation.frames[usize::from(animation.default_is_frame)..] {
            out.extend(control(frame.control, sequence));
            sequence += 1;
            let image = frame.image.as_ref().unwrap();
            // An empty frame_data field is legal and still consumes a sequence.
            out.extend(chunks::chunk(*b"fdAT", &sequence.to_be_bytes()));
            sequence += 1;
            for data in chunks::stored(&filtered(image, png.interlaced)).chunks(self.split) {
                let mut body = sequence.to_be_bytes().to_vec();
                body.extend(data);
                out.extend(chunks::chunk(*b"fdAT", &body));
                sequence += 1;
            }
        }
        out.extend(chunks::chunk(*b"IEND", &[]));
        out
    }
}

pub fn seed(format: usize, interlaced: bool, default_is_frame: bool, variant: u8) -> Vec<u8> {
    let mut state = 0x9286_71a3u32 ^ (format as u32 * 7919) ^ u32::from(variant);
    let mut bytes = Vec::new();
    for _ in 0..128 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        bytes.push(state as u8);
    }
    bytes[..5].copy_from_slice(&[
        format as u8,
        8,
        6,
        u8::from(default_is_frame) | (u8::from(interlaced) << 1) | 8 | ((variant & 2) << 3),
        7,
    ]);
    bytes
}

pub fn fixtures() -> Vec<(String, Scenario)> {
    let mut cases = Vec::new();
    let disposal = [
        DisposeOp::None,
        DisposeOp::None,
        DisposeOp::Background,
        DisposeOp::None,
        DisposeOp::Previous,
        DisposeOp::Background,
        DisposeOp::Background,
        DisposeOp::Previous,
        DisposeOp::Previous,
    ];
    for format in 0..FORMATS.len() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                for variant in 0..4 {
                    let mut case =
                        Scenario::from_bytes(&seed(format, interlaced, default_is_frame, variant));
                    for (index, frame) in case
                        .png
                        .animation
                        .as_mut()
                        .unwrap()
                        .frames
                        .iter_mut()
                        .enumerate()
                    {
                        frame.control.dispose =
                            disposal[(index + usize::from(variant) * 2) % disposal.len()];
                        frame.control.blend = if (index + usize::from(variant) / 2) % 2 == 0 {
                            BlendOp::Source
                        } else {
                            BlendOp::Over
                        };
                    }
                    cases.push((
                        format!(
                            "format-{format:02}-adam7-{}-default-{}-pattern-{variant}",
                            u8::from(interlaced),
                            u8::from(default_is_frame)
                        ),
                        case,
                    ));
                }
            }
        }
    }
    cases
}

// Rational operations are deliberately distinct from the production u64
// weighted-sum implementation. Every operation reduces before reuse.
#[derive(Clone, Copy)]
struct Fraction {
    n: u128,
    d: u128,
}
impl Fraction {
    fn new(n: u128, d: u128) -> Self {
        assert_ne!(d, 0);
        let (mut a, mut b) = (n, d);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Self { n: n / a, d: d / a }
    }
    fn add(self, b: Self) -> Self {
        Self::new(self.n * b.d + b.n * self.d, self.d * b.d)
    }
    fn mul(self, b: Self) -> Self {
        Self::new(self.n * b.n, self.d * b.d)
    }
    fn div(self, b: Self) -> Self {
        Self::new(self.n * b.d, self.d * b.n)
    }
    fn inverse_alpha(self) -> Self {
        Self::new(self.d - self.n, self.d)
    }
    fn nearest(self) -> u16 {
        u16::try_from((2 * self.n + self.d) / (2 * self.d)).unwrap()
    }
}

pub fn over(foreground: [u16; 4], background: [u16; 4]) -> [u16; 4] {
    if foreground[3] == 0 {
        return background;
    }
    let a = Fraction::new(u128::from(foreground[3]), 65535);
    let b = Fraction::new(u128::from(background[3]), 65535);
    let inverse = a.inverse_alpha();
    let alpha = a.add(b.mul(inverse));
    let mut out = [0; 4];
    for c in 0..3 {
        let f = Fraction::new(u128::from(foreground[c]), 1);
        let d = Fraction::new(u128::from(background[c]), 1);
        out[c] = f.mul(a).add(d.mul(b).mul(inverse)).div(alpha).nearest();
    }
    out[3] = alpha.mul(Fraction::new(65535, 1)).nearest();
    out
}

pub fn expand(png: &Png, image: &Image) -> Vec<[u16; 4]> {
    let channels = match image.color_type as u8 {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => unreachable!(),
    };
    let raw: Vec<u16> = match &image.samples {
        Samples::U8(v) => v.iter().map(|&v| u16::from(v)).collect(),
        Samples::U16(v) => v.clone(),
    };
    let repeat = |v: u16| {
        let mut out = 0u32;
        for _ in 0..16 / image.bit_depth {
            out = (out << image.bit_depth) | u32::from(v);
        }
        out as u16
    };
    let mask = ((1u32 << image.bit_depth) - 1) as u16;
    raw.chunks_exact(channels).map(|v| match image.color_type as u8 {
        0 => { let a = if matches!(png.transparency, Some(Transparency::Grayscale(k)) if k & mask == v[0]) { 0 } else { 65535 }; [repeat(v[0]), repeat(v[0]), repeat(v[0]), a] }
        2 => { let a = if matches!(png.transparency, Some(Transparency::Rgb(k)) if k.map(|n| n & mask) == v) { 0 } else { 65535 }; [repeat(v[0]), repeat(v[1]), repeat(v[2]), a] }
        3 => {
            let index = usize::from(v[0]);
            if let Some(rgb) = png.palette.get(index) {
                let a = match &png.transparency { Some(Transparency::Palette(t)) => t.get(index).copied().unwrap_or(255), _ => 255 };
                [u16::from(rgb[0])*257, u16::from(rgb[1])*257, u16::from(rgb[2])*257, u16::from(a)*257]
            } else { [0,0,0,65535] }
        }
        4 => [repeat(v[0]), repeat(v[0]), repeat(v[0]), repeat(v[1])],
        6 => [repeat(v[0]), repeat(v[1]), repeat(v[2]), repeat(v[3])],
        _ => unreachable!(),
    }).collect()
}

/// Full-canvas snapshots and immediate disposal, unlike the native renderer's
/// saved rectangles and disposal deferred until the following frame.
pub fn render(png: &Png) -> Vec<Vec<u16>> {
    let width = png.image.width as usize;
    let mut canvas = vec![[0; 4]; width * png.image.height as usize];
    let mut out = Vec::new();
    for (index, frame) in png.animation.as_ref().unwrap().frames.iter().enumerate() {
        let c = frame.control;
        let before = canvas.clone();
        let source = expand(png, frame.image.as_ref().unwrap_or(&png.image));
        for y in 0..c.height as usize {
            for x in 0..c.width as usize {
                let dest = (y + c.y_offset as usize) * width + x + c.x_offset as usize;
                let pixel = source[y * c.width as usize + x];
                canvas[dest] = if c.blend == BlendOp::Source {
                    pixel
                } else {
                    over(pixel, canvas[dest])
                };
            }
        }
        out.push(canvas.iter().flatten().copied().collect());
        match c.dispose {
            DisposeOp::Previous if index > 0 => canvas = before,
            DisposeOp::Previous | DisposeOp::Background => {
                for y in 0..c.height as usize {
                    let start = (y + c.y_offset as usize) * width + c.x_offset as usize;
                    canvas[start..start + c.width as usize].fill([0; 4]);
                }
            }
            DisposeOp::None => (),
        }
    }
    out
}
