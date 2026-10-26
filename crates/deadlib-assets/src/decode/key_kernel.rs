use super::*;
use std::hint::black_box;

#[path = "../../../../tests/support/kernel_bench.rs"]
mod bench;

#[test]
fn key_preserves_edges_alpha_and_trailing_storage() {
    for width in [0, 1, 3, 17, 64] {
        for height in [0, 1, 2, 7] {
            for off_pink in [false, true] {
                let palette = [
                    [255, 0, 255, 255],
                    [248, 0, 248, 255],
                    [255, 0, 255, 254],
                    [248, 0, 248, 0],
                    [12, 34, 56, 78],
                ];
                let mut image = RgbaImage::from_fn(width, height, |x, y| {
                    image::Rgba(palette[((x + y * width) % 5) as usize])
                });
                if off_pink && width > 0 && height > 0 {
                    image.put_pixel(width - 1, height - 1, image::Rgba(palette[1]));
                }
                let uses_off_pink = image
                    .enumerate_pixels()
                    .any(|(_, y, p)| (y == 0 || y + 1 == height) && p.0 == palette[1]);
                let key = palette[usize::from(uses_off_pink)];
                let expected: Vec<_> = image
                    .pixels()
                    .flat_map(|p| if p.0 == key { [0; 4] } else { p.0 })
                    .chain([99; 12])
                    .collect();
                let mut bytes = image.into_raw();
                bytes.extend([99; 12]);
                let mut image = RgbaImage::from_raw(width, height, bytes).unwrap();
                key_rgba(&mut image);
                assert_eq!(image.into_raw(), expected);
            }
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_key_kernel() {
    for size in [32, 256, 1024] {
        for pattern in ["none", "sparse", "dense"] {
            let input = RgbaImage::from_fn(size, size, |x, y| {
                let index = x + y * size;
                image::Rgba(
                    if (pattern == "sparse" && index % 97 == 0)
                        || (pattern == "dense" && index % 3 != 0)
                    {
                        [255, 0, 255, 255]
                    } else {
                        [x as u8, y as u8, 71, 255]
                    },
                )
            });
            let mut output = input.clone();
            let label = format!("key/{size}/{pattern}");
            let mut work = || {
                output.as_mut().copy_from_slice(input.as_raw());
                let key: fn(&mut RgbaImage) = black_box(key_rgba);
                key(black_box(&mut output));
                black_box(&output);
            };
            let (_, counts) = crate::perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(
                &label,
                (2_000_000 / (size * size) as usize).max(8),
                &mut work,
            );
        }
    }
}
