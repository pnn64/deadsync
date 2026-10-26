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
