//! MPEG audio protection: polynomial x^16+x^15+x^2+1, initial all ones.
pub const fn update(mut crc: u16, value: u32, mut bits: u32) -> u16 {
    assert!(bits <= 32);
    while bits != 0 {
        bits -= 1;
        let feedback = ((crc >> 15) as u32 ^ (value >> bits)) & 1;
        crc <<= 1;
        if feedback != 0 {
            crc ^= 0x8005;
        }
    }
    crc
}
static TABLE: [u16; 256] = {
    let mut table = [0; 256];
    let mut byte = 0;
    while byte < table.len() {
        table[byte] = update(0, byte as u32, 8);
        byte += 1;
    }
    table
};

pub fn bytes(mut crc: u16, data: &[u8]) -> u16 {
    for &byte in data {
        crc = (crc << 8) ^ TABLE[((crc >> 8) as u8 ^ byte) as usize];
    }
    crc
}
