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
const TABLE: [u32; 256] = table();

#[inline]
pub(crate) fn update(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        crc = (crc << 8) ^ TABLE[((crc >> 24) as u8 ^ byte) as usize];
    }
    crc
}

pub fn crc32(bytes: &[u8]) -> u32 {
    update(0, bytes)
}

pub(crate) fn page_crc(bytes: &[u8]) -> u32 {
    let crc = update(0, &bytes[..22]);
    let crc = update(crc, &[0; 4]);
    update(crc, &bytes[26..])
}
