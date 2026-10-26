use deadlib_audio_decode_vorbis::{
    bitpack::{BitReader, BitWriter},
    codebook::{Codebook, Huffman, float32_unpack, lookup1_values},
    *,
};

fn identification() -> Identification {
    Identification {
        channels: 2,
        sample_rate: 48000,
        bitrate_maximum: -1,
        bitrate_nominal: 96000,
        bitrate_minimum: 0,
        block_sizes: [256, 2048],
    }
}
fn packed_book(lookup: u32, sequence: bool) -> Vec<u8> {
    let mut bits = BitWriter::new();
    bits.write(0x564342, 24).unwrap();
    bits.write(2, 16).unwrap();
    bits.write(4, 24).unwrap();
    bits.write(0, 1).unwrap();
    bits.write(0, 1).unwrap();
    for _ in 0..4 {
        bits.write(1, 5).unwrap();
    } // Four 2-bit codewords.
    bits.write(lookup, 4).unwrap();
    if lookup != 0 {
        bits.write(0, 32).unwrap();
        bits.write((788 << 21) | 1, 32).unwrap();
        bits.write(3, 4).unwrap();
        bits.write(sequence as u32, 1).unwrap();
        if lookup == 1 {
            bits.write(2, 4).unwrap();
            bits.write(5, 4).unwrap();
        } else {
            for i in 0..8 {
                bits.write(i, 4).unwrap();
            }
        }
    }
    bits.finish().unwrap()
}
fn minimal_setup(floor0: bool, residue: u32) -> Vec<u8> {
    let mut bits = BitWriter::new();
    bits.write(0, 8).unwrap(); // One codebook, dimension=1, one entry.
    bits.write(0x564342, 24).unwrap();
    bits.write(1, 16).unwrap();
    bits.write(1, 24).unwrap();
    bits.write(0, 1).unwrap();
    bits.write(0, 1).unwrap();
    bits.write(0, 5).unwrap();
    bits.write(1, 4).unwrap();
    bits.write(0, 32).unwrap();
    bits.write((788 << 21) | 1, 32).unwrap();
    bits.write(0, 4).unwrap();
    bits.write(0, 1).unwrap();
    bits.write(1, 1).unwrap();
    bits.write(0, 6).unwrap();
    bits.write(0, 16).unwrap(); // One time, type zero.
    bits.write(0, 6).unwrap();
    bits.write(if floor0 { 0 } else { 1 }, 16).unwrap();
    if floor0 {
        bits.write(1, 8).unwrap();
        bits.write(48000, 16).unwrap();
        bits.write(64, 16).unwrap();
        bits.write(6, 6).unwrap();
        bits.write(60, 8).unwrap();
        bits.write(0, 4).unwrap();
        bits.write(0, 8).unwrap();
    } else {
        bits.write(1, 5).unwrap();
        bits.write(0, 4).unwrap(); // One partition, class zero.
        bits.write(0, 3).unwrap();
        bits.write(0, 2).unwrap();
        bits.write(0, 8).unwrap(); // Dimension one, no subclass book.
        bits.write(0, 2).unwrap();
        bits.write(6, 4).unwrap();
        bits.write(32, 6).unwrap();
    }
    bits.write(0, 6).unwrap();
    bits.write(residue, 16).unwrap();
    bits.write(0, 24).unwrap();
    bits.write(32, 24).unwrap();
    bits.write(7, 24).unwrap();
    bits.write(0, 6).unwrap();
    bits.write(0, 8).unwrap();
    bits.write(1, 3).unwrap();
    bits.write(0, 1).unwrap();
    bits.write(0, 8).unwrap();
    bits.write(0, 6).unwrap();
    bits.write(0, 16).unwrap(); // One mapping, type zero.
    bits.write(0, 1).unwrap();
    bits.write(1, 1).unwrap();
    bits.write(0, 8).unwrap();
    bits.write(0, 1).unwrap();
    bits.write(1, 1).unwrap();
    bits.write(0, 2).unwrap();
    bits.write(255, 8).unwrap();
    bits.write(0, 8).unwrap();
    bits.write(0, 8).unwrap(); // Ignored time placeholder.
    bits.write(1, 6).unwrap(); // Two modes.
    for long in [0, 1] {
        bits.write(long, 1).unwrap();
        bits.write(0, 16).unwrap();
        bits.write(0, 16).unwrap();
        bits.write(0, 8).unwrap();
    }
    bits.write(1, 1).unwrap();
    let mut packet = b"\x05vorbis".to_vec();
    packet.extend(bits.finish().unwrap());
    packet
}

#[test]
fn bitpacking_known_wire_values_and_sticky_end_of_packet() {
    let mut writer = BitWriter::new();
    writer.write(5, 3).unwrap();
    writer.write(0x12, 5).unwrap();
    writer.write(0x89abcdef, 32).unwrap();
    writer.write(u32::MAX, 0).unwrap();
    writer.write(7, 2).unwrap();
    assert_eq!(writer.finish().unwrap(), [0x95, 0xef, 0xcd, 0xab, 0x89, 3]);
    let mut bits = BitReader::new(&[]);
    assert_eq!(bits.read(0), Ok(0));
    assert_eq!(bits.read(1), Err(Error::EndOfPacket));
    assert_eq!(bits.read(0), Err(Error::EndOfPacket));
    let mut bits = BitReader::new(&[0x95]);
    assert_eq!(bits.peek(12), None);
    assert!(!bits.is_exhausted());
    assert_eq!(bits.read(3), Ok(5));
    assert_eq!(bits.position(), 3);
    assert_eq!(bits.read(5), Ok(0x12));
    assert_eq!(bits.remaining(), 0);
    assert_eq!(bits.read(0), Ok(0));
}

#[test]
fn bitpacking_matches_independent_bit_indexer_for_all_widths_and_alignments() {
    let data: Vec<u8> = (0..127).map(|n| (n * 73 + 39) as u8).collect();
    for alignment in 0..8 {
        for width in 0..=32 {
            let mut bits = BitReader::new(&data);
            bits.read(alignment).unwrap();
            for _ in 0..20 {
                let start = bits.position();
                let mut expected = 0;
                for n in 0..width as usize {
                    expected |= (((data[(start + n) / 8] >> ((start + n) % 8)) & 1) as u32) << n;
                }
                assert_eq!(bits.read(width).unwrap(), expected);
            }
        }
    }
}

#[test]
fn identification_ranges_hints_and_channel_order() {
    let value = identification();
    let packet = value.encode().unwrap();
    assert_eq!(packet.len(), 30);
    assert_eq!(Identification::parse(&packet).unwrap(), value);
    for end in 0..packet.len() {
        assert!(Identification::parse(&packet[..end]).is_err());
    }
    for (offset, new) in [(7, 1), (11, 0), (28, 0x56), (28, 0x6d), (29, 0)] {
        let mut bad = packet.clone();
        bad[offset] = new;
        assert!(Identification::parse(&bad).is_err());
    }
    assert_eq!(
        channel_order(6).unwrap(),
        [
            Channel::FrontLeft,
            Channel::FrontCenter,
            Channel::FrontRight,
            Channel::RearLeft,
            Channel::RearRight,
            Channel::Lfe
        ]
    );
    assert_eq!(channel_order(8).unwrap().len(), 8);
    assert!(channel_order(255).is_none());
    let mut unusual = value;
    unusual.channels = 255;
    unusual.sample_rate = u32::MAX;
    assert_eq!(
        Identification::parse(&unusual.encode().unwrap()).unwrap(),
        unusual
    );
}

#[test]
fn comments_preserve_duplicates_binary_vectors_and_limits() {
    let original = Comments {
        vendor: b"vendor\0\xff",
        comments: vec![
            b"ARTIST=one",
            b"artist=two",
            b"odd",
            b"X=\0\xff",
            b"\x01bad=value",
        ],
    };
    let packet = original.encode(&Limits::default()).unwrap();
    let decoded = Comments::parse(&packet, &Limits::default()).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(
        decoded.values(b"Artist").collect::<Vec<_>>(),
        [b"one", b"two"]
    );
    assert_eq!(decoded.fields().count(), 3);
    for end in 0..packet.len() {
        assert!(Comments::parse(&packet[..end], &Limits::default()).is_err());
    }
    let limits = Limits {
        max_comments: 1,
        ..Limits::default()
    };
    assert!(matches!(
        Comments::parse(&packet, &limits),
        Err(Error::LimitExceeded(_))
    ));
    let mut bomb = b"\x03vorbis".to_vec();
    bomb.extend(0u32.to_le_bytes());
    bomb.extend(1_000_000u32.to_le_bytes());
    assert_eq!(
        Comments::parse(&bomb, &Limits::default()),
        Err(Error::EndOfPacket)
    );
}

#[test]
fn huffman_entry_order_matches_specification_example() {
    let huffman = Huffman::from_lengths(&[2, 4, 4, 4, 4, 2, 3, 3], 100).unwrap();
    // Wire values correspond to paths 00,0100,0101,0110,0111,10,110,111.
    let mut writer = BitWriter::new();
    for (wire, length) in [
        (0, 2),
        (2, 4),
        (10, 4),
        (6, 4),
        (14, 4),
        (1, 2),
        (3, 3),
        (7, 3),
    ] {
        writer.write(wire, length).unwrap();
    }
    let data = writer.finish().unwrap();
    let mut bits = BitReader::new(&data);
    for expected in 0..8 {
        assert_eq!(huffman.decode(&mut bits), Ok(expected));
    }
    assert_eq!(bits.position(), 26);
}

#[test]
fn huffman_sparse_singleton_deep_and_invalid_trees() {
    let singleton = Huffman::from_lengths(&[0, 1, 0], 0).unwrap();
    let mut bits = BitReader::new(&[0b101]);
    for _ in 0..3 {
        assert_eq!(singleton.decode(&mut bits), Ok(1));
    }
    assert_eq!(bits.position(), 3);
    let mut lengths: Vec<u8> = (1..=32).collect();
    lengths.push(32);
    let deep = Huffman::from_lengths(&lengths, 100).unwrap();
    let bytes = u32::MAX.to_le_bytes();
    let mut bits = BitReader::new(&bytes);
    assert_eq!(deep.decode(&mut bits), Ok(32));
    assert_eq!(bits.position(), 32);
    let mut empty = BitReader::new(&[]);
    assert_eq!(deep.decode(&mut empty), Err(Error::EndOfPacket));
    for invalid in [&[0, 0][..], &[2], &[1, 1, 1], &[2, 2, 2], &[33, 1]] {
        assert!(Huffman::from_lengths(invalid, 100).is_err());
    }
    assert!(matches!(
        Huffman::from_lengths(&[1, 1], 0),
        Err(Error::LimitExceeded(_))
    ));
}

#[test]
fn huffman_assignment_matches_exhaustive_prefix_oracle() {
    let mut seed = 761_223u32;
    for _ in 0..30 {
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed as usize
        };
        let mut lengths = vec![1u8, 1];
        for _ in 0..30 {
            let slot = random() % lengths.len();
            if lengths[slot] < 8 {
                lengths[slot] += 1;
                lengths.push(lengths[slot]);
            }
        }
        for i in (1..lengths.len()).rev() {
            let j = random() % (i + 1);
            lengths.swap(i, j);
        }
        let huffman = Huffman::from_lengths(&lengths, 100).unwrap();
        // Deliberately slow, independent assignment: enumerate all numeric
        // prefixes, rejecting overlap with any previously allocated leaf.
        let mut assigned: Vec<(u32, u8)> = Vec::new();
        let mut writer = BitWriter::new();
        for &length in &lengths {
            let code = (0u32..1 << length)
                .find(|&candidate| {
                    assigned.iter().all(|&(old, old_len)| {
                        if old_len <= length {
                            candidate >> (length - old_len) != old
                        } else {
                            old >> (old_len - length) != candidate
                        }
                    })
                })
                .unwrap();
            assigned.push((code, length));
            writer
                .write(code.reverse_bits() >> (32 - length), length as u32)
                .unwrap();
        }
        let wire = writer.finish().unwrap();
        let mut bits = BitReader::new(&wire);
        for entry in 0..lengths.len() {
            assert_eq!(huffman.decode(&mut bits).unwrap(), entry as u32);
        }
    }
}

#[test]
fn codebook_lookup_types_and_sequence_vectors() {
    for lookup in 0..=2 {
        for sequence in [false, true] {
            let data = packed_book(lookup, sequence);
            let book = Codebook::read(&mut BitReader::new(&data), &Limits::default()).unwrap();
            assert_eq!(book.dimensions(), 2);
            assert_eq!(book.entries(), 4);
            for entry in 0..4 {
                if lookup == 0 {
                    assert!(book.vector(entry).is_err());
                    continue;
                }
                let first = if lookup == 1 {
                    if entry % 2 == 0 { 2.0 } else { 5.0 }
                } else {
                    (entry * 2) as f32
                };
                let second = if lookup == 1 {
                    if entry / 2 == 0 { 2.0 } else { 5.0 }
                } else {
                    (entry * 2 + 1) as f32
                };
                assert_eq!(
                    book.vector(entry).unwrap(),
                    [first, second + if sequence { first } else { 0.0 }]
                );
            }
        }
    }
    assert_eq!(float32_unpack((788 << 21) | 5), 5.0);
    assert_eq!(float32_unpack(0x80000000 | (788 << 21) | 5), -5.0);
    assert_eq!(float32_unpack(0x7fe00000), 0.0); // Avoid 0 * infinity at extreme exponent.
    for (entries, dimensions, expected) in [
        (1, 65535, 1),
        (8, 3, 2),
        (15, 2, 3),
        (16, 2, 4),
        (u32::MAX, 2, 65535),
        (0xffffff, 24, 1),
    ] {
        assert_eq!(lookup1_values(entries, dimensions), expected);
    }
}

#[test]
fn setup_both_floors_all_residues_modes_and_truncation() {
    for floor0 in [false, true] {
        for residue in 0..=2 {
            let packet = minimal_setup(floor0, residue);
            let setup = Setup::parse(&packet, &identification(), &Limits::default()).unwrap();
            assert_eq!(setup.codebooks().len(), 1);
            assert_eq!(setup.modes().len(), 2);
            assert_eq!(setup.residues()[0].kind, residue as u8);
            assert_eq!(setup.mappings()[0].coupling, [[0, 1]]);
            match &setup.floors()[0] {
                Floor::Type0(value) => assert_eq!(value.books, [0]),
                Floor::Type1(value) => {
                    assert_eq!(value.x, [0, 64, 32]);
                    assert_eq!(value.neighbors[2], [0, 1]);
                    assert_eq!(value.sorted_order, [0, 2, 1]);
                }
            }
            let mut bits = BitReader::new(&[0b0110]);
            let audio = setup.read_audio_header(&mut bits).unwrap();
            assert_eq!(
                audio,
                AudioHeader {
                    mode: 1,
                    block_size: 2048,
                    previous_long: true,
                    next_long: false
                }
            );
            assert_eq!(bits.position(), 4);
            for end in 0..packet.len() {
                assert!(
                    Setup::parse(&packet[..end], &identification(), &Limits::default()).is_err(),
                    "prefix {end}"
                );
            }
        }
    }
    let packet = minimal_setup(false, 0);
    let limits = Limits {
        max_setup_memory: 0,
        ..Limits::default()
    };
    assert!(matches!(
        Setup::parse(&packet, &identification(), &limits),
        Err(Error::LimitExceeded(_))
    ));
}
