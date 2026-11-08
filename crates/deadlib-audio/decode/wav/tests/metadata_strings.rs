use deadlib_audio_decode_wav::metadata::nul_terminated;

fn check(bytes: &[u8]) {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let actual = nul_terminated(bytes);
    assert_eq!(actual, &bytes[..end]);
    assert_eq!(actual.as_ptr(), bytes.as_ptr());
}

#[test]
fn zero_scan_matches_scalar_for_all_byte_pairs_at_word_boundaries() {
    let mut storage = [0x81; 48];
    for pair in 0..=u16::MAX {
        let pair = pair.to_le_bytes();
        for offset in [0, 3, 7, 8, 15] {
            for alignment in 0..4 {
                storage.fill(0x81);
                storage[alignment + offset..alignment + offset + 2].copy_from_slice(&pair);
                check(&storage[alignment..alignment + 24]);
            }
        }
    }
}

#[test]
fn zero_scan_respects_every_short_tail_and_unaligned_long_field() {
    let mut storage = [0xff; 4104];
    for length in (0..=128).chain([255, 256, 257, 511, 512, 513, 4095, 4096]) {
        for alignment in 0..8 {
            storage.fill(0xff);
            let field = &mut storage[alignment..alignment + length];
            check(field);
            for position in 0..length {
                field[position] = 0;
                check(field);
                // A 01 byte following zero catches subtraction-borrow ambiguity.
                if position + 1 < length {
                    field[position + 1] = 1;
                    check(field);
                    field[position + 1] = 0xff;
                }
                field[position] = 0xff;
            }
        }
    }
}
