//! LPC reconstruction. The scalar path also handles 33-bit stereo side signals.
use crate::{Error, frame::check_sample};

#[inline(always)]
pub(crate) fn restore(
    output: &mut [i64],
    coeff: &[i64],
    shift: u32,
    width: u32,
) -> Result<(), Error> {
    let order = coeff.len();
    assert!((1..=32).contains(&order) && order < output.len());
    assert!((1..=33).contains(&width) && shift <= 15);
    // AVX2 multiplies the signed low 32 bits of each i64 lane. Every previous
    // sample must therefore fit i32; the 33-bit side channel uses scalar i64.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if width <= 32 && order >= 4 && output.len() >= 64 && std::is_x86_feature_detected!("avx2") {
        return x86::restore(output, coeff, shift, width);
    }
    macro_rules! short {
        ($($n:literal),*) => {
            match order {
                $($n => return short_scalar::<$n>(output, coeff, shift, width),)*
                _ => (),
            }
        }
    }
    short!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
    scalar(output, coeff, shift, width)
}

#[inline(never)]
fn short_scalar<const N: usize>(
    output: &mut [i64],
    coeff: &[i64],
    shift: u32,
    width: u32,
) -> Result<(), Error> {
    let coeff: &[i64; N] = coeff.try_into().unwrap();
    for i in N..output.len() {
        let mut sum = 0i64;
        for j in 0..N {
            sum += coeff[j] * output[i - j - 1];
        }
        output[i] += sum >> shift;
        check_sample(output[i], width)?;
    }
    Ok(())
}

#[inline(always)]
fn scalar(output: &mut [i64], coeff: &[i64], shift: u32, width: u32) -> Result<(), Error> {
    let order = coeff.len();
    for i in order..output.len() {
        let mut sum = 0i64;
        for j in 0..order {
            sum += coeff[j] * output[i - j - 1];
        }
        output[i] += sum >> shift;
        check_sample(output[i], width)?;
    }
    Ok(())
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[allow(unsafe_code)]
mod x86 {
    use super::*;
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    pub(super) fn restore(
        output: &mut [i64],
        coeff: &[i64],
        shift: u32,
        width: u32,
    ) -> Result<(), Error> {
        // These checks make the memory and arithmetic preconditions local to
        // the only unsafe entry point, including for direct unit-test calls.
        assert!(std::is_x86_feature_detected!("avx2"));
        assert!((4..=32).contains(&coeff.len()) && coeff.len() < output.len());
        assert!((1..=32).contains(&width) && shift <= 15);
        assert!(coeff.iter().all(|&c| (-16384..=16383).contains(&c)));
        for &sample in &output[..coeff.len()] {
            check_sample(sample, width)?;
        }
        let mut padded = [0i64; 35];
        padded[..coeff.len()].copy_from_slice(coeff);
        macro_rules! dispatch {
            ($($n:literal),*) => {
                match coeff.len() {
                    // SAFETY: AVX2 is detected above. The kernel reads only
                    // N initialized coefficients and N earlier, checked samples
                    // in the same output slice. It checks each new sample before
                    // allowing the next iteration to use its low 32 bits.
                    $($n => unsafe { kernel::<$n>(output, &padded, shift, width) },)*
                    _ => unreachable!(),
                }
            }
        }
        dispatch!(
            4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
            27, 28, 29, 30, 31, 32
        )
    }

    #[target_feature(enable = "avx2")]
    #[allow(
        clippy::needless_range_loop,
        reason = "Constant tap indices retain unrolled predictor code"
    )]
    unsafe fn kernel<const N: usize>(
        output: &mut [i64],
        coeff: &[i64; 35],
        shift: u32,
        width: u32,
    ) -> Result<(), Error> {
        // SAFETY: the dispatcher guarantees N <= 32 < output.len(), valid
        // coefficient/sample widths and AVX2 support. Coefficient loads cover
        // initialized elements in the explicitly zero-padded stack array. Each
        // broadcast reads one earlier, checked sample inside the output slice.
        unsafe {
            let base = output.as_mut_ptr();
            let mut i = N;
            while output.len() - i >= 4 {
                let mut sums = [_mm256_setzero_si256(); 4];
                for j in 0..N {
                    let c = _mm256_loadu_si256(coeff.as_ptr().add(j).cast());
                    let previous = _mm256_set1_epi64x(*base.add(i - j - 1));
                    sums[j % 4] = _mm256_add_epi64(sums[j % 4], _mm256_mul_epi32(previous, c));
                }
                let sum = _mm256_add_epi64(
                    _mm256_add_epi64(sums[0], sums[1]),
                    _mm256_add_epi64(sums[2], sums[3]),
                );
                let mut partial = [0i64; 4];
                _mm256_storeu_si256(partial.as_mut_ptr().cast(), sum);
                // Lane a contains the terms from samples before this batch.
                // Add the a newly reconstructed samples before shifting, so
                // the original LPC rounding is preserved separately per output.
                for a in 0..4 {
                    let mut prediction = partial[a];
                    for b in 0..a {
                        prediction += coeff[a - b - 1] * *base.add(i + b);
                    }
                    let value = *base.add(i + a) + (prediction >> shift);
                    check_sample(value, width)?;
                    *base.add(i + a) = value;
                }
                i += 4;
            }
            for i in i..output.len() {
                let mut prediction = 0i64;
                for j in 0..N {
                    prediction += coeff[j] * *base.add(i - j - 1);
                }
                let value = *base.add(i) + (prediction >> shift);
                check_sample(value, width)?;
                *base.add(i) = value;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatched_lpc_matches_scalar_at_every_order_and_width() {
        let mut random = 9639u64;
        for width in 1..=33 {
            let bound = 1i64 << (width - 1);
            for order in 1..=32 {
                for len in [order + 1, 65, 257] {
                    let mut pcm = Vec::new();
                    for i in 0..len {
                        random ^= random << 13;
                        random ^= random >> 7;
                        random ^= random << 17;
                        pcm.push(match i % 7 {
                            0 => -bound,
                            1 => bound - 1,
                            _ => (random % (2 * bound) as u64) as i64 - bound,
                        });
                    }
                    let coeff: Vec<i64> = (0..order)
                        .map(|i| ((i * 11 + order) % 31) as i64 - 15)
                        .collect();
                    for shift in [0, 5, 15] {
                        let mut residual = pcm.clone();
                        for i in order..len {
                            let prediction: i64 = coeff
                                .iter()
                                .enumerate()
                                .map(|(j, c)| c * pcm[i - j - 1])
                                .sum();
                            residual[i] -= prediction >> shift;
                        }
                        let mut reference = residual.clone();
                        scalar(&mut reference, &coeff, shift, width).unwrap();
                        assert_eq!(reference, pcm);
                        restore(&mut residual, &coeff, shift, width).unwrap();
                        assert_eq!(
                            residual, pcm,
                            "width={width}, order={order}, len={len}, shift={shift}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn maximum_precision_coefficients_and_signed_samples() {
        for order in 4..=32 {
            let pcm: Vec<i64> = (0..129)
                .map(|i| {
                    if i % 3 == 0 {
                        i32::MIN as i64
                    } else {
                        i32::MAX as i64
                    }
                })
                .collect();
            for pattern in 0..3 {
                let coeff: Vec<i64> = (0..order)
                    .map(|i| {
                        if pattern == 0 || (pattern == 2 && i % 2 == 0) {
                            -16384
                        } else {
                            16383
                        }
                    })
                    .collect();
                let mut residual = pcm.clone();
                for i in order..pcm.len() {
                    let prediction: i64 = coeff
                        .iter()
                        .enumerate()
                        .map(|(j, c)| c * pcm[i - j - 1])
                        .sum();
                    residual[i] -= prediction >> 15;
                }
                restore(&mut residual, &coeff, 15, 32).unwrap();
                assert_eq!(residual, pcm);
            }
        }
    }

    #[test]
    fn dispatched_lpc_rejects_reconstructed_overflow() {
        for width in 1..=33 {
            for order in 1..=32 {
                for sign in [-1, 1] {
                    let bound = 1i64 << (width - 1);
                    let extreme = if sign < 0 { -bound } else { bound - 1 };
                    let mut residual = vec![0; 65];
                    residual[..order].fill(extreme);
                    residual[order] = sign;
                    let mut coeff = vec![0; order];
                    coeff[0] = 1;
                    let mut reference = residual.clone();
                    assert!(scalar(&mut reference, &coeff, 0, width).is_err());
                    assert!(restore(&mut residual, &coeff, 0, width).is_err());
                }
            }
        }
    }
}
