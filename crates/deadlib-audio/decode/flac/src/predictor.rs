//! LPC reconstruction. The scalar path also handles 33-bit stereo side signals.
use crate::{Error, frame::check_sample, sample::Sample};

#[inline(always)]
pub(crate) fn restore<T: Sample>(
    output: &mut [T],
    coeff: &[i64],
    shift: u32,
    width: u32,
) -> Result<(), Error> {
    let order = coeff.len();
    assert!((1..=32).contains(&order) && order < output.len());
    assert!((1..=33).contains(&width) && shift <= 15);
    assert!(width <= T::BITS);
    // AVX2 multiplies the signed low 32 bits of each i64 lane. Every previous
    // sample must therefore fit i32; the 33-bit side channel uses scalar i64.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if width <= 32 && order >= 4 && output.len() >= 64 && std::is_x86_feature_detected!("avx2") {
        return x86::restore(output, coeff, shift, width);
    }
    macro_rules! short {
        ($($n:literal),*) => {
            match order {
                $($n => return short_scalar::<T, $n>(output, coeff, shift, width),)*
                _ => (),
            }
        }
    }
    short!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
    scalar(output, coeff, shift, width)
}

#[inline(never)]
fn short_scalar<T: Sample, const N: usize>(
    output: &mut [T],
    coeff: &[i64],
    shift: u32,
    width: u32,
) -> Result<(), Error> {
    let coeff: &[i64; N] = coeff.try_into().unwrap();
    for i in N..output.len() {
        let mut sum = 0i64;
        for j in 0..N {
            sum += coeff[j] * output[i - j - 1].wide();
        }
        let value = output[i].wide() + (sum >> shift);
        check_sample(value, width)?;
        output[i] = T::store(value);
    }
    Ok(())
}

#[inline(always)]
fn scalar<T: Sample>(output: &mut [T], coeff: &[i64], shift: u32, width: u32) -> Result<(), Error> {
    let order = coeff.len();
    for i in order..output.len() {
        let mut sum = 0i64;
        for j in 0..order {
            sum += coeff[j] * output[i - j - 1].wide();
        }
        let value = output[i].wide() + (sum >> shift);
        check_sample(value, width)?;
        output[i] = T::store(value);
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

    pub(super) fn restore<T: Sample>(
        output: &mut [T],
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
            check_sample(sample.wide(), width)?;
        }
        let mut padded = [0i64; 35];
        padded[..coeff.len()].copy_from_slice(coeff);
        macro_rules! dispatch {
            ($($n:literal),*) => {
                match coeff.len() {
                    // SAFETY: AVX2 is detected above. The kernel reads only
                    // N initialized coefficients and N earlier, checked samples
                    // in the same output slice. It checks all four new samples
                    // before the next batch can use their low 32 bits.
                    $($n => unsafe { kernel::<T, $n>(output, &padded, shift, width) },)*
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
    unsafe fn kernel<T: Sample, const N: usize>(
        output: &mut [T],
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
            let bias = 1u64 << (width - 1);
            while output.len() - i >= 4 {
                // Explicit taps keep four independent accumulators in registers.
                // An indexed loop leaves stack loads/stores in high-order code.
                let (mut s0, mut s1, mut s2, mut s3) = (
                    _mm256_setzero_si256(),
                    _mm256_setzero_si256(),
                    _mm256_setzero_si256(),
                    _mm256_setzero_si256(),
                );
                macro_rules! tap {
                    ($j:literal, $sum:ident) => {
                        if $j < N {
                            let c = _mm256_loadu_si256(coeff.as_ptr().add($j).cast());
                            let previous = _mm256_set1_epi64x((*base.add(i - $j - 1)).wide());
                            $sum = _mm256_add_epi64($sum, _mm256_mul_epi32(previous, c));
                        }
                    };
                }
                tap!(0, s0);
                tap!(1, s1);
                tap!(2, s2);
                tap!(3, s3);
                tap!(4, s0);
                tap!(5, s1);
                tap!(6, s2);
                tap!(7, s3);
                tap!(8, s0);
                tap!(9, s1);
                tap!(10, s2);
                tap!(11, s3);
                tap!(12, s0);
                tap!(13, s1);
                tap!(14, s2);
                tap!(15, s3);
                tap!(16, s0);
                tap!(17, s1);
                tap!(18, s2);
                tap!(19, s3);
                tap!(20, s0);
                tap!(21, s1);
                tap!(22, s2);
                tap!(23, s3);
                tap!(24, s0);
                tap!(25, s1);
                tap!(26, s2);
                tap!(27, s3);
                tap!(28, s0);
                tap!(29, s1);
                tap!(30, s2);
                tap!(31, s3);
                let sum = _mm256_add_epi64(_mm256_add_epi64(s0, s1), _mm256_add_epi64(s2, s3));
                let mut partial = [0i64; 4];
                _mm256_storeu_si256(partial.as_mut_ptr().cast(), sum);
                // Lane a contains the terms from samples before this batch.
                // Add the a newly reconstructed samples before shifting, so
                // the original LPC rounding is preserved separately per output.
                let mut range_bits = 0u64;
                for a in 0..4 {
                    let mut prediction = partial[a];
                    for b in 0..a {
                        if N >= 16 {
                            prediction = prediction.wrapping_add(
                                coeff[a - b - 1].wrapping_mul((*base.add(i + b)).wide()),
                            );
                        } else {
                            prediction += coeff[a - b - 1] * (*base.add(i + b)).wide();
                        }
                    }
                    let value = if N >= 16 {
                        (*base.add(i + a)).wide().wrapping_add(prediction >> shift)
                    } else {
                        (*base.add(i + a)).wide() + (prediction >> shift)
                    };
                    if N >= 16 {
                        range_bits |= (value as u64).wrapping_add(bias);
                    } else {
                        check_sample(value, width)?;
                    }
                    *base.add(i + a) = T::store(value);
                }
                // Each biased value is in 0..2^width iff it fits. OR keeps any
                // invalid high bit set, checking every sample with one branch.
                // Arithmetic uses i64, wrapping only after an invalid value.
                // Narrow storage may truncate that invalid value, but its high
                // range bit is already latched. Reject the batch before reuse.
                if N >= 16 && range_bits >= (1u64 << width) {
                    return Err(Error::Invalid("sample outside declared bit depth"));
                }
                i += 4;
            }
            for i in i..output.len() {
                let mut prediction = 0i64;
                for j in 0..N {
                    prediction += coeff[j] * (*base.add(i - j - 1)).wide();
                }
                let value = (*base.add(i)).wide() + (prediction >> shift);
                check_sample(value, width)?;
                *base.add(i) = T::store(value);
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
                        if width <= 32 {
                            let narrow: Result<Vec<i32>, _> =
                                residual.iter().copied().map(i32::try_from).collect();
                            if let Ok(mut narrow) = narrow {
                                let mut scalar_narrow = narrow.clone();
                                scalar(&mut scalar_narrow, &coeff, shift, width).unwrap();
                                restore(&mut narrow, &coeff, shift, width).unwrap();
                                assert_eq!(narrow, scalar_narrow);
                                assert!(
                                    narrow
                                        .iter()
                                        .copied()
                                        .map(i64::from)
                                        .eq(pcm.iter().copied())
                                );
                            }
                        }
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
                    for position in 0..4 {
                        let mut residual = residual.clone();
                        residual[order + position] = sign;
                        let mut coeff = vec![0; order];
                        coeff[0] = 1;
                        let mut reference = residual.clone();
                        assert!(scalar(&mut reference, &coeff, 0, width).is_err());
                        if width <= 32 {
                            let mut narrow: Vec<i32> = residual
                                .iter()
                                .map(|&s| i32::try_from(s).unwrap())
                                .collect();
                            assert!(restore(&mut narrow, &coeff, 0, width).is_err());
                        }
                        assert!(restore(&mut residual, &coeff, 0, width).is_err());
                    }
                }
            }
        }
    }

    #[test]
    fn invalid_batch_cannot_overflow_before_range_rejection() {
        for order in 4..=32 {
            let mut residual = vec![0; 65];
            residual[..order].fill(i32::MAX as i64);
            let coeff = vec![16383; order];
            let mut narrow: Vec<i32> = residual.iter().map(|&s| s as i32).collect();
            assert!(restore(&mut narrow, &coeff, 0, 32).is_err());
            assert!(restore(&mut residual, &coeff, 0, 32).is_err());
        }
    }
}
