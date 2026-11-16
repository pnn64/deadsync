//! Production pixel kernels, measured without a window or GPU.
#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod alloc;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod pipeline {
        use super::*;


        fn reference_alpha(
            texels: Texels<'_>,
            u: f32,
            v: f32,
            sampler: SamplerDesc,
        ) -> Option<f32> {
            match sampler.filter {
                SamplerFilter::Nearest => sample_tex_nearest::<false, 4>(
                    texels,
                    texels.width,
                    texels.height,
                    u,
                    v,
                    sampler,
                )
                .map(|c| c[3]),
                SamplerFilter::Linear => sample_tex_linear::<false, 4>(
                    texels,
                    texels.width,
                    texels.height,
                    u,
                    v,
                    sampler,
                )
                .map(|c| c[3])
                .or(Some(0.0)),
            }
        }

        fn alpha(texels: Texels<'_>, u: f32, v: f32, sampler: SamplerDesc) -> Option<f32> {
            match sampler.filter {
                SamplerFilter::Nearest => {
                    sample_alpha_nearest(texels, texels.width, texels.height, u, v, sampler)
                }
                SamplerFilter::Linear => {
                    sample_alpha_linear(texels, texels.width, texels.height, u, v, sampler)
                }
            }
        }

        #[test]
        fn alpha_sampling_matches_rgba_including_nonfinite_and_missing_texels() {
            let image = RgbaImage::from_fn(7, 5, |x, y| {
                image::Rgba([x as u8, 71, y as u8, (x * 47 + y * 23) as u8])
            });
            let halves: Vec<_> = (0..35)
                .map(|i| {
                    let values: [u16; 9] =
                        [0, 1, 0x3c00, 0x8000, 0x7c00, 0xfc00, 0x7c01, 0x7e35, 0x3555];
                    u64::from(values[i % values.len()]) << 48 | 0x7e357c018001
                })
                .collect();
            let coordinates = [
                -f32::INFINITY,
                -10.25,
                -1.0,
                -0.1,
                -0.0,
                0.0,
                0.01,
                0.25,
                0.5,
                0.99,
                1.0,
                1.5,
                f32::INFINITY,
                f32::NAN,
            ];
            for half in [&[][..], halves.as_slice(), &halves[..1]] {
                for bytes in [image.as_raw().as_slice(), &image.as_raw()[..4], &[][..]] {
                    let texels = Texels {
                        bytes,
                        half,
                        width: 7,
                        height: 5,
                    };
                    for wrap in [SamplerWrap::Clamp, SamplerWrap::Repeat] {
                        for filter in [SamplerFilter::Nearest, SamplerFilter::Linear] {
                            let sampler = SamplerDesc {
                                wrap,
                                filter,
                                mipmaps: false,
                            };
                            for u in coordinates {
                                for v in coordinates {
                                    let expected = reference_alpha(texels, u, v, sampler);
                                    let actual = alpha(texels, u, v, sampler);
                                    assert_eq!(
                                        actual.map(f32::to_bits),
                                        expected.map(f32::to_bits),
                                        "u={u}, v={v}, sampler={sampler:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        #[test]
        fn alpha_sampling_preserves_every_binary16_encoding() {
            let half: Vec<_> = (0..=u16::MAX).map(|bits| u64::from(bits) << 48).collect();
            let texels = Texels {
                bytes: &[],
                half: &half,
                width: 256,
                height: 256,
            };
            for filter in [SamplerFilter::Nearest, SamplerFilter::Linear] {
                let sampler = SamplerDesc {
                    filter,
                    wrap: SamplerWrap::Repeat,
                    mipmaps: false,
                };
                for index in 0..half.len() {
                    let u = ((index % 256) as f32 + 0.875) / 256.0;
                    let v = ((index / 256) as f32 + 0.75) / 256.0;
                    assert_eq!(
                        alpha(texels, u, v, sampler).map(f32::to_bits),
                        reference_alpha(texels, u, v, sampler).map(f32::to_bits),
                        "half={index:04x}, filter={filter:?}"
                    );
                }
            }
        }


        fn reference_yuv(upload: Yuv420Upload<'_>) -> RgbaImage {
            RgbaImage::from_fn(upload.width, upload.height, |x, y| {
                let chroma = (y / 2 * (upload.width / 2) + x / 2) as usize;
                let u = (f32::from(upload.u[chroma]) / 255.0)
                    .mul_add(upload.levels[2], upload.levels[3]);
                let v = (f32::from(upload.v[chroma]) / 255.0)
                    .mul_add(upload.levels[2], upload.levels[3]);
                let y = (f32::from(upload.y[(y * upload.width + x) as usize]) / 255.0)
                    .mul_add(upload.levels[0], upload.levels[1]);
                image::Rgba([
                    (upload.coeffs[0].mul_add(v, y) * 255.0).round() as u8,
                    (upload.coeffs[2].mul_add(v, upload.coeffs[1].mul_add(u, y)) * 255.0).round()
                        as u8,
                    (upload.coeffs[3].mul_add(u, y) * 255.0).round() as u8,
                    255,
                ])
            })
        }

        #[test]
        fn yuv_conversion_matches_ordered_fused_reference() {
            let y: Vec<_> = (0..256).cycle().take(256 * 256).map(|v| v as u8).collect();
            let u: Vec<_> = (0..256)
                .flat_map(|v| std::iter::repeat_n(v as u8, 64))
                .collect();
            let v: Vec<_> = (0..16384).map(|v| (v * 71) as u8).collect();
            let mut image = RgbaImage::new(256, 256);
            for levels in [
                [1.0, 0.0, 1.0, -0.5],
                [1.164_383_5, -0.073_059_365, 1.138_392_9, -0.571_428_6],
                [0.0, -0.0, 0.0, -0.0],
                [f32::INFINITY, f32::NAN, -f32::INFINITY, 0.0],
            ] {
                for coeffs in [
                    [1.402, -0.344136, -0.714136, 1.772],
                    [1.5748, -0.187324, -0.468124, 1.8556],
                    [f32::NAN, f32::INFINITY, -0.0, -f32::INFINITY],
                ] {
                    let upload = Yuv420Upload {
                        width: 256,
                        height: 256,
                        y: &y,
                        u: &u,
                        v: &v,
                        levels,
                        coeffs,
                    };
                    let expected = reference_yuv(upload);
                    crate::alloc::assert_no_churn(|| {
                        update_yuv420_image(&mut image, upload).unwrap()
                    });
                    assert_eq!(image.as_raw(), expected.as_raw());
                }
            }
        }

        #[test]
        fn yuv_tables_preserve_rounding_and_the_small_image_path() {
            for (width, height) in [
                (2, 2),
                (2, 510),
                (2, 512),
                (2, 514),
                (30, 34),
                (32, 32),
                (64, 64),
            ] {
                let count = width as usize * height as usize;
                let y: Vec<_> = (0..count).map(|i| (i * 71) as u8).collect();
                let u: Vec<_> = (0..count / 4).map(|i| (i * 47) as u8).collect();
                let v: Vec<_> = (0..count / 4).map(|i| (i * 23) as u8).collect();
                let mut image = RgbaImage::new(width, height);
                for levels in [
                    [1.0, 0.0, 1.0, -0.5],
                    [1.164_383_5, -0.073_059_365, 1.138_392_9, -0.571_428_6],
                    [f32::INFINITY, f32::NAN, -f32::INFINITY, -0.0],
                ] {
                    let upload = Yuv420Upload {
                        width,
                        height,
                        y: &y,
                        u: &u,
                        v: &v,
                        levels,
                        coeffs: [1.402, -0.344136, -0.714136, 1.772],
                    };
                    let expected = reference_yuv(upload);
                    let pointer = image.as_raw().as_ptr();
                    crate::alloc::assert_no_churn(|| {
                        update_yuv420_image(&mut image, upload).unwrap()
                    });
                    assert_eq!(image.as_raw(), expected.as_raw());
                    assert_eq!(image.as_raw().as_ptr(), pointer);
                }
            }
        }


        #[test]
        fn repeat_indices_match_signed_remainder_at_boundaries() {
            for max in [
                0usize,
                1,
                2,
                3,
                7,
                63,
                127,
                128,
                255,
                640,
                1920,
                i32::MAX as usize,
            ] {
                for i in (-4000..=4000).chain([i32::MIN, i32::MAX]) {
                    let expected = if max == 0 {
                        0
                    } else {
                        i.rem_euclid(max as i32) as usize
                    };
                    assert_eq!(
                        wrap_index(i, max, SamplerWrap::Repeat),
                        expected,
                        "{i}/{max}"
                    );
                }
            }
        }

    }
}
