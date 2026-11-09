//! Complete retained-texture upload measurements without a window or GPU.
//! Archive both release binaries, then alternate them with matching rotations.
use deadlib_render_backend_software::{
    Texture, create_texture, create_yuv420_texture, update_texture, update_yuv420_texture,
};
use deadlib_render_core::{SamplerDesc, Yuv420Upload};
use image::RgbaImage;
use std::{hint::black_box, time::Instant};
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod allocations;
#[allow(dead_code)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

#[inline(never)]
fn run_yuv(texture: &mut Texture, upload: Yuv420Upload<'_>, iterations: usize) -> (f64, f64) {
    let c = bench::cycles();
    let t = Instant::now();
    for _ in 0..iterations {
        update_yuv420_texture(black_box(texture), black_box(upload)).unwrap();
        black_box(&texture.image);
    }
    let ns = t.elapsed().as_nanos() as f64 / iterations as f64;
    ((bench::cycles() - c) as f64 / iterations as f64, ns)
}

#[inline(never)]
fn run_rgba(texture: &mut Texture, image: &RgbaImage, iterations: usize) -> (f64, f64) {
    let c = bench::cycles();
    let t = Instant::now();
    for _ in 0..iterations {
        update_texture(black_box(texture), black_box(image)).unwrap();
        black_box(&texture.image);
    }
    let ns = t.elapsed().as_nanos() as f64 / iterations as f64;
    ((bench::cycles() - c) as f64 / iterations as f64, ns)
}

fn main() {
    bench::pin_thread();
    let rotation: usize = std::env::args()
        .nth(1)
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let cases = [
        ("tiny", 2, 2, 0, 0),
        ("below-lut", 30, 34, 0, 1),
        ("at-lut", 32, 32, 1, 1),
        ("thumbnail", 128, 72, 1, 1),
        ("480p", 640, 480, 0, 1),
        ("720p", 1280, 720, 1, 1),
        ("1080p", 1920, 1080, 1, 1),
        ("1080p-full", 1920, 1080, 0, 0),
        ("rgba-tiny-control", 2, 2, 0, 0),
        ("rgba-1080p-control", 1920, 1080, 0, 0),
    ];
    let coefficients = [
        [1.402, -0.344136, -0.714136, 1.772],
        [1.5748, -0.187324, -0.468124, 1.8556],
    ];
    let levels = [
        [1.0, 0.0, 1.0, -128.0 / 255.0],
        [255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0, -128.0 / 224.0],
    ];
    println!("case,pixels,round,iterations,cycles,ns");
    for i in 0..cases.len() {
        let (name, width, height, matrix, range) = cases[(i + rotation) % cases.len()];
        let pixels = width as usize * height as usize;
        let y: Vec<_> = (0..pixels).map(|i| (i * 73 + i / 61) as u8).collect();
        let u: Vec<_> = (0..pixels / 4).map(|i| (i * 31 + i / 17) as u8).collect();
        let v: Vec<_> = (0..pixels / 4).map(|i| (i * 97 + i / 23) as u8).collect();
        let upload = Yuv420Upload {
            width,
            height,
            y: &y,
            u: &u,
            v: &v,
            levels: levels[range],
            coeffs: coefficients[matrix],
        };
        let rgba_control = name.starts_with("rgba");
        let image = RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([x as u8, y as u8, (x * 31 + y * 17) as u8, 255])
        });
        let mut texture = if rgba_control {
            create_texture(&image, SamplerDesc::default()).unwrap()
        } else {
            create_yuv420_texture(upload, SamplerDesc::default()).unwrap()
        };
        allocations::assert_no_churn(|| {
            if rgba_control {
                update_texture(&mut texture, &image).unwrap();
            } else {
                update_yuv420_texture(&mut texture, upload).unwrap();
            }
        });
        let iterations = (2_000_000 / pixels).clamp(2, 30000);
        for round in 0..10 {
            let (cycles, ns) = if rgba_control {
                run_rgba(&mut texture, &image, iterations)
            } else {
                run_yuv(&mut texture, upload, iterations)
            };
            println!("{name},{pixels},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
