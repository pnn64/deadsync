mod support;
use deadlib_audio_decode_ogg::{PacketWriter, PageReader, WriteOptions, encode_page};
use deadlib_audio_decode_vorbis::*;
use std::io::Cursor;
const SOURCE: &[u8] = include_bytes!("fixtures/stereo.ogg");

fn baseline(flush: bool) -> Vec<u8> {
    let packets = support::packets(SOURCE);
    let mut decoder = support::decoder(&packets);
    let mut writer = PacketWriter::with_options(
        Vec::new(),
        31,
        WriteOptions {
            target_page_body: usize::MAX,
        },
    );
    for packet in &packets[..3] {
        writer.write_packet(packet, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    let mut audio = packets[3..].to_vec();
    let repeated = audio
        .iter()
        .find(|packet| {
            let header = decoder
                .setup()
                .read_audio_header(&mut bitpack::BitReader::new(packet))
                .unwrap();
            header.previous_long && header.next_long
        })
        .unwrap()
        .clone();
    audio.extend(std::iter::repeat_n(repeated, 64));
    for (number, packet) in audio.iter().enumerate() {
        let block = decoder.decode(packet).unwrap().unwrap();
        let last = number + 1 == audio.len();
        writer
            .write_packet(packet, block.start_frame + block.frames as u64, last)
            .unwrap();
        if !last && (flush || number == 1) {
            writer.flush_page().unwrap();
        }
    }
    writer.into_inner()
}
fn rewrite(
    bytes: &[u8],
    mut change: impl FnMut(&mut deadlib_audio_decode_ogg::PageHeader, usize),
) -> Vec<u8> {
    let mut input = PageReader::new(bytes);
    let mut out = Vec::new();
    while let Some(page) = input.next_page().unwrap() {
        let mut header = page.page.header();
        change(&mut header, page.page.completed_packets());
        out.extend(encode_page(header, page.page.lacing(), page.page.body()).unwrap());
    }
    out
}

#[test]
fn completed_audio_pages_reject_negative_and_unknown_granules_in_every_policy() {
    let source = baseline(true);
    for value in [u64::MAX, u64::MAX - 1, i64::MAX as u64 + 1] {
        for location in [2, 10, u32::MAX] {
            let broken = rewrite(&source, |header, complete| {
                assert!(complete != 0);
                if header.sequence == location || (location == u32::MAX && header.is_eos()) {
                    header.granule_position = value;
                }
            });
            assert_ne!(source, broken);
            for eos_behavior in [EosBehavior::Vorbis, EosBehavior::Libvorbis] {
                let options = OggOptions {
                    eos_behavior,
                    allow_zero_span_granules: true,
                    ..Default::default()
                };
                assert!(decode_reader(broken.as_slice(), options, |_| {}).is_err());
                let mut pull = OggReader::with_options(broken.as_slice(), options);
                while matches!(pull.read_next(|_| {}), Ok(true)) {}
                assert_eq!(pull.read_next(|_| {}), Err(Error::Failed));
                let mut push = OggStreamDecoder::new(options);
                let mut failed = false;
                for chunk in broken.chunks(17) {
                    let mut offset = 0;
                    while offset < chunk.len() {
                        match push.push(&chunk[offset..], |_| {}) {
                            Ok(n) => {
                                assert!(n != 0);
                                offset += n;
                            }
                            Err(_) => {
                                failed = true;
                                break;
                            }
                        }
                    }
                    if failed {
                        break;
                    }
                }
                assert!(failed || push.finish().is_err());
                assert_eq!(push.push(&[], |_| {}), Err(Error::Failed));
                push.reset();
                let mut offset = 0;
                while offset < source.len() {
                    offset += push.push(&source[offset..], |_| {}).unwrap();
                }
                push.finish().unwrap();
                for seek_behavior in [SeekBehavior::Retained, SeekBehavior::Libvorbis] {
                    assert!(
                        OggSeekReader::with_options(
                            Cursor::new(&broken),
                            SeekOptions {
                                eos_behavior,
                                seek_behavior,
                                allow_zero_span_granules: true,
                                ..Default::default()
                            }
                        )
                        .is_err()
                    );
                }
            }
        }
    }
}
fn full(bytes: &[u8], options: OggOptions) -> Vec<Vec<f32>> {
    let mut pcm = Vec::new();
    decode_reader(bytes, options, |event| match event {
        OggEvent::StreamStart(info) => {
            pcm.resize(info.identification.channels as usize, Vec::new())
        }
        OggEvent::Pcm(block) => {
            for (out, plane) in pcm.iter_mut().zip(block.pcm.planes()) {
                out.extend_from_slice(plane);
            }
        }
        _ => {}
    })
    .unwrap();
    pcm
}
fn suffix(reader: &mut OggSeekReader<Cursor<&[u8]>>, target: usize, expected: &[Vec<f32>]) {
    reader
        .seek(reader.selected().info.id, target as u64)
        .unwrap();
    let mut position = target;
    let mut ended = false;
    while reader
        .read_next(|event| match event {
            OggEvent::Pcm(block) => {
                assert_eq!(block.start_frame as usize, position);
                for (expected, plane) in expected.iter().zip(block.pcm.planes()) {
                    assert_eq!(plane, &expected[position..position + block.pcm.frames]);
                }
                position += block.pcm.frames;
            }
            OggEvent::StreamEnd(end) => {
                assert_eq!(end.frames as usize, expected[0].len());
                ended = true;
            }
            _ => {}
        })
        .unwrap()
    {}
    assert_eq!(position, expected[0].len());
    assert!(ended);
}

#[test]
fn indexed_endpoint_policy_clips_across_packets_and_preceding_pages() {
    for flush in [true, false] {
        let source = baseline(flush);
        let reference = full(&source, OggOptions::default());
        let length = reference[0].len() - 6000;
        let broken = rewrite(&source, |header, _| {
            if header.is_eos() {
                header.granule_position = length as u64;
            }
        });
        assert!(OggSeekReader::new(Cursor::new(broken.as_slice())).is_err());
        let options = SeekOptions {
            trim_to_final_granule: true,
            ..SeekOptions::default()
        };
        let mut reader =
            OggSeekReader::with_options(Cursor::new(broken.as_slice()), options).unwrap();
        assert_eq!(reader.selected().end.frames, length as u64);
        assert_eq!(reader.selected().end_padding_frames, 6000);
        assert!(reader.selected().extended_end_trim);
        assert_eq!(
            reader
                .index()
                .container()
                .compatibility()
                .decreasing_eos_granules,
            flush as u64
        );
        let expected: Vec<_> = reference.iter().map(|p| p[..length].to_vec()).collect();
        for frame in [length, length - 1, length / 2, 0, 1, length - 4096] {
            suffix(&mut reader, frame, &expected);
        }
        let invalid = rewrite(&source, |header, _| {
            if header.is_eos() {
                header.granule_position += 1;
            }
        });
        assert!(OggSeekReader::with_options(Cursor::new(invalid.as_slice()), options).is_err());
        // Forward-only decoding remains strict; it cannot retract earlier PCM.
        assert!(decode_reader(broken.as_slice(), OggOptions::default(), |_| {}).is_err());
    }
    let source = baseline(false);
    let shifted = rewrite(&source, |header, _| {
        if header.sequence >= 2 && header.granule_position != u64::MAX {
            header.granule_position += 900;
        }
    });
    let control = OggSeekReader::new(Cursor::new(shifted.as_slice())).unwrap();
    assert_eq!(control.selected().end.start_granule, 900);
    let invalid = rewrite(&shifted, |header, _| {
        if header.is_eos() {
            header.granule_position = 899;
        }
    });
    assert!(
        OggSeekReader::with_options(
            Cursor::new(invalid.as_slice()),
            SeekOptions {
                trim_to_final_granule: true,
                ..SeekOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn zero_span_header_quirk_works_in_streaming_indexed_and_reset_paths() {
    let mut packets = support::packets(SOURCE);
    let long = vec![b'x'; 70000];
    packets[1] = Comments {
        vendor: b"test",
        comments: vec![&long],
    }
    .encode(&Limits::default())
    .unwrap();
    let mut decoder = support::decoder(&packets);
    let mut writer = PacketWriter::new(Vec::new(), 41);
    for packet in &packets[..3] {
        writer.write_packet(packet, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    for (number, packet) in packets[3..].iter().enumerate() {
        let block = decoder.decode(packet).unwrap().unwrap();
        writer
            .write_packet(
                packet,
                block.start_frame + block.frames as u64,
                number + 4 == packets.len(),
            )
            .unwrap();
    }
    let source = writer.into_inner();
    let expected = full(&source, OggOptions::default());
    let broken = rewrite(&source, |header, complete| {
        if complete == 0 {
            header.granule_position = 0;
        }
    });
    assert!(decode_reader(broken.as_slice(), OggOptions::default(), |_| {}).is_err());
    let options = OggOptions {
        allow_zero_span_granules: true,
        ..OggOptions::default()
    };
    assert_eq!(full(&broken, options), expected);
    let mut stream = OggStreamDecoder::new(options);
    for _ in 0..2 {
        let mut count = 0;
        for input in broken.chunks(17) {
            let mut consumed = 0;
            while consumed < input.len() {
                consumed += stream
                    .push(&input[consumed..], |event| {
                        if let OggEvent::Pcm(pcm) = event {
                            count += pcm.pcm.frames;
                        }
                    })
                    .unwrap();
            }
        }
        stream.finish().unwrap();
        assert_eq!(count, expected[0].len());
        assert_eq!(stream.compatibility().zero_span_granules, 1);
        stream.reset();
        assert_eq!(stream.compatibility().zero_span_granules, 0);
    }
    let mut indexed = OggSeekReader::with_options(
        Cursor::new(broken.as_slice()),
        SeekOptions {
            allow_zero_span_granules: true,
            ..SeekOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        indexed
            .index()
            .container()
            .compatibility()
            .zero_span_granules,
        1
    );
    for frame in [0, 17, expected[0].len()] {
        suffix(&mut indexed, frame, &expected);
    }
    assert!(
        OggSeekReader::with_options(
            Cursor::new(broken.as_slice()),
            SeekOptions {
                trim_to_final_granule: true,
                ..SeekOptions::default()
            }
        )
        .is_err()
    );
}

struct Reference {
    serial: u32,
    rate: u32,
    declared: u64,
    planes: Vec<Vec<f32>>,
}
fn reference(bytes: &[u8]) -> Reference {
    let wire = support::file_oracle("oggpcm", bytes);
    let mut input = wire.as_slice();
    assert_eq!(support::u32_in(&mut input), 1);
    let serial = support::u32_in(&mut input);
    let channels = support::u32_in(&mut input) as usize;
    let rate = support::u32_in(&mut input);
    let declared = u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap());
    let mut planes = vec![Vec::new(); channels];
    while !input.is_empty() {
        assert_eq!(support::u32_in(&mut input), 0);
        let count = support::u32_in(&mut input);
        for plane in &mut planes {
            for _ in 0..count {
                plane.push(f32::from_bits(support::u32_in(&mut input)));
            }
        }
    }
    Reference {
        serial,
        rate,
        declared,
        planes,
    }
}

#[test]
#[ignore = "requires pinned external corpus and libvorbisfile oracle"]
fn historical_and_itg_song_pcm_counts_and_native_seeks() {
    use std::{fmt::Write, fs, path::Path};
    let root = std::env::var_os("DEADLIB_VORBIS_EXTERNAL_CORPUS")
        .expect("set DEADLIB_VORBIS_EXTERNAL_CORPUS");
    let root = Path::new(&root);
    for (chart, title, music) in [
        ("itgmania/goin-under.ssc", "Goin' Under", "Goin' Under.ogg"),
        (
            "itgmania/mecha-tribe-assault.ssc",
            "Mecha-Tribe Assault",
            "Mecha-Tribe Assault.ogg",
        ),
    ] {
        let chart = fs::read_to_string(root.join(chart)).unwrap();
        assert!(chart.contains(&format!("#TITLE:{title};")));
        assert!(chart.contains(&format!("#MUSIC:{music};")));
    }
    let mut report = String::from(
        "path\tzero_span\textended_end_trim\tdeclared_frames\treference_emitted_frames\tnative_frames\tend_padding_frames\tsamples\tpeak_abs_error\trms_error\n",
    );
    for (name, zero_span, final_trim, frames, ref_frames) in [
        (
            "floor0/vorbis_floor_type_0.ogg",
            true,
            false,
            6262786,
            6262786,
        ),
        ("legacy/Lumme-Badloop.ogg", false, false, 17862912, 17862912),
        ("itgmania/goin-under.ogg", false, false, 5443200, 5443200),
        (
            "itgmania/mecha-tribe-assault.ogg",
            false,
            true,
            4630500,
            4762304,
        ),
    ] {
        eprintln!("{name}");
        let bytes = fs::read(root.join(name)).unwrap();
        let reference = reference(&bytes);
        assert_eq!(reference.declared, frames);
        assert_eq!(reference.planes.len(), 2);
        assert_eq!(reference.rate, 44100);
        assert_eq!(reference.planes[0].len() as u64, ref_frames);
        let strict = OggSeekReader::new(Cursor::new(bytes.as_slice()));
        assert_eq!(strict.is_err(), zero_span || final_trim);
        drop(strict);
        let mut reader = OggSeekReader::with_options(
            Cursor::new(bytes.as_slice()),
            SeekOptions {
                allow_zero_span_granules: zero_span,
                trim_to_final_granule: final_trim,
                ..SeekOptions::default()
            },
        )
        .unwrap();
        let id = reader.selected().info.id;
        assert_eq!(id.serial, reference.serial);
        assert_eq!(reader.selected().end.frames, frames);
        assert_eq!(reader.selected().extended_end_trim, final_trim);
        let stats = reader.index().container().compatibility();
        assert_eq!(stats.zero_span_granules, zero_span as u64);
        assert_eq!(stats.decreasing_eos_granules, final_trim as u64);
        let padding = reader.selected().end_padding_frames;
        if final_trim {
            assert_eq!(padding, 132828);
        }
        let mut position = 0;
        let mut difference = support::Difference::default();
        while reader
            .read_packet(|event| match event {
                OggEvent::Pcm(block) => {
                    assert_eq!(block.start_frame as usize, position);
                    assert_eq!(block.start_granule as usize, position);
                    assert!(position + block.pcm.frames <= frames as usize);
                    for (native, reference) in block.pcm.planes().zip(&reference.planes) {
                        for (&a, &b) in native
                            .iter()
                            .zip(&reference[position..position + block.pcm.frames])
                        {
                            difference.add(a, b);
                        }
                    }
                    position += block.pcm.frames;
                }
                OggEvent::StreamEnd(end) => assert_eq!(end.frames, frames),
                _ => {}
            })
            .unwrap()
        {}
        assert_eq!(position as u64, frames);
        let (peak, rms) = if zero_span {
            (5e-5, 3e-6)
        } else {
            (3e-6, 5e-7)
        };
        difference.check_with_bounds(name, peak, rms);
        writeln!(&mut report, "{name}\t{zero_span}\t{final_trim}\t{frames}\t{ref_frames}\t{position}\t{padding}\t{}\t{:.12e}\t{:.12e}", difference.samples, difference.peak_error, difference.rms()).unwrap();
        let mut seeks = support::Difference::default();
        for target in [
            frames,
            frames - 1,
            0,
            1,
            127,
            575,
            576,
            frames / 2,
            frames - 4096,
        ] {
            reader.seek(id, target).unwrap();
            let wanted = (frames - target).min(4096) as usize;
            let mut position = 0;
            while position < wanted {
                assert!(
                    reader
                        .read_packet(|event| {
                            if let OggEvent::Pcm(block) = event {
                                if position == wanted {
                                    return;
                                }
                                assert_eq!(block.start_frame, target + position as u64);
                                let count = block.pcm.frames.min(wanted - position);
                                for (native, reference) in block.pcm.planes().zip(&reference.planes)
                                {
                                    for (&a, &b) in native[..count].iter().zip(
                                        &reference[target as usize + position
                                            ..target as usize + position + count],
                                    ) {
                                        seeks.add(a, b);
                                    }
                                }
                                position += count;
                            }
                        })
                        .unwrap()
                );
            }
        }
        seeks.check_with_bounds("legacy/native seek", peak, rms);
        if zero_span {
            let mut reader = OggReader::with_options(
                bytes.as_slice(),
                OggOptions {
                    allow_zero_span_granules: true,
                    ..OggOptions::default()
                },
            );
            let mut frames_seen = 0;
            while reader
                .read_next(|event| {
                    if let OggEvent::Pcm(block) = event {
                        frames_seen += block.pcm.frames as u64;
                    }
                })
                .unwrap()
            {}
            assert_eq!(frames_seen, frames);
            assert_eq!(reader.compatibility().zero_span_granules, 1);
        }
    }
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_LEGACY_REPORT") {
        fs::write(path, report).unwrap();
    }
}
