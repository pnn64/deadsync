//! Native and independent inflation on identical external PNG IDAT streams.
use deadlib_image_png::zlib::{self, Inflater, StreamStatus};
use std::{hint::black_box, io::Read, path::PathBuf, time::Instant};

fn streamed(input: &[u8], limit: usize) -> Vec<u8> {
    let mut inflater = Inflater::new(limit).unwrap();
    let mut offset = 0;
    let mut raw = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let p = inflater.process(&input[offset..], &mut buffer).unwrap();
        offset += p.consumed;
        raw.extend_from_slice(&buffer[..p.written]);
        if p.status == StreamStatus::Done {
            inflater.finish().unwrap();
            assert_eq!(offset, input.len());
            return raw;
        }
        assert!(p.consumed != 0 || p.written != 0);
    }
}
fn reference(input: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(input)
        .read_to_end(&mut raw)
        .unwrap();
    raw
}
fn time(mut run: impl FnMut() -> Vec<u8>, iterations: usize) -> u128 {
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
    let root =
        PathBuf::from(std::env::var_os("DEADLIB_PNG_CORPUS").expect("set DEADLIB_PNG_CORPUS"));
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
        let png = std::fs::read(&path).unwrap();
        deadlib_image_png::decode(&png).unwrap();
        let mut idat = Vec::new();
        let mut offset = 8;
        while offset < png.len() {
            let size = u32::from_be_bytes(png[offset..offset + 4].try_into().unwrap()) as usize;
            if &png[offset + 4..offset + 8] == b"IDAT" {
                idat.extend_from_slice(&png[offset + 8..offset + 8 + size]);
            }
            offset += 12 + size;
        }
        let expected = reference(&idat);
        assert_eq!(zlib::decompress(&idat, expected.len()).unwrap(), expected);
        assert_eq!(streamed(&idat, expected.len()), expected);
        for (codec, operation, ns) in [
            (
                "deadlib-image-png",
                "inflate",
                time(
                    || zlib::decompress(black_box(&idat), expected.len()).unwrap(),
                    iterations,
                ),
            ),
            (
                "deadlib-image-png",
                "inflate-stream",
                time(|| streamed(black_box(&idat), expected.len()), iterations),
            ),
            (
                "flate2-1.1.10",
                "inflate",
                time(|| reference(black_box(&idat)), iterations),
            ),
        ] {
            let rate = expected.len() as f64 * 1e9 / ns as f64 / (1024.0 * 1024.0);
            println!(
                "{name}\t{codec}\t{operation}\t{iterations}\t{}\t{}\t{ns}\t{rate:.2}\t0",
                idat.len(),
                expected.len()
            );
        }
    }
}
