//! Literal APNG control fields, independent framing and native sample answers.
use super::{apng_recovery as wire, chunks};
use deadlib_image_png::{BlendOp, DisposeOp, FrameControl, Image};
use wire::{Chunk, Fixture};

const INTEGERS: [u32; 15] = [
    0,
    1,
    2,
    3,
    4,
    5,
    255,
    256,
    65535,
    65536,
    0x0100_0000,
    0x7fff_fffe,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
];
const WORDS: [u16; 10] = [0, 1, 99, 100, 255, 256, 32767, 32768, 65534, 65535];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Valid,
    AnimationFault,
    Resource,
}
pub struct Case {
    pub name: String,
    pub category: &'static str,
    pub chunks: Vec<Chunk>,
    pub controls: [FrameControl; 2],
    pub plays: u32,
    pub outcome: Outcome,
    pub changed_image: Option<(usize, Image)>,
}
impl Case {
    fn new(f: &Fixture, category: &'static str, name: String) -> Self {
        let control = |width, height, x_offset, y_offset| FrameControl {
            width,
            height,
            x_offset,
            y_offset,
            delay_num: 7,
            delay_den: 0,
            dispose: DisposeOp::None,
            blend: BlendOp::Source,
        };
        Self {
            name,
            category,
            chunks: f.chunks.clone(),
            controls: [
                if f.default_is_frame {
                    control(3, 2, 0, 0)
                } else {
                    control(1, 1, 0, 0)
                },
                control(1, 1, 1, 1),
            ],
            plays: 3,
            outcome: Outcome::Valid,
            changed_image: None,
        }
    }
    pub fn bytes(&self) -> Vec<u8> {
        wire::file(&self.chunks)
    }
    pub fn image<'a>(&'a self, f: &'a Fixture, index: usize) -> &'a Image {
        match &self.changed_image {
            Some((at, image)) if *at == index => image,
            _ => &f.frames[index],
        }
    }
}
fn index(f: &Fixture, kind: &[u8; 4], number: usize) -> usize {
    f.chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| &c.kind == kind)
        .nth(number)
        .unwrap()
        .0
}
fn put32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn integer_seed(value: u32) -> bool {
    value <= 5 || [0x7fff_ffff, 0x8000_0000, 0xffff_ffff].contains(&value)
}
fn rectangle(f: &Fixture, frame: usize, c: FrameControl) -> bool {
    c.width > 0
        && c.height > 0
        && c.width <= 3
        && c.height <= 2
        && c.x_offset <= 3 - c.width
        && c.y_offset <= 2 - c.height
        && (!(f.default_is_frame && frame == 0)
            || (c.width == 3 && c.height == 2 && c.x_offset == 0 && c.y_offset == 0))
}
pub fn fields(f: &Fixture, full: bool, mut visit: impl FnMut(Case)) {
    let ac = index(f, b"acTL", 0);
    for size in 0..=12 {
        if !full && ![0, 7, 8, 9, 12].contains(&size) {
            continue;
        }
        let mut c = Case::new(f, "length", format!("actl-length-{size}"));
        c.chunks[ac].data.resize(size, 0);
        if size != 8 {
            c.outcome = Outcome::AnimationFault;
        }
        visit(c);
    }
    for (offset, category) in [(0, "count"), (4, "plays")] {
        for value in INTEGERS {
            if !full && !integer_seed(value) {
                continue;
            }
            let mut c = Case::new(f, category, format!("actl-{offset}-{value:08x}"));
            put32(&mut c.chunks[ac].data, offset, value);
            c.outcome = if value > 0x7fff_ffff || (offset == 0 && value == 0) {
                Outcome::AnimationFault
            } else if offset == 0 && value > 4 {
                Outcome::Resource
            } else if offset == 0 && value != 2 {
                Outcome::AnimationFault
            } else {
                Outcome::Valid
            };
            if offset == 4 {
                c.plays = value;
            }
            visit(c);
        }
    }
    for frame in 0..2 {
        let fc = index(f, b"fcTL", frame);
        for size in 0..=30 {
            if !full && ![0, 25, 26, 27, 30].contains(&size) {
                continue;
            }
            let mut c = Case::new(f, "length", format!("fctl-{frame}-length-{size}"));
            c.chunks[fc].data.resize(size, 0);
            if size != 26 {
                c.outcome = Outcome::AnimationFault;
            }
            visit(c);
        }
        for value in INTEGERS {
            if !full && !integer_seed(value) {
                continue;
            }
            let mut c = Case::new(f, "sequence", format!("fctl-{frame}-sequence-{value:08x}"));
            let expected = u32::from_be_bytes(c.chunks[fc].data[..4].try_into().unwrap());
            put32(&mut c.chunks[fc].data, 0, value);
            if value != expected {
                c.outcome = Outcome::AnimationFault;
            }
            visit(c);
        }
        for offset in [4, 8, 12, 16] {
            for value in INTEGERS {
                if !full && !integer_seed(value) {
                    continue;
                }
                let mut c = Case::new(f, "rectangle", format!("fctl-{frame}-{offset}-{value:08x}"));
                put32(&mut c.chunks[fc].data, offset, value);
                let control = &mut c.controls[frame];
                match offset {
                    4 => control.width = value,
                    8 => control.height = value,
                    12 => control.x_offset = value,
                    _ => control.y_offset = value,
                }
                if !rectangle(f, frame, *control) {
                    c.outcome = Outcome::AnimationFault;
                } else if control.width != f.frames[frame].width
                    || control.height != f.frames[frame].height
                {
                    let image = wire::image(
                        control.width,
                        control.height,
                        f.image.color_type,
                        f.image.bit_depth,
                        if frame == 0 { 7 } else { 11 },
                    );
                    let fd = index(f, b"fdAT", frame - usize::from(f.default_is_frame));
                    let mut data = c.chunks[fd].data[..4].to_vec();
                    data.extend(chunks::stored(&wire::filtered(&image, f.interlaced)));
                    c.chunks[fd].data = data;
                    c.changed_image = Some((frame, image));
                }
                visit(c);
            }
        }
        for (offset, category) in [(24, "dispose"), (25, "blend")] {
            for value in 0..=255u8 {
                if !full && ![0, 1, 2, 3, 255].contains(&value) {
                    continue;
                }
                let mut c = Case::new(f, category, format!("fctl-{frame}-{offset}-{value}"));
                c.chunks[fc].data[offset] = value;
                if offset == 24 {
                    c.controls[frame].dispose = match value {
                        0 => DisposeOp::None,
                        1 => DisposeOp::Background,
                        2 => DisposeOp::Previous,
                        _ => {
                            c.outcome = Outcome::AnimationFault;
                            DisposeOp::None
                        }
                    };
                } else {
                    c.controls[frame].blend = match value {
                        0 => BlendOp::Source,
                        1 => BlendOp::Over,
                        _ => {
                            c.outcome = Outcome::AnimationFault;
                            BlendOp::Source
                        }
                    };
                }
                visit(c);
            }
        }
        for offset in [20, 22] {
            for value in WORDS {
                if !full && ![0, 1, 255, 256, 65535].contains(&value) {
                    continue;
                }
                visit(timing(f, frame, offset, value));
            }
        }
    }
    let fd = index(f, b"fdAT", 0);
    for size in 0..4 {
        let mut c = Case::new(f, "length", format!("fdat-length-{size}"));
        c.chunks[fd].data.truncate(size);
        c.outcome = Outcome::AnimationFault;
        visit(c);
    }
    for value in INTEGERS {
        if !full && !integer_seed(value) {
            continue;
        }
        let mut c = Case::new(f, "sequence", format!("fdat-sequence-{value:08x}"));
        let expected = u32::from_be_bytes(c.chunks[fd].data[..4].try_into().unwrap());
        put32(&mut c.chunks[fd].data, 0, value);
        if value != expected {
            c.outcome = Outcome::AnimationFault;
        }
        visit(c);
    }
    for count in [1, 2, 7] {
        let mut c = Case::new(f, "empty-data", format!("fdat-empty-prefix-{count}"));
        let first = u32::from_be_bytes(c.chunks[fd].data[..4].try_into().unwrap());
        for chunk in &mut c.chunks[fd..] {
            if matches!(&chunk.kind, b"fcTL" | b"fdAT") {
                let sequence = u32::from_be_bytes(chunk.data[..4].try_into().unwrap());
                put32(&mut chunk.data, 0, sequence + count);
            }
        }
        for n in 0..count {
            c.chunks.insert(
                fd + n as usize,
                Chunk::new(*b"fdAT", (first + n).to_be_bytes().to_vec()),
            );
        }
        visit(c);
    }
}
pub fn timing(f: &Fixture, frame: usize, offset: usize, value: u16) -> Case {
    let mut c = Case::new(f, "delay", format!("fctl-{frame}-{offset}-{value}"));
    let fc = index(f, b"fcTL", frame);
    c.chunks[fc].data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    if offset == 20 {
        c.controls[frame].delay_num = value;
    } else {
        c.controls[frame].delay_den = value;
    }
    c
}
pub fn seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds = Vec::new();
    for (color, depth) in wire::formats() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let f = Fixture::new(color, depth, interlaced, default_is_frame);
                fields(&f, false, |c| {
                    seeds.push((
                        format!(
                            "animation-fields-{}-{depth}-{interlaced}-{default_is_frame}-{}",
                            color as u8, c.name
                        ),
                        c.bytes(),
                    ));
                });
            }
        }
    }
    seeds
}
