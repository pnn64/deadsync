use super::*;
use std::hint::black_box;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

// Frozen from starting main a60358c4d, including its redundant alpha pass.
fn original_copy_target_pixels<const PRESERVE_ALPHA: bool>(target: &mut OffscreenTarget) {
    if target.with_depth {
        for (source, dest) in target
            .depth
            .chunks_exact(target.viewport[0] as usize)
            .zip(target.depth_image.chunks_exact_mut(target.width as usize))
        {
            dest[..source.len()].copy_from_slice(source);
        }
    }

    if target.float_color {
        for (source, dest) in target
            .half_pixels
            .chunks_exact(target.viewport[0] as usize)
            .zip(
                target
                    .texture
                    .half_pixels
                    .chunks_exact_mut(target.width as usize),
            )
        {
            dest[..source.len()].copy_from_slice(source);
        }
        for (rgba, &pixel) in target
            .texture
            .image
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&target.texture.half_pixels)
        {
            *rgba = unpack_half(pixel).map(|v| (clamp01(v) * 255.0).round() as u8);
        }
        return;
    }

    if !PRESERVE_ALPHA {
        for rgba in target.texture.image.as_mut().as_chunks_mut::<4>().0 {
            rgba[3] = 255;
        }
    }
    for (row, pixels) in target
        .texture
        .image
        .as_mut()
        .chunks_exact_mut(target.width as usize * 4)
        .zip(target.pixels.chunks_exact_mut(target.viewport[0] as usize))
    {
        for (rgba, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            if !PRESERVE_ALPHA {
                *pixel |= 0xff00_0000;
            }
            let pixel = *pixel;
            rgba[0] = (pixel >> 16) as u8;
            rgba[1] = (pixel >> 8) as u8;
            rgba[2] = pixel as u8;
            rgba[3] = (pixel >> 24) as u8;
        }
    }
}

fn target(
    width: u32,
    height: u32,
    viewport: [u32; 2],
    float: bool,
    depth: bool,
) -> OffscreenTarget {
    let mut target = create_offscreen_target(7, width, height, viewport, float, depth);
    for (i, pixel) in target.pixels.iter_mut().enumerate() {
        *pixel = u32::from_be_bytes([(i % 256) as u8, 17, (i * 31) as u8, 209]);
    }
    for (i, pixel) in target.half_pixels.iter_mut().enumerate() {
        *pixel = pack_half([i as f32 * 0.03, -0.5, 0.7, (i % 256) as f32 / 255.0]);
    }
    target.texture.image.as_mut().fill(73);
    target
        .texture
        .half_pixels
        .fill(pack_half([-0.5, 0.2, 1.25, 0.4]));
    target.depth.fill(0.3);
    target.depth_image.fill(0.7);
    target
}

#[test]
fn offscreen_copy_matches_original_including_padding_and_mode_changes() {
    for (width, height) in [(1, 1), (17, 13)] {
        for viewport in [[width, height], [1, height], [width, 1], [1, 1]] {
            for float in [false, true] {
                for depth in [false, true] {
                    let mut a = target(width, height, viewport, float, depth);
                    let mut b = target(width, height, viewport, float, depth);
                    for alpha in [false, true, false, false, true] {
                        if alpha {
                            original_copy_target_pixels::<true>(&mut a);
                            copy_target_pixels::<true>(&mut b);
                        } else {
                            original_copy_target_pixels::<false>(&mut a);
                            copy_target_pixels::<false>(&mut b);
                        }
                        assert_eq!(a.texture.image, b.texture.image);
                        assert_eq!(a.pixels, b.pixels);
                        assert_eq!(a.half_pixels, b.half_pixels);
                        assert_eq!(a.texture.half_pixels, b.texture.half_pixels);
                        assert_eq!(a.depth, b.depth);
                        assert_eq!(a.depth_image, b.depth_image);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_offscreen_alpha_copy() {
    for size in [[64, 64], [512, 512], [1920, 1080]] {
        for (name, viewport, preserve) in [
            ("full opaque", size, false),
            ("full alpha control", size, true),
            ("padded opaque control", [size[0] / 2, size[1] / 2], false),
        ] {
            let mut a = target(size[0], size[1], viewport, false, false);
            let mut b = target(size[0], size[1], viewport, false, false);
            paired::compare(
                &format!("offscreen {}x{} {name}", size[0], size[1]),
                200,
                |current| {
                    if current {
                        if preserve {
                            copy_target_pixels::<true>(black_box(&mut b));
                        } else {
                            copy_target_pixels::<false>(black_box(&mut b));
                        }
                        black_box(&b.texture.image);
                    } else {
                        if preserve {
                            original_copy_target_pixels::<true>(black_box(&mut a));
                        } else {
                            original_copy_target_pixels::<false>(black_box(&mut a));
                        }
                        black_box(&a.texture.image);
                    }
                },
            );
            assert_eq!(a.texture.image, b.texture.image);
            assert_eq!(a.pixels, b.pixels);
        }
    }
}
