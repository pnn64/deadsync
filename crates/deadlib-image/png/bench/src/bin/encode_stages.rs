//! Diagnostic encoder stages; input preparation is outside all measurements.
#![allow(dead_code, unused_imports)]
pub use deadlib_image_png::{ColorType, Error, Image, Samples};
use deadlib_image_png::{EncodeOptions as PngOptions, Filter, Png};
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
mod codec {
    pub use deadlib_image_png::Filter;
}
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), Error> {
    v.try_reserve(n).map_err(|_| Error::Allocation)
}
#[path = "../../../src/checksum.rs"]
mod checksum;
#[path = "../../../src/stream/encode_rows.rs"]
mod encode_rows;
#[path = "../../../src/scanline.rs"]
mod scanline;
#[path = "../../../src/zlib.rs"]
mod zlib;
/// The source-level options that `encode_rows` reads.
pub struct EncodeOptions {
    pub interlaced: bool,
    pub filter: Filter,
    pub compression: zlib::Compression,
}

/// Every filtered row of a plain image, collected into one buffer.
fn adaptive_filter(image: &Image) -> Vec<u8> {
    let size = scanline::filtered_size(
        image.width,
        image.height,
        image.color_type,
        image.bit_depth,
        false,
    )
    .unwrap();
    let mut filtered = Vec::with_capacity(size);
    let mut rows = encode_rows::Rows::new(image, false, Filter::Adaptive).unwrap();
    while let Some(row) = rows.next_row().unwrap() {
        filtered.extend_from_slice(row.bytes);
    }
    filtered
}

#[cfg(windows)]
mod cycles {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }
    pub fn pin() {
        // The pseudo-handle refers to this thread, and CPU 0 is in its group.
        assert_ne!(unsafe { SetThreadAffinityMask(GetCurrentThread(), 1) }, 0);
    }
    pub fn read() -> u64 {
        let mut value = 0;
        // The output pointer remains valid throughout the synchronous call.
        assert_ne!(
            unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
            0
        );
        value
    }
}
#[cfg(not(windows))]
mod cycles {
    pub fn pin() {}
    pub fn read() -> u64 {
        0
    }
}

fn measure(name: &str, mut run: impl FnMut()) {
    run();
    let mut ns = [0; 5];
    let mut cpu = [0; 5];
    for batch in 0..5 {
        let start_cycles = cycles::read();
        let start = Instant::now();
        let mut iterations = 0u64;
        loop {
            run();
            iterations += 1;
            if start.elapsed() >= Duration::from_millis(40) {
                break;
            }
        }
        ns[batch] = start.elapsed().as_nanos() as u64 / iterations;
        cpu[batch] = (cycles::read() - start_cycles) / iterations;
    }
    ns.sort_unstable();
    cpu.sort_unstable();
    println!("{name}\t{}\t{}", ns[2], cpu[2]);
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().expect("usage: encode_stages PNG_PATH");
    assert!(args.next().is_none(), "expected exactly one PNG path");
    let bytes = std::fs::read(path).expect("could not read input PNG");
    let rgba = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .expect("could not decode input PNG")
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut pixels = rgba.into_raw();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    let image = Image {
        width,
        height,
        color_type: ColorType::Rgba,
        bit_depth: 8,
        samples: Samples::U8(pixels),
    };
    image.validate().expect("invalid prepared RGBA8 image");
    let png = Png::new(image);
    let options = PngOptions::default();
    let filtered = adaptive_filter(&png.image);
    let compressed = zlib::compress(&filtered, zlib::Compression::Fast).unwrap();
    eprintln!(
        "Opaque RGBA8 {width}x{height}; filtered={} zlib={} bytes; five median batches of at least 40 ms.",
        filtered.len(),
        compressed.len()
    );
    eprintln!(
        "Diagnostic stages overlap; fresh outputs include destruction. Cycles are zero outside Windows."
    );
    cycles::pin();
    println!("stage\tns\tthread_cycles");
    measure("native-buffered", || {
        drop(black_box(
            deadlib_image_png::encode(black_box(&png), &options).unwrap(),
        ));
    });
    measure("native-writer", || {
        let mut output = Vec::new();
        deadlib_image_png::encode_writer(black_box(&png), &options, &mut output).unwrap();
        drop(black_box(output));
    });
    measure("adaptive-filter", || {
        drop(black_box(adaptive_filter(black_box(&png.image))));
    });
    measure("deflate-fast", || {
        drop(black_box(
            zlib::compress(black_box(&filtered), zlib::Compression::Fast).unwrap(),
        ));
    });
    measure("deflate-rle", || {
        drop(black_box(
            zlib::compress(black_box(&filtered), zlib::Compression::Rle).unwrap(),
        ));
    });
    let rle_options = PngOptions {
        compression: deadlib_image_png::zlib::Compression::Rle,
        ..options
    };
    measure("native-rle-buffered", || {
        drop(black_box(
            deadlib_image_png::encode(black_box(&png), &rle_options).unwrap(),
        ));
    });
    measure("native-rle-writer", || {
        let mut output = Vec::new();
        deadlib_image_png::encode_writer(black_box(&png), &rle_options, &mut output).unwrap();
        drop(black_box(output));
    });
    measure("adler32-filtered", || {
        black_box(checksum::adler32(black_box(&filtered)));
    });
    measure("crc32-compressed", || {
        black_box(checksum::crc32(&[black_box(&compressed)]));
    });
}
