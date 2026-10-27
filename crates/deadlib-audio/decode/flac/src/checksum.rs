//! FLAC CRC polynomials and RFC 1321 MD5, implemented without dependencies.
const fn crc8_table() -> [u8; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut x = i as u8;
        let mut j = 0;
        while j < 8 {
            x = (x << 1) ^ if x & 0x80 != 0 { 7 } else { 0 };
            j += 1;
        }
        table[i] = x;
        i += 1;
    }
    table
}
const fn crc16_table() -> [u16; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut x = (i as u16) << 8;
        let mut j = 0;
        while j < 8 {
            x = (x << 1) ^ if x & 0x8000 != 0 { 0x8005 } else { 0 };
            j += 1;
        }
        table[i] = x;
        i += 1;
    }
    table
}
const CRC8: [u8; 256] = crc8_table();
const CRC16: [u16; 256] = crc16_table();
const fn crc16_slices() -> [[u16; 256]; 8] {
    let mut tables = [[0; 256]; 8];
    tables[0] = CRC16;
    let mut slice = 1;
    while slice < 8 {
        let mut byte = 0;
        while byte < 256 {
            let previous = tables[slice - 1][byte];
            tables[slice][byte] = (previous << 8) ^ CRC16[(previous >> 8) as usize];
            byte += 1;
        }
        slice += 1;
    }
    tables
}
const CRC16_SLICES: [[u16; 256]; 8] = crc16_slices();
pub fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0, |crc, &x| CRC8[(crc ^ x) as usize])
}
pub fn crc16(data: &[u8]) -> u16 {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if data.len() >= 256
        && std::is_x86_feature_detected!("pclmulqdq")
        && std::is_x86_feature_detected!("ssse3")
    {
        return x86::crc16(data);
    }
    crc16_update(0, data)
}

fn crc16_update(mut crc: u16, data: &[u8]) -> u16 {
    let (chunks, tail) = data.as_chunks::<8>();
    for b in chunks {
        crc = CRC16_SLICES[7][((crc >> 8) as u8 ^ b[0]) as usize]
            ^ CRC16_SLICES[6][(crc as u8 ^ b[1]) as usize]
            ^ CRC16_SLICES[5][b[2] as usize]
            ^ CRC16_SLICES[4][b[3] as usize]
            ^ CRC16_SLICES[3][b[4] as usize]
            ^ CRC16_SLICES[2][b[5] as usize]
            ^ CRC16_SLICES[1][b[6] as usize]
            ^ CRC16_SLICES[0][b[7] as usize];
    }
    for &byte in tail {
        crc = (crc << 8) ^ CRC16[((crc >> 8) as u8 ^ byte) as usize];
    }
    crc
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[allow(unsafe_code)]
mod x86 {
    use super::*;
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    // x^n modulo x^16 + x^15 + x^2 + 1. The constants are derived here,
    // rather than relying on reflected-CRC tables or external folding values.
    const fn power(n: usize) -> i64 {
        let mut value = 1u16;
        let mut i = 0;
        while i < n {
            value = (value << 1) ^ if value & 0x8000 != 0 { 0x8005 } else { 0 };
            i += 1;
        }
        value as i64
    }

    pub(super) fn crc16(data: &[u8]) -> u16 {
        assert!(data.len() >= 64);
        assert!(std::is_x86_feature_detected!("pclmulqdq"));
        assert!(std::is_x86_feature_detected!("ssse3"));
        // SAFETY: instruction sets are checked above. The kernel loads only
        // checked 16-byte fields within complete 64-byte chunks.
        unsafe { folded(data) }
    }

    #[target_feature(enable = "pclmulqdq,ssse3")]
    unsafe fn folded(data: &[u8]) -> u16 {
        // SAFETY: every load covers one initialized [u8;16]. The sole store
        // targets a local [u8;16]. Unaligned intrinsics require no extra padding.
        unsafe {
            let reverse = _mm_setr_epi8(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0);
            let k512 = _mm_set_epi64x(const { power(576) }, const { power(512) });
            let k128 = _mm_set_epi64x(const { power(192) }, const { power(128) });
            macro_rules! load {
                ($b:expr) => {
                    _mm_shuffle_epi8(_mm_loadu_si128($b.as_ptr().cast()), reverse)
                };
            }
            macro_rules! fold {
                ($v:expr, $k:expr) => {
                    _mm_xor_si128(
                        _mm_clmulepi64_si128($v, $k, 0x00),
                        _mm_clmulepi64_si128($v, $k, 0x11),
                    )
                };
            }
            let (blocks, tail) = data.as_chunks::<64>();
            let first = blocks[0].as_chunks::<16>().0;
            let (mut a, mut b, mut c, mut d) = (
                load!(first[0]),
                load!(first[1]),
                load!(first[2]),
                load!(first[3]),
            );
            for block in &blocks[1..] {
                let fields = block.as_chunks::<16>().0;
                a = _mm_xor_si128(fold!(a, k512), load!(fields[0]));
                b = _mm_xor_si128(fold!(b, k512), load!(fields[1]));
                c = _mm_xor_si128(fold!(c, k512), load!(fields[2]));
                d = _mm_xor_si128(fold!(d, k512), load!(fields[3]));
            }
            // Each lane advances by 512 bits. Recombine consecutive 128-bit
            // fields; folds preserve congruence modulo the FLAC polynomial.
            a = _mm_xor_si128(fold!(a, k128), b);
            a = _mm_xor_si128(fold!(a, k128), c);
            a = _mm_xor_si128(fold!(a, k128), d);
            let mut final_bytes = [0u8; 16];
            _mm_storeu_si128(
                final_bytes.as_mut_ptr().cast(),
                _mm_shuffle_epi8(a, reverse),
            );
            crc16_update(crc16_update(0, &final_bytes), tail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folded_crc_matches_bitwise_polynomial_at_all_alignments_and_tails() {
        let mut data = vec![0u8; 65536 + 16];
        let mut state = 9639u64;
        for byte in &mut data {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        for offset in 0..16 {
            let mut reference = 0u16;
            for len in 0..=data.len() - offset {
                if len > 0 {
                    reference ^= u16::from(data[offset + len - 1]) << 8;
                    for _ in 0..8 {
                        reference =
                            (reference << 1) ^ if reference & 0x8000 != 0 { 0x8005 } else { 0 };
                    }
                }
                if len <= 1024 || len.is_multiple_of(127) || len >= 65472 {
                    let input = &data[offset..offset + len];
                    assert_eq!(crc16_update(0, input), reference);
                    assert_eq!(crc16(input), reference, "offset={offset}, len={len}");
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Md5 {
    state: [u32; 4],
    buffer: [u8; 64],
    len: u64,
}
impl Default for Md5 {
    fn default() -> Self {
        Self {
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buffer: [0; 64],
            len: 0,
        }
    }
}
impl Md5 {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn update(&mut self, mut data: &[u8]) {
        let used = (self.len % 64) as usize;
        self.len = self.len.wrapping_add(data.len() as u64);
        if used != 0 {
            let n = data.len().min(64 - used);
            self.buffer[used..used + n].copy_from_slice(&data[..n]);
            data = &data[n..];
            if used + n != 64 {
                return;
            }
            compress(&mut self.state, &self.buffer);
        }
        while data.len() >= 64 {
            compress(&mut self.state, data[..64].try_into().unwrap());
            data = &data[64..];
        }
        self.buffer[..data.len()].copy_from_slice(data);
    }
    pub fn finalize(mut self) -> [u8; 16] {
        let bit_len = self.len.wrapping_mul(8);
        self.update(&[0x80]);
        let zeros = (56 + 64 - (self.len % 64)) % 64;
        self.update(&[0; 64][..zeros as usize]);
        self.update(&bit_len.to_le_bytes());
        let mut result = [0; 16];
        for (chunk, word) in result.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        result
    }
}
fn compress(state: &mut [u32; 4], block: &[u8; 64]) {
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let mut words = [0u32; 16];
    for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
        *word = u32::from_le_bytes(*bytes);
    }
    let [mut a, mut b, mut c, mut d] = *state;
    // Constant word indices and rotate counts avoid per-round dispatch in this
    // serial compression chain. Variables rotate by the macro argument order.
    macro_rules! mix {
        (f, $b:ident, $c:ident, $d:ident) => {
            $d ^ ($b & ($c ^ $d))
        };
        (g, $b:ident, $c:ident, $d:ident) => {
            $c ^ ($d & ($b ^ $c))
        };
        (h, $b:ident, $c:ident, $d:ident) => {
            $b ^ $c ^ $d
        };
        (i, $b:ident, $c:ident, $d:ident) => {
            $c ^ ($b | !$d)
        };
    }
    macro_rules! step {
        ($f:ident, $a:ident, $b:ident, $c:ident, $d:ident, $i:literal, $g:literal, $s:literal) => {
            $a = $b.wrapping_add(
                $a.wrapping_add(mix!($f, $b, $c, $d))
                    .wrapping_add(words[$g])
                    .wrapping_add(K[$i])
                    .rotate_left($s),
            );
        };
    }
    step!(f, a, b, c, d, 0, 0, 7);
    step!(f, d, a, b, c, 1, 1, 12);
    step!(f, c, d, a, b, 2, 2, 17);
    step!(f, b, c, d, a, 3, 3, 22);
    step!(f, a, b, c, d, 4, 4, 7);
    step!(f, d, a, b, c, 5, 5, 12);
    step!(f, c, d, a, b, 6, 6, 17);
    step!(f, b, c, d, a, 7, 7, 22);
    step!(f, a, b, c, d, 8, 8, 7);
    step!(f, d, a, b, c, 9, 9, 12);
    step!(f, c, d, a, b, 10, 10, 17);
    step!(f, b, c, d, a, 11, 11, 22);
    step!(f, a, b, c, d, 12, 12, 7);
    step!(f, d, a, b, c, 13, 13, 12);
    step!(f, c, d, a, b, 14, 14, 17);
    step!(f, b, c, d, a, 15, 15, 22);
    step!(g, a, b, c, d, 16, 1, 5);
    step!(g, d, a, b, c, 17, 6, 9);
    step!(g, c, d, a, b, 18, 11, 14);
    step!(g, b, c, d, a, 19, 0, 20);
    step!(g, a, b, c, d, 20, 5, 5);
    step!(g, d, a, b, c, 21, 10, 9);
    step!(g, c, d, a, b, 22, 15, 14);
    step!(g, b, c, d, a, 23, 4, 20);
    step!(g, a, b, c, d, 24, 9, 5);
    step!(g, d, a, b, c, 25, 14, 9);
    step!(g, c, d, a, b, 26, 3, 14);
    step!(g, b, c, d, a, 27, 8, 20);
    step!(g, a, b, c, d, 28, 13, 5);
    step!(g, d, a, b, c, 29, 2, 9);
    step!(g, c, d, a, b, 30, 7, 14);
    step!(g, b, c, d, a, 31, 12, 20);
    step!(h, a, b, c, d, 32, 5, 4);
    step!(h, d, a, b, c, 33, 8, 11);
    step!(h, c, d, a, b, 34, 11, 16);
    step!(h, b, c, d, a, 35, 14, 23);
    step!(h, a, b, c, d, 36, 1, 4);
    step!(h, d, a, b, c, 37, 4, 11);
    step!(h, c, d, a, b, 38, 7, 16);
    step!(h, b, c, d, a, 39, 10, 23);
    step!(h, a, b, c, d, 40, 13, 4);
    step!(h, d, a, b, c, 41, 0, 11);
    step!(h, c, d, a, b, 42, 3, 16);
    step!(h, b, c, d, a, 43, 6, 23);
    step!(h, a, b, c, d, 44, 9, 4);
    step!(h, d, a, b, c, 45, 12, 11);
    step!(h, c, d, a, b, 46, 15, 16);
    step!(h, b, c, d, a, 47, 2, 23);
    step!(i, a, b, c, d, 48, 0, 6);
    step!(i, d, a, b, c, 49, 7, 10);
    step!(i, c, d, a, b, 50, 14, 15);
    step!(i, b, c, d, a, 51, 5, 21);
    step!(i, a, b, c, d, 52, 12, 6);
    step!(i, d, a, b, c, 53, 3, 10);
    step!(i, c, d, a, b, 54, 10, 15);
    step!(i, b, c, d, a, 55, 1, 21);
    step!(i, a, b, c, d, 56, 8, 6);
    step!(i, d, a, b, c, 57, 15, 10);
    step!(i, c, d, a, b, 58, 6, 15);
    step!(i, b, c, d, a, 59, 13, 21);
    step!(i, a, b, c, d, 60, 4, 6);
    step!(i, d, a, b, c, 61, 11, 10);
    step!(i, c, d, a, b, 62, 2, 15);
    step!(i, b, c, d, a, 63, 9, 21);
    for (word, value) in state.iter_mut().zip([a, b, c, d]) {
        *word = word.wrapping_add(value);
    }
}
