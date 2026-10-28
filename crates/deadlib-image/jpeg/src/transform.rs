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

pub(crate) fn idct(block: &[i16; 64], quant: &[u16; 64], precision: u8) -> [u16; 64] {
    let center = 1i64 << (precision - 1);
    let max = (1i64 << precision) - 1;
    if block[1..].iter().all(|&v| v == 0) {
        let value = ((i64::from(block[0]) * i64::from(quant[0]) + 4) >> 3) + center;
        return [value.clamp(0, max) as u16; 64];
    }
    let mut tmp = [[0i64; 8]; 8];
    for v in 0..8 {
        for x in 0..8 {
            let mut sum = 0;
            for u in 0..8 {
                sum += i64::from(block[v * 8 + u]) * i64::from(quant[v * 8 + u]) * BASIS[x][u];
            }
            tmp[v][x] = (sum + (1 << 15)) >> 16;
        }
    }
    let mut out = [0; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut sum = 0;
            for v in 0..8 {
                sum += tmp[v][x] * BASIS[y][v];
            }
            out[y * 8 + x] = (((sum + (1 << 23)) >> 24) + center).clamp(0, max) as u16;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
