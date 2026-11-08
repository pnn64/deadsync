//! Independent resource-boundary fixtures; no production encoder is used.
use super::{apng_recovery as apng, chunks as wire};
use deadlib_image_png::{ColorType, Image, Limits};

pub const FORMATS: [(ColorType, u8); 15] = [
    (ColorType::Grayscale, 1),
    (ColorType::Grayscale, 2),
    (ColorType::Grayscale, 4),
    (ColorType::Grayscale, 8),
    (ColorType::Grayscale, 16),
    (ColorType::Rgb, 8),
    (ColorType::Rgb, 16),
    (ColorType::Indexed, 1),
    (ColorType::Indexed, 2),
    (ColorType::Indexed, 4),
    (ColorType::Indexed, 8),
    (ColorType::GrayscaleAlpha, 8),
    (ColorType::GrayscaleAlpha, 16),
    (ColorType::Rgba, 8),
    (ColorType::Rgba, 16),
];

pub struct Case {
    pub name: String,
    pub chunks: Vec<apng::Chunk>,
    pub image: Image,
    pub frames: Vec<Image>,
    pub palette: Vec<[u8; 3]>,
    pub interlaced: bool,
    pub default_is_frame: bool,
    pub minimum: Limits,
}

pub fn native_bytes(image: &Image) -> usize {
    image.width as usize
        * image.height as usize
        * image.color_type.channels()
        * if image.bit_depth == 16 { 2 } else { 1 }
}

impl Case {
    pub fn wire(&self) -> Vec<u8> {
        apng::file(&self.chunks)
    }

    pub fn seed(&self, limits: Limits, flags: u8, fragment: u8, fault_at: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [
            limits.max_input_bytes,
            limits.max_output_bytes,
            limits.max_metadata_bytes,
        ] {
            bytes.extend(u32::try_from(value).unwrap().to_be_bytes());
        }
        for value in [limits.max_chunks, limits.max_frames] {
            bytes.extend(u16::try_from(value).unwrap().to_be_bytes());
        }
        bytes.extend([flags, fragment]);
        bytes.extend(u16::try_from(fault_at).unwrap().to_be_bytes());
        assert_eq!(bytes.len(), 20);
        bytes.extend(self.wire());
        bytes
    }
}

pub fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (color, depth) in FORMATS {
        for interlaced in [false, true] {
            for role in 0..4 {
                let fixture = apng::Fixture::new(color, depth, interlaced, role != 2);
                let mut chunks = fixture.chunks.clone();
                if role == 0 {
                    chunks.retain(|chunk| !apng::is_animation(&chunk.kind));
                } else if role == 3 {
                    let control = chunks
                        .iter()
                        .position(|chunk| &chunk.kind == b"fcTL")
                        .unwrap();
                    let control = chunks.remove(control);
                    chunks.insert(1, control);
                }
                let expanded = b"independent bounded metadata contents";
                let mut compressed = b"Budget\0\0".to_vec();
                compressed.extend(wire::stored(expanded));
                let before_data = chunks
                    .iter()
                    .position(|chunk| &chunk.kind == b"IDAT")
                    .unwrap();
                chunks.insert(before_data, apng::Chunk::new(*b"zTXt", compressed));
                chunks.insert(before_data, apng::Chunk::new(*b"qsAf", vec![29; 17]));
                let frames = if role == 0 {
                    Vec::new()
                } else {
                    fixture.frames
                };
                let images: Vec<_> = std::iter::once(&fixture.image)
                    .chain(frames.iter().skip(usize::from(role != 2)))
                    .collect();
                let total_native: usize = images.iter().map(|image| native_bytes(image)).sum();
                let largest_filtered = images
                    .iter()
                    .map(|image| apng::filtered(image, interlaced).len())
                    .max()
                    .unwrap();
                let metadata_bytes: usize = chunks
                    .iter()
                    .filter(|chunk| matches!(&chunk.kind, b"tEXt" | b"zTXt" | b"qsAf"))
                    .map(|chunk| chunk.data.len())
                    .sum::<usize>()
                    + expanded.len();
                let minimum = Limits {
                    max_input_bytes: apng::file(&chunks).len(),
                    max_output_bytes: total_native.max(largest_filtered),
                    max_metadata_bytes: metadata_bytes,
                    max_chunks: chunks.len(),
                    max_frames: frames.len(),
                };
                result.push(Case {
                    name: format!(
                        "color-{}-depth-{depth}-adam7-{interlaced}-role-{role}",
                        color as u8
                    ),
                    chunks,
                    image: fixture.image,
                    frames,
                    palette: fixture.palette,
                    interlaced,
                    default_is_frame: role != 0 && role != 2,
                    minimum,
                });
            }
        }
    }
    assert_eq!(result.len(), 120);
    result
}

pub fn value(limits: Limits, axis: usize) -> usize {
    match axis {
        0 => limits.max_input_bytes,
        1 => limits.max_output_bytes,
        2 => limits.max_metadata_bytes,
        3 => limits.max_chunks,
        4 => limits.max_frames,
        _ => unreachable!(),
    }
}

pub fn set(limits: &mut Limits, axis: usize, value: usize) {
    match axis {
        0 => limits.max_input_bytes = value,
        1 => limits.max_output_bytes = value,
        2 => limits.max_metadata_bytes = value,
        3 => limits.max_chunks = value,
        4 => limits.max_frames = value,
        _ => unreachable!(),
    }
}

pub fn boundaries(minimum: usize) -> Vec<usize> {
    let mut values = vec![0, minimum.saturating_sub(1), minimum, minimum + 1];
    values.sort_unstable();
    values.dedup();
    values
}

pub fn seeds() -> Vec<(String, Vec<u8>)> {
    let mut seeds = Vec::new();
    for case in cases() {
        for policy in 0..2u8 {
            for axis in 0..5 {
                for boundary in boundaries(value(case.minimum, axis)) {
                    let mut limits = case.minimum;
                    set(&mut limits, axis, boundary);
                    let flags = 16 | policy | (((axis + boundary) & 1) as u8) << 1;
                    seeds.push((
                        format!("{}-axis-{axis}-limit-{boundary}-policy-{policy}", case.name),
                        case.seed(limits, flags, (axis * 7) as u8, 0),
                    ));
                }
            }
            for mode in 1..4u8 {
                let points = if mode == 1 {
                    vec![0]
                } else {
                    vec![
                        0,
                        case.minimum.max_input_bytes / 2,
                        case.minimum.max_input_bytes,
                    ]
                };
                for point in points {
                    seeds.push((
                        format!("{}-reader-{mode}-at-{point}-policy-{policy}", case.name),
                        case.seed(case.minimum, 16 | policy | mode << 2, 6, point),
                    ));
                }
            }
        }
    }
    assert_eq!(seeds.len(), 6360);
    seeds
}
