//! Production decoder CPU-cycle, allocation, and exact-pixel checks.
use super::*;
use crate::decode::preparation_perf::bench;
use std::hint::black_box;

fn png_file(path: &Path, width: u32, height: u32, color: png::ColorType, bits: png::BitDepth) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), width, height);
    encoder.set_color(color);
    encoder.set_depth(bits);
    encoder.set_compression(png::Compression::Fastest);
    if color == png::ColorType::Indexed {
        encoder.set_palette(
            (0..1usize << bits as usize)
                .flat_map(|i| [i as u8, i as u8 ^ 85, i as u8 ^ 170])
                .collect::<Vec<_>>(),
        );
    }
    let pitch = (width as usize * color.samples() * bits as usize).div_ceil(8);
    let bytes: Vec<_> = (0..pitch * height as usize)
        .map(|i| (i.wrapping_mul(71) ^ (i / pitch).wrapping_mul(13)) as u8)
        .collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&bytes)
        .unwrap();
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = crate::perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU-cycle and allocation benchmark"]
fn benchmark_png_expansion() {
    let dir = crate::perf_fixture::TempDir::new("png-expansion-bench");
    let path = dir.path.join("input.png");
    for (name, color, bits) in [
        ("rgb", png::ColorType::Rgb, png::BitDepth::Eight),
        ("gray", png::ColorType::Grayscale, png::BitDepth::Eight),
        (
            "gray-alpha",
            png::ColorType::GrayscaleAlpha,
            png::BitDepth::Eight,
        ),
        ("rgba-control", png::ColorType::Rgba, png::BitDepth::Eight),
        ("index1", png::ColorType::Indexed, png::BitDepth::One),
        ("index2", png::ColorType::Indexed, png::BitDepth::Two),
        ("index4", png::ColorType::Indexed, png::BitDepth::Four),
        (
            "index8-control",
            png::ColorType::Indexed,
            png::BitDepth::Eight,
        ),
    ] {
        for size in [64, 512, 1024] {
            png_file(&path, size, size, color, bits);
            measure(&format!("expand/{name}/{size}"), 16, || {
                let decode =
                    black_box(decode_png as fn(&Path, bool) -> image::ImageResult<RgbaImage>);
                black_box(decode(black_box(&path), false).unwrap());
            });
        }
    }
}

#[test]
#[ignore = "release CPU-cycle and allocation benchmark"]
fn benchmark_bmp_expansion() {
    let dir = crate::perf_fixture::TempDir::new("bmp-expansion-bench");
    let path = dir.path.join("input.bmp");
    for bits in [1, 4, 8] {
        for size in [64, 512, 1024] {
            write_bmp(&path, size, size, bits, false);
            measure(&format!("expand/bmp{bits}/{size}"), 16, || {
                let decode = black_box(
                    decode_indexed_bmp as fn(&Path, bool) -> image::ImageResult<Option<RgbaImage>>,
                );
                black_box(decode(black_box(&path), false).unwrap());
            });
        }
    }
}

#[test]
fn png_expansion_preserves_pixels() {
    let dir = crate::perf_fixture::TempDir::new("png-expansion-parity");
    let path = dir.path.join("input.png");
    for (width, height) in [
        (1, 1),
        (2, 3),
        (3, 5),
        (4, 7),
        (5, 3),
        (6, 9),
        (7, 9),
        (8, 3),
        (9, 2),
        (15, 5),
        (16, 2),
        (17, 257),
        (31, 2),
        (33, 17),
        (65, 129),
    ] {
        for color in [
            png::ColorType::Rgb,
            png::ColorType::Grayscale,
            png::ColorType::GrayscaleAlpha,
            png::ColorType::Indexed,
        ] {
            let depths: &[png::BitDepth] = if color == png::ColorType::Indexed {
                &[
                    png::BitDepth::One,
                    png::BitDepth::Two,
                    png::BitDepth::Four,
                    png::BitDepth::Eight,
                ]
            } else {
                &[png::BitDepth::Eight, png::BitDepth::Sixteen]
            };
            for &bits in depths {
                png_file(&path, width, height, color, bits);
                for key in [false, true] {
                    assert_eq!(
                        decode_png(&path, key).unwrap(),
                        decode_png_original(&path, key).unwrap(),
                        "{width}x{height} {color:?} {bits:?} key={key}"
                    );
                }
            }
        }
    }
}

#[test]
fn bmp_partial_palettes_preserve_errors_and_ignore_padding() {
    let dir = crate::perf_fixture::TempDir::new("bmp-palette-parity");
    let path = dir.path.join("input.bmp");
    for bits in [1u8, 4, 8] {
        let entries = 1usize << bits;
        for count in [1, entries / 2, entries - 1, entries] {
            for width in [1u32, 3, 7, 8, 9, 17, 33] {
                for top_down in [false, true] {
                    write_bmp(&path, width, 3, bits, top_down);
                    let mut bytes = std::fs::read(&path).unwrap();
                    let offset = 54 + count * 4;
                    bytes.drain(offset..54 + entries * 4);
                    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
                    let length = bytes.len() as u32;
                    bytes[2..6].copy_from_slice(&length.to_le_bytes());
                    let pitch = (width as usize * bits as usize).div_ceil(32) * 4;
                    for row in bytes[offset..].chunks_exact_mut(pitch) {
                        row.fill(255);
                        let used_bits = width as usize * bits as usize;
                        row[..used_bits.div_ceil(8)].fill(0);
                        if !used_bits.is_multiple_of(8) {
                            row[used_bits / 8] = (1 << (8 - used_bits % 8)) - 1;
                        }
                        let last_bit = (width as usize - 1) * bits as usize;
                        row[last_bit / 8] |=
                            ((count - 1) as u8) << (8 - bits as usize - last_bit % 8);
                    }
                    std::fs::write(&path, &bytes).unwrap();
                    for key in [false, true] {
                        assert_eq!(
                            decode_indexed_bmp(&path, key).unwrap(),
                            decode_indexed_bmp_original(&path, key).unwrap(),
                            "bits={bits} count={count} width={width} top_down={top_down} key={key}"
                        );
                    }
                    if count < entries {
                        let shift = 8 - bits;
                        bytes[offset] =
                            (bytes[offset] & !(u8::MAX << shift)) | ((count as u8) << shift);
                        std::fs::write(&path, &bytes).unwrap();
                        let actual = decode_indexed_bmp(&path, false).map_err(|e| e.to_string());
                        assert!(
                            actual
                                .as_ref()
                                .unwrap_err()
                                .contains("palette index out of range")
                        );
                        assert_eq!(
                            actual,
                            decode_indexed_bmp_original(&path, false).map_err(|e| e.to_string())
                        );
                    }
                }
            }
        }
    }
}
