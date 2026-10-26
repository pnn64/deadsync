mod support;
use deadlib_audio_decode_vorbis::*;
use std::{fs, io::Cursor, path::Path};
use support::*;
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef, Position},
    codecs::audio::{AudioDecoder, AudioDecoderOptions},
    formats::{FormatOptions, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    packet::PacketRef,
    units::{Duration, Timestamp},
};

fn symphonia(bytes: &[u8]) -> Box<dyn AudioDecoder> {
    let mss = MediaSourceStream::new(
        Box::new(Cursor::new(bytes.to_vec())),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let cp = format
        .tracks()
        .iter()
        .find_map(|track| track.codec_params.as_ref()?.audio())
        .unwrap();
    symphonia::default::get_codecs()
        .make_audio_decoder(cp, &AudioDecoderOptions::default())
        .unwrap()
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
fn compare_symphonia(bytes: &[u8]) -> Difference {
    compare_symphonia_packets(bytes, &packets(bytes))
}
fn compare_symphonia_packets(bytes: &[u8], packets: &[Vec<u8>]) -> Difference {
    let mut native = decoder(packets);
    let mut reference = symphonia(bytes);
    let order = channel_order(native.setup().channels()).unwrap();
    let mut difference = Difference::default();
    let mut frames = 0;
    for (index, packet) in packets[3..].iter().enumerate() {
        let block = native.decode(packet).unwrap().unwrap();
        assert!(!block.truncated);
        let data = PacketRef::new(0, Timestamp::ZERO, Duration::ZERO, packet);
        let audio = reference.decode_ref(&data).unwrap();
        assert_eq!(block.frames, audio.frames(), "packet {index}");
        assert_eq!(block.start_frame, frames);
        frames += block.frames as u64;
        let GenericAudioBufferRef::F32(audio) = audio else {
            panic!("expected f32");
        };
        for (ch, &channel) in order.iter().enumerate() {
            let reference = if order.len() == 1 {
                audio.plane(0).unwrap()
            } else {
                audio.plane_by_position(position(channel)).unwrap()
            };
            for (&a, &b) in block.plane(ch).unwrap().iter().zip(reference) {
                difference.add(a, b);
            }
        }
    }
    assert!(difference.samples > 0);
    difference
}
fn compare_libvorbis(bytes: &[u8]) -> Difference {
    let packets = packets(bytes);
    compare_libvorbis_packets(&packets)
}
fn compare_libvorbis_packets(packets: &[Vec<u8>]) -> Difference {
    let reference = oracle(
        "pcm",
        &packets.iter().map(Vec::as_slice).collect::<Vec<_>>(),
    );
    compare_reference_packets(packets, &reference)
}
fn compare_reference_packets(packets: &[Vec<u8>], reference: &[u8]) -> Difference {
    let mut input = reference;
    let mut native = decoder(packets);
    assert_eq!(u32_in(&mut input), native.setup().channels() as u32);
    let mut difference = Difference::default();
    let mut frames = 0;
    for (index, packet) in packets[3..].iter().enumerate() {
        let kind = take(&mut input, 1)[0];
        let block = native.decode(packet).unwrap();
        if kind == b'S' {
            assert!(block.is_none());
            continue;
        }
        assert_eq!(kind, b'P');
        let block = block.unwrap();
        assert_eq!(
            u32_in(&mut input) as usize,
            block.block_size,
            "packet {index}"
        );
        assert_eq!(u32_in(&mut input) as usize, block.frames, "packet {index}");
        assert_eq!(block.start_frame, frames);
        frames += block.frames as u64;
        for plane in block.planes() {
            for &value in plane {
                difference.add(value, f32::from_bits(u32_in(&mut input)));
            }
        }
    }
    assert!(input.is_empty());
    difference
}

#[test]
fn generated_pcm_matches_symphonia_including_priming_and_reset() {
    for (label, bytes) in fixtures() {
        if label == "8ch tones" {
            compare_reference_packets(
                &packets(bytes),
                include_bytes!("fixtures/surround.libvorbis-pcm"),
            )
            .check("8ch native/libvorbis golden");
        } else {
            compare_symphonia(bytes).check(label);
        }
        let packets = packets(bytes);
        let mut native = decoder(&packets);
        let mut expected = Vec::new();
        for packet in &packets[3..] {
            let block = native.decode(packet).unwrap().unwrap();
            expected.push(block.planes().map(<[f32]>::to_vec).collect::<Vec<_>>());
        }
        native.reset();
        assert_eq!(native.position(), 0);
        for (packet, expected) in packets[3..].iter().zip(expected) {
            let block = native.decode(packet).unwrap().unwrap();
            assert_eq!(
                block.planes().collect::<Vec<_>>(),
                expected.iter().map(Vec::as_slice).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
#[ignore = "requires external libvorbis oracle"]
fn generated_pcm_matches_libvorbis() {
    for (label, bytes) in fixtures() {
        compare_libvorbis(bytes).check(label);
    }
}

fn fixtures() -> [(&'static str, &'static [u8]); 4] {
    [
        ("stereo", include_bytes!("fixtures/stereo.ogg")),
        ("8ch silence", include_bytes!("fixtures/eight-channel.ogg")),
        ("8ch tones", include_bytes!("fixtures/surround.ogg")),
        ("8 kHz mono", include_bytes!("fixtures/mono.ogg")),
    ]
}

#[test]
fn symphonia_061_dependent_coupling_disagreement_is_explicit() {
    // Symphonia 0.6.1 applies coupling in forward order. Section 4.3.5 requires
    // reverse order; the independent libvorbis golden verifies native output.
    // Preserve this discrepancy instead of changing native channel data or
    // relaxing the normal PCM tolerance to fit a reference defect.
    let bytes = include_bytes!("fixtures/surround.ogg");
    compare_reference_packets(
        &packets(bytes),
        include_bytes!("fixtures/surround.libvorbis-pcm"),
    )
    .check("8ch libvorbis golden");
    let mismatch = compare_symphonia(bytes);
    assert!(mismatch.peak_error > 0.1 && mismatch.rms() > 0.01);
}

#[test]
#[ignore = "requires libvorbis oracle and DEADLIB_VORBIS_FLOOR0 historical file"]
fn historical_floor0_raw_pcm_matches_independent_decoders() {
    let path = std::env::var_os("DEADLIB_VORBIS_FLOOR0").expect("set DEADLIB_VORBIS_FLOOR0");
    let bytes = fs::read(path).unwrap();
    // Historical container violations must not weaken strict Ogg parsing.
    // Use an independent container to exercise only the raw codec here.
    let mut reader = ogg::PacketReader::new(Cursor::new(&bytes));
    let mut packets = Vec::new();
    while let Some(packet) = reader.read_packet().unwrap() {
        packets.push(packet.data);
    }
    let native = decoder(&packets);
    assert!(
        native
            .setup()
            .floors()
            .iter()
            .any(|floor| matches!(floor, Floor::Type0(_)))
    );
    let libvorbis = compare_libvorbis_packets(&packets);
    let symphonia = compare_symphonia_packets(&bytes, &packets);
    eprintln!(
        "floor0 raw: libvorbis peak={} rms={}; Symphonia peak={} rms={}",
        libvorbis.peak_error,
        libvorbis.rms(),
        symphonia.peak_error,
        symphonia.rms()
    );
    // LSP cosine/product reconstruction is nonlinear and sensitive to the
    // reference's f32 intermediate rounding. Native evaluation uses f64.
    // See COVERAGE.md: separate full-scale peak/RMS bounds for floor zero;
    // packet counts, positions and channel layout still require exact equality.
    libvorbis.check_with_bounds("historical floor0/libvorbis", 5.0e-5, 3.0e-6);
    symphonia.check_with_bounds("historical floor0/Symphonia", 5.0e-5, 3.0e-6);
}

#[test]
#[ignore = "requires external libvorbis oracle and DEADLIB_VORBIS_CORPUS"]
fn local_corpus_pcm_matches_libvorbis_and_symphonia() {
    let root = std::env::var_os("DEADLIB_VORBIS_CORPUS").expect("set DEADLIB_VORBIS_CORPUS");
    let paths = paths(Path::new(&root));
    assert!(!paths.is_empty());
    let count = paths.len();
    let mut total_libvorbis = Difference::default();
    let mut total_symphonia = Difference::default();
    let mut report =
        String::from("path\tdecoder\tsamples\tpeak_abs_error\trms_error\treference_peak\n");
    for path in paths {
        let bytes = fs::read(&path).unwrap();
        eprintln!("{}", path.display());
        let libvorbis = compare_libvorbis(&bytes);
        libvorbis.check("libvorbis");
        total_libvorbis.merge(&libvorbis);
        let symphonia = compare_symphonia(&bytes);
        symphonia.check("Symphonia");
        total_symphonia.merge(&symphonia);
        for (name, result) in [
            ("libvorbis-1.3.7", &libvorbis),
            ("Symphonia-0.6.1", &symphonia),
        ] {
            use std::fmt::Write;
            writeln!(
                &mut report,
                "{}\t{name}\t{}\t{:.12e}\t{:.12e}\t{:.9}",
                path.strip_prefix(&root).unwrap().display(),
                result.samples,
                result.peak_error,
                result.rms(),
                result.peak_reference
            )
            .unwrap();
        }
    }
    total_libvorbis.check(&format!("all {count} files/libvorbis"));
    total_symphonia.check(&format!("all {count} files/Symphonia"));
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_REPORT") {
        fs::write(path, report).unwrap();
    }
}

#[test]
#[ignore = "explicit golden regeneration; requires oracle and DEADLIB_VORBIS_GOLDEN_OUTPUT"]
fn regenerate_surround_pcm_golden() {
    let output = std::env::var_os("DEADLIB_VORBIS_GOLDEN_OUTPUT")
        .expect("set DEADLIB_VORBIS_GOLDEN_OUTPUT explicitly");
    let packets = packets(include_bytes!("fixtures/surround.ogg"));
    let reference = oracle(
        "pcm",
        &packets.iter().map(Vec::as_slice).collect::<Vec<_>>(),
    );
    fs::write(output, reference).unwrap();
}

#[test]
#[ignore = "requires external libvorbis oracle"]
fn mdct_matches_libvorbis_at_all_sizes() {
    let mut seed = 81u32;
    for n in [64usize, 128, 256, 512, 1024, 2048, 4096, 8192] {
        let mut input = (n as u32).to_le_bytes().to_vec();
        let spectrum: Vec<f32> = (0..n / 2)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                seed as i32 as f32 / i32::MAX as f32 / (n as f32).sqrt()
            })
            .collect();
        for &value in &spectrum {
            input.extend(value.to_le_bytes());
        }
        let reference = oracle("mdct", &[&input]);
        let mut reference = reference.as_slice();
        let mut transform = Imdct::new(n).unwrap();
        let mut output = vec![0.0; n];
        transform.inverse(&spectrum, &mut output).unwrap();
        let mut difference = Difference::default();
        for value in output {
            difference.add(value, f32::from_bits(u32_in(&mut reference)));
        }
        assert!(reference.is_empty());
        difference.check(&format!("MDCT {n}"));
    }
}
