use super::*;
use preparation_perf::bench;
use std::hint::black_box;

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_resize_kernel() {
    for (width, height, size) in [
        (16, 16, [32, 32]),
        (256, 256, [512, 512]),
        (256, 256, [128, 128]),
        (257, 129, [259, 127]),
        (640, 360, [256, 128]),
        (1024, 512, [259, 127]),
    ] {
        let image = RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba((x.wrapping_mul(1664525) ^ y.wrapping_mul(1013904223)).to_le_bytes())
        });
        let label = format!("resize/{width}x{height}/{}x{}", size[0], size[1]);
        let work = || {
            let resize: fn(RgbaImage, [u32; 2]) -> RgbaImage = black_box(zoom_image);
            black_box(resize(black_box(image.clone()), black_box(size)));
        };
        let (_, counts) = crate::perf::measure(work);
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 32, work);
    }
}
