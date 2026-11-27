//! Inverse Transform module.
//!
//! Reference spec: VP9 Bitstream & Decoding Process Specification v0.7, §8.7
//! "Inverse transform process".
//!
//! This module provides the following:
//! - Inverse DCT (4x4 / 8x8 / 16x16 / 32x32): `idct4/8/16/32`, a direct
//!   implementation of the permutation + butterfly network from spec §8.7.1.2-8.7.1.3.
//! - Inverse ADST (4x4 / 8x8 / 16x16): `iadst4/8/16` from spec §8.7.1.4-8.7.1.9.
//! - Inverse Walsh-Hadamard transform (4x4, for lossless): `iwht4` from spec §8.7.1.10.
//! - Assembly of the 2D transform (row transform -> column transform -> final
//!   shift): `inverse_transform_block` from spec §8.7.2.
//!
//! All transform intermediates are computed as `i64` with plenty of headroom
//! to hold the precision the spec requires (`8 + BitDepth` bits; the ADST `S`
//! array needs `24 + BitDepth` bits).

/// Spec §4.6 `Round2(x, n) = (x + (1 << (n-1))) >> n`.
///
/// Calls within this module always use `n >= 1` (the spec never calls this
/// with `n == 0`).
#[inline]
fn round2(x: i64, n: u32) -> i64 {
    debug_assert!(n >= 1);
    (x + (1i64 << (n - 1))) >> n
}

/// Spec §8.7.1.1 `cos64_lookup[ 33 ]`.
/// `cos64_lookup[i] == round(16384 * cos(i * pi / 64))`.
#[rustfmt::skip]
const COS64_LOOKUP: [i64; 33] = [
    16384, 16364, 16305, 16207, 16069, 15893, 15679, 15426,
    15137, 14811, 14449, 14053, 13623, 13160, 12665, 12140,
    11585, 11003, 10394, 9760, 9102, 8423, 7723, 7005,
    6270, 5520, 4756, 3981, 3196, 2404, 1606, 804,
    0,
];

/// Precomputed full-period `cos64(angle & 127)` table so `cos64`/`sin64` are single table
/// lookups with no runtime `rem_euclid` or branch chain.
const COS64_FULL: [i64; 128] = {
    let mut table = [0i64; 128];
    let mut a = 0usize;
    while a < 128 {
        table[a] = if a <= 32 {
            COS64_LOOKUP[a]
        } else if a <= 64 {
            -COS64_LOOKUP[64 - a]
        } else if a <= 96 {
            -COS64_LOOKUP[a - 64]
        } else {
            COS64_LOOKUP[128 - a]
        };
        a += 1;
    }
    table
};

/// Spec §8.7.1.1 `cos64( angle )`.
#[inline]
pub(crate) fn cos64(angle: i32) -> i64 {
    COS64_FULL[(angle & 127) as usize]
}

/// Spec §8.7.1.1 `sin64( angle ) = cos64( angle - 32 )`.
#[inline]
pub(crate) fn sin64(angle: i32) -> i64 {
    COS64_FULL[((angle - 32) & 127) as usize]
}

/// Spec §8.7.1.1 `brev(numBits, x)` (bit reversal).
#[inline]
pub(crate) fn brev(num_bits: u32, x: usize) -> usize {
    if num_bits == 0 {
        0
    } else {
        x.reverse_bits() >> (usize::BITS - num_bits)
    }
}

/// Spec §8.7.1.1 `B( a, b, angle, flip )` butterfly rotation.
fn b_op(t: &mut [i64], a: usize, b: usize, angle: i32, flip: bool) {
    let ta = t[a];
    let tb = t[b];
    let x = ta * cos64(angle) - tb * sin64(angle);
    let y = ta * sin64(angle) + tb * cos64(angle);
    t[a] = round2(x, 14);
    t[b] = round2(y, 14);
    if flip {
        t.swap(a, b);
    }
}

/// Spec §8.7.1.1 `H( a, b, flip )` Hadamard rotation.
fn h_op(t: &mut [i64], a: usize, b: usize, flip: bool) {
    let (a, b) = if flip { (b, a) } else { (a, b) };
    let x = t[a];
    let y = t[b];
    t[a] = x + y;
    t[b] = x - y;
}

/// Spec §8.7.1.1 `SB( a, b, angle, flip )` (rotation into the high-precision `S` array).
fn sb_op(s: &mut [i64], t: &[i64], a: usize, b: usize, angle: i32, flip: bool) {
    let ta = t[a];
    let tb = t[b];
    let sa = ta * cos64(angle) - tb * sin64(angle);
    let sb = ta * sin64(angle) + tb * cos64(angle);
    if flip {
        s[a] = sb;
        s[b] = sa;
    } else {
        s[a] = sa;
        s[b] = sb;
    }
}

/// Spec §8.7.1.1 `SH( a, b )`.
fn sh_op(t: &mut [i64], s: &[i64], a: usize, b: usize) {
    t[a] = round2(s[a] + s[b], 14);
    t[b] = round2(s[a] - s[b], 14);
}

/// Spec §8.7.1.2: input array reordering for the inverse DCT (bit-reversal permutation).
/// Generic over `T: Copy` so both scalar (`i64`) and AVX2 (`__m256i`) networks share one copy.
#[inline]
pub(crate) fn idct_permute<T: Copy>(t: &mut [T], n: u32) {
    let n0 = 1usize << n;
    let mut copy_t = [t[0]; 32];
    copy_t[..n0].copy_from_slice(&t[..n0]);
    for (i, dst) in t[..n0].iter_mut().enumerate() {
        *dst = copy_t[brev(n, i)];
    }
}

/// Shared inverse-DCT butterfly network body (spec §8.7.1.3), expanded identically into the
/// scalar `i64` driver and the AVX2 `i32`/`i64` SIMD drivers (preserving `#[target_feature]`
/// inlining with zero abstraction overhead).
macro_rules! idct_body {
    ($t:expr, $n:expr, $b_op:expr, $h_op:expr, $recurse:expr) => {{
        let n = $n;
        let t = $t;
        let n0 = 1i64 << n;
        let n1 = 1i64 << (n - 1);
        let n2 = 1i64 << (n - 2);

        if n == 2 {
            $b_op(t, 0, 1, 16, true);
        } else {
            $recurse(t, n - 1);
        }

        for i in 0..n2 {
            let a = (n1 + i) as usize;
            let b = (n0 - 1 - i) as usize;
            let angle = 32 - $crate::transform::brev(5, a) as i32;
            $b_op(t, a, b, angle, false);
        }

        if n >= 3 {
            let n3 = 1i64 << (n - 3);
            for i in 0..n3 {
                for j in 0..2i64 {
                    let a = (n1 + 4 * i + 2 * j) as usize;
                    let b = (n1 + 1 + 4 * i + 2 * j) as usize;
                    $h_op(t, a, b, j == 1);
                }
            }
        }

        if n == 5 {
            for i in 0..2i64 {
                for j in 0..2i64 {
                    let a = (n0 - n as i64 + 3 - n2 * j - 4 * i) as usize;
                    let b = (n1 + n as i64 - 4 + n2 * j + 4 * i) as usize;
                    let angle = 28 - 16 * i as i32 + 56 * j as i32;
                    $b_op(t, a, b, angle, true);
                }
            }
            let n3 = 1i64 << (n - 3);
            for i in 0..2i64 {
                for j in 0..4i64 {
                    let a = (n1 + n3 * j + i) as usize;
                    let b = (n1 + n2 - 5 + n3 * j - i) as usize;
                    $h_op(t, a, b, (j & 1) == 1);
                }
            }
        }

        if n >= 4 {
            let imax_a: i64 = if n == 5 { 1 } else { 0 };
            for i in 0..=imax_a {
                for j in 0..2i64 {
                    let a = (n0 - n as i64 + 2 - i - n2 * j) as usize;
                    let b = (n1 + n as i64 - 3 + i + n2 * j) as usize;
                    let angle = 24 + 48 * j as i32;
                    $b_op(t, a, b, angle, true);
                }
            }
            let imax_b: i64 = 2 * n as i64 - 7;
            for j in 0..2i64 {
                for i in 0..=imax_b {
                    let a = (n1 + n2 * j + i) as usize;
                    let b = (n1 + n2 - 1 + n2 * j - i) as usize;
                    $h_op(t, a, b, (j & 1) == 1);
                }
            }
        }

        if n >= 3 {
            let n3 = 1i64 << (n - 3);
            for i in 0..n3 {
                let a = (n0 - n3 - 1 - i) as usize;
                let b = (n1 + n3 + i) as usize;
                $b_op(t, a, b, 16, true);
            }
        }

        for i in 0..n1 {
            let a = i as usize;
            let b = (n0 - 1 - i) as usize;
            $h_op(t, a, b, false);
        }
    }};
}
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
pub(crate) use idct_body;

/// Spec §8.7.1.3: the inverse DCT butterfly network itself (`2 <= n <= 5`).
///
/// Assumes [`idct_permute`] has already been applied before this is called.
pub(crate) fn idct(t: &mut [i64], n: u32) {
    idct_body!(t, n, b_op, h_op, idct);
}

/// Spec §8.7.1.4: input array reordering for the inverse ADST (`n` is 3 or 4).
#[inline]
pub(crate) fn adst_input_permute<T: Copy>(t: &mut [T], n: u32) {
    let n0 = 1usize << n;
    let n1 = 1usize << (n - 1);
    let mut copy_t = [t[0]; 16];
    copy_t[..n0].copy_from_slice(&t[..n0]);
    for i in 0..n1 {
        t[2 * i] = copy_t[n0 - 1 - 2 * i];
        t[2 * i + 1] = copy_t[2 * i];
    }
}

const ADST_OUT_PERM_8: [u8; 8] = {
    let mut p = [0u8; 8];
    let mut i = 0;
    while i < 8 {
        let (a, b, c) = ((i >> 2) & 1, (i >> 1) & 1, i & 1);
        p[i] = (4 * (c ^ b) + 2 * (b ^ a) + a) as u8;
        i += 1;
    }
    p
};

const ADST_OUT_PERM_16: [u8; 16] = {
    let mut p = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        let (a, b, c, d) = ((i >> 3) & 1, (i >> 2) & 1, (i >> 1) & 1, i & 1);
        p[i] = (8 * (d ^ c) + 4 * (c ^ b) + 2 * (b ^ a) + a) as u8;
        i += 1;
    }
    p
};

/// Spec §8.7.1.5: output array reordering for the inverse ADST (`n` is 3 or 4).
#[inline]
pub(crate) fn adst_output_permute<T: Copy>(t: &mut [T], n: u32) {
    let perm: &[u8] = if n == 4 {
        &ADST_OUT_PERM_16
    } else {
        debug_assert_eq!(n, 3);
        &ADST_OUT_PERM_8
    };
    let mut copy_t = [t[0]; 16];
    copy_t[..perm.len()].copy_from_slice(&t[..perm.len()]);
    for (dst, &src) in perm.iter().enumerate() {
        t[dst] = copy_t[src as usize];
    }
}

/// Spec §8.7.1.6 constants `SINPI_k_9`.
pub(crate) const SINPI_1_9: i64 = 5283;
pub(crate) const SINPI_2_9: i64 = 9929;
pub(crate) const SINPI_3_9: i64 = 13377;
pub(crate) const SINPI_4_9: i64 = 15212;

/// Shared inverse ADST4 body (spec §8.7.1.6) for scalar `i64` and AVX2 `i32`/`i64` drivers.
macro_rules! iadst4_body {
    ($t:expr, $mul:expr, $add:expr, $sub:expr, $round14:expr) => {{
        use $crate::transform::{SINPI_1_9, SINPI_2_9, SINPI_3_9, SINPI_4_9};
        let t = $t;
        let s0 = $mul(t[0], SINPI_1_9);
        let s1 = $mul(t[0], SINPI_2_9);
        let s2 = $mul(t[1], SINPI_3_9);
        let s3 = $mul(t[2], SINPI_4_9);
        let s4 = $mul(t[2], SINPI_1_9);
        let s5 = $mul(t[3], SINPI_2_9);
        let s6 = $mul(t[3], SINPI_4_9);
        let v = $add($sub(t[0], t[2]), t[3]);
        let s7 = $mul(v, SINPI_3_9);
        let x0 = $add($add(s0, s3), s5);
        let x1 = $sub($sub(s1, s4), s6);
        let o0 = $add(x0, s2);
        let o1 = $add(x1, s2);
        let o3 = $sub($add(x0, x1), s2);
        t[0] = $round14(o0);
        t[1] = $round14(o1);
        t[2] = $round14(s7);
        t[3] = $round14(o3);
    }};
}
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
pub(crate) use iadst4_body;

/// Shared inverse ADST8 body (spec §8.7.1.7) for scalar `i64` and AVX2 `i32`/`i64` drivers.
macro_rules! iadst8_body {
    ($t:expr, $zero:expr, $sb_op:expr, $sh_op:expr, $h_op:expr, $b_op:expr, $neg:expr) => {{
        let t = $t;
        $crate::transform::adst_input_permute(t, 3);
        let mut s = [$zero; 8];
        for i in 0..4usize {
            $sb_op(&mut s, t, 2 * i, 1 + 2 * i, 30 - 8 * i as i32, true);
        }
        for i in 0..4usize {
            $sh_op(t, &s, i, 4 + i);
        }
        for i in 0..2usize {
            $sb_op(&mut s, t, 4 + 3 * i, 5 + i, 24 - 16 * i as i32, true);
        }
        for i in 0..2usize {
            $sh_op(t, &s, 4 + i, 6 + i);
        }
        for i in 0..2usize {
            $h_op(t, i, 2 + i, false);
        }
        for i in 0..2usize {
            $b_op(t, 2 + 4 * i, 3 + 4 * i, 16, true);
        }
        $crate::transform::adst_output_permute(t, 3);
        for i in 0..4usize {
            t[1 + 2 * i] = $neg(t[1 + 2 * i]);
        }
    }};
}
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
pub(crate) use iadst8_body;

/// Shared inverse ADST16 body (spec §8.7.1.8) for scalar `i64` and AVX2 `i32`/`i64` drivers.
macro_rules! iadst16_body {
    ($t:expr, $zero:expr, $sb_op:expr, $sh_op:expr, $h_op:expr, $b_op:expr, $neg:expr) => {{
        let t = $t;
        $crate::transform::adst_input_permute(t, 4);
        let mut s = [$zero; 16];
        for i in 0..8usize {
            $sb_op(&mut s, t, 2 * i, 1 + 2 * i, 31 - 4 * i as i32, true);
        }
        for i in 0..8usize {
            $sh_op(t, &s, i, 8 + i);
        }
        for i in 0..4usize {
            $sb_op(&mut s, t, 8 + 2 * i, 9 + 2 * i, 28 - 16 * i as i32, true);
        }
        for i in 0..4usize {
            $sh_op(t, &s, 8 + i, 12 + i);
        }
        for i in 0..4usize {
            $h_op(t, i, 4 + i, false);
        }
        for i in 0..2usize {
            for j in 0..2usize {
                $sb_op(
                    &mut s,
                    t,
                    4 + 8 * i + 3 * j,
                    5 + 8 * i + j,
                    24 - 16 * j as i32,
                    true,
                );
            }
        }
        for i in 0..2usize {
            for j in 0..2usize {
                $sh_op(t, &s, 4 + 8 * j + i, 6 + 8 * j + i);
            }
        }
        for i in 0..2usize {
            for j in 0..2usize {
                $h_op(t, 8 * j + i, 2 + 8 * j + i, false);
            }
        }
        for i in 0..2usize {
            for j in 0..2usize {
                let angle = 48 + 64 * (i ^ j) as i32;
                $b_op(t, 2 + 4 * j + 8 * i, 3 + 4 * j + 8 * i, angle, false);
            }
        }
        $crate::transform::adst_output_permute(t, 4);
        for i in 0..2usize {
            for j in 0..2usize {
                t[1 + 12 * j + 2 * i] = $neg(t[1 + 12 * j + 2 * i]);
            }
        }
    }};
}
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
pub(crate) use iadst16_body;

/// Spec §8.7.1.6: inverse ADST4 body.
fn iadst4_impl(t: &mut [i64]) {
    iadst4_body!(
        t,
        |v: i64, c: i64| v * c,
        |a: i64, b: i64| a + b,
        |a: i64, b: i64| a - b,
        |v: i64| round2(v, 14)
    );
}

/// Spec §8.7.1.7: inverse ADST8 body.
fn iadst8_impl(t: &mut [i64]) {
    iadst8_body!(t, 0i64, sb_op, sh_op, h_op, b_op, |v: i64| -v);
}

/// Spec §8.7.1.8: inverse ADST16 body.
fn iadst16_impl(t: &mut [i64]) {
    iadst16_body!(t, 0i64, sb_op, sh_op, h_op, b_op, |v: i64| -v);
}

/// Spec §8.7.1.9: selects ADST4/8/16 based on `n` (for internal dispatch).
fn iadst(t: &mut [i64], n: u32) {
    match n {
        2 => iadst4_impl(t),
        3 => iadst8_impl(t),
        4 => iadst16_impl(t),
        _ => unreachable!("iadst only supports n = 2..=4"),
    }
}

/// Spec §8.7.1.10: inverse Walsh-Hadamard transform (lossless, 4x4 only).
///
/// `shift` should be 2 for the row transform and 0 for the column transform
/// (spec §8.7.2).
pub fn iwht4(t: &mut [i64; 4], shift: u32) {
    let mut a = t[0] >> shift;
    let mut c = t[1] >> shift;
    let mut d = t[2] >> shift;
    let mut b = t[3] >> shift;
    a += c;
    d -= b;
    let e = (a - d) >> 1;
    b = e - b;
    c = e - c;
    a -= b;
    d += c;
    t[0] = a;
    t[1] = b;
    t[2] = c;
    t[3] = d;
}

/// Spec §8.7.2 `TxType` (named in column_row order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxType {
    /// Column: DCT / Row: DCT
    DctDct,
    /// Column: ADST / Row: DCT
    AdstDct,
    /// Column: DCT / Row: ADST
    DctAdst,
    /// Column: ADST / Row: ADST
    AdstAdst,
}

/// Spec §8.7.2: 2D inverse transform.
///
/// `dequant` is a row-major `n0 x n0` (`n0 = 1 << n`) array of dequantized
/// coefficients; the transformed result overwrites this same array.
/// If `lossless` is true, the inverse WHT (`n == 2` only) is used regardless
/// of `tx_type`.
///
/// # Panics
/// Panics if `dequant.len() != (1 << n) * (1 << n)`, or if `lossless == true`
/// and `n != 2`.
pub fn inverse_transform_block(dequant: &mut [i64], n: u32, tx_type: TxType, lossless: bool) {
    let n0 = 1usize << n;
    assert_eq!(
        dequant.len(),
        n0 * n0,
        "dequant size does not match n0 x n0"
    );
    if lossless {
        assert_eq!(n, 2, "lossless transform (WHT) only supports 4x4");
        let mut t = [0i64; 4];
        for i in 0..4 {
            t.copy_from_slice(&dequant[i * 4..(i + 1) * 4]);
            iwht4(&mut t, 2);
            dequant[i * 4..(i + 1) * 4].copy_from_slice(&t);
        }
        for j in 0..4 {
            for (i, slot) in t.iter_mut().enumerate() {
                *slot = dequant[i * 4 + j];
            }
            iwht4(&mut t, 0);
            for (i, &val) in t.iter().enumerate() {
                dequant[i * 4 + j] = val;
            }
        }
        return;
    }

    // Fixed-size scratch (n0 <= 32, the 32x32 max transform), reused for every row and
    // every column below instead of a per-block heap allocation.
    let mut t_storage = [0i64; 32];
    let t = &mut t_storage[..n0];

    // Row transform: i = 0..(n0-1).
    for i in 0..n0 {
        t[..n0].copy_from_slice(&dequant[i * n0..(i + 1) * n0]);
        match tx_type {
            TxType::DctDct | TxType::AdstDct => {
                idct_permute(t, n);
                idct(t, n);
            }
            TxType::DctAdst | TxType::AdstAdst => {
                iadst(t, n);
            }
        }
        dequant[i * n0..(i + 1) * n0].copy_from_slice(&t[..n0]);
    }

    // Column transform: j = 0..(n0-1).
    let shift = (n + 2).min(6);
    for j in 0..n0 {
        for i in 0..n0 {
            t[i] = dequant[i * n0 + j];
        }
        match tx_type {
            TxType::DctDct | TxType::DctAdst => {
                idct_permute(t, n);
                idct(t, n);
            }
            TxType::AdstDct | TxType::AdstAdst => {
                iadst(t, n);
            }
        }
        for i in 0..n0 {
            dequant[i * n0 + j] = round2(t[i], shift);
        }
    }
}
