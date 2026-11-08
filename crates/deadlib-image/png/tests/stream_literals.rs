#[allow(dead_code)]
#[path = "support/stream_literals.rs"]
mod fixtures;
#[path = "support/huffman.rs"]
mod huffman;
use deadlib_image_png::{
    Error,
    zlib::{self, Inflater, StreamStatus},
};
use std::io::Read;

fn drive(
    bytes: &[u8],
    input_size: usize,
    output_size: usize,
    limit: usize,
) -> Result<(Vec<u8>, usize), Error> {
    let mut decoder = Inflater::new(limit)?;
    let memory = decoder.working_memory_bytes();
    assert!(memory < 40 * 1024);
    let (mut offset, mut raw) = (0, Vec::new());
    let mut output = vec![0xcc; output_size];
    for turn in 0..bytes.len().saturating_mul(4) + limit.saturating_mul(4) + 64 {
        let empty = turn % 2 == 0;
        output.fill(0xcc);
        let end = bytes.len().min(offset + input_size);
        let result = decoder.process(
            &bytes[offset..end],
            if empty { &mut [] } else { &mut output },
        );
        let p = match result {
            Ok(p) => p,
            Err(e) => {
                assert_eq!(decoder.finish(), Err(e.clone()));
                assert_eq!(decoder.process(&[], &mut []), Err(e.clone()));
                assert!(!decoder.is_done());
                return Err(e);
            }
        };
        assert_eq!(decoder.working_memory_bytes(), memory);
        assert!(p.consumed <= end - offset && p.written <= if empty { 0 } else { output.len() });
        assert!(output[p.written..].iter().all(|&v| v == 0xcc));
        offset += p.consumed;
        raw.extend_from_slice(&output[..p.written]);
        assert_eq!(
            (decoder.total_in(), decoder.total_out()),
            (offset, raw.len())
        );
        match p.status {
            StreamStatus::Done => {
                decoder.finish()?;
                let done = decoder.process(b"untouched", &mut output)?;
                assert_eq!(
                    (done.consumed, done.written, done.status),
                    (0, 0, StreamStatus::Done)
                );
                return Ok((raw, offset));
            }
            StreamStatus::NeedInput if offset == bytes.len() => {
                return match decoder.finish() {
                    Err(e) => Err(e),
                    Ok(()) => panic!("NeedInput after verified zlib completion"),
                };
            }
            _ => (),
        }
    }
    panic!("literal stream stalled");
}
#[test]
fn literal_runs_history_wrap_pending_output_limits_and_trailer_fragments() {
    let cases = fixtures::cases();
    for case in &cases {
        let expected = case.output.as_ref().unwrap();
        let mut reference = Vec::new();
        flate2::read::ZlibDecoder::new(case.bytes.as_slice())
            .read_to_end(&mut reference)
            .unwrap();
        assert_eq!(&reference, expected, "{}", case.name);
        let mut input = case.bytes.clone();
        input.extend([0, 0xff, 0x78, 1, 0, 0x80, 0x42]);
        assert_eq!(
            zlib::decompress_prefix(&input, expected.len()).unwrap(),
            (expected.clone(), case.bytes.len()),
            "{}",
            case.name
        );
        for (chunk, room) in [(1, 1), (7, 31), (input.len(), 257)] {
            assert_eq!(
                drive(&input, chunk, room, expected.len()).unwrap(),
                (expected.clone(), case.bytes.len()),
                "{} {chunk}/{room}",
                case.name
            );
        }
        if !expected.is_empty() {
            for limit in [0, expected.len() - 1] {
                assert_eq!(
                    drive(&input, 7, 31, limit),
                    Err(Error::LimitExceeded),
                    "{} limit={limit}",
                    case.name
                );
            }
        }
        for missing in 1..=4 {
            assert!(
                drive(
                    &case.bytes[..case.bytes.len() - missing],
                    7,
                    31,
                    expected.len()
                )
                .is_err(),
                "{} missing={missing}",
                case.name
            );
        }
    }
    println!(
        "Streaming literal audit: {} independent fixed/dynamic streams; exact flate2/native bytes and prefixes; alternating zero-output calls, input/output 1/1, 7/31 and whole/257, history wraps, limits and four trailer truncations",
        cases.len()
    );
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn c_zlib_literal_runs_and_history_wraps() {
    use std::{fs, io::Write, process::Command};
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "deadlib-stream-literals-{}-{nonce}.bin",
        std::process::id()
    ));
    let mut file = std::io::BufWriter::new(fs::File::create(&path).unwrap());
    let mut expected = Vec::new();
    let mut add = |case: huffman::Case| {
        file.write_all(&(case.bytes.len() as u32).to_be_bytes())
            .unwrap();
        file.write_all(&((case.output.as_ref().unwrap().len() + 1) as u32).to_be_bytes())
            .unwrap();
        file.write_all(&case.bytes).unwrap();
        expected.push((case.name, case.output.unwrap()));
    };
    for case in fixtures::cases() {
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
        "C streaming literal audit: {} exact independent streams; batch retained in {}",
        expected.len(),
        path.display()
    );
}
