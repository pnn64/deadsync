use deadlib_audio_decode_mp3::{
    Error,
    bits::{BitReader, BitWriter},
    huffman,
};

#[derive(Clone, Copy)]
struct Code {
    book: u8,
    x: u16,
    y: u16,
    bits: u32,
    code: u32,
}
fn codes() -> Vec<Code> {
    include_str!("data/huffman.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let row: Vec<u32> = line.split(',').map(|s| s.parse().unwrap()).collect();
            Code {
                book: row[0] as u8,
                x: row[1] as u16,
                y: row[2] as u16,
                bits: row[3],
                code: row[4],
            }
        })
        .collect()
}
fn book(number: u8) -> (u8, u32) {
    match number {
        16..=23 => (16, [1, 2, 3, 4, 6, 8, 10, 13][(number - 16) as usize]),
        24..=31 => (24, [4, 5, 6, 7, 8, 9, 11, 13][(number - 24) as usize]),
        _ => (number, 0),
    }
}
fn pair_data(
    row: Code,
    lin: u32,
    extra: [u32; 2],
    signs: u8,
    prefix: usize,
) -> (Vec<u8>, usize, [i16; 2]) {
    let mut writer = BitWriter::new();
    writer.write(0, prefix as u32).unwrap();
    writer.write(row.code, row.bits).unwrap();
    let mut expected = [0; 2];
    for (i, value) in [row.x, row.y].into_iter().enumerate() {
        let magnitude = if value == 15 && lin > 0 {
            writer.write(extra[i], lin).unwrap();
            value + extra[i] as u16
        } else {
            value
        };
        let negative = signs & (1 << i) != 0;
        if magnitude != 0 {
            writer.write(u32::from(negative), 1).unwrap();
        }
        expected[i] = if negative {
            -(magnitude as i16)
        } else {
            magnitude as i16
        };
    }
    let bits = writer.len_bits() - prefix;
    (writer.into_bytes(), bits, expected)
}

#[test]
fn every_pair_codeword_sign_offset_and_escape_boundary() {
    let codes = codes();
    let mut cases = 0;
    for number in 1..32 {
        if number == 4 || number == 14 {
            continue;
        }
        let (base, lin) = book(number);
        for &row in codes.iter().filter(|r| r.book == base) {
            let max = (1 << lin) - 1;
            let mut extras = vec![[0, 0]];
            if lin != 0 && row.x == 15 {
                extras.push([max, 0]);
            }
            if lin != 0 && row.y == 15 {
                extras.push([0, max]);
            }
            if lin != 0 && row.x == 15 && row.y == 15 {
                extras.push([max, max]);
            }
            for extra in extras {
                for signs in 0..4 {
                    for prefix in 0..8 {
                        let (data, length, expected) = pair_data(row, lin, extra, signs, prefix);
                        let mut reader = BitReader::bounded(&data, prefix, length).unwrap();
                        assert_eq!(
                            huffman::pair(&mut reader, number).unwrap(),
                            expected,
                            "book {number}, {},{}, escape {extra:?}",
                            row.x,
                            row.y
                        );
                        assert_eq!(reader.remaining(), 0);
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 175680);
}

#[test]
fn pair_truncation_and_reserved_books_are_atomic() {
    let codes = codes();
    for number in [1, 3, 13, 15, 16, 23, 24, 31] {
        let (base, lin) = book(number);
        let row = *codes
            .iter()
            .filter(|r| r.book == base)
            .max_by_key(|r| r.x + r.y)
            .unwrap();
        for signs in 0..4 {
            for prefix in 0..8 {
                let (data, length, _) = pair_data(row, lin, [(1 << lin) - 1; 2], signs, prefix);
                for available in 0..length {
                    let mut reader = BitReader::bounded(&data, prefix, available).unwrap();
                    assert_eq!(huffman::pair(&mut reader, number), Err(Error::Truncated));
                    assert_eq!(reader.position(), prefix);
                }
            }
        }
    }
    let mut empty = BitReader::new(&[]);
    assert_eq!(huffman::pair(&mut empty, 0).unwrap(), [0, 0]);
    assert_eq!(empty.position(), 0);
    for number in [4, 14, 32, 255] {
        assert!(huffman::pair(&mut empty, number).is_err());
        assert_eq!(empty.position(), 0);
    }
}

#[test]
fn every_pair_codeword_truncates_atomically_across_prefix_boundaries() {
    let codes = codes();
    for number in 1..32 {
        if number == 4 || number == 14 {
            continue;
        }
        let (base, lin) = book(number);
        for &row in codes.iter().filter(|row| row.book == base) {
            for prefix in [0, 3, 7] {
                for signs in [0, 3] {
                    let (data, length, _) = pair_data(row, lin, [(1 << lin) - 1; 2], signs, prefix);
                    for available in 0..length {
                        let mut bits = BitReader::bounded(&data, prefix, available).unwrap();
                        assert_eq!(huffman::pair(&mut bits, number), Err(Error::Truncated));
                        assert_eq!(bits.position(), prefix);
                        assert_eq!(bits.remaining(), available);
                    }
                }
            }
        }
    }
}

#[test]
fn all_quadruples_signs_offsets_and_final_symbol_truncations() {
    for row in codes().into_iter().filter(|r| r.book >= 32) {
        for signs in 0..16 {
            for prefix in 0..8 {
                let mut writer = BitWriter::new();
                writer.write(0, prefix as u32).unwrap();
                writer.write(row.code, row.bits).unwrap();
                let mut expected = [0i16; 4];
                for (n, sample) in expected.iter_mut().enumerate() {
                    if row.y & (1 << (3 - n)) != 0 {
                        let sign = (signs >> n) & 1;
                        writer.write(sign, 1).unwrap();
                        *sample = if sign == 0 { 1 } else { -1 };
                    }
                }
                let length = writer.len_bits() - prefix;
                let data = writer.as_bytes();
                let mut reader = BitReader::bounded(data, prefix, length).unwrap();
                assert_eq!(
                    huffman::quad(&mut reader, row.book == 33).unwrap(),
                    expected
                );
                assert_eq!(reader.remaining(), 0);
                for available in 0..length {
                    let mut reader = BitReader::bounded(data, prefix, available).unwrap();
                    assert_eq!(
                        huffman::quad(&mut reader, row.book == 33),
                        Err(Error::Truncated)
                    );
                    assert_eq!(reader.position(), prefix);
                }
            }
        }
    }
}
