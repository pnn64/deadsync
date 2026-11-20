use deadlib_image_bmp::{Bitmap, Error, Limits, Pixels, decode};

#[derive(Clone, Copy)]
struct Format {
    bits: u16,
    header: u32,
    compression: u32,
    masks: [u32; 4],
}

impl Format {
    fn rgb(bits: u16) -> Self {
        Self {
            bits,
            header: 40,
            compression: 0,
            masks: [0; 4],
        }
    }
}

// Independent wire construction. Rows are supplied top-to-bottom and include
// padding, so the expected pixels never depend on this crate's encoder.
fn wire(
    format: Format,
    width: usize,
    height: usize,
    top_down: bool,
    palette: &[[u8; 3]],
    rows: &[u8],
) -> Vec<u8> {
    let pitch = (width * usize::from(format.bits)).div_ceil(32) * 4;
    assert_eq!(rows.len(), pitch * height);
    let external = usize::from(format.header == 40 && format.compression == 3) * 12;
    let offset = 14 + format.header as usize + external + palette.len() * 4;
    let mut bytes = vec![0; offset + rows.len()];
    let length = bytes.len() as u32;
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&format.header.to_le_bytes());
    bytes[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    let stored_height = if top_down {
        -(height as i32)
    } else {
        height as i32
    };
    bytes[22..26].copy_from_slice(&stored_height.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&format.bits.to_le_bytes());
    bytes[30..34].copy_from_slice(&format.compression.to_le_bytes());
    if format.header >= 52 || external != 0 {
        for (i, mask) in format.masks[..3].iter().enumerate() {
            bytes[54 + i * 4..58 + i * 4].copy_from_slice(&mask.to_le_bytes());
        }
    }
    if format.header >= 56 {
        bytes[66..70].copy_from_slice(&format.masks[3].to_le_bytes());
    }
    let palette_start = 14 + format.header as usize + external;
    for (entry, &[red, green, blue]) in bytes[palette_start..offset]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(palette)
    {
        *entry = [blue, green, red, 0xff];
    }
    for (encoded_y, output) in bytes[offset..].chunks_exact_mut(pitch).enumerate() {
        let y = if top_down {
            encoded_y
        } else {
            height - encoded_y - 1
        };
        output.copy_from_slice(&rows[y * pitch..(y + 1) * pitch]);
    }
    bytes
}

fn palette(length: usize) -> Vec<[u8; 3]> {
    (0..length)
        .map(|i| [i as u8, (i * 71 + 17) as u8, (i * 139 + 29) as u8])
        .collect()
}

#[test]
fn every_16_bit_word_preserves_555_565_and_explicit_alpha() {
    // An odd width exercises the final pixel and padding on every row while
    // covering every possible input word, including the unused high RGB555 bit.
    let (width, height) = (257usize, 256usize);
    let pitch = (width * 2).div_ceil(4) * 4;
    let mut rows = vec![0xa5; pitch * height];
    for y in 0..height {
        for x in 0..width {
            let value = (y * width + x) as u16;
            rows[y * pitch + x * 2..y * pitch + x * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
    }
    for (format, green_bits, has_alpha) in [
        (Format::rgb(16), 5, false),
        (
            Format {
                compression: 3,
                masks: [0x7c00, 0x03e0, 0x001f, 0],
                ..Format::rgb(16)
            },
            5,
            false,
        ),
        (
            Format {
                compression: 3,
                masks: [0xf800, 0x07e0, 0x001f, 0],
                ..Format::rgb(16)
            },
            6,
            false,
        ),
        (
            Format {
                header: 56,
                compression: 3,
                masks: [0x7c00, 0x03e0, 0x001f, 0x8000],
                ..Format::rgb(16)
            },
            5,
            true,
        ),
        (
            Format {
                header: 108,
                masks: [0, 0, 0, 0x8000],
                ..Format::rgb(16)
            },
            5,
            true,
        ),
    ] {
        let mut expected = Vec::new();
        for i in 0..width * height {
            let word = i as u16;
            let red = ((word >> (5 + green_bits)) & 31) as u8;
            let green = ((word >> 5) & ((1 << green_bits) - 1)) as u8;
            let blue = (word & 31) as u8;
            expected.extend([
                (red << 3) | (red >> 2),
                if green_bits == 5 {
                    (green << 3) | (green >> 2)
                } else {
                    (green << 2) | (green >> 4)
                },
                (blue << 3) | (blue >> 2),
            ]);
            if has_alpha {
                expected.push(if word & 0x8000 == 0 { 0 } else { 255 });
            }
        }
        for top_down in [false, true] {
            let bitmap = decode(&wire(format, width, height, top_down, &[], &rows)).unwrap();
            match &bitmap.pixels {
                Pixels::Rgb(samples) if !has_alpha => assert_eq!(samples, &expected),
                Pixels::Rgba(samples) if has_alpha => assert_eq!(samples, &expected),
                _ => panic!("unexpected pixel model for 16-bit mask combination"),
            }
        }
    }
}

#[test]
fn packed_indices_cover_every_byte_with_short_palettes_and_partial_tails() {
    for bits in [1u16, 4] {
        let sizes: &[usize] = if bits == 1 { &[1, 2] } else { &[1, 7, 15, 16] };
        for width in [1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33] {
            let pitch = (width * usize::from(bits)).div_ceil(32) * 4;
            for byte in 0..=255u8 {
                let mut rows = vec![0xff; pitch * 3];
                let mut expected = Vec::new();
                for y in 0..3 {
                    let mut unpacked = Vec::new();
                    for x in 0..(width * usize::from(bits)).div_ceil(8) {
                        let value = byte.wrapping_add((y * 47 + x * 19) as u8);
                        rows[y * pitch + x] = value;
                        if bits == 1 {
                            unpacked.extend([
                                value >> 7,
                                value >> 6 & 1,
                                value >> 5 & 1,
                                value >> 4 & 1,
                                value >> 3 & 1,
                                value >> 2 & 1,
                                value >> 1 & 1,
                                value & 1,
                            ]);
                        } else {
                            unpacked.extend([value >> 4, value & 15]);
                        }
                    }
                    expected.extend(&unpacked[..width]);
                }
                for &size in sizes {
                    let colors = palette(size);
                    let valid = expected.iter().all(|&index| usize::from(index) < size);
                    for top_down in [false, true] {
                        let result =
                            decode(&wire(Format::rgb(bits), width, 3, top_down, &colors, &rows));
                        if valid {
                            assert_eq!(
                                result.unwrap().pixels,
                                Pixels::Indexed {
                                    bit_depth: bits as u8,
                                    indices: expected.clone(),
                                    palette: colors.clone(),
                                }
                            );
                        } else {
                            assert_eq!(
                                result,
                                Err(Error::Invalid("pixel index exceeds the palette"))
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn byte_indices_validate_short_palettes_without_including_padding() {
    let (width, height) = (257usize, 3usize);
    let pitch = width.div_ceil(4) * 4;
    for size in 1..=256 {
        let colors = palette(size);
        let mut rows = vec![0xff; pitch * height];
        let mut expected = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let index = ((x + y * 31) % size) as u8;
                rows[y * pitch + x] = index;
                expected.push(index);
            }
        }
        for top_down in [false, true] {
            assert_eq!(
                decode(&wire(
                    Format::rgb(8),
                    width,
                    height,
                    top_down,
                    &colors,
                    &rows
                ))
                .unwrap()
                .pixels,
                Pixels::Indexed {
                    bit_depth: 8,
                    indices: expected.clone(),
                    palette: colors.clone(),
                }
            );
            if size < 256 {
                let last = (height - 1) * pitch + width - 1;
                let original = rows[last];
                rows[last] = size as u8;
                assert_eq!(
                    decode(&wire(
                        Format::rgb(8),
                        width,
                        height,
                        top_down,
                        &colors,
                        &rows
                    )),
                    Err(Error::Invalid("pixel index exceeds the palette"))
                );
                rows[last] = original;
            }
        }
    }
}

// Repeat bits from most significant to least significant until eight are filled.
// This avoids using the decoder's multiplier/downshift expansion formula.
fn repeat_bits(value: u32, bits: u32) -> u8 {
    let mut output = 0;
    for i in 0..8 {
        output = (output << 1) | ((value >> (bits - 1 - i % bits)) & 1) as u8;
    }
    output
}

#[test]
fn common_and_unusual_32_bit_masks_preserve_channel_order_and_bit_replication() {
    for layout in [
        [(16, 8), (8, 8), (0, 8), (24, 8)],
        [(16, 8), (8, 8), (0, 8), (0, 0)],
        [(0, 8), (8, 8), (16, 8), (24, 8)],
        [(8, 8), (0, 8), (24, 8), (16, 8)],
        [(0, 3), (3, 5), (8, 7), (15, 8)],
        [(1, 2), (6, 6), (17, 4), (27, 1)],
    ] {
        let masks = layout.map(|(shift, bits)| ((1u32 << bits) - 1) << shift);
        let format = Format {
            bits: 32,
            header: 56,
            compression: 3,
            masks,
        };
        for width in [1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 257] {
            let mut rows = Vec::new();
            let mut expected = Vec::new();
            for i in 0..width * 3 {
                let word = (i as u32).wrapping_mul(0x9e37_79b9).rotate_left(7);
                rows.extend(word.to_le_bytes());
                for (shift, bits) in layout {
                    expected.push(if bits == 0 {
                        255
                    } else {
                        repeat_bits((word >> shift) & ((1 << bits) - 1), bits)
                    });
                }
            }
            for top_down in [false, true] {
                let bitmap = decode(&wire(format, width, 3, top_down, &[], &rows)).unwrap();
                assert_eq!(bitmap.to_rgba8().unwrap(), expected);
            }
        }
    }
}

#[test]
fn bgra_zero_alpha_repair_covers_row_boundaries_and_final_pixels() {
    for width in [1usize, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 257] {
        let count = width * 3;
        let mut rows = Vec::new();
        let mut expected = Vec::new();
        for i in 0..count {
            rows.extend([(i * 139) as u8, (i * 71) as u8, i as u8, 0]);
            expected.extend([i as u8, (i * 71) as u8, (i * 139) as u8, 0]);
        }
        for nonzero in [None, Some(0), Some(width - 1), Some(width), Some(count - 1)] {
            let mut source = rows.clone();
            let mut alpha = expected.clone();
            if let Some(i) = nonzero {
                source[i * 4 + 3] = 1;
                alpha[i * 4 + 3] = 1;
            }
            for explicit in [false, true] {
                let format = if explicit {
                    Format {
                        header: 56,
                        compression: 3,
                        masks: [0xff0000, 0xff00, 0xff, 0xff000000],
                        ..Format::rgb(32)
                    }
                } else {
                    Format::rgb(32)
                };
                let mut wanted = alpha.clone();
                if !explicit && nonzero.is_none() {
                    for pixel in wanted.as_chunks_mut::<4>().0 {
                        pixel[3] = 255;
                    }
                }
                for top_down in [false, true] {
                    assert_eq!(
                        decode(&wire(format, width, 3, top_down, &[], &source))
                            .unwrap()
                            .pixels,
                        Pixels::Rgba(wanted.clone())
                    );
                }
            }
        }
    }
}

#[test]
fn rgba_conversion_handles_all_palette_sizes_and_rgb_chunk_tails() {
    for size in 1..=256 {
        let colors = palette(size);
        let indices: Vec<u8> = (0..4097).map(|i| ((i * 71) % size) as u8).collect();
        let expected: Vec<u8> = indices
            .iter()
            .flat_map(|&index| {
                let [r, g, b] = colors[usize::from(index)];
                [r, g, b, 255]
            })
            .collect();
        let bitmap = Bitmap {
            width: 4097,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth: if size <= 2 {
                    1
                } else if size <= 16 {
                    4
                } else {
                    8
                },
                indices,
                palette: colors,
            },
        };
        assert_eq!(bitmap.to_rgba8().unwrap(), expected);
    }
    for count in [
        1usize, 2, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 255, 256, 257, 1023, 1024, 1025,
    ] {
        let mut rgb = Vec::new();
        let mut expected = Vec::new();
        for i in 0..count {
            let color = [i as u8, (i * 71) as u8, (i * 139) as u8];
            rgb.extend(color);
            expected.extend([color[0], color[1], color[2], 255]);
        }
        let bitmap = Bitmap {
            width: count as u32,
            height: 1,
            pixels: Pixels::Rgb(rgb),
        };
        assert_eq!(bitmap.to_rgba8().unwrap(), expected);
    }
}

#[test]
fn full_packed_palettes_still_validate_caller_created_unpacked_indices() {
    for (bit_depth, size) in [(1, 2usize), (4, 16)] {
        let bitmap = Bitmap {
            width: 1,
            height: 1,
            pixels: Pixels::Indexed {
                bit_depth,
                indices: vec![size as u8],
                palette: palette(size),
            },
        };
        assert_eq!(
            bitmap.validate(),
            Err(Error::Invalid("pixel index exceeds the palette"))
        );
        assert_eq!(
            bitmap.to_rgba8(),
            Err(Error::Invalid("pixel index exceeds the palette"))
        );
        assert_eq!(
            bitmap.to_rgba8_with_limits(Limits {
                max_decoded_bytes: 3,
                ..Limits::default()
            }),
            Err(Error::LimitExceeded)
        );
    }
}
