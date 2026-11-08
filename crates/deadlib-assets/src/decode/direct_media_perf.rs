//! Production resize measurements, with unchanged GIF decoding as a control.
use super::*;
use preparation_perf::bench;
use std::hint::black_box;

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_direct_media() {
    for (width, height, size) in [
        (16, 16, [32, 32]),
        (256, 256, [128, 128]),
        (256, 256, [512, 512]),
        (1024, 512, [64, 32]),
        (1, 256, [2, 512]),
        (257, 129, [259, 127]),
    ] {
        let image = RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba((x.wrapping_mul(1664525) ^ y.wrapping_mul(1013904223)).to_le_bytes())
        });
        let label = format!("resize/{width}x{height}/{}x{}", size[0], size[1]);
        let (_, counts) = crate::perf::measure(|| black_box(zoom_image(image.clone(), size)));
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 32, || {
            let resize: fn(RgbaImage, [u32; 2]) -> RgbaImage = black_box(zoom_image);
            black_box(resize(black_box(image.clone()), black_box(size)));
        });
    }
    let directory = crate::perf_fixture::TempDir::new("direct-gif");
    let path = directory.path.join("fixture.gif");
    for size in [1, 16, 256, 1024] {
        for offset in [false, true] {
            super::texture_pixels_perf::write_gif(&path, size, size, offset);
            let label = format!("gif/{size}/offset={offset}");
            let (_, counts) = crate::perf::measure(|| black_box(decode_gif(&path, false).unwrap()));
            println!("{label}: allocation {counts:?}");
            let iterations = if size >= 256 { 32 } else { 256 };
            bench::measure(&label, iterations, || {
                let decode: fn(&Path, bool) -> image::ImageResult<RgbaImage> =
                    black_box(decode_gif);
                black_box(decode(black_box(&path), black_box(false)).unwrap());
            });
        }
    }
}
