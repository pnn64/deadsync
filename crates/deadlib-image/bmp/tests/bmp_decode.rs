use deadlib_image_bmp::{
    Bitmap, Error, Info, Limits, Pixels, decode, decode_with_limits, probe, probe_with_limits,
};

#[derive(Clone, Copy)]
struct Header {
    size: u32,
    width: i32,
    height: i32,
    bits: u16,
    compression: u32,
    masks: [u32; 4],
}

impl Header {
    fn new(width: i32, height: i32, bits: u16) -> Self {
        Self {
            size: 40,
            width,
            height,
            bits,
            compression: 0,
            masks: [0; 4],
        }
    }
}

// Independent BMP wire construction: no native encoder supplies these inputs.
fn bmp(header: Header, palette: &[[u8; 3]], rows: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 14 + header.size as usize];
    bytes[..2].copy_from_slice(b"BM");
    bytes[14..18].copy_from_slice(&header.size.to_le_bytes());
    if header.size == 12 {
        bytes[18..20].copy_from_slice(&(header.width as u16).to_le_bytes());
        bytes[20..22].copy_from_slice(&(header.height as u16).to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&header.bits.to_le_bytes());
    } else {
        bytes[18..22].copy_from_slice(&header.width.to_le_bytes());
        bytes[22..26].copy_from_slice(&header.height.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&header.bits.to_le_bytes());
        bytes[30..34].copy_from_slice(&header.compression.to_le_bytes());
        bytes[34..38].copy_from_slice(&(rows.len() as u32).to_le_bytes());
        if header.size >= 52 {
            for (i, mask) in header.masks[..3].iter().enumerate() {
                bytes[54 + i * 4..58 + i * 4].copy_from_slice(&mask.to_le_bytes());
            }
        }
        if header.size >= 56 {
            bytes[66..70].copy_from_slice(&header.masks[3].to_le_bytes());
        }
    }
    if header.size == 40 && header.compression == 3 {
        for mask in &header.masks[..3] {
            bytes.extend(mask.to_le_bytes());
        }
    }
    for &[r, g, b] in palette {
        bytes.extend([b, g, r]);
        if header.size != 12 {
            bytes.push(0xa5); // Palette's reserved byte is never alpha.
        }
    }
    let offset = bytes.len() as u32;
    bytes[10..14].copy_from_slice(&offset.to_le_bytes());
    bytes.extend(rows);
    let length = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes
}

fn palette(count: usize) -> Vec<[u8; 3]> {
    (0..count)
        .map(|i| [i as u8, (i * 71) as u8, (i * 139) as u8])
        .collect()
}

fn rgba(rgb: &[u8]) -> Vec<u8> {
    rgb.as_chunks::<3>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
        .collect()
}

fn assert_info(info: Info, width: u32, height: u32, bits: u16, top_down: bool) {
    assert_eq!((info.width, info.height), (width, height));
    assert_eq!(info.bit_depth, bits);
    assert_eq!(info.top_down, top_down);
}

fn assert_rgb(image: &Bitmap, expected: &[u8]) {
    let Pixels::Rgb(actual) = &image.pixels else {
        panic!("RGB pixels required");
    };
    assert_eq!(actual, expected);
    image.validate().unwrap();
    assert_eq!(image.to_rgba8().unwrap(), rgba(expected));
}

fn assert_rgba(image: &Bitmap, expected: &[u8]) {
    let Pixels::Rgba(actual) = &image.pixels else {
        panic!("RGBA pixels required");
    };
    assert_eq!(actual, expected);
    image.validate().unwrap();
    assert_eq!(image.to_rgba8().unwrap(), expected);
}

#[test]
fn indexed_headers_preserve_indices_palettes_orientation_and_packed_tails() {
    for size in [12, 40, 52, 56, 108, 124] {
        for bits in [1u16, 4, 8] {
            let colors = palette(if bits == 1 { 2 } else { 7 });
            for width in [1usize, 2, 3, 7, 8, 9, 17, 33, 1025] {
                for top_down in [false, true] {
                    if size == 12 && top_down {
                        continue;
                    }
                    let mut header = Header::new(width as i32, if top_down { -3 } else { 3 }, bits);
                    header.size = size;
                    let stride = (width * usize::from(bits)).div_ceil(32) * 4;
                    let mut rows = vec![0xed; stride * 3];
                    let mut indices = Vec::new();
                    for y in 0..3 {
                        for x in 0..width {
                            let index = ((x + 3 * y) % colors.len()) as u8;
                            indices.push(index);
                            let row = if top_down { y } else { 2 - y };
                            let bit = x * usize::from(bits);
                            let shift = 8 - usize::from(bits) - bit % 8;
                            let mask = ((1u16 << bits) - 1) as u8;
                            let byte = &mut rows[row * stride + bit / 8];
                            *byte = (*byte & !(mask << shift)) | (index << shift);
                        }
                    }
                    let bytes = bmp(header, &colors, &rows);
                    let image = decode(&bytes).unwrap();
                    assert_eq!((image.width, image.height), (width as u32, 3));
                    let Pixels::Indexed {
                        bit_depth,
                        indices: actual,
                        palette: actual_palette,
                    } = &image.pixels
                    else {
                        panic!("indexed pixels required");
                    };
                    assert_eq!(*bit_depth, bits as u8);
                    assert_eq!(actual, &indices);
                    assert_eq!(actual_palette, &colors);
                    let expected: Vec<u8> = indices
                        .iter()
                        .flat_map(|&i| {
                            let [r, g, b] = colors[usize::from(i)];
                            [r, g, b, 255]
                        })
                        .collect();
                    assert_eq!(image.to_rgba8().unwrap(), expected);
                    assert_info(probe(&bytes).unwrap(), width as u32, 3, bits, top_down);
                }
            }
        }
    }
}

#[test]
fn rgb24_headers_decode_bgr_rows_with_every_padding_length() {
    for size in [12, 40, 52, 56, 108, 124] {
        for width in [1usize, 2, 3, 4, 5, 12, 17] {
            for top_down in [false, true] {
                if size == 12 && top_down {
                    continue;
                }
                let mut header = Header::new(width as i32, if top_down { -2 } else { 2 }, 24);
                header.size = size;
                let stride = (width * 3).div_ceil(4) * 4;
                let mut rows = vec![0xad; stride * 2];
                let mut expected = Vec::new();
                for y in 0..2 {
                    for x in 0..width {
                        let pixel = [
                            (17 * x + y) as u8,
                            (31 * y + x) as u8,
                            (x ^ (y * 153)) as u8,
                        ];
                        expected.extend(pixel);
                        let row = if top_down { y } else { 1 - y };
                        rows[row * stride + x * 3..row * stride + x * 3 + 3]
                            .copy_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                    }
                }
                let bytes = bmp(header, &[], &rows);
                assert_rgb(&decode(&bytes).unwrap(), &expected);
                assert_info(probe(&bytes).unwrap(), width as u32, 2, 24, top_down);
            }
        }
    }
}

#[test]
fn rgb555_and_rgb565_use_defined_masks_and_bit_replication() {
    let words = [0x7c00u16, 0x03e0, 0x001f, 0x4210];
    let rows: Vec<u8> = words.into_iter().flat_map(u16::to_le_bytes).collect();
    let expected = [255, 0, 0, 0, 255, 0, 0, 0, 255, 132, 132, 132];
    for size in [40, 52, 56, 108, 124] {
        let mut header = Header::new(4, 1, 16);
        header.size = size;
        assert_rgb(&decode(&bmp(header, &[], &rows)).unwrap(), &expected);
        header.compression = 3;
        header.masks = [0xf800, 0x07e0, 0x001f, 0];
        let rows: Vec<u8> = [0xf800u16, 0x07e0, 0x001f, 0x8410]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_rgb(
            &decode(&bmp(header, &[], &rows)).unwrap(),
            &[255, 0, 0, 0, 255, 0, 0, 0, 255, 132, 130, 132],
        );
        for top_down in [false, true] {
            let mut header = Header::new(1, if top_down { -2 } else { 2 }, 16);
            header.size = size;
            let rows = if top_down {
                [0, 0x7c, 0xaa, 0xaa, 0x1f, 0, 0xbb, 0xbb]
            } else {
                [0x1f, 0, 0xbb, 0xbb, 0, 0x7c, 0xaa, 0xaa]
            };
            assert_rgb(
                &decode(&bmp(header, &[], &rows)).unwrap(),
                &[255, 0, 0, 0, 0, 255],
            );
        }
    }

    // These literal midpoint values distinguish bit replication from rounded
    // or floor division by the channel maximum.
    for (bits, one, midpoint) in [
        (1, 255, 255),
        (2, 85, 170),
        (3, 36, 146),
        (4, 17, 136),
        (5, 8, 132),
        (6, 4, 130),
        (7, 2, 129),
        (8, 1, 128),
    ] {
        let mask = (1u32 << bits) - 1;
        let mut header = Header::new(4, 1, 32);
        header.size = 52;
        header.compression = 3;
        header.masks = [mask << (bits * 2), mask << bits, mask, 0];
        let rows: Vec<u8> = [0, 1, 1 << (bits - 1), mask]
            .into_iter()
            .flat_map(|v| (v | v << bits | v << (bits * 2)).to_le_bytes())
            .collect();
        assert_rgb(
            &decode(&bmp(header, &[], &rows)).unwrap(),
            &[
                0, 0, 0, one, one, one, midpoint, midpoint, midpoint, 255, 255, 255,
            ],
        );
    }
}

#[test]
fn bitfield_masks_are_external_or_inline_at_the_declared_header_boundary() {
    for size in [40, 52, 56, 108, 124] {
        for top_down in [false, true] {
            let mut header = Header::new(1, if top_down { -2 } else { 2 }, 32);
            header.size = size;
            header.compression = 3;
            // Deliberately reversed R/B masks; output must follow the masks.
            header.masks = [0x000000ff, 0x0000ff00, 0x00ff0000, 0];
            let rows = if top_down {
                [11, 22, 33, 0xee, 44, 55, 66, 0xdd]
            } else {
                [44, 55, 66, 0xdd, 11, 22, 33, 0xee]
            };
            assert_rgb(
                &decode(&bmp(header, &[], &rows)).unwrap(),
                &[11, 22, 33, 44, 55, 66],
            );
        }
    }
    for size in [56, 108, 124] {
        let mut header = Header::new(2, 1, 32);
        header.size = size;
        header.compression = 3;
        header.masks = [0xff, 0xff00, 0xff0000, 0xff000000];
        // An explicit alpha mask preserves all-zero alpha without repair.
        assert_rgba(
            &decode(&bmp(header, &[], &[11, 22, 33, 0, 44, 55, 66, 0])).unwrap(),
            &[11, 22, 33, 0, 44, 55, 66, 0],
        );
    }
}

#[test]
fn truecolor_pixel_offsets_skip_nonzero_gaps_before_both_row_orientations() {
    for (mut header, top, bottom, alpha) in [
        (
            Header::new(1, 2, 24),
            [0, 0, 255, 0xee],
            [255, 0, 0, 0xdd],
            false,
        ),
        (
            Header {
                compression: 3,
                masks: [0xf800, 0x07e0, 0x001f, 0],
                ..Header::new(1, 2, 16)
            },
            [0, 0xf8, 0xee, 0xee],
            [0x1f, 0, 0xdd, 0xdd],
            false,
        ),
        (
            Header {
                compression: 3,
                masks: [0xff0000, 0xff00, 0xff, 0],
                ..Header::new(1, 2, 32)
            },
            [0, 0, 255, 37],
            [255, 0, 0, 91],
            false,
        ),
        (
            Header {
                size: 56,
                compression: 3,
                masks: [0xff0000, 0xff00, 0xff, 0xff000000],
                ..Header::new(1, 2, 32)
            },
            [0, 0, 255, 37],
            [255, 0, 0, 91],
            true,
        ),
    ] {
        for top_down in [false, true] {
            header.height = if top_down { -2 } else { 2 };
            let rows = if top_down {
                [top, bottom]
            } else {
                [bottom, top]
            }
            .concat();
            let original = bmp(header, &[], &rows);
            let offset = u32::from_le_bytes(original[10..14].try_into().unwrap()) as usize;
            for gap_len in [1, 7, 12] {
                let mut bytes = original[..offset].to_vec();
                bytes.resize(bytes.len() + gap_len, 0x9b);
                bytes.extend(&original[offset..]);
                bytes[10..14].copy_from_slice(&((offset + gap_len) as u32).to_le_bytes());
                let length = bytes.len() as u32;
                bytes[2..6].copy_from_slice(&length.to_le_bytes());
                let image = decode(&bytes).unwrap();
                if alpha {
                    assert_rgba(&image, &[255, 0, 0, 37, 0, 0, 255, 91]);
                } else {
                    assert_rgb(&image, &[255, 0, 0, 0, 0, 255]);
                }
                assert_info(probe(&bytes).unwrap(), 1, 2, header.bits, top_down);
            }
        }
    }
}

#[test]
fn implicit_bgra_repairs_only_an_entirely_zero_alpha_channel() {
    for size in [40, 52, 56, 108, 124] {
        let mut header = Header::new(1, -2, 32);
        header.size = size;
        assert_rgba(
            &decode(&bmp(header, &[], &[3, 2, 1, 0, 6, 5, 4, 0])).unwrap(),
            &[1, 2, 3, 255, 4, 5, 6, 255],
        );
        assert_rgba(
            &decode(&bmp(header, &[], &[3, 2, 1, 0, 6, 5, 4, 1])).unwrap(),
            &[1, 2, 3, 0, 4, 5, 6, 1],
        );
    }
    for size in [56, 108, 124] {
        let mut header = Header::new(2, 1, 16);
        header.size = size;
        header.masks[3] = 0x8000;
        let bytes = bmp(header, &[], &[0, 0x7c, 0x1f, 0x80]);
        if size == 56 {
            assert_rgb(&decode(&bytes).unwrap(), &[255, 0, 0, 0, 0, 255]);
        } else {
            assert_rgba(&decode(&bytes).unwrap(), &[255, 0, 0, 0, 0, 0, 255, 255]);
        }
    }
}

#[test]
fn palette_capacity_comes_from_pixel_offset_and_unused_bits_are_ignored() {
    let colors = [[13, 29, 47], [193, 211, 229]];
    let original = bmp(Header::new(1, 1, 1), &colors, &[0x7f, 0xff, 0xff, 0xff]);
    for claimed_count in [0u32, 1, 2, 256, u32::MAX] {
        let mut bytes = original.clone();
        bytes[46..50].copy_from_slice(&claimed_count.to_le_bytes());
        assert_eq!(
            decode(&bytes).unwrap().to_rgba8().unwrap(),
            [13, 29, 47, 255]
        );
    }
    // A full palette caps capacity even when more bytes precede the pixels.
    let offset = u32::from_le_bytes(original[10..14].try_into().unwrap()) as usize;
    let mut gap = original[..offset].to_vec();
    gap.extend([0xcc; 11]);
    gap.extend(&original[offset..]);
    gap[10..14].copy_from_slice(&((offset + 11) as u32).to_le_bytes());
    let decoded = decode(&gap).unwrap();
    let Pixels::Indexed { palette, .. } = &decoded.pixels else {
        panic!("indexed pixels required")
    };
    assert_eq!(palette, &colors);
    assert_eq!(decoded.to_rgba8().unwrap(), [13, 29, 47, 255]);

    let short = bmp(Header::new(1, 1, 8), &colors, &[1, 0xff, 0xff, 0xff]);
    assert_eq!(
        decode(&short).unwrap().to_rgba8().unwrap(),
        [193, 211, 229, 255]
    );
    let mut absent = short.clone();
    let offset = u32::from_le_bytes(absent[10..14].try_into().unwrap()) as usize;
    absent[offset] = 2;
    assert!(decode(&absent).is_err());
    assert!(decode(&bmp(Header::new(1, 1, 8), &[], &[0; 4])).is_err());
}

#[test]
fn probe_needs_headers_and_masks_but_not_palette_or_rows() {
    for (header, colors, rows, header_end) in [
        (Header::new(2, -1, 8), palette(3), vec![2, 1, 0, 0], 54),
        (
            Header {
                size: 12,
                ..Header::new(2, 1, 4)
            },
            palette(3),
            vec![0x21, 0, 0, 0],
            26,
        ),
        (
            Header {
                compression: 3,
                masks: [0xf800, 0x7e0, 0x1f, 0],
                ..Header::new(2, 1, 16)
            },
            vec![],
            vec![0, 0xf8, 0xe0, 7],
            66,
        ),
        (
            Header {
                size: 56,
                compression: 3,
                masks: [0xff0000, 0xff00, 0xff, 0xff000000],
                ..Header::new(2, 1, 32)
            },
            vec![],
            vec![0; 8],
            70,
        ),
    ] {
        let bytes = bmp(header, &colors, &rows);
        assert_info(
            probe(&bytes[..header_end]).unwrap(),
            2,
            1,
            header.bits,
            header.height < 0,
        );
        assert!(decode(&bytes[..header_end]).is_err());
        for end in 0..header_end {
            assert!(
                probe(&bytes[..end]).is_err(),
                "DIB {} prefix {end}",
                header.size
            );
        }
        decode(&bytes).unwrap();
        for end in 0..bytes.len() {
            assert!(
                decode(&bytes[..end]).is_err(),
                "DIB {} prefix {end}",
                header.size
            );
        }
    }
}

#[test]
fn declared_sizes_and_suffixes_do_not_replace_physical_input_bounds() {
    let original = bmp(Header::new(1, 1, 24), &[], &[3, 2, 1, 0]);
    for declared in [0u32, 1, u32::MAX] {
        let mut bytes = original.clone();
        bytes[2..6].copy_from_slice(&declared.to_le_bytes());
        bytes[34..38].copy_from_slice(&declared.to_le_bytes());
        assert_rgb(&decode(&bytes).unwrap(), &[1, 2, 3]);
    }
    let mut bytes = original.clone();
    bytes.extend(b"trailing application data");
    assert_rgb(&decode(&bytes).unwrap(), &[1, 2, 3]);
    assert!(matches!(
        decode_with_limits(
            &bytes,
            Limits {
                max_input_bytes: original.len(),
                ..Limits::default()
            }
        ),
        Err(Error::LimitExceeded)
    ));
    let mut truncated = original;
    truncated.pop(); // Even the final row's padding must be present.
    assert!(decode(&truncated).is_err());
}

#[test]
fn malformed_headers_and_unsupported_compressions_return_errors() {
    let original = bmp(Header::new(2, 2, 24), &[], &[0; 16]);
    let mut cases = Vec::new();
    for (offset, value) in [
        (14, 16u32),
        (18, 0),
        (18, u32::MAX),
        (22, 0),
        (22, 0x80000000),
        (10, 53),
    ] {
        let mut bytes = original.clone();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        cases.push(bytes);
    }
    for (offset, value) in [(26, 0u16), (26, 2), (28, 0), (28, 2), (28, 15), (28, 48)] {
        let mut bytes = original.clone();
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        cases.push(bytes);
    }
    let mut bad_signature = original.clone();
    bad_signature[..2].copy_from_slice(b"BA");
    cases.push(bad_signature);
    for bytes in cases {
        assert!(probe(&bytes).is_err());
        assert!(decode(&bytes).is_err());
    }
    for (compression, bits) in [(1, 8), (2, 4), (4, 24), (5, 24), (6, 32), (99, 24)] {
        let header = Header {
            compression,
            ..Header::new(2, 2, bits)
        };
        let bytes = bmp(header, &palette(if bits <= 8 { 2 } else { 0 }), &[0; 16]);
        assert!(matches!(probe(&bytes), Err(Error::Unsupported(_))));
        assert!(matches!(decode(&bytes), Err(Error::Unsupported(_))));
    }
    for bits in [16, 32] {
        let bytes = bmp(
            Header {
                size: 12,
                ..Header::new(1, 1, bits)
            },
            &[],
            &[0; 4],
        );
        assert!(probe(&bytes).is_err());
        assert!(decode(&bytes).is_err());
    }
    for bits in [1, 4, 8, 24] {
        let bytes = bmp(
            Header {
                compression: 3,
                masks: [0xff0000, 0xff00, 0xff, 0],
                ..Header::new(1, 1, bits)
            },
            &[],
            &[0; 4],
        );
        assert!(probe(&bytes).is_err());
        assert!(decode(&bytes).is_err());
    }
}

#[test]
fn malformed_bit_masks_are_rejected_before_decoding_pixels() {
    for (bits, masks) in [
        (16, [0, 0x03e0, 0x001f, 0]),
        (16, [0x7c00, 0x7c00, 0x001f, 0]),
        (16, [0x7400, 0x03e0, 0x001f, 0]),
        (16, [0x1f000, 0x03e0, 0x001f, 0]),
        (32, [0x1ff0000, 0xff00, 0xff, 0]),
        (32, [0xff0000, 0xff00, 0xff, 0xff]),
        (32, [0xff0000, 0xff00, 0xff, 0xa0000000]),
    ] {
        let header = Header {
            size: 56,
            compression: 3,
            masks,
            ..Header::new(1, 1, bits)
        };
        let bytes = bmp(header, &[], &[0; 4]);
        assert!(probe(&bytes).is_err(), "{bits}: {masks:x?}");
        assert!(decode(&bytes).is_err(), "{bits}: {masks:x?}");
    }
}

#[test]
fn full_width_bitfield_keeps_the_noncontiguous_mask_error() {
    let header = Header {
        size: 56,
        compression: 3,
        masks: [u32::MAX, 0, 0, 0],
        ..Header::new(1, 1, 32)
    };
    let bytes = bmp(header, &[], &[0; 4]);
    let error = Error::Invalid("noncontiguous bitfield mask");
    assert_eq!(probe(&bytes), Err(error.clone()));
    assert_eq!(decode(&bytes), Err(error));
}

#[test]
fn input_pixel_and_decoded_limits_apply_to_decode_and_header_only_probe() {
    for (bytes, decoded_bytes, header_end) in [
        (
            bmp(Header::new(2, 1, 8), &palette(3), &[1, 2, 0, 0]),
            11,
            54,
        ),
        (
            bmp(Header::new(2, 1, 24), &[], &[3, 2, 1, 6, 5, 4, 0, 0]),
            6,
            54,
        ),
        (
            bmp(Header::new(2, 1, 32), &[], &[3, 2, 1, 0, 6, 5, 4, 1]),
            8,
            54,
        ),
    ] {
        let limits = Limits {
            max_input_bytes: bytes.len(),
            max_pixels: 2,
            max_decoded_bytes: decoded_bytes,
        };
        decode_with_limits(&bytes, limits).unwrap();
        probe_with_limits(&bytes, limits).unwrap();
        probe_with_limits(&bytes[..header_end], limits).unwrap();
        for limited in [
            Limits {
                max_input_bytes: bytes.len() - 1,
                ..limits
            },
            Limits {
                max_pixels: 1,
                ..limits
            },
            Limits {
                max_decoded_bytes: decoded_bytes - 1,
                ..limits
            },
        ] {
            assert!(matches!(
                decode_with_limits(&bytes, limited),
                Err(Error::LimitExceeded)
            ));
            assert!(matches!(
                probe_with_limits(&bytes, limited),
                Err(Error::LimitExceeded)
            ));
        }
        assert!(matches!(
            probe_with_limits(
                &bytes[..header_end],
                Limits {
                    max_decoded_bytes: decoded_bytes - 1,
                    ..limits
                }
            ),
            Err(Error::LimitExceeded)
        ));
    }
}

#[test]
fn positive_maximum_signed_dimensions_hit_limits_before_pixel_allocation() {
    for (width, height) in [(i32::MAX, 1), (1, i32::MAX), (i32::MAX, i32::MAX)] {
        // A complete, structurally valid header with no pixel allocation or data.
        let bytes = bmp(Header::new(width, height, 24), &[], &[]);
        assert!(matches!(probe(&bytes), Err(Error::LimitExceeded)));
        assert!(matches!(decode(&bytes), Err(Error::LimitExceeded)));
        let limits = Limits {
            max_pixels: usize::MAX,
            max_decoded_bytes: 1,
            ..Limits::default()
        };
        assert!(matches!(
            probe_with_limits(&bytes, limits),
            Err(Error::LimitExceeded)
        ));
        assert!(matches!(
            decode_with_limits(&bytes, limits),
            Err(Error::LimitExceeded)
        ));
    }
}

#[test]
fn maximum_32_bit_raster_respects_address_space_limits() {
    let limits = Limits {
        max_pixels: usize::MAX,
        max_decoded_bytes: usize::MAX,
        ..Limits::default()
    };
    for height in [1, i32::MAX] {
        let bytes = bmp(Header::new(i32::MAX, height, 32), &[], &[]);
        if usize::BITS == 64 {
            assert_eq!(
                probe_with_limits(&bytes, limits).unwrap(),
                Info {
                    width: i32::MAX as u32,
                    height: height as u32,
                    bit_depth: 32,
                    top_down: false,
                }
            );
            assert!(matches!(
                decode_with_limits(&bytes, limits),
                Err(Error::Truncated)
            ));
        } else {
            assert!(matches!(
                probe_with_limits(&bytes, limits),
                Err(Error::LimitExceeded)
            ));
            assert!(matches!(
                decode_with_limits(&bytes, limits),
                Err(Error::LimitExceeded)
            ));
        }
    }
}
