use deadlib_audio_decode_mp3::{Error, bits::BitReader};

fn reference(data: &[u8], start: usize, width: usize) -> u32 {
    (start..start + width).fold(0, |value, bit| {
        (value << 1) | u32::from((data[bit / 8] >> (7 - bit % 8)) & 1)
    })
}

#[test]
fn every_two_byte_pattern_preserves_fields_and_flags() {
    for word in 0..=u16::MAX {
        let bytes = word.to_be_bytes();
        for width in 0..=16 {
            let mut bits = BitReader::bounded(&bytes, 0, width).unwrap();
            assert_eq!(bits.read(width as u32), Ok(reference(&bytes, 0, width)));
            assert_eq!(bits.position(), width);
            assert_eq!(bits.flag(), Err(Error::Truncated));
            assert_eq!(bits.position(), width);
        }
        let mut bits = BitReader::new(&bytes);
        for position in 0..16 {
            assert_eq!(bits.flag(), Ok(reference(&bytes, position, 1) != 0));
        }
        assert_eq!(bits.flag(), Err(Error::Truncated));
        assert_eq!(bits.position(), 16);
    }
}

#[test]
fn bounded_fields_preserve_unaligned_tails_errors_and_mixed_reads() {
    let mut state = 0x7342983abu64;
    for size in 0..=40 {
        let data: Vec<_> = (0..size)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        for start in 0..=size * 8 {
            for available in [0, 1, 7, 8, 9, 15, 16, 24, 31, 32, 33, 39] {
                let available = available.min(size * 8 - start);
                for width in 0..=33 {
                    let mut bits = BitReader::bounded(&data, start, available).unwrap();
                    let expected = if width > 32 {
                        Err(Error::Invalid("bit field width"))
                    } else if width as usize > available {
                        Err(Error::Truncated)
                    } else {
                        Ok(reference(&data, start, width as usize))
                    };
                    let result = bits.read(width);
                    assert_eq!(
                        result, expected,
                        "size={size} start={start} available={available} width={width}"
                    );
                    let consumed = if result.is_ok() { width as usize } else { 0 };
                    assert_eq!(bits.position(), start + consumed);
                    assert_eq!(bits.remaining(), available - consumed);
                }
            }
            let mut bits = BitReader::bounded(&data, start, size * 8 - start).unwrap();
            for width in [1, 0, 32, 7, 16, 3, 24, 9, 31, 4, 8, 2] {
                let position = bits.position();
                if width <= bits.remaining() {
                    assert_eq!(
                        bits.read(width as u32),
                        Ok(reference(&data, position, width))
                    );
                } else {
                    assert_eq!(bits.read(width as u32), Err(Error::Truncated));
                    assert_eq!(bits.position(), position);
                }
            }
        }
    }
}
