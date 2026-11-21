mod support;
use deadlib_image_png::{
    Error,
    zlib::{self, Compression, Deflater, StreamStatus},
};
use std::io::Read;

fn encode(input: &[u8], mode: Compression, input_chunk: usize, output_chunk: usize) -> Vec<u8> {
    let mut encoder = Deflater::new(mode).unwrap();
    let memory = encoder.working_memory_bytes();
    assert!(
        memory
            < if mode == Compression::Fast {
                700 * 1024
            } else {
                80 * 1024
            }
    );
    let mut offset = 0usize;
    let mut output = vec![0xcc; output_chunk];
    let mut encoded = Vec::new();
    loop {
        output.fill(0xcc);
        let end = input.len().min(offset.saturating_add(input_chunk));
        let p = encoder
            .process(&input[offset..end], &mut output, end == input.len())
            .unwrap();
        offset += p.consumed;
        encoded.extend_from_slice(&output[..p.written]);
        assert_eq!(encoder.working_memory_bytes(), memory);
        assert_eq!(encoder.total_in(), offset);
        assert_eq!(encoder.total_out(), encoded.len());
        assert!(output[p.written..].iter().all(|&b| b == 0xcc));
        if p.status == StreamStatus::Done {
            assert_eq!(offset, input.len());
            assert!(encoder.is_done());
            break;
        }
        if p.status == StreamStatus::NeedInput {
            assert_eq!(offset, end);
            assert_ne!(end, input.len());
        }
        assert!(p.consumed != 0 || p.written != 0);
    }
    assert_eq!(zlib::decompress(&encoded, input.len()).unwrap(), input);
    let mut reference = Vec::new();
    flate2::read::ZlibDecoder::new(encoded.as_slice())
        .read_to_end(&mut reference)
        .unwrap();
    assert_eq!(reference, input);
    encoded
}

#[test]
fn input_output_fragments_and_block_boundaries() {
    let mut random = 0x8131_0293u32;
    for len in [0, 1, 31, 257, 32767, 32768, 32769, 65535, 65536, 100000] {
        let noise: Vec<_> = (0..len)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        let repeated: Vec<_> = (0..len).map(|n| ((n / 13) ^ (n % 259)) as u8).collect();
        for raw in [noise, repeated] {
            for mode in [Compression::Stored, Compression::Fast, Compression::Rle] {
                let expected = encode(&raw, mode, len.max(1), len * 2 + 64);
                for (input, output) in [(1, 1), (2, 7), (7, 2), (31, 258), (32768, 4096)] {
                    assert_eq!(encode(&raw, mode, input, output), expected);
                }
            }
        }
    }
}

#[test]
fn mixed_compressibility_block_transitions_and_full_window_reuse() {
    let block = 32768;
    let mut random = 0x8fa1_32b7u32;
    let noise: Vec<_> = (0..block)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            random as u8
        })
        .collect();
    let compressed = zlib::compress(&noise, Compression::Fast).unwrap();
    assert!(
        compressed.len() <= noise.len() + 11,
        "incompressible data must not expand beyond a stored block"
    );
    let repeated = [noise.as_slice(), noise.as_slice()].concat();
    let compressed = encode(&repeated, Compression::Fast, 31, 7);
    assert!(
        compressed.len() < block + 4096,
        "full-window reuse must save most of the repeated block"
    );
    for prefix in 0..8 {
        let mut first = vec![b'a'; block];
        for (n, byte) in first[..prefix].iter_mut().enumerate() {
            *byte = n as u8;
        }
        let raw = [
            first.as_slice(),
            noise.as_slice(),
            first.as_slice(),
            noise.as_slice(),
        ]
        .concat();
        for (input, output) in [(raw.len(), raw.len() + 64), (1, 7), (29, 1)] {
            let compressed = encode(&raw, Compression::Fast, input, output);
            assert!(compressed.len() <= raw.len() + 6 * raw.len().div_ceil(block) + 6);
            assert_eq!(
                support::inflate(&compressed, 1, 7, raw.len()).unwrap().0,
                raw
            );
        }
    }
}

#[test]
fn cross_block_history_and_separate_finish() {
    let mut raw: Vec<_> = (0..32768).map(|n| ((n * 71) ^ (n / 39)) as u8).collect();
    raw.extend_from_within(..32768);
    raw.extend_from_within(..32768);
    raw.extend_from_slice(&[0xff; 65536]);
    for mode in [Compression::Stored, Compression::Fast, Compression::Rle] {
        let compressed = encode(&raw, mode, 1, 1);
        assert_eq!(
            support::inflate(&compressed, 1, 1, raw.len()).unwrap(),
            (raw.clone(), compressed.len())
        );
        let mut encoder = Deflater::new(mode).unwrap();
        let mut input = raw.as_slice();
        let mut bytes = Vec::new();
        let mut out = [0; 1031];
        while !input.is_empty() {
            let p = encoder.process(input, &mut out, false).unwrap();
            input = &input[p.consumed..];
            bytes.extend_from_slice(&out[..p.written]);
            assert_ne!(p.status, StreamStatus::Done);
        }
        assert!(!encoder.is_done());
        loop {
            let p = encoder.process(&[], &mut out, true).unwrap();
            bytes.extend_from_slice(&out[..p.written]);
            if p.status == StreamStatus::Done {
                break;
            }
        }
        assert_eq!(zlib::decompress(&bytes, raw.len()).unwrap(), raw);
    }
}

#[test]
fn empty_output_and_input_after_finish() {
    for mode in [Compression::Stored, Compression::Fast, Compression::Rle] {
        let mut encoder = Deflater::new(mode).unwrap();
        let p = encoder.process(b"hello", &mut [], true).unwrap();
        assert_eq!(
            (p.consumed, p.written, p.status),
            (0, 0, StreamStatus::NeedOutput)
        );
        let mut input = b"hello".as_slice();
        let mut bytes = Vec::new();
        loop {
            let mut out = [0];
            let p = encoder.process(input, &mut out, true).unwrap();
            input = &input[p.consumed..];
            bytes.extend_from_slice(&out[..p.written]);
            if p.status == StreamStatus::Done {
                break;
            }
        }
        assert_eq!(zlib::decompress(&bytes, 5).unwrap(), b"hello");
        assert_eq!(
            encoder.process(&[], &mut [], false).unwrap().status,
            StreamStatus::Done
        );
        let error = encoder.process(b"late", &mut [0; 20], true).unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
        assert_eq!(encoder.process(&[], &mut [], true), Err(error));
    }
}

#[test]
fn rle_zero_run_boundaries_and_literal_alignment() {
    for length in [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 14, 15, 16, 17, 127, 128, 129, 255, 256, 257, 258, 259, 260,
        261, 515, 516, 517, 32767, 32768, 32769,
    ] {
        for leading in 0usize..8 {
            let mut raw: Vec<_> = (0..leading)
                .map(|n| if n.is_multiple_of(2) { 255 } else { 254 })
                .collect();
            raw.resize(leading + length, 0);
            if leading != 0 {
                raw.extend_from_slice(&[255, 0, 1, 143, 144, 255]);
            }
            let expected = zlib::compress(&raw, Compression::Rle).unwrap();
            for (input, output) in [
                (raw.len().max(1), raw.len() * 2 + 64),
                (1, 1),
                (7, 2),
                (257, 31),
                (32768, 4096),
            ] {
                // encode also checks flate2, native inflation, counters,
                // untouched output tails, and constant bounded scratch.
                assert_eq!(
                    encode(&raw, Compression::Rle, input, output),
                    expected,
                    "run={length} leading={leading} input={input} output={output}"
                );
            }
        }
    }
    // Exercise the 16-byte lookahead boundary with zero runs starting around
    // both four-byte groups. A compressible prefix keeps the short tail in a
    // dynamic block, so stored fallback cannot hide a literal/run error.
    for tail_len in 14usize..=17 {
        for leading in 1..8 {
            for run in [4, 5, 6, 7, 8, 9] {
                if leading + run > tail_len {
                    continue;
                }
                let mut raw = vec![0; 1024];
                raw.extend((0..tail_len).map(|n| {
                    if (leading..leading + run).contains(&n) {
                        0
                    } else if n.is_multiple_of(2) {
                        255
                    } else {
                        254
                    }
                }));
                let expected = zlib::compress(&raw, Compression::Rle).unwrap();
                assert_eq!((expected[2] >> 1) & 3, 2, "tail must stay compressed");
                for (input, output) in [(raw.len(), 128), (1, 1), (17, 7)] {
                    assert_eq!(
                        encode(&raw, Compression::Rle, input, output),
                        expected,
                        "tail={tail_len} leading={leading} run={run} input={input} output={output}"
                    );
                }
            }
        }
    }
    // Every literal code, including the widest codes, remains in compressed
    // blocks because the surrounding zero runs prevent a stored fallback.
    let mut raw: Vec<u8> = (0..=255).collect();
    raw.resize(32768 - 3, 0);
    raw.extend((0..=255).rev());
    raw.resize(raw.len() + 1024, 0);
    let expected = zlib::compress(&raw, Compression::Rle).unwrap();
    for (input, output) in [(1, 3), (32767, 1), (32768, 4096)] {
        assert_eq!(encode(&raw, Compression::Rle, input, output), expected);
    }
}

#[test]
fn rle_checksum_spans_cover_interleaved_zero_runs() {
    let mut raw = vec![7; 13];
    for length in [127, 128, 129, 255, 5553] {
        raw.resize(raw.len() + length, 0);
        raw.extend_from_slice(&[255, 0, 1, 2, 0, 0, 3]);
    }
    raw.extend_from_slice(&[41; 40]);
    let expected = zlib::compress(&raw, Compression::Rle).unwrap();
    for (input, output) in [(raw.len(), raw.len() + 64), (1, 1), (257, 31)] {
        assert_eq!(
            encode(&raw, Compression::Rle, input, output),
            expected,
            "input={input} output={output}"
        );
    }
}

#[test]
fn rle_stored_fallback_and_block_transitions() {
    const BLOCK: usize = 32768;
    let mut random = 0x4c3a_91e7u32;
    let noise: Vec<_> = (0..BLOCK)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            39 + (random % 179) as u8
        })
        .collect();
    let stored = zlib::compress(&noise, Compression::Rle).unwrap();
    assert_eq!(stored.len(), noise.len() + 11);
    assert_eq!((stored[2] >> 1) & 3, 0);
    assert_eq!(
        encode(&noise, Compression::Rle, 29, 7),
        stored,
        "ten-bit literals must fall back to one stored block"
    );
    for leading in 0..8 {
        let mut zeros = vec![0; BLOCK];
        zeros[..leading].fill(144);
        let raw = [
            zeros.as_slice(),
            noise.as_slice(),
            zeros.as_slice(),
            noise.as_slice(),
        ]
        .concat();
        let expected = zlib::compress(&raw, Compression::Rle).unwrap();
        assert!(expected.len() < 2 * BLOCK + 1024);
        for (input, output) in [(raw.len(), 8192), (17, 1), (32768, 31)] {
            assert_eq!(
                encode(&raw, Compression::Rle, input, output),
                expected,
                "leading={leading} input={input} output={output}"
            );
        }
    }
}
