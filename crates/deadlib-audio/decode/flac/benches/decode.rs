//! Resident-input benchmark CLI. Output is one TSV row; file I/O is untimed.
use deadlib_audio_decode_flac::*;
use std::{hint::black_box, io::Cursor, time::Instant};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_FLAC},
    packet::PacketRef,
    units::{Duration, Timestamp},
};
struct Location {
    offset: usize,
    bytes: usize,
    samples: usize,
}
fn consume(pcm: Pcm<'_>) -> u64 {
    let mut sum = 0u64;
    for p in pcm.planes() {
        black_box(p);
        sum = sum
            .wrapping_add(p[0] as u32 as u64)
            .wrapping_add(p[p.len() - 1] as u32 as u64);
    }
    sum
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 3,
        "decode MODE INPUT ITERATIONS [INDEX_OUTPUT]"
    );
    let mode = &args[0];
    let data = std::fs::read(&args[1]).unwrap();
    let iterations: usize = args[2].parse().unwrap();
    assert!(iterations > 0);
    let options = DecodeOptions {
        verify_md5: false,
        limits: Limits {
            metadata_bytes: 128 << 20,
            ..Default::default()
        },
        ..Default::default()
    };
    let metadata = Metadata::parse(&data, options.limits).unwrap();
    let info = metadata.stream_info;
    let mut frame_decoder = FrameDecoder::new(Some(info)).unwrap();
    let mut frames = Vec::new();
    let mut offset = metadata.audio_offset;
    let mut samples = 0u64;
    let mut expected = 0u64;
    while offset < data.len() {
        let frame = frame_decoder.decode(&data[offset..]).unwrap();
        expected = expected.wrapping_add(consume(frame.pcm));
        samples += frame.pcm.samples() as u64 * frame.pcm.channels() as u64;
        frames.push(Location {
            offset,
            bytes: frame.bytes,
            samples: frame.pcm.samples(),
        });
        offset += frame.bytes;
    }
    if let Some(path) = args.get(3) {
        let mut index = b"DLFPERF1".to_vec();
        index.extend_from_slice(&info.encode().unwrap());
        index.extend_from_slice(&(frames.len() as u32).to_le_bytes());
        for f in &frames {
            index.extend_from_slice(&(f.offset as u64).to_le_bytes());
            index.extend_from_slice(&(f.bytes as u32).to_le_bytes());
            index.extend_from_slice(&(f.samples as u32).to_le_bytes());
        }
        std::fs::write(path, index).unwrap();
    }
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_FLAC)
        .with_extra_data(info.encode().unwrap().to_vec().into_boxed_slice());
    let mut symphonia = if mode.starts_with("symphonia") {
        Some(
            symphonia::default::get_codecs()
                .make_audio_decoder(&params, &AudioDecoderOptions::default())
                .unwrap(),
        )
    } else {
        None
    };
    let mut seek = if mode == "seek" {
        Some(
            SeekReader::with_options(
                Cursor::new(&data),
                SeekOptions {
                    decode: options,
                    ..Default::default()
                },
            )
            .unwrap(),
        )
    } else {
        None
    };
    let mut elapsed = 0;
    let mut checksum = 0u64;
    let mut units = 0u64;
    for iteration in 0..=iterations {
        let begin = Instant::now();
        let mut sum = 0u64;
        let mut count = 0u64;
        match mode.as_str() {
            "frames" => {
                for f in &frames {
                    let frame = frame_decoder
                        .decode(&data[f.offset..f.offset + f.bytes])
                        .unwrap();
                    sum = sum.wrapping_add(consume(frame.pcm));
                    count += frame.pcm.samples() as u64 * frame.pcm.channels() as u64;
                }
            }
            "symphonia" | "symphonia-crc" => {
                for f in &frames {
                    let bytes = &data[f.offset..f.offset + f.bytes];
                    if mode == "symphonia-crc" {
                        assert_eq!(checksum::crc16(bytes), 0);
                    }
                    let packet = PacketRef::new(0, Timestamp::ZERO, Duration::ZERO, bytes);
                    let GenericAudioBufferRef::S32(pcm) =
                        symphonia.as_mut().unwrap().decode_ref(&packet).unwrap()
                    else {
                        panic!("PCM format")
                    };
                    for ch in 0..info.channels as usize {
                        let p = pcm.plane(ch).unwrap();
                        black_box(p);
                        sum = sum
                            .wrapping_add((p[0] >> (32 - info.bits_per_sample)) as u32 as u64)
                            .wrapping_add(
                                (p[p.len() - 1] >> (32 - info.bits_per_sample)) as u32 as u64,
                            );
                    }
                    count += pcm.frames() as u64 * info.channels as u64;
                }
            }
            "borrowed" => {
                let mut decoder = Decoder::with_options(&data, options).unwrap();
                while let Some(frame) = decoder.read_next().unwrap() {
                    sum = sum.wrapping_add(consume(frame.pcm));
                    count += frame.pcm.samples() as u64 * frame.pcm.channels() as u64;
                }
            }
            "stream" | "stream-md5" | "startup" => {
                let mut options = options;
                options.verify_md5 = mode == "stream-md5";
                let mut decoder = Reader::with_options(Cursor::new(&data), options).unwrap();
                while let Some(frame) = decoder.read_next().unwrap() {
                    sum = sum.wrapping_add(consume(frame.pcm));
                    count += frame.pcm.samples() as u64 * frame.pcm.channels() as u64;
                    if mode == "startup" {
                        break;
                    }
                }
            }
            "seek" => {
                let target = info.total_samples.saturating_sub(1) * (iteration as u64 % 97) / 97;
                let decoder = seek.as_mut().unwrap();
                decoder.seek(target).unwrap();
                if let Some(frame) = decoder.read_next().unwrap() {
                    sum = consume(frame.pcm);
                    count = 1;
                }
            }
            "crc" => {
                sum = checksum::crc16(black_box(&data[metadata.audio_offset..])) as u64;
                count = (data.len() - metadata.audio_offset) as u64;
            }
            _ => panic!("unknown benchmark mode"),
        }
        let ns = begin.elapsed().as_nanos();
        black_box(sum);
        if !matches!(mode.as_str(), "startup" | "seek" | "crc") {
            assert_eq!(count, samples);
            assert_eq!(sum, expected, "PCM endpoint check");
        }
        if iteration != 0 {
            elapsed += ns;
            checksum = checksum.wrapping_add(sum);
            units += count;
        }
    }
    println!(
        "{mode}\t{iterations}\t{elapsed}\t{units}\t{checksum}\t{}\t{}\t{}\t{}",
        data.len(),
        frames.len(),
        info.bits_per_sample,
        info.channels
    );
}
