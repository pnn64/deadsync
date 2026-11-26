//! Runtime CPU dispatch, the only module that may use `unsafe`.
//!
//! Hot kernels are written once as portable Rust shaped for vectorization.
//! [`dispatch`] runs a [`Kernel`] either as compiled for the baseline target or
//! inlined into a wrapper compiled with AVX2, where LLVM vectorizes the same
//! operations with 256-bit registers. `Kernel::run` must be `#[inline(always)]`
//! (as must everything hot beneath it): closures proved unreliable, since LLVM
//! declined to inline a large closure body and compiled it for the baseline. Both versions
//! evaluate identical integer and IEEE single-precision operations (AVX2 does
//! not enable FMA contraction), so their output is bit-identical.
//!
//! Building with `--cfg deadlib_image_jpeg_portable` compiles dispatch out, to
//! measure or fuzz the baseline kernels on an AVX2-capable machine.

/// A unit of hot work; `run` must be `#[inline(always)]`.
pub(crate) trait Kernel {
    type Output;
    fn run(self) -> Self::Output;
}

/// Run `kernel`, compiled with AVX2 when the CPU supports it.
#[inline(always)]
pub(crate) fn dispatch<K: Kernel>(kernel: K) -> K::Output {
    #[cfg(all(target_arch = "x86_64", not(deadlib_image_jpeg_portable)))]
    if !portable_forced() && std::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was detected on this CPU just above; `avx2`
        // only inlines the safe kernel into AVX2-enabled code.
        #[allow(unsafe_code)]
        return unsafe { avx2(kernel) };
    }
    kernel.run()
}

#[cfg(all(target_arch = "x86_64", not(deadlib_image_jpeg_portable)))]
#[target_feature(enable = "avx2")]
fn avx2<K: Kernel>(kernel: K) -> K::Output {
    kernel.run()
}

#[cfg(test)]
thread_local! {
    static PORTABLE: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

/// Run `f` with dispatch restricted to the baseline target on this thread.
#[cfg(test)]
pub(crate) fn portable<R>(f: impl FnOnce() -> R) -> R {
    PORTABLE.with(|flag| flag.set(true));
    let result = f();
    PORTABLE.with(|flag| flag.set(false));
    result
}

#[cfg(any(test, all(target_arch = "x86_64", not(deadlib_image_jpeg_portable))))]
#[inline(always)]
fn portable_forced() -> bool {
    #[cfg(test)]
    {
        PORTABLE.with(|flag| flag.get())
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Find the next JPEG marker prefix, or the length if there is none.
pub(crate) fn find_ff(bytes: &[u8]) -> usize {
    #[cfg(all(target_arch = "x86_64", not(deadlib_image_jpeg_portable)))]
    if bytes.len() >= 32 && !portable_forced() && std::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 is available and the kernel loads complete chunks only.
        #[allow(unsafe_code)]
        return unsafe { x86::find_ff(bytes) };
    }
    find_ff_word(bytes)
}

fn find_ff_word(bytes: &[u8]) -> usize {
    let (words, tail) = bytes.as_chunks::<8>();
    for (i, &word) in words.iter().enumerate() {
        let inverted = !u64::from_le_bytes(word);
        let found =
            inverted.wrapping_sub(0x0101_0101_0101_0101) & !inverted & 0x8080_8080_8080_8080;
        // Borrows carry up, so the lowest flagged lane is a real match.
        if found != 0 {
            return i * 8 + found.trailing_zeros() as usize / 8;
        }
    }
    words.len() * 8 + tail.iter().position(|&v| v == 0xff).unwrap_or(tail.len())
}

/// The 8-bit float IDCT with explicit AVX2 operations, when available.
/// Returns `false` (writing nothing) without AVX2. `out` holds 8 rows of 8
/// bytes, `stride` apart.
#[inline(always)]
pub(crate) fn idct8(block: &[i16; 64], quant: &[f32; 64], out: &mut [u8], stride: usize) -> bool {
    #[cfg(all(target_arch = "x86_64", not(deadlib_image_jpeg_portable)))]
    if !portable_forced() && std::is_x86_feature_detected!("avx2") {
        assert!(
            stride
                .checked_mul(7)
                .and_then(|n| n.checked_add(8))
                .is_some_and(|span| span <= out.len())
        );
        // SAFETY: AVX2 is available, and the check bounds the last and
        // all earlier 8-byte row stores within the mutable slice.
        #[allow(unsafe_code)]
        unsafe {
            x86::idct8(block, quant, out.as_mut_ptr(), stride)
        };
        return true;
    }
    let _ = (block, quant, out, stride);
    false
}

#[cfg(all(target_arch = "x86_64", not(deadlib_image_jpeg_portable)))]
#[allow(unsafe_code)]
mod x86 {
    use crate::transform::{C2, C2_MINUS_C6, C2_PLUS_C6, R2};
    use std::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn find_ff(bytes: &[u8]) -> usize {
        let (chunks, tail) = bytes.as_chunks::<32>();
        let ff = _mm256_set1_epi8(-1);
        for (i, chunk) in chunks.iter().enumerate() {
            // SAFETY: each chunk contains 32 readable bytes.
            let block = unsafe { _mm256_loadu_si256(chunk.as_ptr().cast()) };
            let matches = _mm256_movemask_epi8(_mm256_cmpeq_epi8(block, ff)) as u32;
            if matches != 0 {
                return i * 32 + matches.trailing_zeros() as usize;
            }
        }
        chunks.len() * 32 + super::find_ff_word(tail)
    }

    /// One jidctflt pass over eight lanes, in the portable operation order.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn aan(x: [__m256; 8], c: [__m256; 4]) -> [__m256; 8] {
        let [r2, c2, c2_minus_c6, c2_plus_c6] = c;
        let tmp10 = _mm256_add_ps(x[0], x[4]);
        let tmp11 = _mm256_sub_ps(x[0], x[4]);
        let tmp13 = _mm256_add_ps(x[2], x[6]);
        let tmp12 = _mm256_sub_ps(_mm256_mul_ps(_mm256_sub_ps(x[2], x[6]), r2), tmp13);
        let tmp0 = _mm256_add_ps(tmp10, tmp13);
        let tmp3 = _mm256_sub_ps(tmp10, tmp13);
        let tmp1 = _mm256_add_ps(tmp11, tmp12);
        let tmp2 = _mm256_sub_ps(tmp11, tmp12);
        let z13 = _mm256_add_ps(x[5], x[3]);
        let z10 = _mm256_sub_ps(x[5], x[3]);
        let z11 = _mm256_add_ps(x[1], x[7]);
        let z12 = _mm256_sub_ps(x[1], x[7]);
        let tmp7 = _mm256_add_ps(z11, z13);
        let tmp11 = _mm256_mul_ps(_mm256_sub_ps(z11, z13), r2);
        let z5 = _mm256_mul_ps(_mm256_add_ps(z10, z12), c2);
        let tmp10 = _mm256_sub_ps(z5, _mm256_mul_ps(z12, c2_minus_c6));
        let tmp12 = _mm256_sub_ps(z5, _mm256_mul_ps(z10, c2_plus_c6));
        let tmp6 = _mm256_sub_ps(tmp12, tmp7);
        let tmp5 = _mm256_sub_ps(tmp11, tmp6);
        let tmp4 = _mm256_sub_ps(tmp10, tmp5);
        [
            _mm256_add_ps(tmp0, tmp7),
            _mm256_add_ps(tmp1, tmp6),
            _mm256_add_ps(tmp2, tmp5),
            _mm256_add_ps(tmp3, tmp4),
            _mm256_sub_ps(tmp3, tmp4),
            _mm256_sub_ps(tmp2, tmp5),
            _mm256_sub_ps(tmp1, tmp6),
            _mm256_sub_ps(tmp0, tmp7),
        ]
    }

    /// The AAN pass when the four upper frequencies are zero.
    #[inline]
    #[target_feature(enable = "avx2")]
    fn aan_half(x: [__m256; 4], c: [__m256; 4]) -> [__m256; 8] {
        let [r2, c2, c2_minus_c6, c2_plus_c6] = c;
        let even = _mm256_sub_ps(_mm256_mul_ps(x[2], r2), x[2]);
        let tmp0 = _mm256_add_ps(x[0], x[2]);
        let tmp3 = _mm256_sub_ps(x[0], x[2]);
        let tmp1 = _mm256_add_ps(x[0], even);
        let tmp2 = _mm256_sub_ps(x[0], even);
        let diff = _mm256_sub_ps(x[1], x[3]);
        let tmp7 = _mm256_add_ps(x[1], x[3]);
        let tmp11 = _mm256_mul_ps(diff, r2);
        let z5 = _mm256_mul_ps(diff, c2);
        let tmp10 = _mm256_sub_ps(z5, _mm256_mul_ps(x[1], c2_minus_c6));
        let tmp12 = _mm256_add_ps(z5, _mm256_mul_ps(x[3], c2_plus_c6));
        let tmp6 = _mm256_sub_ps(tmp12, tmp7);
        let tmp5 = _mm256_sub_ps(tmp11, tmp6);
        let tmp4 = _mm256_sub_ps(tmp10, tmp5);
        [
            _mm256_add_ps(tmp0, tmp7),
            _mm256_add_ps(tmp1, tmp6),
            _mm256_add_ps(tmp2, tmp5),
            _mm256_add_ps(tmp3, tmp4),
            _mm256_sub_ps(tmp3, tmp4),
            _mm256_sub_ps(tmp2, tmp5),
            _mm256_sub_ps(tmp1, tmp6),
            _mm256_sub_ps(tmp0, tmp7),
        ]
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn quarter_support(block: &[i16; 64]) -> bool {
        let (words, _) = block.as_chunks::<16>();
        // SAFETY: each pointer addresses a full 32-byte chunk in the block.
        let [a, b, c, d] = unsafe {
            [
                _mm256_loadu_si256(words[0].as_ptr().cast()),
                _mm256_loadu_si256(words[1].as_ptr().cast()),
                _mm256_loadu_si256(words[2].as_ptr().cast()),
                _mm256_loadu_si256(words[3].as_ptr().cast()),
            ]
        };
        let upper = _mm256_or_si256(a, b);
        let right = _mm256_and_si256(upper, _mm256_setr_epi64x(0, -1, 0, -1));
        let outside = _mm256_or_si256(right, _mm256_or_si256(c, d));
        _mm256_testz_si256(outside, outside) != 0
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn dequantize<const ROWS: usize>(block: &[i16; 64], quant: &[f32; 64]) -> [__m256; ROWS] {
        assert!(ROWS <= 8);
        let mut x = [_mm256_setzero_ps(); ROWS];
        for (v, row) in x.iter_mut().enumerate() {
            // SAFETY: rows 0..ROWS of 8 i16 / 8 f32 lie inside both arrays.
            let (coefficients, q) = unsafe {
                (
                    _mm_loadu_si128(block.as_ptr().add(v * 8).cast()),
                    _mm256_loadu_ps(quant.as_ptr().add(v * 8)),
                )
            };
            *row = _mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_cvtepi16_epi32(coefficients)), q);
        }
        x
    }

    #[inline]
    #[target_feature(enable = "avx2")]
    fn transpose(r: [__m256; 8]) -> [__m256; 8] {
        let t0 = _mm256_unpacklo_ps(r[0], r[1]);
        let t1 = _mm256_unpackhi_ps(r[0], r[1]);
        let t2 = _mm256_unpacklo_ps(r[2], r[3]);
        let t3 = _mm256_unpackhi_ps(r[2], r[3]);
        let t4 = _mm256_unpacklo_ps(r[4], r[5]);
        let t5 = _mm256_unpackhi_ps(r[4], r[5]);
        let t6 = _mm256_unpacklo_ps(r[6], r[7]);
        let t7 = _mm256_unpackhi_ps(r[6], r[7]);
        let u0 = _mm256_shuffle_ps::<0x44>(t0, t2);
        let u1 = _mm256_shuffle_ps::<0xee>(t0, t2);
        let u2 = _mm256_shuffle_ps::<0x44>(t1, t3);
        let u3 = _mm256_shuffle_ps::<0xee>(t1, t3);
        let u4 = _mm256_shuffle_ps::<0x44>(t4, t6);
        let u5 = _mm256_shuffle_ps::<0xee>(t4, t6);
        let u6 = _mm256_shuffle_ps::<0x44>(t5, t7);
        let u7 = _mm256_shuffle_ps::<0xee>(t5, t7);
        [
            _mm256_permute2f128_ps::<0x20>(u0, u4),
            _mm256_permute2f128_ps::<0x20>(u1, u5),
            _mm256_permute2f128_ps::<0x20>(u2, u6),
            _mm256_permute2f128_ps::<0x20>(u3, u7),
            _mm256_permute2f128_ps::<0x31>(u0, u4),
            _mm256_permute2f128_ps::<0x31>(u1, u5),
            _mm256_permute2f128_ps::<0x31>(u2, u6),
            _mm256_permute2f128_ps::<0x31>(u3, u7),
        ]
    }

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn idct8(block: &[i16; 64], quant: &[f32; 64], out: *mut u8, stride: usize) {
        #[allow(clippy::redundant_closure)] // target-feature fns are not `Fn`
        let c = [R2, C2, C2_MINUS_C6, C2_PLUS_C6].map(|v| _mm256_set1_ps(v));
        let bias = _mm256_set1_ps(128.5);
        // Columns first (lanes are columns), then rows as transposed lanes.
        let rows = if quarter_support(block) {
            let work = transpose(aan_half(dequantize::<4>(block, quant), c));
            let left = [_mm256_add_ps(work[0], bias), work[1], work[2], work[3]];
            transpose(aan_half(left, c))
        } else {
            let mut work = transpose(aan(dequantize::<8>(block, quant), c));
            work[0] = _mm256_add_ps(work[0], bias);
            transpose(aan(work, c))
        };
        let high = _mm256_set1_ps(255.0);
        // The unsigned pack clips negatives after signed saturation. Clip
        // positives before float-to-int conversion to handle large samples.
        let ints = rows.map(|r| _mm256_cvttps_epi32(_mm256_min_ps(r, high)));
        let pairs: [__m256i; 4] =
            core::array::from_fn(|i| _mm256_packs_epi32(ints[2 * i], ints[2 * i + 1]));
        // Both packs act within each 128-bit lane. The first lane has the
        // first four pixels of each row, and the second has the last four.
        let order = _mm256_setr_epi32(0, 4, 1, 5, 2, 6, 3, 7);
        let bytes = [
            _mm256_permutevar8x32_epi32(_mm256_packus_epi16(pairs[0], pairs[1]), order),
            _mm256_permutevar8x32_epi32(_mm256_packus_epi16(pairs[2], pairs[3]), order),
        ];
        for (i, half) in bytes.into_iter().enumerate() {
            let low = _mm256_castsi256_si128(half);
            let high = _mm256_extracti128_si256::<1>(half);
            // SAFETY: every destination is an eight-byte row within the
            // caller's checked output span. The stores allow unaligned rows.
            unsafe {
                let row = out.add(i * 4 * stride);
                _mm_storel_epi64(row.cast(), low);
                _mm_storel_epi64(row.add(stride).cast(), _mm_srli_si128::<8>(low));
                _mm_storel_epi64(row.add(2 * stride).cast(), high);
                _mm_storel_epi64(row.add(3 * stride).cast(), _mm_srli_si128::<8>(high));
            }
        }
    }
}
