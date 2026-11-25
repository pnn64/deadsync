//! Ogg's direct, unreflected CRC: polynomial 0x04c11db7, init/xor-out zero.

const fn table() -> [u32; 256] {
    let mut out = [0; 256];
    let mut i = 0;
    while i < 256 {
        let mut value = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            value = (value << 1)
                ^ if value & 0x8000_0000 != 0 {
                    0x04c1_1db7
                } else {
                    0
                };
            bit += 1;
        }
        out[i] = value;
        i += 1;
    }
    out
}
const fn tables() -> [[u32; 256]; 25] {
    let mut out = [[0; 256]; 25];
    out[0] = table();
    let mut power = 1;
    while power < out.len() {
        let mut byte = 0;
        while byte < 256 {
            let previous = out[power - 1][byte];
            // Each row applies one more zero-byte transition to the remainder.
            out[power][byte] = (previous << 8) ^ out[0][(previous >> 24) as usize];
            byte += 1;
        }
        power += 1;
    }
    out
}
static TABLES: [[u32; 256]; 25] = tables();

#[inline]
pub(crate) fn update(crc: u32, bytes: &[u8]) -> u32 {
    #[cfg(target_arch = "x86_64")]
    if let Some(crc) = crate::crc_hardware::update(crc, bytes) {
        return crc;
    }
    update_scalar(crc, bytes)
}

#[inline]
pub(crate) fn update_scalar(mut crc: u32, bytes: &[u8]) -> u32 {
    let (words, tail) = bytes.as_chunks::<16>();
    for &[a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p] in words {
        // CRC is linear: merge its four bytes with the first four inputs,
        // then XOR each byte's remainder after its remaining zero transitions.
        let state = crc ^ u32::from_be_bytes([a, b, c, d]);
        crc = TABLES[15][(state >> 24) as usize]
            ^ TABLES[14][((state >> 16) & 255) as usize]
            ^ TABLES[13][((state >> 8) & 255) as usize]
            ^ TABLES[12][(state & 255) as usize]
            ^ TABLES[11][e as usize]
            ^ TABLES[10][f as usize]
            ^ TABLES[9][g as usize]
            ^ TABLES[8][h as usize]
            ^ TABLES[7][i as usize]
            ^ TABLES[6][j as usize]
            ^ TABLES[5][k as usize]
            ^ TABLES[4][l as usize]
            ^ TABLES[3][m as usize]
            ^ TABLES[2][n as usize]
            ^ TABLES[1][o as usize]
            ^ TABLES[0][p as usize];
    }
    let (words, tail) = tail.as_chunks::<8>();
    for &[a, b, c, d, e, f, g, h] in words {
        let state = crc ^ u32::from_be_bytes([a, b, c, d]);
        crc = TABLES[7][(state >> 24) as usize]
            ^ TABLES[6][((state >> 16) & 255) as usize]
            ^ TABLES[5][((state >> 8) & 255) as usize]
            ^ TABLES[4][(state & 255) as usize]
            ^ TABLES[3][e as usize]
            ^ TABLES[2][f as usize]
            ^ TABLES[1][g as usize]
            ^ TABLES[0][h as usize];
    }
    let tail = if let Some((word, rest)) = tail.split_first_chunk::<4>() {
        let state = crc ^ u32::from_be_bytes(*word);
        crc = TABLES[3][(state >> 24) as usize]
            ^ TABLES[2][((state >> 16) & 255) as usize]
            ^ TABLES[1][((state >> 8) & 255) as usize]
            ^ TABLES[0][(state & 255) as usize];
        rest
    } else {
        tail
    };
    let [x, y, z, _] = crc.to_be_bytes();
    match *tail {
        [] => crc,
        [a] => (crc << 8) ^ TABLES[0][(x ^ a) as usize],
        [a, b] => (crc << 16) ^ TABLES[1][(x ^ a) as usize] ^ TABLES[0][(y ^ b) as usize],
        [a, b, c] => {
            (crc << 24)
                ^ TABLES[2][(x ^ a) as usize]
                ^ TABLES[1][(y ^ b) as usize]
                ^ TABLES[0][(z ^ c) as usize]
        }
        _ => unreachable!("complete CRC words were already folded"),
    }
}

pub fn crc32(bytes: &[u8]) -> u32 {
    update(0, bytes)
}

const CAPTURE_CRC: u32 = advance_zeros(u32::from_be_bytes(*b"OggS"), 26);

const fn advance_zeros(mut crc: u32, bytes: usize) -> u32 {
    let mut i = 0;
    while i < bytes {
        crc = (crc << 8) ^ TABLES[0][(crc >> 24) as usize];
        i += 1;
    }
    crc
}

// The capture pattern is known. Input may stop after the lacing table.
pub(crate) fn page_crc(bytes: &[u8]) -> u32 {
    let (header, tail) = bytes.split_first_chunk::<26>().unwrap();
    debug_assert!(header.starts_with(b"OggS"));
    if let [a, b, c] = *tail {
        // Fold the three tail bytes with the header, so the CRC needs no final update.
        let mut crc = const { advance_zeros(CAPTURE_CRC, 3) };
        header_crc(&mut crc, &header[4..22], &TABLES[7..25]);
        return crc ^ TABLES[2][a as usize] ^ TABLES[1][b as usize] ^ TABLES[0][c as usize];
    }
    if let [a, b] = *tail {
        // A zero-byte packet can also finish with the header's table fold.
        let mut crc = const { advance_zeros(CAPTURE_CRC, 2) };
        header_crc(&mut crc, &header[4..22], &TABLES[6..24]);
        return crc ^ TABLES[1][a as usize] ^ TABLES[0][b as usize];
    }
    // The fixed capture contributes a constant. The four zero checksum
    // bytes advance the table powers without contributing a remainder.
    let mut crc = CAPTURE_CRC;
    header_crc(&mut crc, &header[4..22], &TABLES[4..22]);
    update(crc, tail)
}

pub(crate) fn serial_crc(serial: u32) -> u32 {
    let [a, b, c, d] = serial.to_le_bytes();
    CAPTURE_CRC
        ^ TABLES[11][a as usize]
        ^ TABLES[10][b as usize]
        ^ TABLES[9][c as usize]
        ^ TABLES[8][d as usize]
}

fn header_crc(crc: &mut u32, bytes: &[u8], tables: &[[u32; 256]]) {
    for (&byte, table) in bytes.iter().zip(tables.iter().rev()) {
        *crc ^= table[usize::from(byte)];
    }
}

pub(crate) fn packet_page_crc(mut crc: u32, bytes: &[u8]) -> u32 {
    let (header, tail) = bytes.split_first_chunk::<26>().unwrap();
    // The writer contributes a fixed capture, version 0 and one serial.
    header_crc(&mut crc, &header[5..14], &TABLES[12..21]);
    header_crc(&mut crc, &header[18..22], &TABLES[4..8]);
    update(crc, tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bitwise(mut state: u32, bytes: &[u8]) -> u32 {
        for &byte in bytes {
            state ^= (byte as u32) << 24;
            for _ in 0..8 {
                let top = state & 0x8000_0000;
                state <<= 1;
                if top != 0 {
                    state ^= 0x04c1_1db7;
                }
            }
        }
        state
    }
    #[test]
    fn page_checksum_masks_only_the_checksum_field() {
        let bytes: Vec<_> = (0usize..65323)
            .map(|i| (i.wrapping_mul(37) ^ (i >> 3)) as u8)
            .collect();
        for offset in 0..16 {
            for length in (27..=300).chain([4096, 65025, 65307]) {
                let input = &bytes[offset..offset + length];
                let mut masked = input.to_vec();
                masked[..4].copy_from_slice(b"OggS");
                masked[22..26].fill(0);
                let expected = bitwise(0, &masked);
                for checksum in [0, u32::MAX, 0x0123_4567, 0xfedc_ba98] {
                    masked[22..26].copy_from_slice(&checksum.to_le_bytes());
                    assert_eq!(page_crc(&masked), expected);
                }
            }
        }
    }
    #[test]
    fn every_byte_table_power_matches_the_bitwise_polynomial() {
        for (power, table) in TABLES.iter().enumerate() {
            for (byte, &entry) in table.iter().enumerate() {
                let mut data = [0; TABLES.len()];
                data[0] = byte as u8;
                assert_eq!(entry, bitwise(0, &data[..power + 1]));
            }
        }
    }
    #[test]
    fn word_updates_match_bitwise_across_alignment_tails_and_seeded_splits() {
        let bytes: Vec<_> = (0usize..65568)
            .map(|i| (i.wrapping_mul(37) ^ (i >> 3) ^ (i >> 9)) as u8)
            .collect();
        for seed in [0, u32::MAX, 0x0123_4567, 0xfedc_ba98] {
            for offset in 0..16 {
                for length in 0..=256 {
                    let input = &bytes[offset..offset + length];
                    assert_eq!(
                        update(seed, input),
                        bitwise(seed, input),
                        "seed={seed}, offset={offset}, length={length}"
                    );
                }
            }
            for length in [4096, 65025, 65307] {
                let input = &bytes[3..3 + length];
                let expected = bitwise(seed, input);
                assert_eq!(update(seed, input), expected);
                for size in [1, 2, 3, 4, 5, 7, 8, 9, 13, 16, 31, 64, 255, 4096, 65025] {
                    let mut state = seed;
                    for chunk in input.chunks(size) {
                        state = update(state, chunk);
                    }
                    assert_eq!(state, expected, "length={length}, chunk={size}");
                }
            }
        }
    }
}
