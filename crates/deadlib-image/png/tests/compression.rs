use deadlib_image_png::{
    Error,
    zlib::{self, Compression},
};
use std::io::{Read, Write};

fn data() -> Vec<Vec<u8>> {
    let mut cases = vec![
        vec![],
        vec![0],
        b"hello hello hello".to_vec(),
        vec![42; 100_000],
    ];
    let mut random = 0x7891_0293u32;
    for size in [2, 3, 7, 31, 258, 1024, 32768, 65535, 65536, 100_000] {
        cases.push(
            (0..size)
                .map(|_| {
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    random as u8
                })
                .collect(),
        );
        cases.push((0..size).map(|n| (n % 251) as u8).collect());
    }
    cases
}

#[test]
fn bidirectional_differential() {
    for bytes in data() {
        for mode in [Compression::Stored, Compression::Fast] {
            let compressed = zlib::compress(&bytes, mode).unwrap();
            assert_eq!(zlib::decompress(&compressed, bytes.len()).unwrap(), bytes);
            let mut reference = Vec::new();
            flate2::read::ZlibDecoder::new(compressed.as_slice())
                .read_to_end(&mut reference)
                .unwrap();
            assert_eq!(reference, bytes);
        }
        for level in [0, 1, 6, 9] {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
            encoder.write_all(&bytes).unwrap();
            let compressed = encoder.finish().unwrap();
            assert_eq!(
                zlib::decompress(&compressed, bytes.len()).unwrap(),
                bytes,
                "len={} level={level}",
                bytes.len()
            );
        }
    }
}

#[test]
fn truncation_checksums_limits_and_trailing_data() {
    for mode in [Compression::Stored, Compression::Fast] {
        let compressed = zlib::compress(&vec![9; 1000], mode).unwrap();
        for end in 0..compressed.len() {
            assert!(
                zlib::decompress(&compressed[..end], 1000).is_err(),
                "end={end}"
            );
        }
        assert_eq!(
            zlib::decompress(&compressed, 999),
            Err(Error::LimitExceeded)
        );
        let mut bad = compressed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert_eq!(zlib::decompress(&bad, 1000), Err(Error::Checksum));
        let mut trailing = compressed.clone();
        trailing.push(0);
        assert!(zlib::decompress(&trailing, 1000).is_err());
        assert_eq!(
            zlib::decompress_prefix(&trailing, 1000).unwrap().1,
            compressed.len()
        );
    }
    for bytes in [
        &[0x78, 0x01, 7, 0, 0, 0, 1][..],
        &[0x78, 0x01, 1, 0, 0, 0, 0, 0, 0, 0, 1][..],
    ] {
        assert!(zlib::decompress(bytes, 1024).is_err());
    }
}

#[test]
fn deterministic_mutation_campaign() {
    // A bounded regression campaign. Accepted mutations must agree with an
    // independent inflater, including the complete output and checksum.
    let input: Vec<u8> = (0..4000).map(|n| ((n * n / 19) % 253) as u8).collect();
    let mut reference = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    reference.write_all(&input).unwrap();
    let compressed = reference.finish().unwrap();
    for index in 0..compressed.len() {
        for bit in 0..8 {
            let mut mutation = compressed.clone();
            mutation[index] ^= 1 << bit;
            if let Ok(actual) = zlib::decompress(&mutation, 8192) {
                let mut expected = Vec::new();
                flate2::read::ZlibDecoder::new(mutation.as_slice())
                    .read_to_end(&mut expected)
                    .unwrap();
                assert_eq!(actual, expected);
            }
        }
    }
}

struct FixtureBits {
    bytes: Vec<u8>,
    word: u64,
    count: u8,
}
impl FixtureBits {
    fn put(&mut self, value: u32, n: u8) {
        for bit in 0..n {
            self.word |= u64::from((value >> bit) & 1) << self.count;
            self.count += 1;
            if self.count == 8 {
                self.bytes.push(self.word as u8);
                self.word = 0;
                self.count = 0;
            }
        }
    }
    fn align(&mut self) {
        if self.count != 0 {
            self.bytes.push(self.word as u8);
            self.word = 0;
            self.count = 0;
        }
    }
    fn finish(mut self, raw: &[u8]) -> Vec<u8> {
        self.align();
        let mut out = vec![0x78, 0x01];
        out.extend_from_slice(&self.bytes);
        let (mut a, mut b) = (1u32, 0u32);
        for &v in raw {
            a = (a + u32::from(v)) % 65521;
            b = (b + a) % 65521;
        }
        out.extend_from_slice(&((b << 16) | a).to_be_bytes());
        out
    }
}

#[test]
fn mixed_block_alignment_and_prefetched_stored_bytes() {
    for leading in 0..20 {
        for stored in 0..20 {
            let mut bits = FixtureBits {
                bytes: Vec::new(),
                word: 0,
                count: 0,
            };
            bits.put(2, 3); // Non-final fixed block.
            for _ in 0..leading {
                bits.put(0x8e, 8);
            } // Canonical fixed 'A', reversed.
            bits.put(0, 7); // End of block.
            bits.put(0, 3);
            bits.align(); // Non-final stored block.
            bits.put(stored, 16);
            bits.put(stored ^ 65535, 16);
            for n in 0..stored {
                bits.put(n, 8);
            }
            bits.put(3, 3);
            bits.put(0x4e, 8);
            bits.put(0, 7); // Final fixed 'B'.
            let mut raw = vec![b'A'; leading];
            raw.extend((0..stored).map(|n| n as u8));
            raw.push(b'B');
            let compressed = bits.finish(&raw);
            assert_eq!(zlib::decompress(&compressed, raw.len()).unwrap(), raw);
            let mut reference = Vec::new();
            flate2::read::ZlibDecoder::new(compressed.as_slice())
                .read_to_end(&mut reference)
                .unwrap();
            assert_eq!(reference, raw);
        }
    }
}

#[test]
fn dynamic_literal_only_and_single_symbol_trees() {
    for length in 0..65 {
        let mut bits = FixtureBits {
            bytes: Vec::new(),
            word: 0,
            count: 0,
        };
        bits.put(5, 3); // Final dynamic block.
        bits.put(0, 5);
        bits.put(0, 5);
        bits.put(14, 4); // 257 literal, 1 distance, 18 CL codes.
        let order = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1];
        for symbol in order {
            bits.put(u32::from(symbol == 0 || symbol == 1), 3);
        }
        for symbol in 0..258 {
            bits.put(u32::from(symbol == 256 || (length != 0 && symbol == 97)), 1);
        }
        for _ in 0..length {
            bits.put(0, 1);
        } // 'a' is code 0.
        bits.put(u32::from(length != 0), 1); // EOB is code 1, or sole code 0.
        let raw = vec![b'a'; length];
        let compressed = bits.finish(&raw);
        assert_eq!(zlib::decompress(&compressed, length).unwrap(), raw);
        let mut reference = Vec::new();
        flate2::read::ZlibDecoder::new(compressed.as_slice())
            .read_to_end(&mut reference)
            .unwrap();
        assert_eq!(reference, raw);
    }
}
