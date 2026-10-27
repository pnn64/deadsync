mod support;
use deadlib_audio_decode_ogg::{Error as OggError, PageReader};
use deadlib_audio_decode_vorbis::*;
use std::{fmt::Write, fs, io::Cursor, path::PathBuf};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef, Position},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

fn corpus(name: &str) -> Vec<u8> {
    let root = std::env::var_os("DEADLIB_VORBIS_EXTERNAL_CORPUS")
        .expect("set DEADLIB_VORBIS_EXTERNAL_CORPUS");
    fs::read(PathBuf::from(root).join("ffmpeg").join(name)).unwrap()
}

fn reference(bytes: &[u8], info: &StreamInfo, frames: usize) -> Vec<Vec<f32>> {
    let wire = support::file_oracle("oggpcm", bytes);
    let mut input = wire.as_slice();
    assert_eq!(support::u32_in(&mut input), 1);
    assert_eq!(support::u32_in(&mut input), info.id.serial);
    assert_eq!(
        support::u32_in(&mut input),
        info.identification.channels as u32
    );
    assert_eq!(support::u32_in(&mut input), info.identification.sample_rate);
    assert_eq!(
        u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap()),
        frames as u64
    );
    let mut planes = vec![Vec::new(); info.identification.channels as usize];
    while !input.is_empty() {
        assert_eq!(support::u32_in(&mut input), 0);
        let count = support::u32_in(&mut input);
        for plane in &mut planes {
            for _ in 0..count {
                plane.push(f32::from_bits(support::u32_in(&mut input)));
            }
        }
    }
    assert!(planes.iter().all(|p| p.len() == frames));
    planes
}

fn position(channel: Channel) -> Position {
    match channel {
        Channel::Mono | Channel::FrontCenter => Position::FRONT_CENTER,
        Channel::FrontLeft => Position::FRONT_LEFT,
        Channel::FrontRight => Position::FRONT_RIGHT,
        Channel::SideLeft => Position::SIDE_LEFT,
        Channel::SideRight => Position::SIDE_RIGHT,
        Channel::RearLeft => Position::REAR_LEFT,
        Channel::RearRight => Position::REAR_RIGHT,
        Channel::RearCenter => Position::REAR_CENTER,
        Channel::Lfe => Position::LFE1,
    }
}

fn symphonia(bytes: &[u8], info: &StreamInfo, pcm: &[Vec<f32>]) -> support::Difference {
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let track = format.default_track(TrackType::Audio).unwrap();
    let track_id = track.id;
    let params = track.codec_params.as_ref().unwrap().audio().unwrap();
    assert_eq!(params.sample_rate, Some(info.identification.sample_rate));
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .unwrap();
    let order = channel_order(info.identification.channels).unwrap();
    let mut difference = support::Difference::default();
    let mut offset = 0;
    while let Some(packet) = format.next_packet().unwrap() {
        assert_eq!(packet.track_id, track_id);
        let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
            panic!("expected f32");
        };
        let frames = audio.frames();
        if frames != 0 {
            assert_eq!(
                packet.pts.get() + packet.trim_start.get() as i64,
                offset as i64
            );
            assert_eq!(packet.dur.get(), frames as u64);
        }
        assert!(offset + frames <= pcm[0].len());
        for (&channel, plane) in order.iter().zip(pcm) {
            let reference = audio.plane_by_position(position(channel)).unwrap();
            assert_eq!(reference.len(), frames);
            for (&a, &b) in plane[offset..offset + frames].iter().zip(reference) {
                difference.add(a, b);
            }
        }
        offset += frames;
    }
    assert_eq!(offset, pcm[0].len());
    difference
}

fn exact_block(block: OggPcm<'_>, info: &StreamInfo, pcm: &[Vec<f32>], offset: &mut usize) {
    assert_eq!(block.stream, info.id);
    assert_eq!(block.sample_rate, info.identification.sample_rate);
    assert_eq!(block.start_frame, *offset as u64);
    assert_eq!(block.start_granule, *offset as u64);
    assert_eq!(block.pcm.channels, pcm.len());
    for (native, expected) in block.pcm.planes().zip(pcm) {
        for (&a, &b) in native
            .iter()
            .zip(&expected[*offset..*offset + block.pcm.frames])
        {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }
    *offset += block.pcm.frames;
}

#[test]
#[ignore = "requires pinned external corpus and libvorbisfile oracle"]
fn historical_floor_zero_and_aotuv_surround_pcm_streaming_reset_and_seeks() {
    let mut report = String::from(
        "path\tdecoder\tchannels\trate\tframes\tsamples\tpeak_abs_error\trms_error\treference_peak\tstream_passes\tseeks\traw_frames\tnominally_truncated_packets\n",
    );
    for (name, vendor, channels, frames, floor_zero) in [
        (
            "coyote.ogg",
            "Xiphophorus libVorbis I 20001031",
            2,
            3671424,
            true,
        ),
        (
            "test6.ogg",
            "AO; aoTuV b2 [20040420]  (based on Xiph.Org's 1.0.1)",
            6,
            508416,
            false,
        ),
    ] {
        let bytes = corpus(name);
        let packets = support::packets(&bytes);
        let setup = support::decoder(&packets);
        assert_eq!(
            setup
                .setup()
                .floors()
                .iter()
                .any(|f| matches!(f, Floor::Type0(_))),
            floor_zero
        );
        drop(setup);
        let mut info = None;
        let mut end = None;
        let mut pcm = vec![Vec::new(); channels];
        decode_reader(
            bytes.as_slice(),
            OggOptions::default(),
            |event| match event {
                OggEvent::StreamStart(value) => {
                    assert!(info.replace(value.clone()).is_none());
                    assert_eq!(value.metadata.vendor(), vendor.as_bytes());
                    assert_eq!(value.identification.channels as usize, channels);
                    assert_eq!(value.identification.sample_rate, 48000);
                }
                OggEvent::Pcm(block) => {
                    assert_eq!(block.start_frame as usize, pcm[0].len());
                    assert_eq!(block.start_frame, block.start_granule);
                    for (plane, samples) in pcm.iter_mut().zip(block.pcm.planes()) {
                        plane.extend_from_slice(samples);
                    }
                }
                OggEvent::StreamEnd(value) => assert!(end.replace(value).is_none()),
                _ => panic!("unexpected event"),
            },
        )
        .unwrap();
        let info = info.unwrap();
        let end = end.unwrap();
        assert_eq!(end.frames, frames as u64);
        assert_eq!((end.start_granule, end.end_granule), (0, frames as u64));
        assert!(pcm.iter().all(|p| p.len() == frames));
        let (raw_frames, nominal_truncations, raw_difference) = raw_reference(&packets);
        assert_eq!(
            (raw_frames, nominal_truncations),
            if floor_zero {
                (3672128, 4)
            } else {
                (508480, 0)
            }
        );
        eprintln!(
            "{name}: {raw_frames} raw frames, {nominal_truncations} nominally truncated packets"
        );
        let reference = reference(&bytes, &info, frames);
        let mut difference = support::Difference::default();
        for (plane, reference) in pcm.iter().zip(&reference) {
            for (&a, &b) in plane.iter().zip(reference) {
                difference.add(a, b);
            }
        }
        let symphonia = symphonia(&bytes, &info, &pcm);
        let (peak, rms) = if floor_zero {
            (5e-5, 3e-6)
        } else {
            (3e-6, 5e-7)
        };
        raw_difference.check_with_bounds(&format!("{name}/raw libvorbis"), peak, rms);
        for (engine, difference) in [
            ("libvorbisfile-1.3.7", &difference),
            ("Symphonia-0.6.1", &symphonia),
        ] {
            difference.check_with_bounds(&format!("{name}/{engine}"), peak, rms);
            assert_eq!(difference.samples, (frames * channels) as u64);
            writeln!(&mut report, "ffmpeg/{name}\t{engine}\t{channels}\t48000\t{frames}\t{}\t{:.12e}\t{:.12e}\t{:.9}\t2\t256\t{raw_frames}\t{nominal_truncations}", difference.samples, difference.peak_error, difference.rms(), difference.peak_reference).unwrap();
        }
        let mut stream = OggStreamDecoder::default();
        for _ in 0..2 {
            stream.reset();
            let (mut input, mut offset, mut ended, mut chunk) = (0, 0, false, 0);
            while input < bytes.len() {
                let stop = (input + [1, 7, 17, 255, 4096, 65025][chunk % 6]).min(bytes.len());
                while input < stop {
                    let consumed = stream
                        .push(&bytes[input..stop], |event| match event {
                            OggEvent::Pcm(block) => exact_block(block, &info, &pcm, &mut offset),
                            OggEvent::StreamEnd(value) => {
                                assert_eq!(value, end);
                                assert!(!ended);
                                ended = true;
                            }
                            OggEvent::StreamStart(value) => assert_eq!(value, &info),
                            _ => panic!("unexpected event"),
                        })
                        .unwrap();
                    assert!(consumed > 0);
                    input += consumed;
                }
                chunk += 1;
            }
            stream.finish().unwrap();
            assert_eq!(offset, frames);
            assert!(ended);
        }
        let mut random = 0x9e37_79b9u32;
        let mut targets = vec![
            frames,
            frames - 1,
            0,
            1,
            127,
            576,
            frames / 2,
            frames - 4096,
        ];
        while targets.len() < 128 {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            targets.push(random as usize % frames);
        }
        for packet_preroll in [false, true] {
            let mut reader = OggSeekReader::with_options(
                Cursor::new(bytes.as_slice()),
                SeekOptions {
                    packet_preroll,
                    ..SeekOptions::default()
                },
            )
            .unwrap();
            assert_eq!(reader.selected().info, info);
            assert_eq!(reader.selected().end, end);
            for (request, &target) in targets.iter().enumerate() {
                reader.seek(info.id, target as u64).unwrap();
                let mut offset = target;
                let wanted = (target + 4096).min(frames);
                let mut ended = false;
                loop {
                    let mut event = |event: OggEvent<'_>| match event {
                        OggEvent::Pcm(block) => exact_block(block, &info, &pcm, &mut offset),
                        OggEvent::StreamEnd(value) => {
                            assert_eq!(value, end);
                            ended = true;
                        }
                        OggEvent::StreamStart(value) => assert_eq!(value, &info),
                        _ => panic!("unexpected event"),
                    };
                    let more = if request % 2 == 0 {
                        reader.read_packet(&mut event)
                    } else {
                        reader.read_next(&mut event)
                    }
                    .unwrap();
                    if !more || offset >= wanted {
                        break;
                    }
                }
                assert!(offset >= wanted);
                if wanted == frames {
                    while reader
                        .read_packet(|event| match event {
                            OggEvent::Pcm(block) => exact_block(block, &info, &pcm, &mut offset),
                            OggEvent::StreamEnd(value) => {
                                assert_eq!(value, end);
                                ended = true;
                            }
                            _ => panic!("unexpected event"),
                        })
                        .unwrap()
                    {}
                    assert_eq!(offset, frames);
                    assert!(ended);
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_HISTORICAL_REPORT") {
        fs::write(path, report).unwrap();
    }
}

#[test]
#[ignore = "requires pinned external malformed corpus"]
fn known_bad_endpoints_sequence_gaps_and_game_crcs_stay_rejected() {
    let cases = [
        (
            "1sec.ogg",
            Error::Invalid("EOS granule outside final packet overlap"),
        ),
        (
            "virginradio-three-consecutive-chains.ogg",
            Error::Container(OggError::Sequence {
                serial: 1151398491,
                expected: 3,
                actual: 1007,
            }),
        ),
        (
            "mgs1-sample1.ogg",
            Error::Container(OggError::Checksum {
                expected: 0x442b2d41,
                actual: 0x32e99750,
            }),
        ),
        (
            "mgs1-sample2.ogg",
            Error::Container(OggError::Checksum {
                expected: 0xf68f9f2e,
                actual: 0x1fc62c31,
            }),
        ),
        (
            "mgs1-sample3.ogg",
            Error::Container(OggError::Checksum {
                expected: 0x40f0c90f,
                actual: 0xebf89a80,
            }),
        ),
    ];
    for (name, expected) in cases {
        let bytes = corpus(name);
        for zero_span in [false, true] {
            let options = OggOptions {
                allow_zero_span_granules: zero_span,
                ..OggOptions::default()
            };
            let mut ended = 0;
            let error = decode_reader(bytes.as_slice(), options, |event| {
                if matches!(event, OggEvent::StreamEnd(_)) {
                    ended += 1;
                }
            })
            .unwrap_err();
            assert_eq!(error, expected, "{name}");
            assert_eq!(ended, 0);
            let mut stream = OggStreamDecoder::new(options);
            let result = (|| {
                for chunk in bytes.chunks(17) {
                    let mut offset = 0;
                    while offset < chunk.len() {
                        let consumed = stream.push(&chunk[offset..], |_| {})?;
                        assert!(consumed > 0);
                        offset += consumed;
                    }
                }
                stream.finish()
            })();
            assert_eq!(result.unwrap_err(), expected, "{name}/stream");
            for trim in [false, true] {
                let error = OggSeekReader::with_options(
                    Cursor::new(bytes.as_slice()),
                    SeekOptions {
                        allow_zero_span_granules: zero_span,
                        trim_to_final_granule: trim,
                        ..SeekOptions::default()
                    },
                )
                .err()
                .expect("malformed input rejected");
                assert_eq!(error, expected, "{name}/index");
            }
        }
        eprintln!("ffmpeg/{name}: {expected}");
    }
    // With a zero origin this endpoint exceeds raw synthesis. A positive
    // initial offset would require the second audio packet to flush its page
    // (Appendix A.2), but all audio packets occupy one page here.
    let bytes = corpus("1sec.ogg");
    let packets = support::packets(&bytes);
    let mut decoder = support::decoder(&packets);
    let mut frames = 0;
    for packet in &packets[3..] {
        let block = decoder.decode(packet).unwrap().unwrap();
        assert!(!block.truncated);
        frames += block.frames as u64;
    }
    let mut pages = PageReader::new(bytes.as_slice());
    let mut endpoint = 0;
    while let Some(page) = pages.next_page().unwrap() {
        endpoint = page.page.header().granule_position;
        if page.page.header().is_eos() {
            assert!(page.page.completed_packets() > 2);
        }
    }
    eprintln!("1sec.ogg: raw frames {frames}, final granule {endpoint}");
    assert_eq!((frames, endpoint), (44608, 44619));
}

#[test]
#[ignore = "requires pinned external corpus and libvorbisfile oracle"]
fn ffmpeg_endpoint_exceeds_independent_raw_synthesis() {
    let bytes = corpus("1sec.ogg");
    let packets = support::packets(&bytes);
    let (frames, nominal_truncations, difference) = raw_reference(&packets);
    assert_eq!(frames, 44608);
    assert_eq!(nominal_truncations, 0);
    assert_eq!(difference.samples, 89216);
    difference.check("1sec.ogg/raw libvorbis");
    // libvorbisfile reports the available raw PCM despite the larger encoded
    // endpoint. Native strict mapping rejects this initial-offset framing.
    let info = StreamInfo {
        id: StreamId {
            serial: 3384106372,
            chain: 0,
        },
        identification: Identification::parse(&packets[0]).unwrap(),
        metadata: Metadata::default(),
    };
    let reference = reference(&bytes, &info, 44608);
    assert_eq!(reference[0].len(), 44608);
    eprintln!(
        "1sec.ogg: libvorbis raw and libvorbisfile declare/emit 44608 frames; encoded EOS 44619"
    );
}

fn raw_reference(packets: &[Vec<u8>]) -> (u64, usize, support::Difference) {
    let wire = support::oracle(
        "pcm",
        &packets.iter().map(Vec::as_slice).collect::<Vec<_>>(),
    );
    let mut input = wire.as_slice();
    let mut decoder = support::decoder(packets);
    assert_eq!(
        support::u32_in(&mut input),
        decoder.setup().channels() as u32
    );
    let mut frames = 0;
    let mut nominal_truncations = 0;
    let mut difference = support::Difference::default();
    for packet in &packets[3..] {
        assert_eq!(support::take(&mut input, 1), b"P");
        let block = decoder.decode(packet).unwrap().unwrap();
        nominal_truncations += usize::from(block.truncated);
        assert_eq!(support::u32_in(&mut input) as usize, block.block_size);
        assert_eq!(support::u32_in(&mut input) as usize, block.frames);
        assert_eq!(block.start_frame, frames);
        frames += block.frames as u64;
        for plane in block.planes() {
            for &sample in plane {
                difference.add(sample, f32::from_bits(support::u32_in(&mut input)));
            }
        }
    }
    assert!(input.is_empty());
    (frames, nominal_truncations, difference)
}
