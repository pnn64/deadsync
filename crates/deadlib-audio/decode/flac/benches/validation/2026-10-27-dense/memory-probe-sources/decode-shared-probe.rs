//! Resident-input benchmark CLI. Output is one TSV row; file I/O is untimed.
use deadlib_audio_decode_flac::*;
use std::{hint::black_box, io::Cursor, sync::Arc, time::Instant};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_FLAC},
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
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
pub trait Hooks {
    fn begin(&mut self) {}
    fn end(&mut self, _iteration: usize) {}
    fn finish(&mut self) {}
}
#[allow(dead_code)]
pub struct Noop;
impl Hooks for Noop {}

pub fn run(hooks: &mut impl Hooks) {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 3,
        "decode MODE INPUT ITERATIONS [INDEX_OUTPUT]"
    );
    let mode = &args[0];
    let data: Arc<[u8]> = std::fs::read(&args[1]).unwrap().into();
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
    let ogg_data: Option<Arc<[u8]>> = if mode.contains("ogg") && args.get(4).is_some() {
        Some(std::fs::read(&args[4]).unwrap().into())
    } else if mode.contains("ogg") || args.get(4).is_some() {
        let mut muxer = OggMuxer::new(
            Vec::new(),
            info,
            &metadata.blocks[1..],
            OggWriteOptions::default(),
        )
        .unwrap();
        for f in &frames {
            muxer
                .write_frame(&data[f.offset..f.offset + f.bytes])
                .unwrap();
        }
        let output = muxer.finish().unwrap().output;
        if let Some(path) = args.get(4) {
            std::fs::write(path, &output).unwrap();
        }
        Some(output.into())
    } else {
        None
    };
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_FLAC)
        .with_extra_data(info.encode().unwrap().to_vec().into_boxed_slice());
    let mut symphonia = if matches!(mode.as_str(), "symphonia" | "symphonia-crc") {
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
        hooks.begin();
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
            "symphonia-new"
            | "symphonia-new-crc"
            | "symphonia-new-startup"
            | "symphonia-new-ogg"
            | "symphonia-new-ogg-crc" => {
                let ogg = mode.contains("ogg");
                let input = if ogg {
                    ogg_data.as_ref().unwrap()
                } else {
                    &data
                };
                let mut hint = Hint::new();
                hint.with_extension(if ogg { "oga" } else { "flac" });
                let mut format = symphonia::default::get_probe()
                    .probe(
                        &hint,
                        MediaSourceStream::new(
                            Box::new(Cursor::new(Arc::clone(input))),
                            Default::default(),
                        ),
                        FormatOptions::default(),
                        MetadataOptions::default(),
                    )
                    .unwrap();
                let track = format.default_track(TrackType::Audio).unwrap();
                let id = track.id;
                let mut decoder = symphonia::default::get_codecs()
                    .make_audio_decoder(
                        track.codec_params.as_ref().and_then(|p| p.audio()).unwrap(),
                        &AudioDecoderOptions::default(),
                    )
                    .unwrap();
                while let Some(packet) = format.next_packet().unwrap() {
                    assert_eq!(packet.track_id, id);
                    if mode.ends_with("crc") {
                        assert_eq!(checksum::crc16(&packet.data), 0);
                    }
                    let GenericAudioBufferRef::S32(pcm) = decoder.decode(&packet).unwrap() else {
                        panic!("expected integer PCM")
                    };
                    for c in 0..info.channels as usize {
                        let plane = pcm.plane(c).unwrap();
                        black_box(plane);
                        sum = sum
                            .wrapping_add((plane[0] >> (32 - info.bits_per_sample)) as u32 as u64)
                            .wrapping_add(
                                (plane[plane.len() - 1] >> (32 - info.bits_per_sample)) as u32
                                    as u64,
                            );
                    }
                    count += pcm.frames() as u64 * info.channels as u64;
                    if mode.contains("startup") {
                        break;
                    }
                }
            }
            "ogg-stream" | "ogg-stream-md5" => {
                let mut mapping = OggOptions {
                    decode: options,
                    ..Default::default()
                };
                mapping.decode.verify_md5 = mode.ends_with("md5");
                let mut decoder =
                    OggReader::new(Cursor::new(ogg_data.as_ref().unwrap().as_ref()), mapping);
                while decoder
                    .read_next(|event| {
                        if let OggEvent::Pcm(frame) = event {
                            sum = sum.wrapping_add(consume(frame.pcm));
                            count += frame.pcm.samples() as u64 * frame.pcm.channels() as u64;
                        }
                    })
                    .unwrap()
                {}
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
        hooks.end(iteration);
        black_box(sum);
        if !mode.contains("startup") && !matches!(mode.as_str(), "seek" | "crc") {
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
        if mode.contains("ogg") {
            ogg_data.as_ref().unwrap().len()
        } else {
            data.len()
        },
        frames.len(),
        info.bits_per_sample,
        info.channels
    );
    hooks.finish();
}
