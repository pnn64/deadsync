//! Diagnostic stage timings compiled from the same native implementation.
#![allow(dead_code, unused_imports)]
pub use deadlib_image_png::{ColorType, Error, Image, Samples};
use std::{hint::black_box, time::Instant};
mod codec {
    pub use deadlib_image_png::Filter;
}
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), Error> {
    v.try_reserve(n).map_err(|_| Error::Allocation)
}
#[path = "../../../src/checksum.rs"]
mod checksum;
#[path = "../../../src/scanline.rs"]
mod scanline;
#[path = "../../../src/zlib.rs"]
mod zlib;

fn measure(name: &str, mut run: impl FnMut()) {
    run();
    let mut values = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let mut iterations = 0;
        while start.elapsed().as_millis() < 100 {
            run();
            iterations += 1;
        }
        values.push(start.elapsed().as_nanos() / iterations);
    }
    values.sort_unstable();
    println!("{name}\t{} ns", values[2]);
}

fn main() {
    let bytes = std::fs::read(std::env::args_os().nth(1).expect("PNG path")).unwrap();
    let image = deadlib_image_png::decode(&bytes).unwrap();
    let (width, height, color, depth) = (
        image.image.width,
        image.image.height,
        image.image.color_type,
        image.image.bit_depth,
    );
    let mut compressed = Vec::new();
    let mut pos = 8;
    let mut start = 0;
    let mut end = 0;
    while pos < bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        if &bytes[pos + 4..pos + 8] == b"IDAT" {
            if start == 0 {
                start = pos;
            }
            end = pos + len + 12;
            compressed.extend_from_slice(&bytes[pos + 8..pos + 8 + len]);
        }
        pos += len + 12;
    }
    let raw = zlib::decompress_prefix(&compressed, 512 << 20).unwrap().0;
    println!(
        "compressed={} filtered={} samples={}",
        compressed.len(),
        raw.len(),
        image.image.samples.len()
    );
    if std::env::args().nth(2).as_deref() == Some("--profile") {
        for _ in 0..500 {
            black_box(deadlib_image_png::decode(black_box(&bytes)).unwrap());
        }
        return;
    }
    measure("whole PNG", || {
        black_box(deadlib_image_png::decode(black_box(&bytes)).unwrap());
    });
    measure("CRC", || {
        black_box(checksum::crc32(&[black_box(&compressed)]));
    });
    measure("Adler", || {
        black_box(checksum::adler32(black_box(&raw)));
    });
    measure("inflate contiguous", || {
        black_box(zlib::decompress_prefix_sized(black_box(&compressed), raw.len()).unwrap());
    });
    measure("inflate IDAT", || {
        black_box(
            zlib::decompress_png_chunks(black_box(&bytes[start..end]), compressed.len(), raw.len())
                .unwrap(),
        );
    });
    measure("unfilter and samples", || {
        black_box(
            scanline::decode(
                black_box(raw.clone()),
                width,
                height,
                color,
                depth,
                image.interlaced,
                512 << 20,
            )
            .unwrap(),
        );
    });
}
