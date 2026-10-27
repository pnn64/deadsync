//! MPEG audio protection: polynomial x^16+x^15+x^2+1, initial all ones.
pub fn update(mut crc: u16, value: u32, bits: u32) -> u16 {
    assert!(bits <= 32);
    for bit in (0..bits).rev() {
        let feedback = ((crc >> 15) as u32 ^ (value >> bit)) & 1;
        crc <<= 1;
        if feedback != 0 {
            crc ^= 0x8005;
        }
    }
    crc
}
pub fn bytes(mut crc: u16, data: &[u8]) -> u16 {
    for &byte in data {
        crc = update(crc, byte as u32, 8);
    }
    crc
}
