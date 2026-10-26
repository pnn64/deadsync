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
const fn crc_slices() -> [[u32; 256]; 8] {
    let mut tables = [[0; 256]; 8];
    tables[0] = CRC_TABLE;
    let mut n = 1;
    while n < 8 {
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
const CRC_SLICES: [[u32; 256]; 8] = crc_slices();

pub(crate) fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for part in parts {
        let (groups, tail) = part.as_chunks::<8>();
        for &bytes in groups {
            let value = u64::from_le_bytes(bytes) ^ u64::from(crc);
            crc = CRC_SLICES[7][(value & 255) as usize]
                ^ CRC_SLICES[6][((value >> 8) & 255) as usize]
                ^ CRC_SLICES[5][((value >> 16) & 255) as usize]
                ^ CRC_SLICES[4][((value >> 24) & 255) as usize]
                ^ CRC_SLICES[3][((value >> 32) & 255) as usize]
                ^ CRC_SLICES[2][((value >> 40) & 255) as usize]
                ^ CRC_SLICES[1][((value >> 48) & 255) as usize]
                ^ CRC_SLICES[0][(value >> 56) as usize];
        }
        for &byte in tail {
            crc = (crc >> 8) ^ CRC_TABLE[((crc as u8) ^ byte) as usize];
        }
    }
    !crc
}

pub(crate) fn adler32(input: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for block in input.chunks(5552) {
        for &byte in block {
            a += u32::from(byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
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
}
