//! External RGB8 corpus benchmark; see BENCHMARKS.md for input acquisition.
use deadlib_image_png::{ColorType, EncodeOptions, Png, Samples, decode, encode, encode_writer};
use std::{hint::black_box, io::Cursor, path::PathBuf, time::Instant};

fn reference_decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut out).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    out.truncate(info.buffer_size());
    reader.finish().unwrap();
    (info.width, info.height, out)
}

fn reference_encode(png: &Png) -> Vec<u8> {
    let Samples::U8(samples) = &png.image.samples else {
        unreachable!()
    };
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, png.image.width, png.image.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(samples)
        .unwrap();
    out
}

fn time<T>(mut run: impl FnMut() -> T, iterations: usize) -> u128 {
    for _ in 0..3 {
        black_box(run());
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(run());
    }
    start.elapsed().as_nanos() / iterations as u128
}

fn main() {
    let root = PathBuf::from(
        std::env::var_os("DEADLIB_PNG_CORPUS")
            .expect("set DEADLIB_PNG_CORPUS to the fetched RGB8 corpus directory"),
    );
    let iterations = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .ok()
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(10)
        .max(1);
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "png"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "corpus contains no PNG files");
    println!(
        "case\tcodec\toperation\titerations\tencoded_bytes\tsample_bytes\tns_per_image\tMiB_per_second\timage_pipeline_bytes"
    );
    for path in paths {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let original = decode(&bytes).unwrap();
        assert_eq!(original.image.color_type, ColorType::Rgb);
        assert_eq!(original.image.bit_depth, 8);
        let expected = (
            original.image.width,
            original.image.height,
            match &original.image.samples {
                Samples::U8(v) => v.clone(),
                _ => unreachable!(),
            },
        );
        assert_eq!(reference_decode(&bytes), expected);
        assert_eq!(
            deadlib_image_png::decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
            original
        );
        // Both encoders receive exactly the same native pixels and no ancillary metadata.
        let png = Png::new(original.image);
        let native = encode(&png, &EncodeOptions::default()).unwrap();
        let reference = reference_encode(&png);
        let mut streamed = Vec::new();
        let stats = encode_writer(&png, &EncodeOptions::default(), &mut streamed).unwrap();
        for encoded in [&native, &reference, &streamed] {
            assert_eq!(decode(encoded).unwrap().image, png.image);
            assert_eq!(reference_decode(encoded), expected);
        }
        let sample_bytes = expected.2.len();
        for (codec, operation, size, ns) in [
            (
                "deadlib-image-png",
                "decode",
                bytes.len(),
                time(|| decode(black_box(&bytes)).unwrap(), iterations),
            ),
            (
                "png-0.18.1",
                "decode",
                bytes.len(),
                time(|| reference_decode(black_box(&bytes)), iterations),
            ),
            (
                "deadlib-image-png",
                "decode-reader",
                bytes.len(),
                time(
                    || {
                        deadlib_image_png::decode_reader(
                            black_box(bytes.as_slice()),
                            &Default::default(),
                        )
                        .unwrap()
                    },
                    iterations,
                ),
            ),
            (
                "deadlib-image-png",
                "encode",
                native.len(),
                time(
                    || encode(black_box(&png), &EncodeOptions::default()).unwrap(),
                    iterations,
                ),
            ),
            (
                "png-0.18.1",
                "encode",
                reference.len(),
                time(|| reference_encode(black_box(&png)), iterations),
            ),
            (
                "deadlib-image-png",
                "encode-writer",
                streamed.len(),
                time(
                    || {
                        let mut out = Vec::new();
                        encode_writer(black_box(&png), &EncodeOptions::default(), &mut out)
                            .unwrap();
                        out
                    },
                    iterations,
                ),
            ),
            (
                "deadlib-image-png",
                "encode-discard",
                streamed.len(),
                time(
                    || {
                        encode_writer(black_box(&png), &EncodeOptions::default(), std::io::sink())
                            .unwrap()
                    },
                    iterations,
                ),
            ),
        ] {
            let memory = if matches!(operation, "encode-writer" | "encode-discard") {
                stats.peak_image_buffer_bytes
            } else {
                0
            };
            println!(
                "{name}\t{codec}\t{operation}\t{iterations}\t{size}\t{sample_bytes}\t{ns}\t{:.3}\t{memory}",
                sample_bytes as f64 * 1e9 / ns as f64 / (1024. * 1024.)
            );
        }
    }
}
