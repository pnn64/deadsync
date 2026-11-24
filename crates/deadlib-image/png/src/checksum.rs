#[cfg(target_arch = "x86_64")]
#[allow(unsafe_code)]
#[path = "checksum/x86.rs"]
mod x86;

const fn crc_table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = i as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(value & 1));
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
}
const CRC_TABLE: [u32; 256] = crc_table();
const fn crc_slices() -> [[u32; 256]; 16] {
    let mut tables = [[0; 256]; 16];
    tables[0] = CRC_TABLE;
    let mut n = 1;
    while n < 16 {
        let mut i = 0;
        while i < 256 {
            let value = tables[n - 1][i];
            tables[n][i] = (value >> 8) ^ CRC_TABLE[(value & 255) as usize];
            i += 1;
        }
        n += 1;
    }
    tables
}
const CRC_SLICES: [[u32; 256]; 16] = crc_slices();

#[inline]
fn crc_word<const OFFSET: usize>(value: u64) -> u32 {
    CRC_SLICES[OFFSET + 7][(value & 255) as usize]
        ^ CRC_SLICES[OFFSET + 6][((value >> 8) & 255) as usize]
        ^ CRC_SLICES[OFFSET + 5][((value >> 16) & 255) as usize]
        ^ CRC_SLICES[OFFSET + 4][((value >> 24) & 255) as usize]
        ^ CRC_SLICES[OFFSET + 3][((value >> 32) & 255) as usize]
        ^ CRC_SLICES[OFFSET + 2][((value >> 40) & 255) as usize]
        ^ CRC_SLICES[OFFSET + 1][((value >> 48) & 255) as usize]
        ^ CRC_SLICES[OFFSET][(value >> 56) as usize]
}

pub(crate) fn crc32(parts: &[&[u8]]) -> u32 {
    let mut checksum = Crc32::new();
    for part in parts {
        checksum.update(part);
    }
    checksum.finish()
}

pub(crate) struct Crc32 {
    state: u32,
}
impl Crc32 {
    pub(crate) fn new() -> Self {
        Self { state: !0 }
    }
    pub(crate) fn update(&mut self, bytes: &[u8]) {
        #[cfg(target_arch = "x86_64")]
        if let Some(state) = x86::update(self.state, bytes) {
            self.state = state;
            return;
        }
        self.state = update_scalar(self.state, bytes);
    }
    pub(crate) fn finish(&self) -> u32 {
        !self.state
    }
}

fn update_scalar(mut crc: u32, bytes: &[u8]) -> u32 {
    let (groups, tail) = bytes.as_chunks::<16>();
    for &bytes in groups {
        let low = u64::from_le_bytes(bytes[..8].try_into().unwrap()) ^ u64::from(crc);
        let high = u64::from_le_bytes(bytes[8..].try_into().unwrap());
        crc = crc_word::<8>(low) ^ crc_word::<0>(high);
    }
    let tail = if let Some((bytes, tail)) = tail.split_first_chunk::<8>() {
        crc = crc_word::<0>(u64::from_le_bytes(*bytes) ^ u64::from(crc));
        tail
    } else {
        tail
    };
    let tail = if let Some((bytes, tail)) = tail.split_first_chunk::<4>() {
        let word = (u32::from_le_bytes(*bytes) ^ crc).to_le_bytes();
        crc = CRC_SLICES[3][usize::from(word[0])]
            ^ CRC_SLICES[2][usize::from(word[1])]
            ^ CRC_SLICES[1][usize::from(word[2])]
            ^ CRC_SLICES[0][usize::from(word[3])];
        tail
    } else {
        tail
    };
    for &byte in tail {
        crc = (crc >> 8) ^ CRC_TABLE[((crc as u8) ^ byte) as usize];
    }
    crc
}

pub(crate) fn adler32(input: &[u8]) -> u32 {
    let mut checksum = Adler32::new();
    checksum.update_bulk(input);
    checksum.finish()
}

pub(crate) struct Adler32 {
    a: u32,
    b: u32,
}
impl Adler32 {
    pub(crate) fn new() -> Self {
        Self { a: 1, b: 0 }
    }
    #[inline]
    pub(crate) fn update(&mut self, input: &[u8]) {
        if input.len() >= 64 {
            self.update_bulk(input);
        } else if !input.is_empty() {
            for &byte in input {
                self.a += u32::from(byte);
                self.b += self.a;
            }
            self.a %= 65521;
            self.b %= 65521;
        }
    }
    #[inline]
    pub(crate) fn update_zeros(&mut self, len: usize) {
        // a and b are normalized. Even their largest possible product plus b
        // is 65520*65520+65520 = 4292935920, which fits in u32.
        self.b = (self.b + self.a * (len % 65521) as u32) % 65521;
    }
    fn update_bulk(&mut self, input: &[u8]) {
        #[cfg(target_arch = "x86_64")]
        if x86::adler_update(self, input) {
            return;
        }
        for block in input.chunks(5552) {
            let tail = if block.len() >= 64 {
                let (groups, tail) = block.as_chunks::<16>();
                let mut sums = [0u32; 16];
                let mut prefixes = [0u32; 16];
                for bytes in groups {
                    for lane in 0..16 {
                        sums[lane] += u32::from(bytes[lane]);
                        prefixes[lane] += sums[lane];
                    }
                }
                // Each lane advances independently, allowing compiler SIMD.
                // For M groups, byte (g, i) contributes 16*(M-g)-i to b.
                // Reduce each nonnegative lane contribution before adding it:
                // Adding the full prefix total before subtracting lane offsets
                // can overflow the initial-state bound at 5552 bytes.
                self.b += self.a * (groups.len() * 16) as u32;
                for lane in 0..16 {
                    self.a += sums[lane];
                    self.b += 16 * prefixes[lane] - lane as u32 * sums[lane];
                }
                tail
            } else {
                block
            };
            for &byte in tail {
                self.a += u32::from(byte);
                self.b += self.a;
            }
            self.a %= 65521;
            self.b %= 65521;
        }
    }
    pub(crate) fn finish(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_checksums() {
        assert_eq!(crc32(&[b"123", b"456789"]), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(crc32(&[b"IEND"]), 0xae42_6082);
    }

    #[test]
    fn crc_bulk_tails_and_split_boundaries() {
        let data: Vec<_> = (0..521).map(|n| (n * 37 + n * n) as u8).collect();
        for len in 0..=data.len() {
            let input = &data[..len];
            let mut expected = !0u32;
            for &b in input {
                expected ^= u32::from(b);
                for _ in 0..8 {
                    expected = (expected >> 1) ^ if expected & 1 != 0 { 0xedb8_8320 } else { 0 };
                }
            }
            expected = !expected;
            assert_eq!(crc32(&[input]), expected);
            for split in [0, len / 2, len] {
                assert_eq!(crc32(&[&input[..split], &input[split..]]), expected);
            }
        }
    }

    #[test]
    fn crc_every_byte_value_in_word_lanes_and_fragmented_tails() {
        for offset in 0..32 {
            for value in 0..=255 {
                let mut data = [0; 40];
                data[offset] = value;
                let mut expected = !0u32;
                for &byte in &data {
                    expected ^= u32::from(byte);
                    for _ in 0..8 {
                        expected =
                            (expected >> 1) ^ if expected & 1 != 0 { 0xedb8_8320 } else { 0 };
                    }
                }
                for split in [0, 7, 8, 15, 16, 17, 31, 32, 39, 40] {
                    assert_eq!(
                        crc32(&[&data[..split], &data[split..]]),
                        !expected,
                        "offset={offset} value={value} split={split}"
                    );
                }
            }
        }
    }

    #[test]
    fn adler_block_and_modulus_boundaries() {
        for len in [
            0,
            1,
            7,
            15,
            16,
            17,
            31,
            32,
            33,
            5551,
            5552,
            5553,
            5567,
            5568,
            5552 * 3,
        ] {
            for byte in [0u8, 1, 127, 255] {
                let data = vec![byte; len];
                let (mut a, mut b) = (1u32, 0u32);
                for &v in &data {
                    a = (a + u32::from(v)) % 65521;
                    b = (b + a) % 65521;
                }
                assert_eq!(adler32(&data), (b << 16) | a, "len={len} byte={byte}");
            }
        }
    }

    #[test]
    fn adler_zero_advancement_matches_modular_addition() {
        // Binary repeated addition avoids both the implementation's product
        // and its reduction of len before multiplying.
        fn oracle(a: u32, b: u32, mut len: usize) -> (u32, u32) {
            let mut delta = u64::from(a);
            let mut sum = u64::from(b);
            while len != 0 {
                if len & 1 != 0 {
                    sum = (sum + delta) % 65521;
                }
                delta = (delta + delta) % 65521;
                len >>= 1;
            }
            (a, sum as u32)
        }
        let mut lengths = vec![
            0,
            1,
            2,
            3,
            31,
            32,
            127,
            128,
            129,
            5551,
            5552,
            5553,
            65520,
            65521,
            65522,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ];
        for factor in [2usize, 65521] {
            if let Some(after) = 65521usize
                .checked_mul(factor)
                .and_then(|n| n.checked_add(1))
            {
                lengths.extend([after - 2, after - 1, after]);
            }
        }
        for a in [0, 1, 65520] {
            for b in [0, 1, 65520] {
                for &len in &lengths {
                    let expected = oracle(a, b, len);
                    let mut actual = Adler32 { a, b };
                    actual.update_zeros(len);
                    assert_eq!((actual.a, actual.b), expected, "a={a} b={b} len={len}");
                    let split = len / 2;
                    let mut fragmented = Adler32 { a, b };
                    fragmented.update_zeros(split);
                    fragmented.update_zeros(len - split);
                    assert_eq!(
                        (fragmented.a, fragmented.b),
                        expected,
                        "fragmented a={a} b={b} len={len}"
                    );
                }
            }
        }
    }

    #[test]
    fn adler_zero_advancement_interleaves_with_nonzero_spans() {
        let prefix: Vec<u8> = (0usize..137).map(|n| (n * 137 + 71) as u8).collect();
        let suffix: Vec<u8> = (0usize..257).map(|n| (n * 59 + 193) as u8).collect();
        for len in [0, 1, 127, 128, 129, 5552, 65520, 65521, 65522] {
            for initial in [(1, 0), (65520, 65520)] {
                let (mut a, mut b) = (u64::from(initial.0), u64::from(initial.1));
                for byte in prefix
                    .iter()
                    .copied()
                    .chain(core::iter::repeat_n(0, len))
                    .chain(suffix.iter().copied())
                {
                    a = (a + u64::from(byte)) % 65521;
                    b = (b + a) % 65521;
                }
                let mut actual = Adler32 {
                    a: initial.0,
                    b: initial.1,
                };
                actual.update(&prefix);
                actual.update_zeros(len);
                actual.update(&suffix);
                assert_eq!(
                    (actual.a, actual.b),
                    (a as u32, b as u32),
                    "initial={initial:?} len={len}"
                );
            }
        }
    }

    #[test]
    fn adler_fragmentation_and_tail_lengths_match_scalar_oracle() {
        fn oracle(input: &[u8], initial: (u32, u32)) -> u32 {
            let (mut a, mut b) = (u64::from(initial.0), u64::from(initial.1));
            for &byte in input {
                a = (a + u64::from(byte)) % 65521;
                b = (b + a) % 65521;
            }
            ((b << 16) | a) as u32
        }
        let lengths = (0..=129).chain([
            255, 256, 257, 511, 512, 513, 5551, 5552, 5553, 11103, 11104, 11105, 16656,
        ]);
        for len in lengths {
            for pattern in 0..4 {
                let mut random = 0x517c_c1b7u32;
                let bytes: Vec<_> = (0..len)
                    .map(|index| match pattern {
                        0 => 255,
                        1 => index as u8,
                        2 => {
                            if index % 2 == 0 {
                                0
                            } else {
                                255
                            }
                        }
                        _ => {
                            random ^= random << 13;
                            random ^= random >> 17;
                            random ^= random << 5;
                            random as u8
                        }
                    })
                    .collect();
                for initial in [(1, 0), (65520, 65520)] {
                    let expected = oracle(&bytes, initial);
                    for fragment in [
                        1, 7, 15, 16, 17, 31, 32, 33, 63, 64, 65, 255, 5551, 5552, 5553,
                    ] {
                        let mut actual = Adler32 {
                            a: initial.0,
                            b: initial.1,
                        };
                        actual.update(&[]);
                        for part in bytes.chunks(fragment) {
                            actual.update(part);
                            actual.update(&[]);
                        }
                        assert_eq!(
                            actual.finish(),
                            expected,
                            "len={len} pattern={pattern} initial={initial:?} fragment={fragment}"
                        );
                    }
                    if len <= 129 {
                        for split in 0..=len {
                            let mut actual = Adler32 {
                                a: initial.0,
                                b: initial.1,
                            };
                            actual.update(&bytes[..split]);
                            actual.update(&bytes[split..]);
                            assert_eq!(
                                actual.finish(),
                                expected,
                                "len={len} pattern={pattern} split={split}"
                            );
                        }
                    }
                }
            }
        }
    }
}
