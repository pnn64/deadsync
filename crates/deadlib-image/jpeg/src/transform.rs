// A separable orthonormal IDCT with Q20 cosine factors. The first pass retains
// four fractional bits; all accumulators are i64. For i16 coefficients and u16
// quantizers even adversarial blocks stay within i64 at both stages.
const BASIS: [[i64; 8]; 8] = [
    [
        370728, 514214, 484379, 435930, 370728, 291279, 200636, 102284,
    ],
    [
        370728, 435930, 200636, -102284, -370728, -514214, -484379, -291279,
    ],
    [
        370728, 291279, -200636, -514214, -370728, 102284, 484379, 435930,
    ],
    [
        370728, 102284, -484379, -291279, 370728, 435930, -200636, -514214,
    ],
    [
        370728, -102284, -484379, 291279, 370728, -435930, -200636, 514214,
    ],
    [
        370728, -291279, -200636, 514214, -370728, -102284, 484379, -435930,
    ],
    [
        370728, -435930, 200636, 102284, -370728, 514214, -484379, 291279,
    ],
    [
        370728, -514214, 484379, -435930, 370728, -291279, 200636, -102284,
    ],
];

#[cfg(test)]
#[path = "../tests/support/t83_transform.rs"]
mod t83_transform;

/// Hierarchical frames keep the exact Q20 transform: lossless corrections are
/// exact only against the encoder's reference reconstruction.
pub(crate) fn idct_exact(block: &[i16; 64], quant: &[u16; 64], precision: u8) -> [u16; 64] {
    idct_mode(block, quant, Some(precision))
}

pub(crate) fn idct(block: &[i16; 64], quant: &[u16; 64], precision: u8) -> [u16; 64] {
    if precision == 8 {
        let mut out = [0u8; 64];
        idct8(block, &FloatQuant::new(quant), &mut out, 8);
        return out.map(u16::from);
    }
    idct_mode(block, quant, Some(precision))
}

// 8-bit samples use libjpeg's floating-point AAN inverse DCT (jidctflt.c,
// the JDCT_FLOAT reference used by the C oracles) with its operation order,
// f64-derived constants, folded 1/8 scale and truncation after +128.5.
// Rust evaluates every f32 operation exactly once in IEEE single precision,
// so vectorized and scalar evaluation produce identical samples.
// libjpeg's decimal literals, rounded through f64 like the C constants.
#[allow(clippy::approx_constant)]
const R2: f32 = 1.414213562f64 as f32;
const C2: f32 = 1.847759065f64 as f32;
const C2_MINUS_C6: f32 = 1.082392200f64 as f32;
const C2_PLUS_C6: f32 = 2.613125930f64 as f32;

/// Dequantization multipliers with AAN row/column scales and the 1/8 descale.
pub(crate) struct FloatQuant([f32; 64]);

impl FloatQuant {
    pub fn new(quant: &[u16; 64]) -> Self {
        const AAN: [f64; 8] = [
            1.0,
            1.387039845,
            1.306562965,
            1.175875602,
            1.0,
            0.785694958,
            0.541196100,
            0.275899379,
        ];
        Self(core::array::from_fn(|i| {
            (f64::from(quant[i]) * AAN[i / 8] * AAN[i % 8] * 0.125) as f32
        }))
    }
}

#[inline(always)]
fn aan_line(x: [f32; 8]) -> [f32; 8] {
    let tmp10 = x[0] + x[4];
    let tmp11 = x[0] - x[4];
    let tmp13 = x[2] + x[6];
    let tmp12 = (x[2] - x[6]) * R2 - tmp13;
    let tmp0 = tmp10 + tmp13;
    let tmp3 = tmp10 - tmp13;
    let tmp1 = tmp11 + tmp12;
    let tmp2 = tmp11 - tmp12;
    let z13 = x[5] + x[3];
    let z10 = x[5] - x[3];
    let z11 = x[1] + x[7];
    let z12 = x[1] - x[7];
    let tmp7 = z11 + z13;
    let tmp11 = (z11 - z13) * R2;
    let z5 = (z10 + z12) * C2;
    let tmp10 = z5 - z12 * C2_MINUS_C6;
    let tmp12 = z5 - z10 * C2_PLUS_C6;
    let tmp6 = tmp12 - tmp7;
    let tmp5 = tmp11 - tmp6;
    let tmp4 = tmp10 - tmp5;
    [
        tmp0 + tmp7,
        tmp1 + tmp6,
        tmp2 + tmp5,
        tmp3 + tmp4,
        tmp3 - tmp4,
        tmp2 - tmp5,
        tmp1 - tmp6,
        tmp0 - tmp7,
    ]
}

/// Reconstruct one 8-bit block into `out` rows `stride` bytes apart.
// Plain counted loops over flat arrays keep both passes vectorized under
// baseline SSE2; value-array formulations left whole passes scalar.
#[inline]
pub(crate) fn idct8(block: &[i16; 64], quant: &FloatQuant, out: &mut [u8], stride: usize) {
    let mut input = [0f32; 64];
    for i in 0..64 {
        input[i] = f32::from(block[i]) * quant.0[i];
    }
    // Columns first, as in jidctflt.
    let mut work = [0f32; 64];
    for u in 0..8 {
        let column = aan_line(core::array::from_fn(|v| input[v * 8 + u]));
        for (y, value) in column.into_iter().enumerate() {
            work[y * 8 + u] = value;
        }
    }
    // Rows go to a contiguous block first: stores `stride` apart would stop
    // the second pass from vectorizing across rows.
    let mut pixels = [0u8; 64];
    for y in 0..8 {
        let mut row: [f32; 8] = work[y * 8..y * 8 + 8].try_into().unwrap();
        // Level shift and rounding bias enter with the DC term.
        row[0] += 128.5;
        let samples = aan_line(row);
        for x in 0..8 {
            pixels[y * 8 + x] = samples[x].clamp(0.0, 255.0) as u8;
        }
    }
    for (y, row) in pixels.as_chunks::<8>().0.iter().enumerate() {
        out[y * stride..y * stride + 8].copy_from_slice(row);
    }
}

/// DC-only blocks reduce exactly to one sample in every position.
#[inline]
pub(crate) fn idct8_dc(dc: i16, quant: &FloatQuant, out: &mut [u8], stride: usize) {
    let value = (f32::from(dc) * quant.0[0] + 128.5).clamp(0.0, 255.0) as u8;
    for y in 0..8 {
        out[y * stride..y * stride + 8].fill(value);
    }
}

/// Annex J residuals have no inverse level shift or unsigned sample clipping.
/// Preserve their two's-complement result modulo 65536 for frame composition.
pub(crate) fn idct_differential(block: &[i16; 64], quant: &[u16; 64]) -> [u16; 64] {
    idct_mode(block, quant, None)
}
fn idct_mode(block: &[i16; 64], quant: &[u16; 64], precision: Option<u8>) -> [u16; 64] {
    let finish = |v: i64| {
        if let Some(p) = precision {
            (v + (1i64 << (p - 1))).clamp(0, (1i64 << p) - 1) as u16
        } else {
            v as u16
        }
    };
    if block[1..].iter().all(|&v| v == 0) {
        return [finish((i64::from(block[0]) * i64::from(quant[0]) + 4) >> 3); 64];
    }
    let mut tmp = [[0i64; 8]; 8];
    for v in 0..8 {
        let input =
            core::array::from_fn(|u| i64::from(block[v * 8 + u]) * i64::from(quant[v * 8 + u]));
        tmp[v] = inverse_line(input).map(|sum| (sum + (1 << 15)) >> 16);
    }
    let mut out = [0; 64];
    for x in 0..8 {
        let column = inverse_line(tmp.map(|row| row[x]));
        for (y, sum) in column.into_iter().enumerate() {
            out[y * 8 + x] = finish((sum + (1 << 23)) >> 24);
        }
    }
    out
}

/// Factor the exact Q20 matrix using its even/odd and mirror symmetries.
/// All rounding stays at the original pass boundaries, including differential
/// reconstruction. No approximation or narrower intermediate changes samples.
#[inline]
fn inverse_line(a: [i64; 8]) -> [i64; 8] {
    if a[1..].iter().all(|&v| v == 0) {
        return [a[0] * 370728; 8];
    }
    let e0 = (a[0] + a[4]) * 370728;
    let e1 = (a[0] - a[4]) * 370728;
    let e2 = a[2] * 484379 + a[6] * 200636;
    let e3 = a[2] * 200636 - a[6] * 484379;
    let even = [e0 + e2, e1 + e3, e1 - e3, e0 - e2];
    let odd = [
        a[1] * 514214 + a[3] * 435930 + a[5] * 291279 + a[7] * 102284,
        a[1] * 435930 - a[3] * 102284 - a[5] * 514214 - a[7] * 291279,
        a[1] * 291279 - a[3] * 514214 + a[5] * 102284 + a[7] * 435930,
        a[1] * 102284 - a[3] * 291279 + a[5] * 435930 - a[7] * 514214,
    ];
    [
        even[0] + odd[0],
        even[1] + odd[1],
        even[2] + odd[2],
        even[3] + odd[3],
        even[3] - odd[3],
        even[2] - odd[2],
        even[1] - odd[1],
        even[0] - odd[0],
    ]
}

/// Forward orthonormal transform, using the transpose of the same Q20 basis.
/// Keep four fractional bits through the first pass and quantize the second
/// pass directly, avoiding an extra rounding to integer DCT coefficients.
pub(crate) fn fdct(samples: &[u16; 64], quant: &[u16; 64], precision: u8) -> [i16; 64] {
    let center = 1i64 << (precision - 1);
    fdct_inner(samples, quant, |v| i64::from(v) - center).map(|v| v as i16)
}

/// Annex J's signed spatial differences undergo no forward level shift.
/// Reject coefficients outside the JPEG signed-16-bit model rather than wrap.
pub(crate) fn fdct_differential(
    samples: &[u16; 64],
    quant: &[u16; 64],
) -> crate::Result<[i16; 64]> {
    let wide = fdct_inner(samples, quant, |v| i64::from(v as i16));
    let mut result = [0; 64];
    for (to, value) in result.iter_mut().zip(wide) {
        *to = i16::try_from(value)
            .map_err(|_| crate::Error::Invalid("differential DCT coefficient range"))?;
    }
    Ok(result)
}

fn fdct_inner(samples: &[u16; 64], quant: &[u16; 64], read: impl Fn(u16) -> i64) -> [i64; 64] {
    if samples.iter().all(|&v| v == samples[0]) {
        let dc = read(samples[0]) * 8;
        let q = i64::from(quant[0]);
        let mut out = [0i64; 64];
        out[0] = dc.signum() * ((dc.abs() + q / 2) / q);
        return out;
    }
    let mut tmp = [[0i64; 8]; 8];
    for y in 0..8 {
        for u in 0..8 {
            let mut sum = 0;
            for x in 0..8 {
                sum += read(samples[y * 8 + x]) * BASIS[x][u];
            }
            tmp[y][u] = (sum + (1 << 15)) >> 16;
        }
    }
    let mut out = [0i64; 64];
    for v in 0..8 {
        for u in 0..8 {
            let mut sum = 0;
            for y in 0..8 {
                sum += tmp[y][u] * BASIS[y][v];
            }
            let divisor = i64::from(quant[v * 8 + u]) << 24;
            out[v * 8 + u] = sum.signum() * ((sum.abs() + divisor / 2) / divisor);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_factorization_is_exact_for_sparse_and_wide_inputs() {
        let matrix =
            |a: [i64; 8]| BASIS.map(|row| row.into_iter().zip(a).map(|(b, v)| b * v).sum());
        for value in [-(1i64 << 38), -1, 0, 1, 1 << 38] {
            assert_eq!(inverse_line([value; 8]), matrix([value; 8]));
            for i in 0..8 {
                let mut a = [0; 8];
                a[i] = value;
                assert_eq!(inverse_line(a), matrix(a));
            }
        }
        let mut state = 0x8723479812345678u64;
        for _ in 0..1000 {
            let input = core::array::from_fn(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 26) as i64 - (1 << 37)
            });
            assert_eq!(inverse_line(input), matrix(input));
        }
    }
    #[test]
    fn signed_forward_dct_has_no_level_shift_and_checks_its_model() {
        assert_eq!(fdct_differential(&[0; 64], &[1; 64]).unwrap(), [0; 64]);
        let mut want = [0; 64];
        want[0] = -80;
        assert_eq!(
            fdct_differential(&[(-10i16) as u16; 64], &[1; 64]).unwrap(),
            want
        );
        assert!(fdct_differential(&[i16::MIN as u16; 64], &[1; 64]).is_err());
        let mut state = 0x12345678u32;
        for _ in 0..100 {
            let mut samples = [0; 64];
            let mut quant = [0; 64];
            for i in 0..64 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                samples[i] = (((state >> 16) % 8191) as i16 - 4095) as u16;
                quant[i] = ((state >> 24) % 31 + 1) as u16;
            }
            let actual = fdct_differential(&samples, &quant).unwrap();
            for v in 0..8 {
                for u in 0..8 {
                    let mut ideal = 0.0;
                    for y in 0..8 {
                        for x in 0..8 {
                            ideal += f64::from(samples[y * 8 + x] as i16)
                                * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0)
                                    .cos()
                                * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / 16.0)
                                    .cos();
                        }
                    }
                    let cu = if u == 0 {
                        std::f64::consts::FRAC_1_SQRT_2
                    } else {
                        1.0
                    };
                    let cv = if v == 0 {
                        std::f64::consts::FRAC_1_SQRT_2
                    } else {
                        1.0
                    };
                    ideal = ideal * cu * cv / 4.0 / f64::from(quant[v * 8 + u]);
                    assert!((f64::from(actual[v * 8 + u]) - ideal.round()).abs() <= 1.0);
                }
            }
        }
    }
    #[test]
    fn impulse_blocks_match_mathematical_idct() {
        for precision in [8, 12] {
            for k in 0..64 {
                for value in [-1023, -1, 1, 1023] {
                    let mut block = [0; 64];
                    block[k] = value;
                    let result = idct(&block, &[1; 64], precision);
                    for y in 0..8 {
                        for x in 0..8 {
                            let u = k % 8;
                            let v = k / 8;
                            let cu = if u == 0 {
                                std::f64::consts::FRAC_1_SQRT_2
                            } else {
                                1.0
                            };
                            let cv = if v == 0 {
                                std::f64::consts::FRAC_1_SQRT_2
                            } else {
                                1.0
                            };
                            let ideal = f64::from(value) / 4.0
                                * cu
                                * cv
                                * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0)
                                    .cos()
                                * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / 16.0)
                                    .cos();
                            let want = (ideal + (1u32 << (precision - 1)) as f64)
                                .round()
                                .clamp(0.0, ((1u32 << precision) - 1) as f64)
                                as i32;
                            assert!((i32::from(result[y * 8 + x]) - want).abs() <= 1);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn extreme_inputs_do_not_overflow() {
        for value in [i16::MIN, i16::MAX] {
            let out = idct(&[value; 64], &[u16::MAX; 64], 12);
            assert!(out.iter().all(|&v| v <= 4095));
        }
    }

    #[test]
    fn differential_idct_matches_signed_mathematical_transform() {
        assert_eq!(idct_differential(&[0; 64], &[1; 64]), [0; 64]);
        let mut dc = [0; 64];
        dc[0] = -80;
        assert_eq!(idct_differential(&dc, &[1; 64]), [(-10i16) as u16; 64]);
        let mut state = 0x76543210u32;
        for _ in 0..100 {
            let mut block = [0; 64];
            let mut quant = [0; 64];
            for i in 0..64 {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                block[i] = ((state >> 16) % 255) as i16 - 127;
                quant[i] = ((state >> 24) % 16 + 1) as u16;
            }
            let actual = idct_differential(&block, &quant);
            for y in 0..8 {
                for x in 0..8 {
                    let mut ideal = 0.0;
                    for v in 0..8 {
                        for u in 0..8 {
                            let cu = if u == 0 {
                                std::f64::consts::FRAC_1_SQRT_2
                            } else {
                                1.0
                            };
                            let cv = if v == 0 {
                                std::f64::consts::FRAC_1_SQRT_2
                            } else {
                                1.0
                            };
                            ideal += f64::from(block[v * 8 + u]) * f64::from(quant[v * 8 + u])
                                / 4.0
                                * cu
                                * cv
                                * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0)
                                    .cos()
                                * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / 16.0)
                                    .cos();
                        }
                    }
                    assert!((f64::from(actual[y * 8 + x] as i16) - ideal.round()).abs() <= 1.0);
                }
            }
        }
    }

    #[test]
    fn forward_transform_matches_mathematical_dct() {
        let mut state = 0x01234567u32;
        for precision in [8, 12] {
            for _ in 0..20 {
                let mut samples = [0u16; 64];
                for v in &mut samples {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    *v = (state >> 16) as u16 & ((1 << precision) - 1);
                }
                let got = fdct(&samples, &[1; 64], precision);
                for v in 0..8 {
                    for u in 0..8 {
                        let mut sum = 0.0;
                        for y in 0..8 {
                            for x in 0..8 {
                                sum += (f64::from(samples[y * 8 + x])
                                    - (1u32 << (precision - 1)) as f64)
                                    * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0)
                                        .cos()
                                    * ((2 * y + 1) as f64 * v as f64 * std::f64::consts::PI / 16.0)
                                        .cos();
                            }
                        }
                        let cu = if u == 0 {
                            std::f64::consts::FRAC_1_SQRT_2
                        } else {
                            1.0
                        };
                        let cv = if v == 0 {
                            std::f64::consts::FRAC_1_SQRT_2
                        } else {
                            1.0
                        };
                        let want = (sum * cu * cv / 4.0).round() as i32;
                        assert!((i32::from(got[v * 8 + u]) - want).abs() <= 1);
                    }
                }
            }
        }
    }
}
