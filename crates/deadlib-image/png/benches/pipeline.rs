use deadlib_image_png::{ColorType, EncodeOptions, Image, Png, Samples, decode, encode};
use std::{
    hint::black_box,
    io::{Cursor, Read, Write},
    time::Instant,
};

fn fixture(width: u32, height: u32, depth: u8, noise: bool) -> Png {
    let mut random = 0x7a19_28c3u32;
    let mut next = |n: usize| {
        if noise {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random as u16
        } else {
            ((n % 4) * 8192 + (n / 4 / width as usize) * 137 + (n / 4 % width as usize) * 71) as u16
        }
    };
    let count = width as usize * height as usize * 4;
    let samples = if depth == 16 {
        Samples::U16((0..count).map(&mut next).collect())
    } else {
        Samples::U8((0..count).map(|n| next(n) as u8).collect())
    };
    Png::new(Image {
        width,
        height,
        color_type: ColorType::Rgba,
        bit_depth: depth,
        samples,
    })
}
fn wire(image: &Image) -> Vec<u8> {
    match &image.samples {
        Samples::U8(v) => v.clone(),
        Samples::U16(v) => v.iter().flat_map(|v| v.to_be_bytes()).collect(),
    }
}
fn reference_encode(image: &Image, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::from_u8(image.bit_depth).unwrap());
    encoder
        .write_header()
        .unwrap()
        .write_image_data(data)
        .unwrap();
    out
}
fn reference_decode(bytes: &[u8]) -> Samples {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut data).unwrap();
    reader.finish().unwrap();
    data.truncate(info.buffer_size());
    if info.bit_depth == png::BitDepth::Sixteen {
        Samples::U16(
            data.as_chunks::<2>()
                .0
                .iter()
                .map(|&v| u16::from_be_bytes(v))
                .collect(),
        )
    } else {
        Samples::U8(data)
    }
}
fn time<T>(mut f: impl FnMut() -> T, iterations: usize) -> u128 {
    for _ in 0..3 {
        black_box(f());
    }
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(f());
    }
    start.elapsed().as_nanos() / iterations as u128
}
fn row_decode(bytes: &[u8]) -> (usize, usize) {
    use deadlib_image_png::{DecodeEvent, DecodeOptions, DecodeStatus, StreamDecoder};
    let mut decoder = StreamDecoder::new(DecodeOptions::default());
    let mut pos = 0;
    let mut rows = 0;
    loop {
        let progress = decoder.process(&bytes[pos..]).unwrap();
        pos += progress.consumed;
        if let Some(DecodeEvent::Row(row)) = decoder.event() {
            black_box(row.samples);
            rows += 1;
        }
        if progress.status == DecodeStatus::Done {
            assert_eq!(pos, bytes.len());
            return (rows, decoder.peak_image_buffer_bytes());
        }
        if progress.status == DecodeStatus::NeedInput && pos == bytes.len() {
            decoder.finish().unwrap();
        }
    }
}
fn writer_encode(png: &Png) -> (Vec<u8>, deadlib_image_png::EncodeStats) {
    let mut output = Vec::new();
    let stats =
        deadlib_image_png::encode_writer(png, &EncodeOptions::default(), &mut output).unwrap();
    (output, stats)
}
fn filtered(bytes: &[u8]) -> Vec<u8> {
    let mut idat = Vec::new();
    let mut pos = 8;
    while pos < bytes.len() {
        let n = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        if &bytes[pos + 4..pos + 8] == b"IDAT" {
            idat.extend_from_slice(&bytes[pos + 8..pos + 8 + n]);
        }
        pos += n + 12;
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(idat.as_slice())
        .read_to_end(&mut out)
        .unwrap();
    out
}
fn reference_deflate(bytes: &[u8], level: u32) -> Vec<u8> {
    let mut compressor =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
    compressor.write_all(bytes).unwrap();
    compressor.finish().unwrap()
}
fn main() {
    let iterations = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .ok()
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(20)
        .max(1);
    println!(
        "case\tcodec\toperation\titerations\tencoded_bytes\tsample_bytes\tns_per_image\tMiB_per_second\timage_pipeline_bytes"
    );
    for (name, png) in [
        ("gradient-rgba8", fixture(512, 512, 8, false)),
        ("noise-rgba8", fixture(512, 512, 8, true)),
        ("gradient-rgba16", fixture(512, 512, 16, false)),
        ("noise-rgba16", fixture(512, 512, 16, true)),
    ] {
        let raw = wire(&png.image);
        let bytes = reference_encode(&png.image, &raw);
        let mut idat = Vec::new();
        let mut pos = 8;
        while pos < bytes.len() {
            let n = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            if &bytes[pos + 4..pos + 8] == b"IDAT" {
                idat.extend_from_slice(&bytes[pos + 8..pos + 8 + n]);
            }
            pos += n + 12;
        }
        let filtered_bytes = raw.len() + png.image.height as usize;
        assert_eq!(decode(&bytes).unwrap().image, png.image);
        assert_eq!(
            deadlib_image_png::decode_reader(bytes.as_slice(), &Default::default())
                .unwrap()
                .image,
            png.image
        );
        let (row_count, row_memory) = row_decode(&bytes);
        assert_eq!(row_count, png.image.height as usize);
        assert_eq!(reference_decode(&bytes), png.image.samples);
        let native = encode(&png, &EncodeOptions::default()).unwrap();
        assert_eq!(reference_decode(&native), png.image.samples);
        let (streamed, encode_stats) = writer_encode(&png);
        assert_eq!(reference_decode(&streamed), png.image.samples);
        if std::env::var("DEADLIB_PNG_BENCH_STAGE").as_deref() == Ok("deflate") {
            for (source, filtered) in [
                ("native-filtered", filtered(&native)),
                ("reference-filtered", filtered(&bytes)),
            ] {
                for (codec, level) in [
                    ("deadlib-image-png", None),
                    ("flate2-level1", Some(1)),
                    ("flate2-level6", Some(6)),
                ] {
                    let compress = || match level {
                        Some(level) => reference_deflate(black_box(&filtered), level),
                        None => deadlib_image_png::zlib::compress(
                            black_box(&filtered),
                            Default::default(),
                        )
                        .unwrap(),
                    };
                    let compressed = compress();
                    let mut checked = Vec::new();
                    flate2::read::ZlibDecoder::new(compressed.as_slice())
                        .read_to_end(&mut checked)
                        .unwrap();
                    assert_eq!(checked, filtered);
                    let ns = time(compress, iterations);
                    println!(
                        "{name}-{source}\t{codec}\tdeflate\t{iterations}\t{}\t{}\t{ns}\t{:.3}\t0",
                        compressed.len(),
                        filtered.len(),
                        filtered.len() as f64 * 1e9 / ns as f64 / (1024. * 1024.)
                    );
                }
            }
            continue;
        }
        for (codec, operation, size, ns) in [
            (
                "deadlib-image-png",
                "inflate",
                idat.len(),
                time(
                    || {
                        deadlib_image_png::zlib::decompress(black_box(&idat), filtered_bytes)
                            .unwrap()
                    },
                    iterations,
                ),
            ),
            (
                "flate2-1.1.10",
                "inflate",
                idat.len(),
                time(
                    || {
                        let mut out = Vec::new();
                        flate2::read::ZlibDecoder::new(black_box(idat.as_slice()))
                            .read_to_end(&mut out)
                            .unwrap();
                        out
                    },
                    iterations,
                ),
            ),
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
                "decode-rows",
                bytes.len(),
                time(|| row_decode(black_box(&bytes)), iterations),
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
                bytes.len(),
                time(
                    || reference_encode(black_box(&png.image), black_box(&raw)),
                    iterations,
                ),
            ),
            (
                "deadlib-image-png",
                "encode-writer",
                streamed.len(),
                time(|| writer_encode(black_box(&png)), iterations),
            ),
            (
                "deadlib-image-png",
                "encode-discard",
                streamed.len(),
                time(
                    || {
                        deadlib_image_png::encode_writer(
                            black_box(&png),
                            &EncodeOptions::default(),
                            std::io::sink(),
                        )
                        .unwrap()
                    },
                    iterations,
                ),
            ),
        ] {
            println!(
                "{name}\t{codec}\t{operation}\t{iterations}\t{size}\t{}\t{ns}\t{:.3}\t{}",
                raw.len(),
                raw.len() as f64 * 1e9 / ns as f64 / (1024. * 1024.),
                if operation == "decode-rows" {
                    row_memory
                } else if matches!(operation, "encode-writer" | "encode-discard") {
                    encode_stats.peak_image_buffer_bytes
                } else {
                    0
                },
            );
        }
    }
}
