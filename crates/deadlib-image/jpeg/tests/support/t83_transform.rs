//! Official numeric data, with an independently computed f64 cosine basis.
//! DREF measures a reconstructed-sample/FDCT chain, not entropy coefficients.
use super::{fdct, idct};
use crate::{Limits, ZIGZAG, decode_coefficients};

fn file(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/t83")
            .join(name),
    )
    .unwrap()
}

fn coefficients(name: &str) -> Vec<i16> {
    file(name)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_be_bytes(*pair))
        .collect()
}

fn basis() -> [[f64; 8]; 8] {
    std::array::from_fn(|u| {
        std::array::from_fn(|x| {
            0.5 * if u == 0 {
                std::f64::consts::FRAC_1_SQRT_2
            } else {
                1.0
            } * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0).cos()
        })
    })
}

fn forward(samples: &[u16; 64], quant: &[u16; 64], precision: u8) -> [i16; 64] {
    let b = basis();
    let center = f64::from(1u16 << (precision - 1));
    let tmp: [[f64; 8]; 8] = std::array::from_fn(|y| {
        std::array::from_fn(|u| {
            (0..8)
                .map(|x| (f64::from(samples[y * 8 + x]) - center) * b[u][x])
                .sum()
        })
    });
    std::array::from_fn(|i| {
        let (v, u) = (i / 8, i % 8);
        ((0..8).map(|y| tmp[y][u] * b[v][y]).sum::<f64>() / f64::from(quant[i])).round() as i16
    })
}

fn inverse(block: &[i16; 64], quant: &[u16; 64], precision: u8) -> [u16; 64] {
    let b = basis();
    let tmp: [[f64; 8]; 8] = std::array::from_fn(|v| {
        std::array::from_fn(|x| {
            (0..8)
                .map(|u| f64::from(block[v * 8 + u]) * f64::from(quant[v * 8 + u]) * b[u][x])
                .sum()
        })
    });
    std::array::from_fn(|i| {
        let (y, x) = (i / 8, i % 8);
        ((0..8).map(|v| tmp[v][x] * b[v][y]).sum::<f64>() + f64::from(1u16 << (precision - 1)))
            .round()
            .clamp(0.0, f64::from((1u16 << precision) - 1)) as u16
    })
}

#[test]
fn native_fdct_matches_official_itu_encoder_references() {
    let (mut checked, mut maximum) = (0, 0);
    for (precision, name) in [(8, "A1.JPG"), (12, "E1.JPG")] {
        let wire = file(name);
        let image = decode_coefficients(&wire, Limits::default()).unwrap();
        for (component, label) in image.components.iter().zip('A'..='D') {
            let reference = coefficients(&format!("EREF_{label}{precision}.DCT"));
            let source = file(&format!("{label}8.SRC"));
            assert_eq!(source.len(), component.width * component.height);
            let (bw, bh) = (component.width.div_ceil(8), component.height.div_ceil(8));
            assert_eq!(reference.len(), bw * bh * 64);
            for by in 0..bh {
                for bx in 0..bw {
                    let samples = std::array::from_fn(|i| {
                        let y = (by * 8 + i / 8).min(component.height - 1);
                        let x = (bx * 8 + i % 8).min(component.width - 1);
                        u16::from(source[y * component.width + x])
                            * if precision == 8 { 1 } else { 16 }
                    });
                    let actual = fdct(&samples, &component.quantization, precision);
                    let floating = forward(&samples, &component.quantization, precision);
                    for (k, &natural) in ZIGZAG.iter().enumerate() {
                        let want = reference[(by * bw + bx) * 64 + k];
                        let error = actual[natural].abs_diff(want);
                        assert!(error <= 1, "P={precision} {label} block {bx},{by} k={k}");
                        assert!(floating[natural].abs_diff(want) <= 1);
                        maximum = maximum.max(error);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 119_936);
    eprintln!("T83 encoder: {checked} coefficients, maximum error {maximum}");
}

#[test]
fn native_idct_matches_official_itu_reconstruction_references() {
    let (mut checked, mut maximum, mut pixel_maximum, mut model_maximum) = (0, 0, 0, 0);
    for (precision, name) in [(8, "A1.JPG"), (12, "E1.JPG")] {
        let wire = file(name);
        let image = decode_coefficients(&wire, Limits::default()).unwrap();
        for (component, label) in image.components.iter().zip('A'..='D') {
            let reference = coefficients(&format!("DREF_{label}{precision}.DCT"));
            let (bw, bh) = (component.width.div_ceil(8), component.height.div_ceil(8));
            assert_eq!(reference.len(), bw * bh * 64);
            let stride = bw * 8;
            let mut actual_samples = vec![0; stride * bh * 8];
            let mut ideal_samples = actual_samples.clone();
            for by in 0..bh {
                for bx in 0..bw {
                    let block = &component.blocks[by * component.blocks_wide + bx];
                    let actual = idct(block, &component.quantization, precision);
                    let ideal = inverse(block, &component.quantization, precision);
                    for y in 0..8 {
                        let start = (by * 8 + y) * stride + bx * 8;
                        actual_samples[start..start + 8].copy_from_slice(&actual[y * 8..y * 8 + 8]);
                        ideal_samples[start..start + 8].copy_from_slice(&ideal[y * 8..y * 8 + 8]);
                    }
                    for (&a, b) in actual.iter().zip(ideal) {
                        let error = a.abs_diff(b);
                        assert!(error <= 1);
                        pixel_maximum = pixel_maximum.max(error);
                    }
                }
            }
            for by in 0..bh {
                for bx in 0..bw {
                    // The distributed 8-bit DREF includes reconstructed edge
                    // block samples. Its 12-bit counterpart repeats the visible
                    // edge after reconstruction. The independent f64 chain must
                    // agree within one bucket before testing Rust. Half-bucket
                    // ties can differ with f64 normalization/summation order.
                    let read = |values: &[u16]| {
                        std::array::from_fn(|i| {
                            let (mut y, mut x) = (by * 8 + i / 8, bx * 8 + i % 8);
                            if precision == 12 {
                                y = y.min(component.height - 1);
                                x = x.min(component.width - 1);
                            }
                            values[y * stride + x]
                        })
                    };
                    let actual =
                        forward(&read(&actual_samples), &component.quantization, precision);
                    let ideal = forward(&read(&ideal_samples), &component.quantization, precision);
                    for (k, &natural) in ZIGZAG.iter().enumerate() {
                        let want = reference[(by * bw + bx) * 64 + k];
                        let model_error = ideal[natural].abs_diff(want);
                        assert!(
                            model_error <= 1,
                            "f64 model P={precision} {label} {bx},{by} k={k}"
                        );
                        model_maximum = model_maximum.max(model_error);
                        let error = actual[natural].abs_diff(want);
                        assert!(error <= 1, "native P={precision} {label} {bx},{by} k={k}");
                        maximum = maximum.max(error);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 119_936);
    eprintln!(
        "T83 decoder: {checked} coefficients, maximum error {maximum}; pixel error {pixel_maximum}; f64 model error {model_maximum}"
    );
}
