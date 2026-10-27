//! Decoder-only pipeline and IDAT comparisons on identical static PNG inputs.
use deadlib_image_png::{ColorType, Image, Samples, decode, decode_reader, zlib};
use std::{
    hint::black_box,
    io::{Cursor, Read},
    path::PathBuf,
    time::Instant,
};

fn reference(input: &[u8]) -> Image {
    let mut reader = png::Decoder::new(Cursor::new(input)).read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes.truncate(info.buffer_size());
    reader.finish().unwrap();
    let depth = info.bit_depth as u8;
    let color_type = ColorType::try_from(info.color_type as u8).unwrap();
    let samples = match depth {
        16 => Samples::U16(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&v| u16::from_be_bytes(v))
                .collect(),
        ),
        8 => Samples::U8(bytes),
        _ => {
            let mut out = Vec::new();
            for row in bytes.chunks_exact(info.line_size) {
                for n in 0..info.width as usize {
                    out.push(
                        (row[n * depth as usize / 8]
                            >> (8 - depth - (n * depth as usize % 8) as u8))
                            & ((1 << depth) - 1),
                    );
                }
            }
            Samples::U8(out)
        }
    };
    Image {
        width: info.width,
        height: info.height,
        color_type,
        bit_depth: depth,
        samples,
    }
}
fn raw_reference(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(input)
        .read_to_end(&mut out)
        .unwrap();
    out
}
fn raw_stream(input: &[u8], limit: usize) -> Vec<u8> {
    let mut inflater = zlib::Inflater::new(limit).unwrap();
    let mut offset = 0;
    let mut out = Vec::new();
    let mut bytes = [0; 8192];
    loop {
        let p = inflater.process(&input[offset..], &mut bytes).unwrap();
        offset += p.consumed;
        out.extend_from_slice(&bytes[..p.written]);
        if p.status == zlib::StreamStatus::Done {
            assert_eq!(offset, input.len());
            inflater.finish().unwrap();
            return out;
        }
        assert_ne!((p.consumed, p.written), (0, 0));
    }
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
    let root = PathBuf::from(std::env::var_os("DEADLIB_PNG_CORPUS").expect("DEADLIB_PNG_CORPUS"));
    let iterations = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .ok()
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(10)
        .max(1);
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    println!(
        "case\tcodec\toperation\titerations\tencoded_bytes\tsample_bytes\tns_per_image\tMiB_per_second\timage_pipeline_bytes"
    );
    for path in files {
        let name = path.file_stem().unwrap().to_str().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let png = decode(&bytes).unwrap();
        assert!(png.animation.is_none(), "benchmark requires static PNG");
        assert_eq!(reference(&bytes), png.image);
        assert_eq!(
            decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
            png
        );
        let sample_bytes = png.image.samples.len() * if png.image.bit_depth == 16 { 2 } else { 1 };
        let mut idat = Vec::new();
        let mut offset = 8;
        while offset < bytes.len() {
            let len = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            if &bytes[offset + 4..offset + 8] == b"IDAT" {
                idat.extend_from_slice(&bytes[offset + 8..offset + 8 + len]);
            }
            offset += 12 + len;
        }
        let raw = raw_reference(&idat);
        assert_eq!(zlib::decompress(&idat, raw.len()).unwrap(), raw);
        assert_eq!(raw_stream(&idat, raw.len()), raw);
        for (codec, operation, encoded, decoded, ns) in [
            (
                "deadlib-image-png",
                "decode",
                bytes.len(),
                sample_bytes,
                time(|| decode(black_box(&bytes)).unwrap(), iterations),
            ),
            (
                "deadlib-image-png",
                "decode-reader",
                bytes.len(),
                sample_bytes,
                time(
                    || decode_reader(black_box(bytes.as_slice()), &Default::default()).unwrap(),
                    iterations,
                ),
            ),
            (
                "png-0.18.1",
                "decode",
                bytes.len(),
                sample_bytes,
                time(|| reference(black_box(&bytes)), iterations),
            ),
            (
                "deadlib-image-png",
                "inflate",
                idat.len(),
                raw.len(),
                time(
                    || zlib::decompress(black_box(&idat), raw.len()).unwrap(),
                    iterations,
                ),
            ),
            (
                "deadlib-image-png",
                "inflate-stream",
                idat.len(),
                raw.len(),
                time(|| raw_stream(black_box(&idat), raw.len()), iterations),
            ),
            (
                "flate2-1.1.10",
                "inflate",
                idat.len(),
                raw.len(),
                time(|| raw_reference(black_box(&idat)), iterations),
            ),
        ] {
            println!(
                "{name}\t{codec}\t{operation}\t{iterations}\t{encoded}\t{decoded}\t{ns}\t{:.3}\t0",
                decoded as f64 * 1e9 / ns as f64 / 1048576.0
            );
        }
    }
}
