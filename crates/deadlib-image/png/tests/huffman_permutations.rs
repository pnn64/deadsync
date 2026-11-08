use deadlib_image_png::{Error, Samples, decode, decode_reader, zlib};
use std::io::{Cursor, Read};
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/huffman.rs"]
mod huffman;
#[path = "support/huffman_permutations.rs"]
mod permutations;
mod support;
use permutations::{Case, Family};

struct Fragments<'a>(&'a [u8]);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out.len().min(self.0.len()).min(7);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}
fn check(case: &Case) {
    let limit = case.raw.len();
    assert_eq!(
        zlib::decompress(&case.bytes, limit).unwrap(),
        case.raw,
        "{}",
        case.name
    );
    let mut reference = Vec::new();
    flate2::read::ZlibDecoder::new(case.bytes.as_slice())
        .read_to_end(&mut reference)
        .unwrap();
    assert_eq!(reference, case.raw, "flate2 {}", case.name);
    let mut suffix = case.bytes.clone();
    suffix.extend_from_slice(b"unconsumed-permutation-suffix");
    assert_eq!(
        zlib::decompress_prefix(&suffix, limit).unwrap(),
        (case.raw.clone(), case.bytes.len()),
        "prefix {}",
        case.name
    );
    assert!(zlib::decompress(&suffix, limit).is_err());
    for (input, output) in [(1, 1), (7, 31), (case.bytes.len(), 257)] {
        assert_eq!(
            support::inflate(&suffix, input, output, limit).unwrap(),
            (case.raw.clone(), case.bytes.len()),
            "stream {} {input}/{output}",
            case.name
        );
    }
    for short in [0, limit - 1] {
        assert_eq!(
            zlib::decompress(&case.bytes, short),
            Err(Error::LimitExceeded),
            "limit {} {short}",
            case.name
        );
        assert_eq!(
            support::inflate(&case.bytes, 7, 31, short),
            Err(Error::LimitExceeded),
            "stream limit {} {short}",
            case.name
        );
    }
    for missing in 1..=4 {
        assert!(
            zlib::decompress(&case.bytes[..case.bytes.len() - missing], limit).is_err(),
            "trailer {} {missing}",
            case.name
        );
    }
    let png = permutations::png(case);
    let native = decode(&png).unwrap();
    assert_eq!(
        native.image.samples,
        Samples::U8(case.raw[1..].to_vec()),
        "PNG {}",
        case.name
    );
    assert_eq!(
        decode_reader(Fragments(&png), &Default::default()).unwrap(),
        native,
        "reader {}",
        case.name
    );
    let mut reader = png::Decoder::new(Cursor::new(&png)).read_info().unwrap();
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    reader.finish().unwrap();
    assert_eq!(
        &output[..info.buffer_size()],
        &case.raw[1..],
        "Rust png {}",
        case.name
    );
}
fn family(family: Family) {
    let cases = permutations::cases(family);
    let mut seen = vec![[false; 16]; family.alphabet()];
    for case in &cases {
        for (symbol, &width) in case.assignment.iter().enumerate() {
            seen[symbol][width as usize] = true;
        }
        check(case);
    }
    for (symbol, widths) in seen.iter().enumerate() {
        for width in 1..=family.maximum() {
            assert!(
                widths[width as usize],
                "{family:?} symbol {symbol}, depth {width}"
            );
        }
    }
    println!(
        "{}: {} independent streams, {} symbols x depths 1..={}, both padding values",
        family.name(),
        cases.len(),
        family.alphabet(),
        family.maximum()
    );
}
#[test]
fn every_literal_and_eob_assignment_with_empty_distances() {
    family(Family::Literals);
}
#[test]
fn every_literal_length_and_eob_assignment_with_matches() {
    family(Family::Lengths);
}
#[test]
fn every_distance_assignment_and_full_history() {
    family(Family::Distances);
}
#[test]
fn every_code_length_assignment_and_all_repeat_symbols() {
    family(Family::CodeLengths);
}

#[test]
#[ignore = "explicit C zlib permutation audit; build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn c_zlib_symbol_permutations() {
    use std::{fs, process::Command};
    let oracle = std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE");
    let cases = permutations::all_cases();
    let mut input = Vec::new();
    for case in &cases {
        input.extend_from_slice(&(case.bytes.len() as u32).to_be_bytes());
        input.extend_from_slice(&(case.raw.len() as u32 + 1).to_be_bytes());
        input.extend_from_slice(&case.bytes);
    }
    let path = std::env::temp_dir().join(format!(
        "deadlib-huffman-permutations-{}.bin",
        std::process::id()
    ));
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
    for case in &cases {
        assert!(remaining.len() >= 8);
        let status = i32::from_be_bytes(remaining[..4].try_into().unwrap());
        let size = u32::from_be_bytes(remaining[4..8].try_into().unwrap()) as usize;
        remaining = &remaining[8..];
        assert!(remaining.len() >= size);
        assert_eq!(status, 0, "C zlib {}", case.name);
        assert_eq!(&remaining[..size], case.raw, "C zlib {}", case.name);
        remaining = &remaining[size..];
    }
    assert!(remaining.is_empty());
    println!(
        "C zlib checked {} symbol-permutation streams with exact outputs",
        cases.len()
    );
}
