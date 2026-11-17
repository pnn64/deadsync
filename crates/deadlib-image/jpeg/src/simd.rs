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

/// A unit of hot work; `run` must be `#[inline(always)]`.
pub(crate) trait Kernel {
    type Output;
    fn run(self) -> Self::Output;
}

/// Run `kernel`, compiled with AVX2 when the CPU supports it.
#[inline(always)]
pub(crate) fn dispatch<K: Kernel>(kernel: K) -> K::Output {
    #[cfg(target_arch = "x86_64")]
    if !portable_forced() && std::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was detected on this CPU just above; `avx2`
        // only inlines the safe kernel into AVX2-enabled code.
        #[allow(unsafe_code)]
        return unsafe { avx2(kernel) };
    }
    kernel.run()
}

#[cfg(target_arch = "x86_64")]
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

/// The 8-bit float IDCT with explicit AVX2 operations, when available.
/// Returns `false` (writing nothing) without AVX2. `out` holds 8 rows of 8
/// bytes, `stride` apart.
#[inline(always)]
pub(crate) fn idct8(
    block: &[i16; 64],
    quant: &[f32; 64],
    constants: [f32; 4],
    out: &mut [u8],
    stride: usize,
) -> bool {
    #[cfg(target_arch = "x86_64")]
    if !portable_forced() && std::is_x86_feature_detected!("avx2") {
        // SAFETY: AVX2 support was detected just above. All loads read
        // fixed-size arrays and every store targets a checked 8-byte slice.
        #[allow(unsafe_code)]
        unsafe {
            x86::idct8(block, quant, constants, out, stride)
        };
        return true;
    }
    let _ = (block, quant, constants, out, stride);
    false
}

#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
mod x86 {
    use std::arch::x86_64::*;

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
    pub(super) unsafe fn idct8(
        block: &[i16; 64],
        quant: &[f32; 64],
        constants: [f32; 4],
        out: &mut [u8],
        stride: usize,
    ) {
        #[allow(clippy::redundant_closure)] // target-feature fns are not `Fn`
        let c = constants.map(|v| _mm256_set1_ps(v));
        let mut x = [_mm256_setzero_ps(); 8];
        for (v, row) in x.iter_mut().enumerate() {
            // SAFETY: rows 0..8 of 8 i16 / 8 f32 lie inside the 64-element arrays.
            let (coefficients, q) = unsafe {
                (
                    _mm_loadu_si128(block.as_ptr().add(v * 8).cast()),
                    _mm256_loadu_ps(quant.as_ptr().add(v * 8)),
                )
            };
            *row = _mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_cvtepi16_epi32(coefficients)), q);
        }
        // Columns first (lanes are columns), then the rows as transposed lanes.
        let mut work = transpose(aan(x, c));
        // Each row's DC term receives the level shift and rounding bias.
        work[0] = _mm256_add_ps(work[0], _mm256_set1_ps(128.5));
        let rows = transpose(aan(work, c));
        let (low, high) = (_mm256_setzero_ps(), _mm256_set1_ps(255.0));
        // max/min then truncation equals `v.clamp(0.0, 255.0) as u8`.
        let ints = rows.map(|r| _mm256_cvttps_epi32(_mm256_min_ps(_mm256_max_ps(r, low), high)));
        let order = |v: __m256i| _mm256_permute4x64_epi64::<0b11_01_10_00>(v);
        let pairs: [__m256i; 4] =
            core::array::from_fn(|i| order(_mm256_packs_epi32(ints[2 * i], ints[2 * i + 1])));
        let bytes = [
            order(_mm256_packus_epi16(pairs[0], pairs[1])),
            order(_mm256_packus_epi16(pairs[2], pairs[3])),
        ];
        for (half, value) in bytes.into_iter().enumerate() {
            let mut rows = [0u8; 32];
            // SAFETY: `rows` is exactly 32 bytes.
            unsafe { _mm256_storeu_si256(rows.as_mut_ptr().cast(), value) };
            for (i, row) in rows.as_chunks::<8>().0.iter().enumerate() {
                let y = half * 4 + i;
                out[y * stride..y * stride + 8].copy_from_slice(row);
            }
        }
    }
}
