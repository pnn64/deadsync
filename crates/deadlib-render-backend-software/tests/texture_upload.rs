//! Pixel parity and heap/CPU comparisons for software texture uploads.
use deadlib_render_backend_software as software;
use deadlib_render_core::{SamplerDesc, Yuv420Upload};
use image::RgbaImage;
use std::error::Error;
use std::hint::black_box;

#[path = "texture_upload/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/texture_upload.rs"]
mod upload;

struct Planes {
    width: u32,
    height: u32,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

impl Planes {
    fn new(width: u32, height: u32) -> Self {
        let len = width as usize * height as usize;
        Self {
            width,
            height,
            y: (0..len)
                .map(|i| i.wrapping_mul(73).wrapping_add(i / 17) as u8)
                .collect(),
            u: (0..len / 4).map(|i| i as u8).collect(),
            v: (0..len / 4).map(|i| (i / 256) as u8).collect(),
        }
    }
    fn upload(&self) -> Yuv420Upload<'_> {
        Yuv420Upload {
            width: self.width,
            height: self.height,
            y: &self.y,
            u: &self.u,
            v: &self.v,
            levels: [255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0, -128.0 / 224.0],
            coeffs: [1.5748, -0.187324, -0.468124, 1.8556],
        }
    }
}

#[test]
fn conversion_matches_all_chroma_pairs_and_varying_luma_bytes() {
    // 65,536 chroma samples cover every possible U/V pair; each has four Y samples.
    let planes = Planes::new(1024, 256);
    for (levels, coeffs) in [
        (planes.upload().levels, planes.upload().coeffs),
        (
            [1.0, 0.0, 1.0, -128.0 / 255.0],
            [1.402, -0.344136, -0.714136, 1.772],
        ),
        (
            [255.0 / 219.0, -16.0 / 219.0, 255.0 / 224.0, -128.0 / 224.0],
            [1.4746, -0.164553, -0.571353, 1.8814],
        ),
    ] {
        let input = Yuv420Upload {
            levels,
            coeffs,
            ..planes.upload()
        };
        let expected = baseline::yuv420_to_rgba(input).unwrap();
        assert_eq!(upload::yuv420_to_rgba(input).unwrap(), expected);
        let actual = software::create_yuv420_texture(input, SamplerDesc::default()).unwrap();
        assert_eq!(actual.image, expected);
        assert!(software::texture_is_yuv420(&actual));
    }
}

#[test]
fn conversion_preserves_arithmetic_for_small_shapes_and_unusual_coefficients() {
    let mut state = 75284319u64;
    for (width, height) in [(2, 2), (2, 18), (22, 2), (6, 10), (34, 18)] {
        let planes = Planes::new(width, height);
        for case in 0..64 {
            let mut values = [0.0f32; 8];
            for (index, value) in values.iter_mut().enumerate() {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                *value = match (case + index) % 23 {
                    0 => f32::NAN,
                    1 => f32::INFINITY,
                    2 => f32::NEG_INFINITY,
                    3 => -0.0,
                    4 => f32::from_bits(1),
                    _ => ((state >> 32) as i32) as f32 / i32::MAX as f32 * 3.0,
                };
            }
            let input = Yuv420Upload {
                levels: values[..4].try_into().unwrap(),
                coeffs: values[4..].try_into().unwrap(),
                ..planes.upload()
            };
            assert_eq!(
                upload::yuv420_to_rgba(input).unwrap(),
                baseline::yuv420_to_rgba(input).unwrap(),
                "{width}x{height}, case {case}"
            );
        }
    }
}

#[test]
fn updates_reuse_storage_and_preserve_public_texture_transitions() {
    let rgba = RgbaImage::from_pixel(34, 18, image::Rgba([3, 5, 7, 100]));
    let mut texture = software::create_texture(&rgba, SamplerDesc::default()).unwrap();
    let pointer = texture.image.as_raw().as_ptr();
    assert!(!software::texture_is_yuv420(&texture));
    let mut planes = Planes::new(34, 18);
    for step in 0..8 {
        planes.y.rotate_left(1 + step);
        planes.u.rotate_left(1);
        let input = planes.upload();
        software::update_yuv420_texture(&mut texture, input).unwrap();
        assert_eq!(texture.image, baseline::yuv420_to_rgba(input).unwrap());
        assert_eq!(texture.image.as_raw().as_ptr(), pointer);
        assert!(software::texture_is_yuv420(&texture));
    }
    for (width, height) in [(2, 2), (128, 64), (2, 8), (8, 2)] {
        let planes = Planes::new(width, height);
        software::update_yuv420_texture(&mut texture, planes.upload()).unwrap();
        assert_eq!(
            texture.image,
            baseline::yuv420_to_rgba(planes.upload()).unwrap()
        );
    }
    // ImageBuffer permits trailing storage. Replacement must discard it as before.
    texture.image = RgbaImage::from_raw(2, 2, vec![77; 29]).unwrap();
    let planes = Planes::new(2, 2);
    software::update_yuv420_texture(&mut texture, planes.upload()).unwrap();
    assert_eq!(
        texture.image,
        baseline::yuv420_to_rgba(planes.upload()).unwrap()
    );
    assert_eq!(texture.image.as_raw().len(), 16);
    software::update_texture(&mut texture, &rgba).unwrap();
    assert_eq!(texture.image, rgba);
    assert!(!software::texture_is_yuv420(&texture));
}

#[test]
fn invalid_uploads_preserve_previous_pixels_storage_and_format() {
    let planes = Planes::new(4, 4);
    let good = planes.upload();
    let rgba = RgbaImage::from_pixel(4, 4, image::Rgba([9, 8, 7, 6]));
    for initially_yuv in [false, true] {
        let mut texture = software::create_texture(&rgba, SamplerDesc::default()).unwrap();
        if initially_yuv {
            software::update_yuv420_texture(&mut texture, good).unwrap();
        }
        let expected = texture.image.clone();
        let pointer = texture.image.as_raw().as_ptr();
        for invalid in [
            Yuv420Upload { width: 0, ..good },
            Yuv420Upload { height: 0, ..good },
            Yuv420Upload { width: 3, ..good },
            Yuv420Upload { height: 3, ..good },
            Yuv420Upload {
                y: &planes.y[..15],
                ..good
            },
            Yuv420Upload {
                u: &planes.u[..3],
                ..good
            },
            Yuv420Upload {
                v: &planes.v[..3],
                ..good
            },
            Yuv420Upload {
                width: u32::MAX,
                height: u32::MAX,
                ..good
            },
        ] {
            let error = baseline::yuv420_to_rgba(invalid).unwrap_err().to_string();
            assert_eq!(
                upload::yuv420_to_rgba(invalid).unwrap_err().to_string(),
                error
            );
            assert_eq!(
                software::update_yuv420_texture(&mut texture, invalid)
                    .unwrap_err()
                    .to_string(),
                error
            );
            assert_eq!(texture.image, expected);
            assert_eq!(texture.image.as_raw().as_ptr(), pointer);
            assert_eq!(software::texture_is_yuv420(&texture), initially_yuv);
        }
    }
}

#[test]
fn opacity_matches_all_alpha_bytes_at_group_and_tail_boundaries() {
    for pixels in [0usize, 1, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 64] {
        for extra in 0..8 {
            let mut image =
                RgbaImage::from_raw(pixels as u32, 1, vec![255; pixels * 4 + extra]).unwrap();
            assert_eq!(
                upload::texture_is_opaque(&image),
                baseline::texture_is_opaque(&image)
            );
            for alpha_index in (3..image.as_raw().len()).step_by(4) {
                for alpha in 0..=255 {
                    image.as_mut()[alpha_index] = alpha;
                    assert_eq!(
                        upload::texture_is_opaque(&image),
                        baseline::texture_is_opaque(&image),
                        "pixels={pixels}, extra={extra}, index={alpha_index}, alpha={alpha}"
                    );
                }
                image.as_mut()[alpha_index] = 255;
            }
        }
    }
    assert!(!upload::texture_is_opaque(&RgbaImage::new(1, 0)));
}

#[test]
fn warmed_upload_and_opacity_checks_have_no_heap_churn() {
    let planes = Planes::new(128, 72);
    let mut texture =
        software::create_yuv420_texture(planes.upload(), SamplerDesc::default()).unwrap();
    perf::assert_no_churn(|| {
        software::update_yuv420_texture(black_box(&mut texture), black_box(planes.upload()))
            .unwrap();
        black_box(texture.image.as_raw());
    });
    perf::assert_no_churn(|| {
        black_box(upload::texture_is_opaque(&texture.image));
    });
    perf::assert_churn_budget(1, 128 * 72 * 4, || {
        black_box(
            software::create_yuv420_texture(planes.upload(), SamplerDesc::default()).unwrap(),
        );
    });
}

fn old_update(image: &mut RgbaImage, input: Yuv420Upload<'_>) -> Result<(), Box<dyn Error>> {
    // The old update replaced the image after successful allocating conversion.
    *image = baseline::yuv420_to_rgba(input)?;
    Ok(())
}

#[test]
#[ignore = "release CPU/allocation benchmark; --ignored --nocapture --test-threads=1"]
fn texture_upload_bench() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let order = if reverse {
        [false, true]
    } else {
        [true, false]
    };
    for (width, height) in [(2, 2), (128, 72), (640, 360), (1920, 1080)] {
        let planes = Planes::new(width, height);
        let pixels = width as usize * height as usize;
        let iterations = (1_000_000 / pixels).clamp(2, 2000);
        for old in order {
            let run = black_box(if old {
                baseline::yuv420_to_rgba
            } else {
                upload::yuv420_to_rgba
            });
            perf::measure_sampled(
                &format!(
                    "create_{width}x{height}_{}",
                    if old { "old" } else { "new" }
                ),
                iterations,
                pixels,
                || run(black_box(planes.upload())).unwrap(),
            );
        }
        for old in order {
            let mut image = baseline::yuv420_to_rgba(planes.upload()).unwrap();
            let run = black_box(if old {
                old_update
            } else {
                upload::update_yuv420_image
            });
            perf::measure_sampled(
                &format!(
                    "update_{width}x{height}_{}",
                    if old { "old" } else { "new" }
                ),
                iterations,
                pixels,
                || {
                    run(black_box(&mut image), black_box(planes.upload())).unwrap();
                    black_box(image.as_raw());
                },
            );
        }
    }
    for (case, width, height, transparent) in [
        ("tiny", 3, 1, None),
        ("small", 128, 72, None),
        ("full", 1920, 1080, None),
        ("early", 1920, 1080, Some(0)),
        ("late", 1920, 1080, Some(1920 * 1080 - 1)),
    ] {
        let mut image = RgbaImage::from_pixel(width, height, image::Rgba([19, 77, 149, 255]));
        if let Some(pixel) = transparent {
            image.as_mut()[pixel * 4 + 3] = 254;
        }
        for old in order {
            let run = black_box(if old {
                baseline::texture_is_opaque
            } else {
                upload::texture_is_opaque
            });
            let pixels = if transparent == Some(0) {
                1
            } else {
                width as usize * height as usize
            };
            perf::measure_sampled(
                &format!("opacity_{case}_{}", if old { "old" } else { "new" }),
                (4_000_000 / pixels).clamp(8, 20000),
                pixels,
                || run(black_box(&image)),
            );
        }
    }
}
