//! Decoder equality checks and CPU/allocation measurements for palette setup.
use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "palette_original.rs"]
mod palette_original;

fn gif_file(path: &Path, size: u16, global: bool, local: bool, offset: bool) {
    let palette: Vec<_> = (0..=255u8).flat_map(|i| [i, i ^ 85, i ^ 170]).collect();
    let mut encoder = gif::Encoder::new(
        std::fs::File::create(path).unwrap(),
        size + 2,
        size + 3,
        if global { &palette } else { &[] },
    )
    .unwrap();
    let frame = gif::Frame {
        width: size,
        height: size,
        left: u16::from(offset),
        top: u16::from(offset) * 2,
        palette: local.then(|| palette.iter().map(|byte| byte ^ 31).collect()),
        transparent: Some(17),
        buffer: (0..usize::from(size).pow(2))
            .map(|i| (i * 71) as u8)
            .collect::<Vec<_>>()
            .into(),
        ..Default::default()
    };
    encoder.write_frame(&frame).unwrap();
}

fn bmp_bytes(width: u32, height: u32, bits: u16, os2: bool, top_down: bool) -> Vec<u8> {
    let header = if os2 { 12usize } else { 40 };
    let entry = if os2 { 3 } else { 4 };
    let count = 1usize << bits;
    let offset = 14 + header + count * entry;
    let pitch = (width as usize * usize::from(bits)).div_ceil(32) * 4;
    let mut bytes = vec![0; offset + pitch * height as usize];
    bytes[..2].copy_from_slice(b"BM");
    let len = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&len.to_le_bytes());
    bytes[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
    bytes[14..18].copy_from_slice(&(header as u32).to_le_bytes());
    if os2 {
        bytes[18..20].copy_from_slice(&(width as u16).to_le_bytes());
        bytes[20..22].copy_from_slice(&(height as u16).to_le_bytes());
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        bytes[24..26].copy_from_slice(&bits.to_le_bytes());
    } else {
        bytes[18..22].copy_from_slice(&width.to_le_bytes());
        let height = if top_down {
            -(height as i32)
        } else {
            height as i32
        };
        bytes[22..26].copy_from_slice(&height.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&bits.to_le_bytes());
    }
    for (i, color) in bytes[14 + header..offset]
        .chunks_exact_mut(entry)
        .enumerate()
    {
        color[..3].copy_from_slice(&[i as u8 ^ 170, i as u8 ^ 85, i as u8]);
    }
    for (i, byte) in bytes[offset..].iter_mut().enumerate() {
        *byte = (i * 71) as u8;
    }
    bytes
}

#[test]
fn palette_decoders_match_frozen_pixels_and_truncation_errors() {
    let dir = crate::perf_fixture::TempDir::new("palette-parity");
    let path = dir.path.join("fixture");
    for global in [false, true] {
        for local in [false, true] {
            if !global && !local {
                continue;
            }
            for offset in [false, true] {
                gif_file(&path, 7, global, local, offset);
                let bytes = std::fs::read(&path).unwrap();
                for len in 0..=bytes.len() {
                    std::fs::write(&path, &bytes[..len]).unwrap();
                    for key in [false, true] {
                        assert_eq!(
                            decode_gif(&path, key).map_err(|e| e.to_string()),
                            palette_original::decode_gif_before(&path, key)
                                .map_err(|e| e.to_string()),
                            "GIF global={global}, local={local}, offset={offset}, len={len}"
                        );
                    }
                }
            }
        }
    }
    for bits in [1, 4, 8] {
        for os2 in [false, true] {
            for top_down in [false, true] {
                if os2 && top_down {
                    continue;
                }
                let bytes = bmp_bytes(9, 3, bits, os2, top_down);
                for len in 0..=bytes.len() {
                    std::fs::write(&path, &bytes[..len]).unwrap();
                    for key in [false, true] {
                        assert_eq!(
                            decode_indexed_bmp(&path, key).map_err(|e| e.to_string()),
                            palette_original::decode_indexed_bmp_before(&path, key)
                                .map_err(|e| e.to_string()),
                            "BMP bits={bits}, os2={os2}, top_down={top_down}, len={len}"
                        );
                    }
                }
            }
        }
    }
}

fn measure<T>(label: &str, iterations: usize, mut work: impl FnMut() -> T) {
    let (_, churn) = crate::perf::measure(|| {
        black_box(work());
    });
    println!("{label}: allocation {churn:?}");
    bench::measure(label, iterations, || {
        black_box(work());
    });
}

#[test]
#[ignore = "optimized CPU-cycle and allocation benchmark"]
fn benchmark_palette_decoders() {
    let dir = crate::perf_fixture::TempDir::new("palette-bench");
    let path = dir.path.join("fixture");
    for (name, global, local) in [
        ("global", true, false),
        ("local", false, true),
        ("both", true, true),
    ] {
        for size in [1, 16, 256] {
            gif_file(&path, size, global, local, true);
            measure(
                &format!("palette/gif/{name}/{size}"),
                if size < 256 { 128 } else { 16 },
                || {
                    black_box(decode_gif as fn(&Path, bool) -> image::ImageResult<RgbaImage>)(
                        black_box(&path),
                        false,
                    )
                    .unwrap()
                },
            );
        }
    }
    for bits in [1, 4, 8] {
        for os2 in [false, true] {
            for size in [1, 16, 256] {
                std::fs::write(&path, bmp_bytes(size, size, bits, os2, false)).unwrap();
                measure(
                    &format!("palette/bmp/{bits}/os2={os2}/{size}"),
                    if size < 256 { 128 } else { 16 },
                    || {
                        black_box(
                            decode_indexed_bmp
                                as fn(&Path, bool) -> image::ImageResult<Option<RgbaImage>>,
                        )(black_box(&path), false)
                        .unwrap()
                    },
                );
            }
        }
    }
}
