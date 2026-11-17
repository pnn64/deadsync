use deadlib_image_jpeg::{Limits, Upsampling, decode_coefficients};
use std::{hint::black_box, time::Instant};

#[cfg(windows)]
mod cycles {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn SetProcessAffinityMask(process: *mut c_void, mask: usize) -> i32;
        fn QueryProcessCycleTime(process: *mut c_void, cycles: *mut u64) -> i32;
    }
    pub fn pin() {
        assert_ne!(unsafe { SetProcessAffinityMask(GetCurrentProcess(), 1) }, 0);
    }
    pub fn read() -> u64 {
        let mut cycles = 0;
        assert_ne!(
            unsafe { QueryProcessCycleTime(GetCurrentProcess(), &mut cycles) },
            0
        );
        cycles
    }
}
#[cfg(not(windows))]
mod cycles {
    pub fn pin() {}
    pub fn read() -> u64 {
        0
    }
}

fn time(name: &str, mut decode: impl FnMut()) {
    let start_cycles = cycles::read();
    let start = Instant::now();
    let mut iterations = 0u64;
    while start.elapsed().as_millis() < 300 {
        decode();
        iterations += 1;
    }
    println!(
        "{name}\t{} ns\t{} process cycles",
        start.elapsed().as_nanos() as u64 / iterations,
        (cycles::read() - start_cycles) / iterations
    );
}

fn main() {
    cycles::pin();
    let bytes = std::fs::read(std::env::args_os().nth(1).expect("JPEG path")).unwrap();
    // `stages FILE N` only repeats the pixel API, for sampling profilers.
    if let Some(n) = std::env::args().nth(2) {
        for _ in 0..n.parse::<u32>().unwrap() {
            black_box(deadlib_image_jpeg::decode(black_box(&bytes), Limits::default()).unwrap());
        }
        return;
    }
    let coefficients = decode_coefficients(&bytes, Limits::default()).unwrap();
    let planes = coefficients.reconstruct().unwrap();
    println!(
        "{}x{} {:?} {:?}",
        planes.width,
        planes.height,
        planes.color_space,
        planes
            .components
            .iter()
            .map(|c| (c.spec.horizontal_sampling, c.spec.vertical_sampling))
            .collect::<Vec<_>>()
    );
    time("entropy", || {
        black_box(decode_coefficients(black_box(&bytes), Limits::default()).unwrap());
    });
    time("idct", || {
        black_box(coefficients.reconstruct().unwrap());
    });
    time("color", || {
        black_box(planes.to_image(Upsampling::Linear).unwrap());
    });
    time("raster", || {
        black_box(deadlib_image_jpeg::decode(black_box(&bytes), Limits::default()).unwrap());
    });
    time("zune", || {
        let options = zune_core::options::DecoderOptions::default()
            .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::RGB);
        black_box(
            zune_jpeg::JpegDecoder::new_with_options(
                zune_core::bytestream::ZCursor::new(black_box(&bytes)),
                options,
            )
            .decode()
            .unwrap(),
        );
    });
    time("complete", || {
        black_box(
            decode_coefficients(black_box(&bytes), Limits::default())
                .unwrap()
                .reconstruct()
                .unwrap()
                .to_image(Upsampling::Linear)
                .unwrap(),
        );
    });
}
