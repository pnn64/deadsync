mod support;
use deadlib_audio_decode_vorbis::*;

fn comments() -> Comments<'static> {
    Comments {
        vendor: b"deadlib native encoder",
        comments: vec![b"TITLE=round trip", b"X-BINARY=a\0b", b"TITLE=duplicate"],
    }
}
fn signal(frames: usize, channels: usize) -> Vec<f32> {
    let mut seed = 31u32;
    (0..frames * channels)
        .map(|i| {
            let frame = i / channels;
            let channel = i % channels;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = seed as i32 as f32 / i32::MAX as f32 * 0.08;
            let tone = (frame as f64 * (0.037 + channel as f64 * 0.011)).sin() as f32 * 0.5;
            tone + noise + if frame.is_multiple_of(197) { 0.2 } else { 0.0 }
        })
        .collect()
}
fn encode(pcm: &[f32], options: EncodeOptions, chunk: usize, planar: bool) -> Vec<u8> {
    let channels = options.channels as usize;
    let mut encoder = OggEncoder::new(Vec::new(), 0x7788, options, &comments()).unwrap();
    for data in pcm.chunks(chunk * channels) {
        if planar {
            let planes: Vec<Vec<f32>> = (0..channels)
                .map(|ch| data.chunks_exact(channels).map(|f| f[ch]).collect())
                .collect();
            let slices: Vec<_> = planes.iter().map(Vec::as_slice).collect();
            encoder.push_planar(&slices).unwrap();
        } else {
            encoder.push_interleaved(data).unwrap();
        }
        encoder.push_interleaved(&[]).unwrap();
    }
    assert_eq!(encoder.position(), (pcm.len() / channels) as u64);
    encoder.finish().unwrap();
    let length = encoder.get_ref().len();
    encoder.finish().unwrap();
    assert_eq!(encoder.get_ref().len(), length);
    assert!(encoder.push_interleaved(&[]).is_err());
    encoder.into_inner()
}
fn decode(bytes: &[u8], frames: usize, channels: usize) -> Vec<Vec<f32>> {
    let mut output = vec![Vec::new(); channels];
    let mut starts = 0;
    let mut ends = 0;
    decode_reader(bytes, OggOptions::default(), |event| match event {
        OggEvent::StreamStart(info) => {
            starts += 1;
            assert_eq!(info.identification.channels as usize, channels);
            assert_eq!(
                info.metadata.values(b"TITLE").collect::<Vec<_>>(),
                [b"round trip".as_slice(), b"duplicate"]
            );
        }
        OggEvent::Pcm(block) => {
            assert_eq!(block.start_frame as usize, output[0].len());
            for (plane, data) in output.iter_mut().zip(block.pcm.planes()) {
                plane.extend_from_slice(data);
            }
        }
        OggEvent::StreamEnd(end) => {
            ends += 1;
            assert_eq!(end.frames as usize, frames);
        }
        _ => panic!("unexpected encoder stream event"),
    })
    .unwrap();
    assert_eq!((starts, ends), (1, 1));
    assert!(output.iter().all(|p| p.len() == frames));
    output
}

fn symphonia(bytes: &[u8], frames: usize, channels: usize) -> Vec<Vec<f32>> {
    use std::io::Cursor;
    use symphonia::core::{
        audio::{Audio, GenericAudioBufferRef, Position},
        codecs::audio::AudioDecoderOptions,
        formats::{FormatOptions, TrackType, probe::Hint},
        io::MediaSourceStream,
        meta::MetadataOptions,
    };
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
    let id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(
            track.codec_params.as_ref().unwrap().audio().unwrap(),
            &AudioDecoderOptions::default(),
        )
        .unwrap();
    let mut output = vec![Vec::new(); channels];
    while let Some(packet) = format.next_packet().unwrap() {
        assert_eq!(packet.track_id, id);
        let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
            panic!("expected f32");
        };
        assert_eq!(audio.spec().channels().count(), channels);
        // Compare declared speaker identities, not the APIs' different plane
        // conventions. Symphonia exposes canonical position order.
        for (ch, out) in output.iter_mut().enumerate() {
            let position = match channel_order(channels as u8).unwrap()[ch] {
                Channel::Mono | Channel::FrontLeft => Position::FRONT_LEFT,
                Channel::FrontCenter => Position::FRONT_CENTER,
                Channel::FrontRight => Position::FRONT_RIGHT,
                Channel::SideLeft => Position::SIDE_LEFT,
                Channel::SideRight => Position::SIDE_RIGHT,
                Channel::RearLeft => Position::REAR_LEFT,
                Channel::RearCenter => Position::REAR_CENTER,
                Channel::RearRight => Position::REAR_RIGHT,
                Channel::Lfe => Position::LFE1,
            };
            let canonical = audio
                .spec()
                .channels()
                .get_canonical_index_for_positioned_channel(position)
                .unwrap();
            out.extend_from_slice(audio.plane(canonical).unwrap());
        }
    }
    assert!(output.iter().all(|p| p.len() == frames));
    output
}

#[test]
fn native_generated_packets_match_symphonia_without_channel_coupling() {
    for channels in 1..=8 {
        for passes in 1..=4 {
            let options = EncodeOptions {
                channels,
                block_size: 256,
                residue_passes: passes,
                ..EncodeOptions::default()
            };
            for frames in [0, 1, 133, 519] {
                let bytes = encode(&signal(frames, channels as usize), options, 31, false);
                let native = decode(&bytes, frames, channels as usize);
                let reference = symphonia(&bytes, frames, channels as usize);
                let mut difference = support::Difference::default();
                for (a, b) in native.iter().zip(reference) {
                    for (&x, y) in a.iter().zip(b) {
                        difference.add(x, y);
                    }
                }
                difference.check("native encoding/Symphonia");
            }
        }
    }
}

#[test]
fn native_encoded_setup_pcm_lengths_chunking_and_precision() {
    for n in [64, 128, 256, 512, 1024, 2048, 4096, 8192] {
        let options = EncodeOptions {
            block_size: n,
            channels: 2,
            ..EncodeOptions::default()
        };
        for frames in [0, 1, n / 2 - 1, n / 2, n / 2 + 1, n * 2 + 17] {
            let pcm = signal(frames, 2);
            let bytes = encode(&pcm, options, 73, false);
            assert_eq!(bytes, encode(&pcm, options, 11, true));
            let output = decode(&bytes, frames, 2);
            let mut error = 0.0;
            let mut energy = 0.0;
            for (frame, actual) in pcm.as_chunks::<2>().0.iter().enumerate() {
                for channel in 0..2 {
                    let delta = actual[channel] as f64 - output[channel][frame] as f64;
                    error += delta * delta;
                    energy += (actual[channel] as f64).powi(2);
                }
            }
            let snr = 10.0 * (energy / error.max(1e-30)).log10();
            if frames > n {
                assert!(snr > 30.0, "N={n}, frames={frames}, SNR={snr}");
            }
        }
    }
}

#[test]
fn encoder_handles_channel_range_silence_dc_impulses_and_precision_stages() {
    for channels in [1, 2, 3, 6, 8, 16, 255] {
        for passes in 1..=4 {
            let options = EncodeOptions {
                channels,
                block_size: 64,
                residue_passes: passes,
                ..EncodeOptions::default()
            };
            let mut mixed = signal(97, channels as usize);
            if channels > 1 {
                for frame in mixed.chunks_exact_mut(channels as usize) {
                    frame[1] = 0.0;
                }
            }
            for pcm in [
                vec![0.0; 97 * channels as usize],
                signal(97, channels as usize),
                mixed,
                vec![1.0; 97 * channels as usize],
            ] {
                let bytes = encode(&pcm, options, 13, false);
                let output = decode(&bytes, 97, channels as usize);
                assert!(output.iter().flatten().all(|x| x.is_finite()));
                if pcm.iter().all(|&x| x == 0.0) {
                    assert!(output.iter().flatten().all(|&x| x == 0.0));
                }
            }
        }
    }
}

#[test]
fn raw_encoder_argument_errors_callback_failures_limits_and_reset() {
    let options = EncodeOptions {
        channels: 1,
        block_size: 64,
        max_frames: 100,
        ..EncodeOptions::default()
    };
    let mut encoder = PacketEncoder::new(options).unwrap();
    let mut packets = Vec::new();
    let collect = |p: EncodedPacket<'_>| {
        packets.push((
            p.data.to_vec(),
            p.number,
            p.granule_position,
            p.last_in_stream,
        ));
        Ok(())
    };
    encoder.push_interleaved(&signal(71, 1), collect).unwrap();
    assert_eq!(encoder.position(), 71);
    assert!(encoder.push_interleaved(&[f32::NAN], |_| Ok(())).is_err());
    assert!(encoder.push_interleaved(&[4.1], |_| Ok(())).is_err());
    assert!(encoder.push_planar(&[], |_| Ok(())).is_err());
    assert!(encoder.push_interleaved(&[0.0; 30], |_| Ok(())).is_err());
    assert_eq!(encoder.position(), 71);
    encoder
        .finish(|p| {
            packets.push((
                p.data.to_vec(),
                p.number,
                p.granule_position,
                p.last_in_stream,
            ));
            Ok(())
        })
        .unwrap();
    encoder.reset();
    let mut replay = Vec::new();
    encoder
        .push_interleaved(&signal(71, 1), |p| {
            replay.push((
                p.data.to_vec(),
                p.number,
                p.granule_position,
                p.last_in_stream,
            ));
            Ok(())
        })
        .unwrap();
    encoder
        .finish(|p| {
            replay.push((
                p.data.to_vec(),
                p.number,
                p.granule_position,
                p.last_in_stream,
            ));
            Ok(())
        })
        .unwrap();
    assert_eq!(packets, replay);
    assert_eq!(packets.last().unwrap().2, 71);
    assert!(packets.last().unwrap().3);
    encoder.reset();
    assert!(
        encoder
            .push_interleaved(&[0.0; 32], |_| Err(Error::Invalid("consumer failure")))
            .is_err()
    );
    assert_eq!(encoder.finish(|_| Ok(())).unwrap_err(), Error::Failed);
    encoder.reset();
    encoder.finish(|_| Ok(())).unwrap();
    for options in [
        EncodeOptions {
            channels: 0,
            ..options
        },
        EncodeOptions {
            block_size: 63,
            ..options
        },
        EncodeOptions {
            residue_passes: 0,
            ..options
        },
        EncodeOptions {
            max_memory: 0,
            ..options
        },
        EncodeOptions {
            max_packet_bytes: 1,
            ..options
        },
        EncodeOptions {
            max_frames: u64::MAX,
            ..options
        },
    ] {
        assert!(PacketEncoder::new(options).is_err());
    }
}

#[test]
#[ignore = "requires independently built libvorbisfile oracle"]
fn native_encoding_is_accepted_by_libvorbis_with_exact_counts_and_pcm_parity() {
    let mut report = String::from(
        "channels\trate\tblock_size\tpasses\tframes\tencoded_bytes\tsnr_db\tpeak_abs_error\trms_error\n",
    );
    for channels in [1, 2, 3, 6, 8, 16, 255] {
        let blocks: &[usize] = match channels {
            8 => &[128, 8192],
            255 => &[128, 512],
            _ => &[128],
        };
        for &block_size in blocks {
            for passes in 1..=4 {
                let options = EncodeOptions {
                    channels,
                    block_size,
                    residue_passes: passes,
                    ..EncodeOptions::default()
                };
                for frames in [0, 1, block_size / 2 + 1, block_size * 2 + 17] {
                    let mut pcm = signal(frames, channels as usize);
                    if channels > 1 && frames == block_size / 2 + 1 {
                        for frame in pcm.chunks_exact_mut(channels as usize) {
                            frame[1] = 0.0;
                        }
                    }
                    let bytes = encode(&pcm, options, 17, false);
                    let native = decode(&bytes, frames, channels as usize);
                    let wire = support::file_oracle("oggpcmraw", &bytes);
                    let mut input = wire.as_slice();
                    assert_eq!(support::u32_in(&mut input), 1);
                    assert_eq!(support::u32_in(&mut input), 0x7788);
                    assert_eq!(support::u32_in(&mut input), channels as u32);
                    assert_eq!(support::u32_in(&mut input), options.sample_rate);
                    let declared =
                        u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap());
                    assert_eq!(declared, frames as u64);
                    let mut position = 0;
                    let mut difference = support::Difference::default();
                    while !input.is_empty() {
                        assert_eq!(support::u32_in(&mut input), 0);
                        let count = support::u32_in(&mut input) as usize;
                        for plane in &native {
                            for &value in &plane[position..position + count] {
                                difference.add(value, f32::from_bits(support::u32_in(&mut input)));
                            }
                        }
                        position += count;
                    }
                    if position != frames {
                        if let Some(path) = std::env::var_os("DEADLIB_VORBIS_ENCODE_FAILURE") {
                            std::fs::write(path, &bytes).unwrap();
                        }
                        panic!(
                            "encoder reference count: channels={channels}, N={block_size}, passes={passes}, got={position}, expected={frames}"
                        );
                    }
                    difference.check("native encoding/libvorbis");
                    let mut energy = 0.0;
                    let mut error = 0.0;
                    for (frame, values) in pcm.chunks_exact(channels as usize).enumerate() {
                        for (channel, &value) in values.iter().enumerate() {
                            energy += (value as f64).powi(2);
                            error += (value as f64 - native[channel][frame] as f64).powi(2);
                        }
                    }
                    let snr = if frames == 0 {
                        "n/a".to_owned()
                    } else {
                        format!("{:.6}", 10.0 * (energy / error.max(1e-30)).log10())
                    };
                    use std::fmt::Write;
                    writeln!(&mut report, "{channels}\t{}\t{block_size}\t{passes}\t{frames}\t{}\t{snr}\t{:.12e}\t{:.12e}", options.sample_rate, bytes.len(), difference.peak_error, difference.rms()).unwrap();
                }
            }
        }
    }
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_ENCODE_REPORT") {
        std::fs::write(path, report).unwrap();
    }
}

#[test]
#[ignore = "requires independently built libvorbis transform oracle"]
fn forward_mdct_matches_libvorbis_at_all_legal_sizes() {
    let mut seed = 918u32;
    for n in [64, 128, 256, 512, 1024, 2048, 4096, 8192] {
        let input: Vec<f32> = (0..n)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                seed as i32 as f32 / i32::MAX as f32
            })
            .collect();
        let mut request = (n as u32).to_le_bytes().to_vec();
        for value in &input {
            request.extend_from_slice(&value.to_le_bytes());
        }
        let wire = support::oracle("mdctforward", &[&request]);
        assert_eq!(wire.len(), n / 2 * 4);
        let mut output = vec![0.0; n / 2];
        Mdct::new(n).unwrap().forward(&input, &mut output).unwrap();
        let mut difference = support::Difference::default();
        for (&a, raw) in output.iter().zip(wire.as_chunks::<4>().0) {
            difference.add(a, f32::from_le_bytes(*raw));
        }
        difference.check_with_bounds("forward MDCT/libvorbis", 3e-7, 5e-8);
    }
}

#[test]
#[ignore = "requires pinned libvorbisfile 1.3.7 oracle"]
fn libvorbisfile_pcm_seek_zero_loses_first_block_when_priming_packet_spans_pages() {
    let frames = 16401;
    // Preserve the original encoder's packets, independently of future entropy
    // improvements that make them too small to span a physical Ogg page.
    let bytes = include_bytes!("fixtures/spanning-prime-v1.ogg");
    let expected = decode(bytes, frames, 8);
    let reference = symphonia(bytes, frames, 8);
    let mut difference = support::Difference::default();
    for (a, b) in expected.iter().zip(reference) {
        for (&x, y) in a.iter().zip(b) {
            difference.add(x, y);
        }
    }
    difference.check("spanning native encoding/Symphonia");
    for (mode, skipped) in [("oggpcmraw", 0), ("oggpcm", 4096)] {
        let wire = support::file_oracle(mode, bytes);
        let mut input = wire.as_slice();
        assert_eq!(support::u32_in(&mut input), 1);
        assert_eq!(support::u32_in(&mut input), 0x7788);
        assert_eq!(support::u32_in(&mut input), 8);
        assert_eq!(support::u32_in(&mut input), 48000);
        assert_eq!(
            u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap()),
            frames as u64
        );
        let mut position = skipped;
        let mut difference = support::Difference::default();
        while !input.is_empty() {
            assert_eq!(support::u32_in(&mut input), 0);
            let count = support::u32_in(&mut input) as usize;
            for plane in &expected {
                for &value in &plane[position..position + count] {
                    difference.add(value, f32::from_bits(support::u32_in(&mut input)));
                }
            }
            position += count;
        }
        assert_eq!(position, frames, "{mode}");
        difference.check(mode);
    }
}

#[test]
#[ignore = "requires independently built libvorbis codebook oracle"]
fn native_encoder_codebooks_match_libvorbis_for_every_entry() {
    use bitpack::BitReader;
    for passes in 1..=4 {
        let encoder = PacketEncoder::new(EncodeOptions {
            residue_passes: passes,
            ..EncodeOptions::default()
        })
        .unwrap();
        let setup = Setup::parse(
            encoder.setup_packet(),
            encoder.identification(),
            &Limits::default(),
        )
        .unwrap();
        let wire = support::oracle("books", &[encoder.setup_packet()]);
        let mut input = wire.as_slice();
        assert_eq!(
            support::u32_in(&mut input) as usize,
            setup.codebooks().len()
        );
        for book in setup.codebooks() {
            assert_eq!(support::u32_in(&mut input) as usize, book.dimensions());
            assert_eq!(support::u32_in(&mut input) as usize, book.entries());
            assert_eq!(support::take(&mut input, 1)[0], book.lookup_type());
            assert_eq!(support::take(&mut input, book.entries()), book.lengths());
            if book.lookup_type() != 0 {
                for entry in 0..book.entries() {
                    for &value in book.vector(entry as u32).unwrap() {
                        assert_eq!(support::u32_in(&mut input), value.to_bits());
                    }
                }
            }
            let bit_count = u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap());
            let bytes = support::u32_in(&mut input) as usize;
            let mut bits = BitReader::new(support::take(&mut input, bytes));
            for (entry, &length) in book.lengths().iter().enumerate() {
                let before = bits.position();
                assert_eq!(book.decode_scalar(&mut bits).unwrap(), entry as u32);
                assert_eq!(bits.position() - before, length as usize);
            }
            assert_eq!(bits.position() as u64, bit_count);
        }
        assert!(input.is_empty());
    }
}

#[test]
fn ogg_encoder_seek_positions_and_write_failures_are_explicit() {
    use std::{
        cell::Cell,
        io::{self, Cursor, Write},
        rc::Rc,
    };
    let options = EncodeOptions::default();
    let pcm = signal(3037, 2);
    let bytes = encode(&pcm, options, 1, false);
    let expected = decode(&bytes, 3037, 2);
    let mut seek = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
    let id = seek.selected().info.id;
    for target in [0, 1, 511, 512, 513, 2000, 3036, 3037] {
        seek.seek(id, target).unwrap();
        let mut position = target as usize;
        while seek
            .read_packet(|event| {
                if let OggEvent::Pcm(block) = event {
                    assert_eq!(block.start_frame as usize, position);
                    for (plane, expected) in block.pcm.planes().zip(&expected) {
                        assert_eq!(plane, &expected[position..position + block.pcm.frames]);
                    }
                    position += block.pcm.frames;
                }
            })
            .unwrap()
        {}
        assert_eq!(position, 3037);
    }
    struct Fallible {
        fail: Rc<Cell<bool>>,
        bytes: Vec<u8>,
    }
    impl Write for Fallible {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail.get() {
                return Err(io::Error::other("injected output failure"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let fail = Rc::new(Cell::new(false));
    let mut encoder = OggEncoder::with_page_options(
        Fallible {
            fail: fail.clone(),
            bytes: Vec::new(),
        },
        4,
        options,
        &comments(),
        deadlib_audio_decode_ogg::WriteOptions {
            target_page_body: 1,
        },
    )
    .unwrap();
    fail.set(true);
    assert!(matches!(
        encoder.push_interleaved(&pcm),
        Err(Error::Container(_))
    ));
    fail.set(false);
    assert_eq!(encoder.finish().unwrap_err(), Error::Failed);
    assert_eq!(encoder.push_interleaved(&pcm).unwrap_err(), Error::Failed);
}
