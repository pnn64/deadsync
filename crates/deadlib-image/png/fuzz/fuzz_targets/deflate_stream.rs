#![no_main]
use deadlib_image_png::zlib::{self, Compression, Deflater, Inflater, StreamStatus};
use libfuzzer_sys::fuzz_target;
use std::io::Read;

fuzz_target!(|input: &[u8]| {
    if input.len() > 131072 {
        return;
    }
    let pick = |n: usize| input.get(n).copied().unwrap_or(0) as usize;
    let mode = if pick(0) & 1 == 0 {
        Compression::Fast
    } else {
        Compression::Stored
    };
    let input_chunk = [1, 7, 257, 32768, input.len().max(1)][pick(1) % 5];
    let output_chunk = [1, 7, 257, 4096][pick(2) % 4];
    let separate_finish = pick(3) & 1 != 0;
    let mut encoder = Deflater::new(mode).unwrap();
    let memory = encoder.working_memory_bytes();
    assert!(memory < 700 * 1024);
    let mut encoded = Vec::new();
    let mut output = vec![0; output_chunk];
    let mut offset = 0;
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls < 4 * (input.len() + 1024));
        let end = input.len().min(offset + input_chunk);
        let empty_output = pick(4) & 1 != 0 && calls % 4 == 0;
        let out = if empty_output {
            &mut [][..]
        } else {
            output.as_mut_slice()
        };
        let finish = end == input.len() && (!separate_finish || offset == input.len());
        let progress = encoder.process(&input[offset..end], out, finish).unwrap();
        offset += progress.consumed;
        encoded.extend_from_slice(&out[..progress.written]);
        assert_eq!(encoder.working_memory_bytes(), memory);
        if progress.status == StreamStatus::Done {
            assert_eq!(offset, input.len());
            break;
        }
        assert!(
            progress.consumed != 0
                || progress.written != 0
                || empty_output
                || (separate_finish && offset == input.len())
        );
    }
    let mut checked = Vec::new();
    flate2::read::ZlibDecoder::new(encoded.as_slice())
        .take(input.len() as u64 + 1)
        .read_to_end(&mut checked)
        .unwrap();
    assert_eq!(checked, input);
    assert_eq!(zlib::decompress(&encoded, input.len()).unwrap(), input);
    let batch = zlib::compress(input, mode).unwrap();
    let mut reference = Vec::new();
    flate2::read::ZlibDecoder::new(batch.as_slice())
        .take(input.len() as u64 + 1)
        .read_to_end(&mut reference)
        .unwrap();
    assert_eq!(reference, input);
    let mut inflater = Inflater::new(input.len()).unwrap();
    let mut decoded = Vec::new();
    let mut offset = 0;
    let mut output = [0; 31];
    loop {
        let end = encoded.len().min(offset + 1 + pick(5) % 31);
        let progress = inflater
            .process(&encoded[offset..end], &mut output)
            .unwrap();
        offset += progress.consumed;
        decoded.extend_from_slice(&output[..progress.written]);
        if progress.status == StreamStatus::Done {
            assert_eq!(offset, encoded.len());
            break;
        }
        assert!(progress.consumed != 0 || progress.written != 0);
    }
    assert_eq!(decoded, input);
});
