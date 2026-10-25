use super::texture_pixels_perf::compare;
use super::*;
use std::hint::black_box;

include!("indexed_pixels_original.rs");

#[path = "decode/expansion_perf.rs"]
mod expansion_perf;

fn indices(width: u32, height: u32, bits: u8) -> Vec<u8> {
    let pitch = (width as usize * usize::from(bits)).div_ceil(8);
    (0..pitch * height as usize)
        .map(|i| (i.wrapping_mul(71) ^ (i / pitch).wrapping_mul(13)) as u8)
        .collect()
}

fn write_png(path: &Path, width: u32, height: u32, bits: u8) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(match bits {
        1 => png::BitDepth::One,
        2 => png::BitDepth::Two,
        4 => png::BitDepth::Four,
        8 => png::BitDepth::Eight,
        _ => unreachable!(),
    });
    let mut palette: Vec<_> = (0..1u16 << bits)
        .flat_map(|i| [i as u8, i as u8 ^ 0x55, i as u8 ^ 0xaa])
        .collect();
    // Duplicate pink entries must remain distinct under palette color keying.
    palette[..6].copy_from_slice(&[255, 0, 255, 255, 0, 255]);
    encoder.set_palette(palette);
    encoder.set_trns(vec![255; 1usize << bits]);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&indices(width, height, bits))
        .unwrap();
}

fn write_bmp(path: &Path, width: u32, height: u32, bits: u8, top_down: bool) {
    let palette_len = 1usize << bits;
    let offset = 54 + 4 * palette_len;
    let pitch = (width as usize * usize::from(bits)).div_ceil(32) * 4;
    let mut bytes = vec![0u8; offset + pitch * height as usize];
    bytes[..2].copy_from_slice(b"BM");
    let length = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&width.to_le_bytes());
    let signed_height = if top_down {
        -(height as i32)
    } else {
        height as i32
    };
    bytes[22..26].copy_from_slice(&signed_height.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&u16::from(bits).to_le_bytes());
    for (i, entry) in bytes[54..offset]
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .enumerate()
    {
        entry.copy_from_slice(&[i as u8 ^ 0xaa, i as u8 ^ 0x55, i as u8, 0]);
    }
    let packed = indices(width, height, bits);
    let source_pitch = (width as usize * usize::from(bits)).div_ceil(8);
    for (source, destination) in packed
        .chunks_exact(source_pitch)
        .zip(bytes[offset..].chunks_exact_mut(pitch))
    {
        destination[..source_pitch].copy_from_slice(source);
    }
    std::fs::write(path, bytes).unwrap();
}

fn keyed_pixels(width: u32, height: u32, pattern: &str) -> RgbaImage {
    let mut state = 0x1234_5678u32;
    RgbaImage::from_fn(width, height, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let pixel = match pattern {
            "mixed" if state & 1 == 0 => [255, 0, 255, 255],
            "off_pink" if state & 1 == 0 || y == 0 => [248, 0, 248, 255],
            "solid" => [255, 0, 255, 255],
            "edge" if x == 0 && y == height - 1 => [248, 0, 248, 255],
            _ => state.to_le_bytes(),
        };
        image::Rgba(pixel)
    })
}

#[test]
fn indexed_decoders_match_original_pixels_and_errors() {
    let dir = crate::perf_fixture::TempDir::new("indexed-parity");
    let path = dir.path.join("input");
    for (width, height) in [(1, 1), (3, 5), (7, 9), (17, 2), (65, 129)] {
        for bits in [1, 2, 4, 8] {
            write_png(&path, width, height, bits);
            for key in [false, true] {
                assert_eq!(
                    decode_png(&path, key).unwrap(),
                    decode_png_original(&path, key).unwrap(),
                    "PNG {width}x{height} bits={bits} key={key}"
                );
            }
            if bits == 2 {
                continue;
            }
            for top_down in [false, true] {
                write_bmp(&path, width, height, bits, top_down);
                for key in [false, true] {
                    assert_eq!(
                        decode_indexed_bmp(&path, key).unwrap(),
                        decode_indexed_bmp_original(&path, key).unwrap(),
                        "BMP {width}x{height} bits={bits} key={key} top_down={top_down}"
                    );
                }
            }
        }
    }
    // Truncation and malformed palette references retain the same errors.
    write_bmp(&path, 17, 5, 8, false);
    let bytes = std::fs::read(&path).unwrap();
    for length in [0, 17, 40, 100, bytes.len() - 1] {
        std::fs::write(&path, &bytes[..length]).unwrap();
        assert_eq!(
            decode_indexed_bmp(&path, true).map_err(|e| e.to_string()),
            decode_indexed_bmp_original(&path, true).map_err(|e| e.to_string())
        );
    }
    for bits in [1, 4, 8] {
        write_bmp(&path, 17, 5, bits, false);
        let mut bytes = std::fs::read(&path).unwrap();
        // A single palette entry, followed by a pixel referring outside it.
        bytes[10..14].copy_from_slice(&58u32.to_le_bytes());
        bytes[58] = 255;
        std::fs::write(&path, bytes).unwrap();
        let result = decode_indexed_bmp(&path, true).map_err(|e| e.to_string());
        assert!(
            result
                .as_ref()
                .unwrap_err()
                .contains("palette index out of range")
        );
        assert_eq!(
            result,
            decode_indexed_bmp_original(&path, true).map_err(|e| e.to_string())
        );
    }
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/itgmania-song-lua-micro");
    for directory in [
        "indexed-bitmap-files",
        "bitmap-file-decoding",
        "texture-surface",
    ] {
        for entry in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = entry.unwrap().path();
            if !path.is_file() {
                continue;
            }
            let signature = image_signature(&path).unwrap();
            for key in [false, true] {
                if signature == *b"\x89PNG\r\n\x1a\n" {
                    assert_eq!(
                        decode_png(&path, key).map_err(|e| e.to_string()),
                        decode_png_original(&path, key).map_err(|e| e.to_string()),
                        "{path:?}"
                    );
                } else if &signature[..2] == b"BM" {
                    assert_eq!(
                        decode_indexed_bmp(&path, key).map_err(|e| e.to_string()),
                        decode_indexed_bmp_original(&path, key).map_err(|e| e.to_string()),
                        "{path:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn color_key_matches_original_edge_selection_and_alpha() {
    for (width, height) in [
        (0, 0),
        (0, 3),
        (3, 0),
        (1, 1),
        (1, 7),
        (7, 1),
        (3, 5),
        (63, 65),
        (512, 513),
    ] {
        for pattern in ["random", "mixed", "off_pink", "solid", "edge"] {
            let mut before = keyed_pixels(width, height, pattern);
            let mut after = before.clone();
            key_rgba_original(&mut before);
            key_rgba(&mut after);
            assert_eq!(before, after, "{width}x{height} {pattern}");
        }
    }
    let mut image = RgbaImage::from_raw(
        4,
        1,
        vec![
            255, 0, 255, 254, 255, 0, 255, 255, 248, 0, 248, 0, 248, 0, 248, 254,
        ],
    )
    .unwrap();
    key_rgba(&mut image);
    assert_eq!(
        image.into_raw(),
        [
            255, 0, 255, 254, 0, 0, 0, 0, 248, 0, 248, 0, 248, 0, 248, 254
        ]
    );
    // ImageBuffer accepts backing buffers longer than its logical image.
    let mut image = RgbaImage::from_raw(1, 1, vec![255, 0, 255, 255, 255, 0, 255, 255, 3]).unwrap();
    key_rgba(&mut image);
    assert_eq!(image.into_raw(), [0, 0, 0, 0, 255, 0, 255, 255, 3]);
}

#[test]
#[ignore = "paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_indexed_pixels() {
    #[cfg(windows)]
    super::texture_pixels_perf::pin_worker();
    let dir = crate::perf_fixture::TempDir::new("indexed-bench");
    let path = dir.path.join("input");
    let filter = std::env::var("DEADSYNC_INDEXED_BENCH").unwrap_or_default();
    if filter.is_empty() || filter == "png" {
        for bits in [1, 2, 4, 8] {
            for size in [64, 512, 1024] {
                write_png(&path, size, size, bits);
                compare(
                    &format!("png_{bits}bit_{size}"),
                    20,
                    || (),
                    |(), current| {
                        if current {
                            decode_png(black_box(&path), false)
                        } else {
                            decode_png_original(black_box(&path), false)
                        }
                        .unwrap()
                    },
                );
            }
        }
    }
    if filter.is_empty() || filter == "bmp" {
        for bits in [1, 4, 8] {
            for size in [64, 512, 1024] {
                write_bmp(&path, size, size, bits, false);
                compare(
                    &format!("bmp_{bits}bit_{size}"),
                    20,
                    || (),
                    |(), current| {
                        if current {
                            decode_indexed_bmp(black_box(&path), false)
                        } else {
                            decode_indexed_bmp_original(black_box(&path), false)
                        }
                        .unwrap()
                        .unwrap()
                    },
                );
            }
        }
    }
    if filter.is_empty() || filter == "key" {
        for size in [64, 512, 1024] {
            for pattern in ["random", "mixed", "off_pink", "solid"] {
                let image = keyed_pixels(size, size, pattern);
                compare(
                    &format!("key_{pattern}_{size}"),
                    20,
                    || image.clone(),
                    |mut image, current| {
                        if current {
                            key_rgba(black_box(&mut image));
                        } else {
                            key_rgba_original(black_box(&mut image));
                        }
                        image
                    },
                );
            }
        }
    }
}
