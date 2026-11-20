use deadlib_image_bmp::{Bitmap, EncodeOptions, Limits, Pixels};
use std::{fmt::Write as _, io::Write as _, path::Path};

#[path = "../fixtures.rs"]
mod fixtures;

fn blob(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    output.extend_from_slice(bytes);
}

fn record(output: &mut Vec<u8>, bytes: &[u8], limits: Limits, records: &mut usize) {
    // Store the complete input alongside all observable results. Snapshot replay
    // detects changed fixtures as well as changed codec pixels, metadata or errors.
    blob(output, bytes);
    let mut result = format!(
        "{limits:?}\n{:?}\n",
        deadlib_image_bmp::probe_with_limits(bytes, limits)
    );
    match deadlib_image_bmp::decode_with_limits(bytes, limits) {
        Ok(bitmap) => {
            writeln!(result, "{bitmap:?}").unwrap();
            writeln!(result, "{:?}", bitmap.to_rgba8_with_limits(limits)).unwrap();
            for top_down in [false, true] {
                writeln!(
                    result,
                    "{:?}",
                    deadlib_image_bmp::encode_with_options(
                        &bitmap,
                        EncodeOptions {
                            top_down,
                            max_output_bytes: limits.max_decoded_bytes
                        }
                    )
                )
                .unwrap();
            }
        }
        Err(error) => {
            writeln!(result, "{error:?}").unwrap();
        }
    }
    blob(output, result.as_bytes());
    *records += 1;
}

fn record_model(output: &mut Vec<u8>, bitmap: &Bitmap, records: &mut usize) {
    let mut result = format!(
        "{bitmap:?}\n{:?}\n{:?}\n",
        bitmap.validate(),
        bitmap.to_rgba8()
    );
    for max_decoded_bytes in [0, 3, 4, 63, 64, 255, 256] {
        let limits = Limits {
            max_decoded_bytes,
            ..Limits::default()
        };
        writeln!(
            result,
            "{max_decoded_bytes}:{:?}",
            bitmap.to_rgba8_with_limits(limits)
        )
        .unwrap();
    }
    for top_down in [false, true] {
        writeln!(
            result,
            "{:?}",
            deadlib_image_bmp::encode_with_options(
                bitmap,
                EncodeOptions {
                    top_down,
                    ..EncodeOptions::default()
                }
            )
        )
        .unwrap();
    }
    blob(output, result.as_bytes());
    *records += 1;
}

fn snapshot() -> (Vec<u8>, usize) {
    let mut output = b"BMP-native-snapshot-v1\n".to_vec();
    let mut records = 0;
    let bounded = Limits {
        max_input_bytes: 16384,
        max_pixels: 4096,
        max_decoded_bytes: 65536,
    };
    for &layout in fixtures::LAYOUTS {
        for (width, height) in [(1, 1), (17, 3), (65, 5)] {
            for top_down in [false, true] {
                if top_down && !layout.supports_top_down() {
                    continue;
                }
                let case = fixtures::build(layout, width, height, top_down);
                blob(&mut output, case.name.as_bytes());
                blob(&mut output, case.policy.as_bytes());
                assert!(case.expected.to_rgba8().unwrap() == case.rgba);
                record(&mut output, &case.bytes, Limits::default(), &mut records);
                record_model(&mut output, &case.expected, &mut records);
                if (width, height) != (17, 3) {
                    continue;
                }
                for prefix in 0..case.bytes.len() {
                    record(&mut output, &case.bytes[..prefix], bounded, &mut records);
                }
                for position in [0, 1, 2, 10, 14, 18, 22, 26, 28, 30, 34, 46, 54, 58, 62, 66] {
                    if position + 4 > case.bytes.len() {
                        continue;
                    }
                    for value in [
                        0,
                        1,
                        2,
                        3,
                        4,
                        8,
                        12,
                        16,
                        24,
                        32,
                        40,
                        52,
                        56,
                        108,
                        124,
                        255,
                        256,
                        4096,
                        u32::MAX,
                    ] {
                        let mut changed = case.bytes.clone();
                        changed[position..position + 4].copy_from_slice(&value.to_le_bytes());
                        record(&mut output, &changed, bounded, &mut records);
                    }
                }
                let offset = u32::from_le_bytes(case.bytes[10..14].try_into().unwrap()) as usize;
                for position in offset..case.bytes.len().min(offset + 16) {
                    for value in [0, 1, 15, 16, 127, 128, 254, 255] {
                        let mut changed = case.bytes.clone();
                        changed[position] = value;
                        record(&mut output, &changed, bounded, &mut records);
                    }
                }
                let count = width * height;
                for limits in [
                    Limits {
                        max_input_bytes: case.bytes.len() - 1,
                        ..bounded
                    },
                    Limits {
                        max_input_bytes: case.bytes.len(),
                        ..bounded
                    },
                    Limits {
                        max_pixels: count - 1,
                        ..bounded
                    },
                    Limits {
                        max_pixels: count,
                        ..bounded
                    },
                    Limits {
                        max_decoded_bytes: count - 1,
                        ..bounded
                    },
                    Limits {
                        max_decoded_bytes: count,
                        ..bounded
                    },
                    Limits {
                        max_decoded_bytes: count * 3,
                        ..bounded
                    },
                    Limits {
                        max_decoded_bytes: count * 4 - 1,
                        ..bounded
                    },
                    Limits {
                        max_decoded_bytes: count * 4,
                        ..bounded
                    },
                ] {
                    record(&mut output, &case.bytes, limits, &mut records);
                }
            }
        }
    }
    for bit_depth in [0, 1, 2, 4, 8, 9] {
        for palette_len in [0, 1, 2, 13, 16, 255, 256, 257] {
            for indices in [
                vec![],
                vec![0],
                vec![0, 1, 2, 255],
                vec![0; 16],
                vec![255; 16],
            ] {
                record_model(
                    &mut output,
                    &Bitmap {
                        width: 4,
                        height: 4,
                        pixels: Pixels::Indexed {
                            bit_depth,
                            indices,
                            palette: vec![[13, 29, 47]; palette_len],
                        },
                    },
                    &mut records,
                );
            }
        }
    }
    for (width, height) in [(0, 0), (0, 1), (1, 0), (1, 1), (4, 4), (u32::MAX, u32::MAX)] {
        for length in [0, 1, 3, 4, 47, 48, 63, 64, 65] {
            for pixels in [
                Pixels::Rgb(vec![31; length]),
                Pixels::Rgba(vec![127; length]),
            ] {
                record_model(
                    &mut output,
                    &Bitmap {
                        width,
                        height,
                        pixels,
                    },
                    &mut records,
                );
            }
        }
    }
    (output, records)
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3, "native_snapshot capture|verify PATH");
    let (bytes, records) = snapshot();
    let path = Path::new(&args[2]);
    match args[1].as_str() {
        "capture" => {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            file.write_all(&bytes).unwrap();
            println!(
                "Captured {records} exact native records ({} bytes): {}",
                bytes.len(),
                path.display()
            );
        }
        "verify" => {
            let previous = std::fs::read(path).unwrap();
            if bytes != previous {
                let index = bytes
                    .iter()
                    .zip(&previous)
                    .position(|(a, b)| a != b)
                    .unwrap_or(bytes.len().min(previous.len()));
                panic!(
                    "native snapshot differs at byte {index}; current {} bytes, expected {}",
                    bytes.len(),
                    previous.len()
                );
            }
            println!(
                "Verified {records} exact native records ({} bytes): {}",
                bytes.len(),
                path.display()
            );
        }
        _ => panic!("native_snapshot capture|verify PATH"),
    }
}
