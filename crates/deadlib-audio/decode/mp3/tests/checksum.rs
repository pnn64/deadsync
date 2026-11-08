use deadlib_audio_decode_mp3::checksum;

// Original bit-at-a-time recurrence, independent of the lookup table.
fn reference(mut crc: u16, value: u32, bits: u32) -> u16 {
    for bit in (0..bits).rev() {
        let feedback = ((crc >> 15) as u32 ^ (value >> bit)) & 1;
        crc <<= 1;
        if feedback != 0 {
            crc ^= 0x8005;
        }
    }
    crc
}

#[test]
fn every_crc_state_and_byte_matches_the_original_bit_recurrence() {
    for crc in 0..=u16::MAX {
        for byte in 0..=u8::MAX {
            assert_eq!(
                checksum::bytes(crc, &[byte]),
                reference(crc, u32::from(byte), 8),
                "crc {crc:04x}, byte {byte:02x}"
            );
        }
    }
}

#[test]
fn field_widths_and_buffer_boundaries_preserve_crc_values() {
    let data: Vec<_> = (0..2048).map(|n| (n * 37 + 13) as u8).collect();
    for crc in [0, 1, 0x8000, 0x5321, 0xffff] {
        for value in [
            0,
            1,
            0x7fff_ffff,
            0x8000_0000,
            0xffff_ffff,
            0x0123_4567,
            0x89ab_cdef,
        ] {
            for width in 0..=32 {
                assert_eq!(
                    checksum::update(crc, value, width),
                    reference(crc, value, width)
                );
            }
        }
        for length in [0, 1, 2, 9, 17, 32, 511, 1024, 2048] {
            let expected = data[..length]
                .iter()
                .fold(crc, |crc, &byte| reference(crc, u32::from(byte), 8));
            assert_eq!(checksum::bytes(crc, &data[..length]), expected);
        }
    }
}

#[test]
#[should_panic(expected = "bits <= 32")]
fn oversized_crc_fields_still_panic() {
    checksum::update(0xffff, 0, 33);
}
