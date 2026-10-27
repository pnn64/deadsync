#![no_main]
use deadlib_image_png::zlib::{Inflater, StreamStatus, decompress_prefix};
use libfuzzer_sys::fuzz_target;
use std::{io::Read, sync::OnceLock};
#[path = "../support/reference.rs"]
mod reference;
#[path = "../../tests/support/huffman.rs"]
mod wire;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 || data.len() > 64 {
        return;
    }
    static CASES: OnceLock<Vec<wire::Case>> = OnceLock::new();
    let cases = CASES.get_or_init(wire::all_cases);
    let selector = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    let case = &cases[selector % cases.len()];
    let byte = |i: usize| data.get(i).copied().unwrap_or(0);
    let control = byte(4);
    let mut bytes = case.bytes.clone();
    if control & 1 != 0 {
        // Preserve zlib framing/checksum to reach the compressed block grammar.
        for pair in data[5..].chunks(3) {
            let at = usize::from(pair[0]) | (usize::from(pair.get(1).copied().unwrap_or(0)) << 8);
            let bit = pair.get(2).copied().unwrap_or(0) % 8;
            let at = 2 + at % (bytes.len() - 6);
            bytes[at] ^= 1 << bit;
        }
    }
    if control & 2 != 0 {
        let at = usize::from(byte(8)) | (usize::from(byte(9)) << 8);
        bytes.truncate(at % (bytes.len() + 1));
    }
    if control & 4 != 0 {
        bytes.extend_from_slice(b"unused suffix");
    }
    let limit = if control & 8 == 0 {
        case.output.as_ref().map_or(4096, |v| v.len().max(4096))
    } else {
        case.output
            .as_ref()
            .map_or(0, |v| v.len().saturating_sub(1))
    };
    let expected = decompress_prefix(&bytes, limit);
    if control & 11 == 0 {
        if let Some(raw) = &case.output {
            assert_eq!(expected.as_ref().unwrap(), &(raw.clone(), case.bytes.len()));
        } else {
            assert!(expected.is_err());
        }
    }
    if let Ok((raw, consumed)) = &expected {
        reference::assert_zlib(&bytes[..*consumed], raw);
        let mut reference = Vec::new();
        let result = flate2::read::ZlibDecoder::new(bytes.as_slice())
            .take((limit + 1) as u64)
            .read_to_end(&mut reference);
        if result.is_ok() {
            assert_eq!(&reference, raw);
        } else if control & 3 == 0 {
            // Default miniz/zlib reject legal, unused HDIST 31/32 definitions.
            // The deterministic C compatibility oracle covers those positives.
            assert!(case.extended_distance);
        }
    }
    let input_chunk = [1, 7, 257, bytes.len().max(1)][usize::from(byte(5) % 4)];
    let output_chunk = [1, 31, 257, 4096][usize::from(byte(6) % 4)];
    let mut inflater = Inflater::new(limit).unwrap();
    let memory = inflater.working_memory_bytes();
    assert!(memory < 40 * 1024);
    let mut offset = 0;
    let mut actual = Vec::new();
    let mut output = [0xa5; 4097];
    let mut previous_written = 0;
    // Every nonterminal call must advance an input or output byte. The extra
    // allowance covers a terminal call and explicit zero-progress assertions.
    for _ in 0..bytes.len() + limit + 2 {
        let end = bytes.len().min(offset + input_chunk);
        // Reset only bytes the last call wrote. Scanning a full 4 KiB buffer
        // for every one-byte input fragment made the harness itself time out.
        // Full untouched-tail checks remain in deterministic streaming tests;
        // this target checks both tail edges and the byte beyond the slice.
        output[..previous_written].fill(0xa5);
        let progress = match inflater.process(&bytes[offset..end], &mut output[..output_chunk]) {
            Ok(p) => p,
            Err(error) => {
                assert!(expected.is_err());
                assert_eq!(inflater.process(&[], &mut []), Err(error.clone()));
                assert_eq!(inflater.finish(), Err(error));
                return;
            }
        };
        assert_eq!(inflater.working_memory_bytes(), memory);
        assert!(progress.consumed <= end - offset && progress.written <= output_chunk);
        let tail_end = (progress.written + 16).min(output_chunk + 1);
        assert!(
            output[progress.written..tail_end]
                .iter()
                .all(|&b| b == 0xa5)
        );
        let tail_start = progress.written.max(output_chunk.saturating_sub(16));
        assert!(output[tail_start..=output_chunk].iter().all(|&b| b == 0xa5));
        previous_written = progress.written;
        offset += progress.consumed;
        actual.extend_from_slice(&output[..progress.written]);
        assert!(actual.len() <= limit);
        assert_eq!(inflater.total_in(), offset);
        assert_eq!(inflater.total_out(), actual.len());
        if progress.status == StreamStatus::Done {
            assert_eq!(expected.unwrap(), (actual, offset));
            inflater.finish().unwrap();
            let done = inflater.process(b"unused", &mut output).unwrap();
            assert_eq!(
                (done.consumed, done.written, done.status),
                (0, 0, StreamStatus::Done)
            );
            return;
        }
        if progress.consumed == 0 && progress.written == 0 {
            assert_eq!(progress.status, StreamStatus::NeedInput);
            assert_eq!(offset, bytes.len());
            assert!(expected.is_err());
            let error = inflater.finish().unwrap_err();
            assert_eq!(inflater.finish(), Err(error));
            return;
        }
    }
    panic!("inflater did not terminate within its progress bound");
});
