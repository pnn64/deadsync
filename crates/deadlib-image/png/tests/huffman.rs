use deadlib_image_png::{
    Error, Samples, decode, decode_reader,
    zlib::{self, Inflater},
};
use std::io::{Cursor, Read};
mod support;
#[path = "support/huffman.rs"]
mod wire;

fn check(case: &wire::Case) {
    let limit = case.output.as_ref().map_or(1 << 20, Vec::len);
    let actual = zlib::decompress(&case.bytes, limit);
    if let Some(expected) = &case.output {
        assert_eq!(actual.as_ref().unwrap(), expected, "{}", case.name);
        for (input, output) in [(1, 1), (7, 31), (257, 7), (case.bytes.len(), 4096)] {
            assert_eq!(
                support::inflate(&case.bytes, input, output, limit).unwrap(),
                (expected.clone(), case.bytes.len()),
                "{}",
                case.name
            );
        }
        let mut trailing = case.bytes.clone();
        trailing.extend_from_slice(b"untouched");
        assert_eq!(
            support::inflate(&trailing, trailing.len(), 1, limit).unwrap(),
            (expected.clone(), case.bytes.len()),
            "prefix {}",
            case.name
        );
        let mut reference = Vec::new();
        let result =
            flate2::read::ZlibDecoder::new(case.bytes.as_slice()).read_to_end(&mut reference);
        if case.extended_distance {
            assert!(result.is_err(), "flate2 policy changed: {}", case.name);
        } else {
            result.unwrap_or_else(|error| panic!("flate2 {}: {error}", case.name));
            assert_eq!(reference, *expected, "reference {}", case.name);
        }
        if expected.len() >= 2 && expected[0] == 0 {
            let bytes = png(&case.bytes, expected.len() - 1);
            let native = decode(&bytes).unwrap();
            assert_eq!(
                native.image.samples,
                Samples::U8(expected[1..].to_vec()),
                "PNG {}",
                case.name
            );
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
                native
            );
            let reference = (|| {
                let mut reader = png::Decoder::new(Cursor::new(&bytes)).read_info()?;
                let mut pixels = vec![0; expected.len() - 1];
                reader.next_frame(&mut pixels)?;
                reader.finish()?;
                Ok::<_, png::DecodingError>(pixels)
            })();
            if case.extended_distance {
                assert!(reference.is_err(), "Rust png policy changed: {}", case.name);
            } else {
                let pixels = reference.unwrap();
                assert_eq!(pixels, expected[1..], "Rust png {}", case.name);
            }
        }
    } else {
        assert!(actual.is_err(), "accepted {}", case.name);
        let wrapper = png(&case.bytes, 1);
        assert!(decode(&wrapper).is_err(), "PNG accepted {}", case.name);
        assert!(
            decode_reader(wrapper.as_slice(), &Default::default()).is_err(),
            "PNG reader accepted {}",
            case.name
        );
        let message = if case.name.starts_with("counts/287/")
            || case.name.starts_with("counts/288/")
            || case.name.starts_with("counts-with-body/287/")
            || case.name.starts_with("counts-with-body/288/")
        {
            Some("too many literal codes")
        } else if case.name.starts_with("counts/") || case.name.starts_with("bad-cl/empty/") {
            Some("empty Huffman tree")
        } else if case.name.starts_with("bad-cl/oversubscribed/")
            || case.name.starts_with("oversubscribed-")
        {
            Some("oversubscribed Huffman tree")
        } else if case.name.starts_with("incomplete/")
            || case.name.starts_with("sole-eob-width/")
            || case.name.starts_with("sole-distance-width/")
            || case.name.starts_with("bad-cl/single/")
        {
            Some("incomplete Huffman tree")
        } else if case.name.starts_with("missing-eob/") {
            Some("missing end of block code")
        } else if case.name.starts_with("repeat-overflow/") {
            Some("code length repeat overflow")
        } else if case.name == "repeat16-without-previous" {
            Some("invalid code length repeat")
        } else if case.name.starts_with("reserved-fixed-literal/") {
            Some("reserved literal symbol")
        } else if case.name.starts_with("reserved-fixed-distance/")
            || case.name.starts_with("consumed-reserved-distance/")
        {
            Some("reserved distance symbol")
        } else {
            Some("invalid Huffman code")
        };
        if let Some(message) = message {
            assert_eq!(actual, Err(Error::Invalid(message)), "{}", case.name);
        }
        assert!(
            support::inflate(&case.bytes, 1, 1, limit).is_err(),
            "stream accepted {}",
            case.name
        );
        let mut inflater = Inflater::new(limit).unwrap();
        let error = inflater
            .process(&case.bytes, &mut [0; 64])
            .expect_err(&case.name);
        assert_eq!(inflater.process(&[], &mut []), Err(error.clone()));
        assert_eq!(inflater.finish(), Err(error));
    }
}
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let at = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = !0u32;
    for &b in &out[at..] {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    out.extend_from_slice(&(!crc).to_be_bytes());
}
fn png(stream: &[u8], width: usize) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut h = (width as u32).to_be_bytes().to_vec();
    h.extend_from_slice(&1u32.to_be_bytes());
    h.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &h);
    chunk(&mut out, b"IDAT", stream);
    chunk(&mut out, b"IEND", &[]);
    out
}
#[test]
fn all_dynamic_count_fields() {
    let cases = wire::count_cases();
    assert_eq!(cases.len(), 17423);
    for case in cases {
        check(&case);
    }
}
#[test]
fn every_complete_code_length_histogram() {
    let cases = wire::code_length_cases();
    assert_eq!(cases.len(), 1525);
    for case in cases {
        check(&case);
    }
}
#[test]
fn malformed_and_single_symbol_trees() {
    for case in wire::tree_cases() {
        check(&case);
    }
}
#[test]
fn all_repeat_counts_and_cross_table_runs() {
    for case in wire::repeat_cases() {
        check(&case);
    }
}
#[test]
fn published_distance_counts_and_reserved_symbols() {
    for case in wire::reserved_cases() {
        check(&case);
    }
}
#[test]
fn all_length_distance_values_and_fifteen_bit_codes() {
    for case in wire::range_cases() {
        check(&case);
    }
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn c_zlib_huffman_oracle() {
    use std::{fs, process::Command};
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let extended = std::env::var_os("DEADLIB_ZLIB_PKZIP_WORKAROUND").is_some_and(|s| s == "1");
    let cases = wire::all_cases();
    assert_eq!(cases.len(), 19645);
    let valid = cases.iter().filter(|c| c.output.is_some()).count();
    let mut input = Vec::new();
    for case in &cases {
        input.extend_from_slice(&(case.bytes.len() as u32).to_be_bytes());
        input.extend_from_slice(
            &(case.output.as_ref().map_or(1 << 20, |v| v.len() + 1) as u32).to_be_bytes(),
        );
        input.extend_from_slice(&case.bytes);
    }
    let path =
        std::env::temp_dir().join(format!("deadlib-huffman-oracle-{}.bin", std::process::id()));
    fs::write(&path, input).unwrap();
    let result = Command::new(oracle)
        .arg("--zlib-batch")
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut remaining = result.stdout.as_slice();
    let mut extended_rejected = 0;
    for case in &cases {
        assert!(remaining.len() >= 8);
        let status = i32::from_be_bytes(remaining[..4].try_into().unwrap());
        let size = u32::from_be_bytes(remaining[4..8].try_into().unwrap()) as usize;
        remaining = &remaining[8..];
        assert!(remaining.len() >= size);
        let excess_literals = case.name.starts_with("counts-with-body/287/")
            || case.name.starts_with("counts-with-body/288/");
        let expected = if extended && excess_literals {
            // PKZIP_BUG_WORKAROUND also relaxes HLIT, contrary to its explicit
            // published limit. It is not the negative oracle for these cases.
            Some(&[0u8, 0][..])
        } else if case.extended_distance && !extended {
            extended_rejected += 1;
            None
        } else {
            case.output.as_deref()
        };
        assert_eq!(
            status == 0,
            expected.is_some(),
            "C zlib {}, status={status}",
            case.name
        );
        if let Some(expected) = expected {
            assert_eq!(&remaining[..size], expected, "{}", case.name);
        } else {
            assert_eq!(size, 0);
        }
        remaining = &remaining[size..];
    }
    assert!(remaining.is_empty());
    println!(
        "C zlib checked {} Huffman fixtures ({valid} valid, {} malformed); {extended_rejected} published-range definitions rejected by default policy",
        cases.len(),
        cases.len() - valid
    );
}
