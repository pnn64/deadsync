mod support;
#[path = "support/float_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use deadlib_audio_decode_vorbis::{ParseOptions, bitpack::BitReader, codebook::Codebook};
use vectors::Book;
fn parsing() -> ParseOptions {
    ParseOptions {
        codebook_behavior: deadlib_audio_decode_vorbis::CodebookBehavior::Libvorbis,
        ..ParseOptions::default()
    }
}
fn compare_book(c: Book, reference: &[u8]) -> usize {
    let book = Codebook::read_with_options(&mut BitReader::new(&c.packed()), parsing()).unwrap();
    let mut wire = reference;
    assert_eq!(support::u32_in(&mut wire), 1);
    assert_eq!(support::u32_in(&mut wire), c.dimensions as u32);
    assert_eq!(support::u32_in(&mut wire), c.entries);
    assert_eq!(support::take(&mut wire, 1), [c.lookup]);
    assert_eq!(support::take(&mut wire, c.entries as usize), book.lengths());
    let mut count = 0;
    for (entry, &length) in book.lengths().iter().enumerate() {
        if length == 0 {
            continue;
        }
        for (dimension, &actual) in book.vector(entry as u32).unwrap().iter().enumerate() {
            let expected = f32::from_bits(support::u32_in(&mut wire));
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "{c:?}, entry {entry}, dimension {dimension}: {actual} != {expected}"
            );
            count += 1;
        }
    }
    let codebits = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
    let n = support::u32_in(&mut wire) as usize;
    let encoded = support::take(&mut wire, n);
    let mut bits = BitReader::new(encoded);
    for (entry, &length) in book.lengths().iter().enumerate() {
        if length != 0 {
            assert_eq!(book.decode_scalar(&mut bits).unwrap(), entry as u32);
        }
    }
    assert_eq!(bits.position(), codebits);
    assert!(wire.is_empty());
    count
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn custom_float_boundaries_match_reference_vq() {
    for exponent in [724, 725, 726, 788, 850, 851, 852, 1023, 0] {
        let c = Book {
            minimum: (exponent << 21) | 1,
            delta: 0,
            ..Book::default()
        };
        let input = c.setup_prefix();
        let output = support::oracle("books", &[&input]);
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("float-baseline-{exponent}.bin")),
                support_request(&input),
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("float-baseline-{exponent}.out")),
                &output,
            )
            .unwrap();
        }
        compare_book(c, &output);
    }
}
fn support_request(data: &[u8]) -> Vec<u8> {
    let mut wire = (data.len() as u64).to_le_bytes().to_vec();
    wire.extend(data);
    wire
}

fn float_words() -> Vec<u32> {
    (0..1024u32)
        .flat_map(|exponent| {
            [0, 0x80000000].into_iter().flat_map(move |sign| {
                [0, 1, 2, 3, 7, 255, 65535, 1048575, 1048576, 2097151]
                    .into_iter()
                    .map(move |mantissa| sign | (exponent << 21) | mantissa)
            })
        })
        .collect()
}
fn float_request() -> Vec<u8> {
    let words = float_words();
    let mut wire = (words.len() as u64).to_le_bytes().to_vec();
    for word in words {
        wire.extend((word as u64).to_le_bytes());
    }
    wire
}
fn compare_floats(mut response: &[u8]) {
    use deadlib_audio_decode_vorbis::{
        CodebookBehavior as Behavior, codebook::float32_unpack_with_behavior,
    };
    let words = float_words();
    assert_eq!(
        u64::from_le_bytes(support::take(&mut response, 8).try_into().unwrap()),
        words.len() as u64
    );
    for word in words {
        let expected = support::u32_in(&mut response);
        let value = float32_unpack_with_behavior(word, Behavior::Libvorbis);
        assert_eq!((value as f32).to_bits(), expected, "{word:08x}");
        // A second oracle constructs IEEE bits from the normalized integer
        // mantissa, without evaluating the implementation's power expression.
        let mantissa = word & 0x1fffff;
        let bits = if mantissa == 0 {
            word & 0x80000000
        } else {
            let k = mantissa.ilog2();
            let e = ((word >> 21) & 1023) as i32 - 788;
            let e = e.clamp(-63, 63) + k as i32 + 127;
            (word & 0x80000000) | ((e as u32) << 23) | ((mantissa << (23 - k)) & 0x7fffff)
        };
        assert_eq!(expected, bits, "independent IEEE normalization {word:08x}");
    }
    assert!(response.is_empty());
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn every_custom_float_exponent_matches_reference_unpack() {
    let input = float_request();
    let output = support::file_oracle("floatcases", &input);
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("float-words.bin"), &input).unwrap();
        std::fs::write(std::path::Path::new(&root).join("float-words.out"), &output).unwrap();
    }
    compare_floats(&output);
    eprintln!("custom floats: 20480 words, all 1024 exponents, both signs and zero/edge mantissas");
}
#[test]
fn custom_float_defaults_retain_full_published_range() {
    use deadlib_audio_decode_vorbis::{
        CodebookBehavior as Behavior,
        codebook::{float32_unpack, float32_unpack_with_behavior},
    };
    for (word, expected) in [
        (1u32, 2.0f64.powi(-788)),
        ((1023 << 21) | 1, 2.0f64.powi(235)),
        ((724 << 21) | 1, 2.0f64.powi(-64)),
        ((852 << 21) | 1, 2.0f64.powi(64)),
    ] {
        assert_eq!(float32_unpack(word), expected);
        assert_eq!(
            float32_unpack_with_behavior(word, Behavior::Vorbis),
            expected
        );
        assert_ne!(
            float32_unpack_with_behavior(word, Behavior::Libvorbis),
            expected
        );
    }
    for exponent in 725..=851u32 {
        for mantissa in [0, 1, 0xfffff, 0x100000, 0x1fffff] {
            for sign in [0, 0x80000000] {
                let word = sign | (exponent << 21) | mantissa;
                assert_eq!(
                    float32_unpack(word).to_bits(),
                    float32_unpack_with_behavior(word, Behavior::Libvorbis).to_bits()
                );
            }
        }
    }
}

fn books() -> Vec<Book> {
    let mut out = Vec::new();
    for exponent in [
        0, 1, 724, 725, 726, 787, 788, 789, 850, 851, 852, 1022, 1023,
    ] {
        for sign in [0, 0x80000000] {
            for delta in [false, true] {
                for lookup in [1, 2] {
                    for sequence in [false, true] {
                        for sparse in [false, true] {
                            for dimensions in [1, 2, 3, 4, 8] {
                                let mut c = Book {
                                    dimensions,
                                    lookup,
                                    sequence,
                                    sparse,
                                    value_bits: [1, 2, 8, 16][out.len() % 4],
                                    ..Book::default()
                                };
                                let word = sign
                                    | (exponent << 21)
                                    | [1, 0x100000, 0x1fffff][out.len() % 3];
                                if delta {
                                    c.minimum = 0x80000000 | (787 << 21) | 1;
                                    c.delta = word;
                                } else {
                                    c.minimum = word;
                                    c.delta = (786 << 21) | 1;
                                }
                                out.push(c);
                            }
                        }
                    }
                }
            }
        }
    }
    for (minimum, delta) in [
        (1, (1023 << 21) | 1),
        ((1023 << 21) | 1, 1),
        ((724 << 21) | 0x1fffff, (852 << 21) | 0x1fffff),
        ((852 << 21) | 1, 0x80000000 | (852 << 21) | 1),
    ] {
        for dimensions in [1, 3, 255, 65535] {
            for lookup in [1, 2] {
                for sequence in [false, true] {
                    for sparse in [false, true] {
                        out.push(Book {
                            minimum,
                            delta,
                            dimensions,
                            lookup,
                            sequence,
                            sparse,
                            ..Book::default()
                        });
                    }
                }
            }
        }
    }
    out
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn custom_float_vq_matrix_matches_reference_values_and_codewords() {
    let configs = books();
    let mut values = 0;
    for (i, c) in configs.iter().copied().enumerate() {
        let input = support_request(&c.setup_prefix());
        let output = support::file_oracle("books", &input);
        values += compare_book(c, &output);
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("vq-{i}.bin")),
                &input,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("vq-{i}.out")),
                &output,
            )
            .unwrap();
        }
    }
    eprintln!(
        "custom float VQ: {} books, {values} exact vector values, both lookup types, sequence and sparse books",
        configs.len()
    );
}

fn boundary_books() -> Vec<Vec<u8>> {
    use deadlib_audio_decode_vorbis::bitpack::BitWriter;
    let mut b = BitWriter::new();
    for (v, n) in [
        (0x564342, 24),
        (1, 16),
        (2, 24),
        (0, 1),
        (0, 1),
        (0, 5),
        (0, 5),
        (0, 4),
    ] {
        b.write(v, n).unwrap();
    }
    let scalar = b.finish().unwrap();
    assert_eq!(scalar.len(), 10);
    let c = Book {
        dimensions: 8,
        lookup: 1,
        value_bits: 1,
        ..Book::default()
    };
    let mut b = BitWriter::new();
    c.write(&mut b);
    let mapped = b.finish().unwrap();
    assert_eq!(mapped.len(), 20);
    let mut out = Vec::new();
    for b in [scalar, mapped] {
        let mut padded = b.clone();
        padded.push(0);
        out.push(b);
        out.push(padded);
    }
    out
}
#[test]
fn published_bit_bounds_and_reference_byte_guards_stay_distinct() {
    use deadlib_audio_decode_vorbis::{CodebookBehavior, Error};
    for (i, data) in boundary_books().iter().enumerate() {
        let mut bits = BitReader::new(data);
        Codebook::read_with_options(&mut bits, ParseOptions::default()).unwrap();
        assert_eq!(bits.position(), if i < 2 { 80 } else { 160 });
        let mut bits = BitReader::new(data);
        let result = Codebook::read_with_options(&mut bits, parsing());
        if i % 2 == 0 {
            assert!(matches!(result, Err(Error::EndOfPacket)));
            assert_eq!(bits.position(), if i < 2 { 66 } else { 159 });
        } else {
            result.unwrap();
            assert_eq!(bits.position(), if i < 2 { 80 } else { 160 });
        }
        assert!(!bits.is_exhausted());
    }
    assert_eq!(
        ParseOptions::default().codebook_behavior,
        CodebookBehavior::Vorbis
    );
}
#[test]
#[ignore = "requires external libvorbis oracle"]
fn reference_byte_guard_decisions_and_positions_match_reference() {
    let input = guard_request();
    let output = support::file_oracle("bookcases", &input);
    guard_compare(&output);
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("byte-guards.bin"), input).unwrap();
        std::fs::write(std::path::Path::new(&root).join("byte-guards.out"), output).unwrap();
    }
}

fn regression_books() -> Vec<Book> {
    vec![
        Book {
            minimum: (852 << 21) | 1,
            delta: 0,
            ..Book::default()
        },
        Book {
            minimum: 1,
            delta: 0,
            ..Book::default()
        },
        Book {
            minimum: (1023 << 21) | 0x1fffff,
            delta: 1,
            dimensions: 3,
            lookup: 1,
            sequence: true,
            ..Book::default()
        },
        Book {
            minimum: 0,
            delta: (1023 << 21) | 0x1fffff,
            dimensions: 255,
            lookup: 2,
            sequence: true,
            sparse: true,
            ..Book::default()
        },
        Book {
            minimum: 0x80000000 | (1023 << 21) | 0x1fffff,
            delta: (852 << 21) | 1,
            dimensions: 8,
            sequence: true,
            ..Book::default()
        },
        Book {
            minimum: (1023 << 21) | 0x1fffff,
            delta: 0x80000000 | (1023 << 21) | 0x1fffff,
            dimensions: 31,
            lookup: 2,
            sequence: true,
            sparse: true,
            ..Book::default()
        },
        Book {
            minimum: (724 << 21) | 1,
            delta: (1023 << 21) | 1,
            dimensions: 3,
            sequence: true,
            ..Book::default()
        },
        Book {
            minimum: 0x80000000,
            delta: 0x80000000,
            dimensions: 3,
            lookup: 1,
            sequence: true,
            ..Book::default()
        },
    ]
}
fn guard_request() -> Vec<u8> {
    let books = guard_books();
    let mut wire = (books.len() as u64).to_le_bytes().to_vec();
    for (prefix, book) in books {
        wire.extend((prefix as u64).to_le_bytes());
        wire.extend(support_request(&book));
    }
    wire
}
fn guard_compare(mut wire: &[u8]) {
    let books = guard_books();
    assert_eq!(
        u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()),
        books.len() as u64
    );
    let (mut accepted, mut rejected) = (0, 0);
    for (prefix, book) in books {
        let parsed = support::take(&mut wire, 1)[0] != 0;
        let position = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap());
        let initialized = support::take(&mut wire, 1)[0] != 0;
        assert_eq!(
            parsed, initialized,
            "complete valid Huffman tree must initialize"
        );
        let mut bits = BitReader::new(&book);
        bits.read(prefix).unwrap();
        let native = Codebook::read_with_options(&mut bits, parsing());
        assert_eq!(native.is_ok(), initialized);
        assert_eq!(bits.position() as u64, position);
        assert!(!bits.is_exhausted());
        if parsed {
            accepted += 1;
        } else {
            rejected += 1;
        }
        let mut bits = BitReader::new(&book);
        bits.read(prefix).unwrap();
        Codebook::read_with_options(&mut bits, ParseOptions::default()).unwrap();
    }
    assert!(wire.is_empty());
    eprintln!(
        "reference book bounds: {accepted} accepted/initialized, {rejected} byte-guard rejections, all 8 bit alignments"
    );
}

#[test]
fn custom_float_unpack_bounds_and_vq_match_retained_reference() {
    let mut wire = include_bytes!("fixtures/codebook-floats-v1.libvorbis").as_slice();
    assert_eq!(support::take(&mut wire, 8), b"DVQFLT01");
    let n = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
    compare_floats(support::take(&mut wire, n));
    let n = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
    guard_compare(support::take(&mut wire, n));
    let books = regression_books();
    assert_eq!(support::u32_in(&mut wire) as usize, books.len());
    for book in books {
        let n = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
        assert_eq!(
            support::take(&mut wire, n),
            support_request(&book.setup_prefix())
        );
        let n = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap()) as usize;
        compare_book(book, support::take(&mut wire, n));
    }
    assert!(wire.is_empty());
}
#[test]
#[ignore = "manual golden regeneration; requires external libvorbis oracle"]
fn regenerate_codebook_float_golden() {
    let mut golden = b"DVQFLT01".to_vec();
    for (mode, input, label) in [
        ("floatcases", float_request(), "float-golden-words"),
        ("bookcases", guard_request(), "float-golden-guards"),
    ] {
        let output = support::file_oracle(mode, &input);
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("{label}.bin")),
                &input,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("{label}.out")),
                &output,
            )
            .unwrap();
        }
        golden.extend(support_request(&output));
    }
    let books = regression_books();
    golden.extend((books.len() as u32).to_le_bytes());
    for (i, book) in books.into_iter().enumerate() {
        let input = support_request(&book.setup_prefix());
        let output = support::file_oracle("books", &input);
        if let Some(root) = std::env::var_os("DEADLIB_VORBIS_FLOAT_INPUTS") {
            std::fs::write(
                std::path::Path::new(&root).join(format!("float-golden-book-{i}.bin")),
                &input,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&root).join(format!("float-golden-book-{i}.out")),
                &output,
            )
            .unwrap();
        }
        golden.extend(support_request(&input));
        golden.extend(support_request(&output));
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codebook-floats-v1.libvorbis"),
        golden,
    )
    .unwrap();
}

fn guard_books() -> Vec<(u32, Vec<u8>)> {
    use deadlib_audio_decode_vorbis::bitpack::BitWriter;
    let mut out = Vec::new();
    for entries in 1..=64u32 {
        for dimensions in [0, 1, 2, 3, 8] {
            for lookup in 0..=2 {
                if dimensions == 0 && lookup == 1 {
                    continue;
                }
                for width in if lookup == 0 {
                    vec![1]
                } else {
                    vec![1, 2, 8, 16]
                } {
                    for sparse in [false, true] {
                        let used = if sparse { entries.div_ceil(2) } else { entries };
                        let longest = if used == 1 {
                            1
                        } else {
                            u32::BITS - (used - 1).leading_zeros()
                        };
                        let short = (1 << longest) - used;
                        for prefix in 0..8 {
                            let mut b = BitWriter::new();
                            b.write(0x55 & ((1 << prefix) - 1), prefix).unwrap();
                            for (v, n) in [
                                (0x564342, 24),
                                (dimensions, 16),
                                (entries, 24),
                                (0, 1),
                                (sparse as u32, 1),
                            ] {
                                b.write(v, n).unwrap();
                            }
                            let mut ordinal = 0;
                            for entry in 0..entries {
                                let active = !sparse || entry % 2 == 0;
                                if sparse {
                                    b.write(active as u32, 1).unwrap();
                                }
                                if active {
                                    let length = if used > 1 && ordinal < short {
                                        longest - 1
                                    } else {
                                        longest
                                    };
                                    b.write(length - 1, 5).unwrap();
                                    ordinal += 1;
                                }
                            }
                            b.write(lookup, 4).unwrap();
                            if lookup != 0 {
                                for (v, n) in [
                                    ((851 << 21) | 1, 32),
                                    ((724 << 21) | 1, 32),
                                    (width - 1, 4),
                                    (1, 1),
                                ] {
                                    b.write(v, n).unwrap();
                                }
                                let values = if lookup == 2 {
                                    entries * dimensions
                                } else {
                                    let mut n = 1u32;
                                    while (n + 1)
                                        .checked_pow(dimensions)
                                        .is_some_and(|x| x <= entries)
                                    {
                                        n += 1;
                                    }
                                    n
                                };
                                for value in 0..values {
                                    b.write(value & ((1 << width) - 1), width).unwrap();
                                }
                            }
                            let raw = b.finish().unwrap();
                            let mut padded = raw.clone();
                            padded.push(0xa5);
                            out.push((prefix, raw));
                            out.push((prefix, padded));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(out.len(), 83968);
    out
}
