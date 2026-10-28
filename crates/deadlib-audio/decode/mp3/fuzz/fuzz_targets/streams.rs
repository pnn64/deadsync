#![no_main]
use deadlib_audio_decode_mp3::*;
use libfuzzer_sys::fuzz_target;
use std::io::{self, Cursor, Read};

struct Chunked<'a> {
    bytes: &'a [u8],
    chunk: usize,
}
impl Read for Chunked<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.chunk).min(self.bytes.len());
        output[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        Ok(count)
    }
}
#[derive(Debug, PartialEq)]
struct Block {
    header: FrameHeader,
    position: u64,
    raw_position: u64,
    offset: u64,
    status: FrameStatus,
    planes: Vec<Vec<u32>>,
}
fn bits(pcm: Pcm<'_>) -> Vec<Vec<u32>> {
    pcm.planes()
        .map(|plane| {
            plane
                .iter()
                .map(|v| {
                    assert!(v.is_finite());
                    v.to_bits()
                })
                .collect()
        })
        .collect()
}
fn stream(source: impl Read, options: StreamOptions) -> (Vec<Block>, Option<Error>, bool) {
    let mut reader = StreamDecoder::with_options(source, options);
    let mut blocks = Vec::new();
    for _ in 0..64 {
        match reader.next_frame() {
            Ok(Some(out)) => blocks.push(Block {
                header: out.header,
                position: out.position,
                raw_position: out.position,
                offset: out.byte_offset,
                status: out.status,
                planes: bits(out.pcm),
            }),
            Ok(None) => {
                assert!(reader.next_frame().unwrap().is_none());
                return (blocks, None, true);
            }
            Err(error) => {
                assert!(reader.next_frame().is_err());
                return (blocks, Some(error), false);
            }
        }
    }
    (blocks, None, false)
}
fn gapless(source: impl Read, options: StreamOptions) -> (Vec<Block>, Option<Error>, bool) {
    let mut reader = GaplessDecoder::with_options(source, options);
    let mut blocks = Vec::new();
    for _ in 0..128 {
        match reader.next_frame() {
            Ok(Some(out)) => blocks.push(Block {
                header: out.header,
                position: out.position,
                raw_position: out.raw_position,
                offset: out.byte_offset,
                status: out.status,
                planes: bits(out.pcm),
            }),
            Ok(None) => {
                assert!(reader.next_frame().unwrap().is_none());
                return (blocks, None, true);
            }
            Err(error) => {
                assert!(reader.next_frame().is_err());
                return (blocks, Some(error), false);
            }
        }
    }
    (blocks, None, false)
}
fn signature(header: FrameHeader, planes: &[Vec<u32>], hash: &mut u64, count: &mut u64) {
    *count += planes[0].len() as u64;
    for n in 0..planes[0].len() {
        for plane in planes {
            *hash = hash.wrapping_mul(0x100000001b3)
                ^ (u64::from(header.sample_rate()) << 32)
                ^ u64::from(plane[n])
                ^ planes.len() as u64;
        }
    }
}
fuzz_target!(|input: &[u8]| {
    if input.is_empty() || input.len() > 32769 {
        return;
    }
    let control = input[0];
    let data = &input[1..];
    let mut options = StreamOptions::default();
    options.recovery = if control & 1 == 0 {
        Recovery::Strict
    } else {
        Recovery::Conceal
    };
    options.truncated = if control & 2 == 0 {
        TruncatedPolicy::Error
    } else {
        TruncatedPolicy::Discard
    };
    options.decode.verify_crc = control & 4 == 0;
    options.decode.limits.frame_bytes = 8192;
    options.decode.limits.metadata_bytes = 16384;
    options.decode.limits.sync_bytes = 32768;
    options.decode.limits.index_frames = 256;
    let a = stream(Cursor::new(data), options);
    let b = stream(
        Chunked {
            bytes: data,
            chunk: usize::from(control % 31) + 1,
        },
        options,
    );
    assert!(
        a == b,
        "read schedule changed PCM, positions, errors or termination"
    );
    let g = gapless(Cursor::new(data), options);
    let fragmented = gapless(
        Chunked {
            bytes: data,
            chunk: usize::from(control % 31) + 1,
        },
        options,
    );
    assert!(
        g == fragmented,
        "gapless read schedule changed output or error"
    );
    if g.2 && g.1.is_none() && !a.0.is_empty() && a.2 && a.1.is_none() {
        let mut indexed = IndexedDecoder::with_options(
            Cursor::new(data),
            FileOptions {
                stream: options,
                samples: SampleMode::Gapless,
            },
        )
        .unwrap();
        let (mut wanted, mut count) = (0, 0);
        for block in &g.0 {
            signature(block.header, &block.planes, &mut wanted, &mut count);
        }
        assert_eq!(count, indexed.samples());
        let (mut actual, mut samples) = (0, 0);
        while let Some(out) = indexed.next_frame().unwrap() {
            signature(out.header, &bits(out.pcm), &mut actual, &mut samples);
        }
        assert_eq!((actual, samples), (wanted, count));
    }
    let (linear, error, ended) = a;
    if error.is_some() || !ended || linear.is_empty() {
        return;
    }
    let mut indexed = IndexedDecoder::with_options(
        Cursor::new(data),
        FileOptions {
            stream: options,
            samples: SampleMode::Raw,
        },
    )
    .unwrap();
    let total: u64 = linear.iter().map(|b| b.planes[0].len() as u64).sum();
    assert_eq!(indexed.samples(), total);
    for target in [0, total / 2, total - 1, total, 0] {
        indexed.seek_sample(target).unwrap();
        assert_eq!(indexed.position(), target);
        if target == total {
            assert!(indexed.next_frame().unwrap().is_none());
            continue;
        }
        let n = linear.partition_point(|b| b.position <= target) - 1;
        let out = indexed.next_frame().unwrap().unwrap();
        let expected = &linear[n];
        assert_eq!(out.position, target);
        assert_eq!(out.header, expected.header);
        assert_eq!(out.byte_offset, Some(expected.offset));
        let within = (target - expected.position) as usize;
        for (actual, expected) in bits(out.pcm).iter().zip(&expected.planes) {
            assert_eq!(actual, &expected[within..]);
        }
    }
});
