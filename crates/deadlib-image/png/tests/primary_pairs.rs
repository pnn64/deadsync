//! Exact two-literal values, EOF/prefix behavior and output limits.
#[path = "support/huffman.rs"]
mod huffman;
#[allow(dead_code)]
#[path = "support/primary_pairs.rs"]
mod pairs;
mod support;
use deadlib_image_png::{Error, zlib};
use std::io::Read;

fn verify(case: huffman::Case) {
    let expected = case.output.as_ref().unwrap();
    let mut reference = Vec::new();
    flate2::read::ZlibDecoder::new(case.bytes.as_slice())
        .read_to_end(&mut reference)
        .unwrap();
    assert_eq!(&reference, expected, "{}", case.name);
    assert_eq!(
        zlib::decompress_prefix(&case.bytes, 2).unwrap(),
        (expected.clone(), case.bytes.len()),
        "{} at EOF",
        case.name
    );
    let mut input = case.bytes.clone();
    input.extend([0, 255, 0x78, 1, 255, 128, 0]);
    assert_eq!(
        zlib::decompress_prefix(&input, 2).unwrap(),
        (expected.clone(), case.bytes.len()),
        "{}",
        case.name
    );
    for limit in [0, 1] {
        assert_eq!(
            zlib::decompress_prefix(&case.bytes, limit),
            Err(Error::LimitExceeded),
            "{} limit={limit}",
            case.name
        );
    }
    for missing in 1..=4 {
        assert!(
            zlib::decompress_prefix(&case.bytes[..case.bytes.len() - missing], 2).is_err(),
            "{} missing={missing}",
            case.name
        );
    }
    assert_eq!(
        support::inflate(&input, 1, 1, 2).unwrap(),
        (expected.clone(), case.bytes.len()),
        "{}",
        case.name
    );
}
#[test]
fn every_two_byte_value_and_short_code_width_keeps_samples_prefixes_and_limits() {
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            verify(pairs::byte_pair(first, second));
        }
    }
    for case in pairs::widths().into_iter().chain(pairs::wide_edges()) {
        verify(case);
    }
    println!(
        "Primary literal audit: 65536 byte pairs plus 132 width/padding/long-code fixtures; independent flate2 and fragmented native samples, exact EOF/suffix consumption, output limits 0/1 and four trailer truncations"
    );
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn c_zlib_literal_pair_values_and_widths() {
    use std::{fs, io::Write, process::Command};
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "deadlib-primary-pairs-{}-{nonce}.bin",
        std::process::id()
    ));
    let mut file = std::io::BufWriter::new(fs::File::create(&path).unwrap());
    let mut expected = Vec::new();
    let mut add = |case: huffman::Case| {
        file.write_all(&(case.bytes.len() as u32).to_be_bytes())
            .unwrap();
        file.write_all(&3u32.to_be_bytes()).unwrap();
        file.write_all(&case.bytes).unwrap();
        expected.push((case.name, case.output.unwrap()));
    };
    for first in 0..=255u8 {
        for second in 0..=255u8 {
            add(pairs::byte_pair(first, second));
        }
    }
    for case in pairs::widths().into_iter().chain(pairs::wide_edges()) {
        add(case);
    }
    file.flush().unwrap();
    drop(file);
    let result = Command::new(oracle)
        .arg("--zlib-batch")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut bytes = result.stdout.as_slice();
    for (name, raw) in &expected {
        assert!(bytes.len() >= 8);
        let status = i32::from_be_bytes(bytes[..4].try_into().unwrap());
        let count = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        bytes = &bytes[8..];
        assert_eq!(status, 0, "{name}");
        assert_eq!(count, raw.len(), "{name}");
        assert!(bytes.len() >= count);
        assert_eq!(&bytes[..count], raw, "{name}");
        bytes = &bytes[count..];
    }
    assert!(bytes.is_empty());
    println!(
        "C primary literal audit: {} exact independent streams; batch retained in {}",
        expected.len(),
        path.display()
    );
}
