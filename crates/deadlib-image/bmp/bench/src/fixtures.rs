use deadlib_image_bmp::{Bitmap, Pixels};

pub struct Case {
    pub name: String,
    pub bytes: Vec<u8>,
    pub expected: Bitmap,
    pub rgba: Vec<u8>,
    pub policy: &'static str,
}

#[derive(Clone, Copy)]
pub struct Layout {
    pub name: &'static str,
    bits: u16,
    dib: usize,
    palette_len: usize,
    masks: [u32; 4],
    masked: bool,
    zero_alpha: bool,
    policy: &'static str,
}

pub const LAYOUTS: &[Layout] = &[
    Layout::indexed("indexed1", 1, 2, 40),
    Layout::indexed("indexed4", 4, 16, 40),
    Layout::indexed("indexed8", 8, 256, 40),
    Layout::indexed("indexed8-short", 8, 13, 40),
    Layout::indexed("core8", 8, 256, 12),
    Layout::rgb24("rgb24", 40),
    Layout::rgb24("core24", 12),
    Layout::masked("rgb555", 16, [0x7c00, 0x03e0, 0x001f, 0], false),
    Layout::masked("rgb565", 16, [0xf800, 0x07e0, 0x001f, 0], true),
    Layout::masked("rgb333", 16, [0x0700, 0x0070, 0x0007, 0], true),
    Layout::masked("rgba4444", 16, [0x0f00, 0x00f0, 0x000f, 0xf000], true),
    Layout::masked("bgrx32", 32, [0x00ff0000, 0x0000ff00, 0x000000ff, 0], true),
    Layout::masked(
        "rgba32",
        32,
        [0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000],
        true,
    ),
    Layout::masked(
        "rgba32-swapped",
        32,
        [0x000000ff, 0x0000ff00, 0x00ff0000, 0xff000000],
        true,
    ),
    Layout {
        name: "bgra32",
        bits: 32,
        dib: 40,
        palette_len: 0,
        masks: [0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000],
        masked: false,
        zero_alpha: false,
        policy: "implicit-alpha",
    },
    Layout {
        name: "bgra32-zero",
        bits: 32,
        dib: 40,
        palette_len: 0,
        masks: [0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000],
        masked: false,
        zero_alpha: true,
        policy: "implicit-alpha",
    },
];

impl Layout {
    const fn indexed(name: &'static str, bits: u16, palette_len: usize, dib: usize) -> Self {
        Self {
            name,
            bits,
            dib,
            palette_len,
            masks: [0; 4],
            masked: false,
            zero_alpha: false,
            policy: "none",
        }
    }

    const fn rgb24(name: &'static str, dib: usize) -> Self {
        Self {
            name,
            bits: 24,
            dib,
            palette_len: 0,
            masks: [0; 4],
            masked: false,
            zero_alpha: false,
            policy: "none",
        }
    }

    const fn masked(name: &'static str, bits: u16, masks: [u32; 4], masked: bool) -> Self {
        Self {
            name,
            bits,
            dib: if masks[3] == 0 { 40 } else { 108 },
            palette_len: 0,
            masks,
            masked,
            zero_alpha: false,
            policy: if bits == 16 { "mask-expansion" } else { "none" },
        }
    }

    pub fn supports_top_down(self) -> bool {
        self.dib != 12
    }
}

// Independent bit-by-bit repetition, deliberately not the codec's multiply/shift
// implementation. This produces the expected samples directly from each field.
fn expand(value: u32, bits: u32) -> u8 {
    let mut output = 0u8;
    for position in 0..8 {
        let source = bits - 1 - position % bits;
        output = (output << 1) | ((value >> source) & 1) as u8;
    }
    output
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn build(layout: Layout, width: usize, height: usize, top_down: bool) -> Case {
    let row_pitch = (width * usize::from(layout.bits)).div_ceil(32) * 4;
    let external = if layout.masked && layout.dib == 40 {
        12
    } else {
        0
    };
    let entry_bytes = if layout.dib == 12 { 3 } else { 4 };
    let offset = 14 + layout.dib + external + layout.palette_len * entry_bytes;
    let mut bytes = vec![0; offset + row_pitch * height];
    let length = bytes.len() as u32;
    bytes[..2].copy_from_slice(b"BM");
    put_u32(&mut bytes, 2, length);
    put_u32(&mut bytes, 10, offset as u32);
    put_u32(&mut bytes, 14, layout.dib as u32);
    if layout.dib == 12 {
        bytes[18..20].copy_from_slice(&(width as u16).to_le_bytes());
        bytes[20..22].copy_from_slice(&(height as u16).to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&layout.bits.to_le_bytes());
    } else {
        put_u32(&mut bytes, 18, width as u32);
        put_u32(
            &mut bytes,
            22,
            if top_down {
                -(height as i32)
            } else {
                height as i32
            } as u32,
        );
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&layout.bits.to_le_bytes());
        put_u32(&mut bytes, 30, if layout.masked { 3 } else { 0 });
        put_u32(&mut bytes, 34, (row_pitch * height) as u32);
        put_u32(&mut bytes, 46, layout.palette_len as u32);
        if layout.masked {
            for (channel, &mask) in
                layout
                    .masks
                    .iter()
                    .enumerate()
                    .take(if layout.dib == 40 { 3 } else { 4 })
            {
                put_u32(&mut bytes, 54 + 4 * channel, mask);
            }
        }
    }
    let palette: Vec<[u8; 3]> = (0..layout.palette_len)
        .map(|i| [i as u8, (i * 71 + 19) as u8, (i * 139 + 41) as u8])
        .collect();
    for (i, &[r, g, b]) in palette.iter().enumerate() {
        let start = 14 + layout.dib + external + i * entry_bytes;
        bytes[start..start + 3].copy_from_slice(&[b, g, r]);
        if entry_bytes == 4 {
            bytes[start + 3] = 0xa5;
        }
    }
    // Nonzero unused bits and DWORD padding must not become output pixels.
    bytes[offset..].fill(0xed);
    let mut indices = Vec::new();
    let mut rgb = Vec::new();
    let mut rgba = Vec::with_capacity(width * height * 4);
    let has_alpha = layout.masks[3] != 0;
    for y in 0..height {
        let row = offset + if top_down { y } else { height - 1 - y } * row_pitch;
        for x in 0..width {
            let seed = x as u32 + (y as u32).wrapping_mul(131);
            let color = [
                seed.wrapping_mul(17).wrapping_add(13) as u8,
                seed.wrapping_mul(31).wrapping_add(29) as u8,
                seed.wrapping_mul(71).wrapping_add(47) as u8,
            ];
            if !palette.is_empty() {
                let index = (seed as usize % palette.len()) as u8;
                indices.push(index);
                let bit = x * usize::from(layout.bits);
                let shift = 8 - usize::from(layout.bits) - bit % 8;
                let mask = ((1u16 << layout.bits) - 1) as u8;
                let byte = &mut bytes[row + bit / 8];
                *byte = (*byte & !(mask << shift)) | (index << shift);
                let [r, g, b] = palette[usize::from(index)];
                rgba.extend_from_slice(&[r, g, b, 255]);
            } else if layout.bits == 24 {
                let [r, g, b] = color;
                bytes[row + x * 3..row + x * 3 + 3].copy_from_slice(&[b, g, r]);
                rgb.extend_from_slice(&color);
                rgba.extend_from_slice(&[r, g, b, 255]);
            } else {
                let source = [
                    color[0],
                    color[1],
                    color[2],
                    if layout.zero_alpha {
                        0
                    } else {
                        seed.wrapping_mul(53).wrapping_add(1) as u8
                    },
                ];
                let mut word = 0u32;
                let mut pixel = [0, 0, 0, 255];
                for channel in 0..4 {
                    let mask = layout.masks[channel];
                    if mask != 0 {
                        let bits = mask.count_ones();
                        let value = u32::from(source[channel]) >> (8 - bits);
                        word |= value << mask.trailing_zeros();
                        pixel[channel] = expand(value, bits);
                    }
                }
                if layout.zero_alpha && !layout.masked {
                    pixel[3] = 255;
                }
                let size = usize::from(layout.bits / 8);
                bytes[row + x * size..row + x * size + size]
                    .copy_from_slice(&word.to_le_bytes()[..size]);
                rgb.extend_from_slice(&pixel[..3]);
                rgba.extend_from_slice(&pixel);
            }
        }
    }
    let pixels = if !palette.is_empty() {
        Pixels::Indexed {
            bit_depth: layout.bits as u8,
            indices,
            palette,
        }
    } else if has_alpha {
        Pixels::Rgba(rgba.clone())
    } else {
        Pixels::Rgb(rgb)
    };
    Case {
        name: format!(
            "{}-{}x{}-{}",
            layout.name,
            width,
            height,
            if top_down { "top" } else { "bottom" }
        ),
        bytes,
        expected: Bitmap {
            width: width as u32,
            height: height as u32,
            pixels,
        },
        rgba,
        policy: layout.policy,
    }
}
