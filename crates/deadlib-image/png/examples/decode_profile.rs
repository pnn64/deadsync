//! Sampling entry point; time comparisons belong to the independent benchmarks.
use deadlib_image_png::{decode, decode_reader, zlib};
use std::{hint::black_box, io::Read, path::PathBuf};

#[inline(never)]
fn sample(inputs: &[Vec<u8>], iterations: usize, operation: &str) {
    for _ in 0..iterations {
        for bytes in inputs {
            match operation {
                "decode" => {
                    black_box(decode(black_box(bytes)).unwrap());
                }
                "reader" => {
                    black_box(
                        decode_reader(black_box(bytes.as_slice()), &Default::default()).unwrap(),
                    );
                }
                "inflate" => {
                    black_box(zlib::decompress(black_box(bytes), 32 << 20).unwrap());
                }
                _ => unreachable!(),
            }
        }
    }
}

fn main() {
    let corpus =
        PathBuf::from(std::env::var_os("DEADLIB_PNG_CORPUS").expect("set DEADLIB_PNG_CORPUS"));
    let operation = std::env::var("DEADLIB_PNG_PROFILE").unwrap_or_else(|_| "decode".into());
    assert!(matches!(
        operation.as_str(),
        "decode" | "reader" | "inflate"
    ));
    let iterations = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .unwrap_or_else(|_| "40".into())
        .parse::<usize>()
        .unwrap()
        .max(1);
    let mut paths: Vec<_> = std::fs::read_dir(corpus)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    let mut inputs = Vec::new();
    for path in paths {
        let bytes = std::fs::read(path).unwrap();
        decode(&bytes).unwrap();
        if operation == "inflate" {
            let mut idat = Vec::new();
            let mut offset = 8;
            while offset < bytes.len() {
                let n = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
                if &bytes[offset + 4..offset + 8] == b"IDAT" {
                    idat.extend_from_slice(&bytes[offset + 8..offset + 8 + n]);
                }
                offset += n + 12;
            }
            let mut expected = Vec::new();
            flate2::read::ZlibDecoder::new(idat.as_slice())
                .read_to_end(&mut expected)
                .unwrap();
            assert_eq!(zlib::decompress(&idat, 32 << 20).unwrap(), expected);
            inputs.push(idat);
        } else {
            inputs.push(bytes);
        }
    }
    eprintln!(
        "Profiling {operation}: {} files, {iterations} corpus passes",
        inputs.len()
    );
    sample(&inputs, iterations, &operation);
}
