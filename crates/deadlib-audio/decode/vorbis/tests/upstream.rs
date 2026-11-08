mod support;
use deadlib_audio_decode_vorbis::*;
use std::{fmt::Write, fs, io::Cursor, path::Path};

fn reference(bytes: &[u8], channels: usize, rate: u32) -> Vec<Vec<f32>> {
    let wire = support::file_oracle("oggpcm", bytes);
    let mut input = wire.as_slice();
    assert_eq!(support::u32_in(&mut input), 1);
    assert_eq!(support::u32_in(&mut input), 12345678);
    assert_eq!(support::u32_in(&mut input) as usize, channels);
    assert_eq!(support::u32_in(&mut input), rate);
    assert_eq!(
        u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap()),
        2048
    );
    let mut pcm = vec![Vec::new(); channels];
    while !input.is_empty() {
        assert_eq!(support::u32_in(&mut input), 0);
        let frames = support::u32_in(&mut input);
        for plane in &mut pcm {
            for _ in 0..frames {
                plane.push(f32::from_bits(support::u32_in(&mut input)));
            }
        }
    }
    assert!(pcm.iter().all(|p| p.len() == 2048));
    pcm
}

#[test]
#[ignore = "requires pinned upstream test outputs and libvorbisfile oracle"]
fn xiph_upstream_coded_grid_pcm_streaming_and_seeks() {
    let root = std::env::var_os("DEADLIB_VORBIS_UPSTREAM_CORPUS")
        .expect("set DEADLIB_VORBIS_UPSTREAM_CORPUS");
    let root = Path::new(&root);
    assert_eq!(
        support::paths(root).len(),
        528,
        "use the complete pinned upstream grid"
    );
    let mut total = support::Difference::default();
    let mut report = String::from(
        "path\tchannels\trate\tquality\tframes\tsamples\tpeak_abs_error\trms_error\treference_peak\n",
    );
    for channels in 1..=8 {
        for rate in [44100, 48000, 32000, 22050, 16000, 96000] {
            for qi in 0..11 {
                let quality = qi as f64 - 0.5;
                let name = format!("vorbis_{channels}ch_q{quality:.1}_{rate}.ogg");
                let bytes = fs::read(root.join(&name)).unwrap();
                let reference = reference(&bytes, channels, rate);
                let mut native = vec![Vec::new(); channels];
                let mut starts = 0;
                let mut ends = 0;
                decode_reader(
                    bytes.as_slice(),
                    OggOptions::default(),
                    |event| match event {
                        OggEvent::StreamStart(info) => {
                            starts += 1;
                            assert_eq!(info.identification.channels as usize, channels);
                            assert_eq!(info.identification.sample_rate, rate);
                            assert_eq!(
                                info.metadata.values(b"ENCODER").collect::<Vec<_>>(),
                                [b"test/util.c"]
                            );
                        }
                        OggEvent::Pcm(block) => {
                            assert_eq!(block.start_frame as usize, native[0].len());
                            assert_eq!(block.start_frame, block.start_granule);
                            for (out, plane) in native.iter_mut().zip(block.pcm.planes()) {
                                out.extend_from_slice(plane);
                            }
                        }
                        OggEvent::StreamEnd(end) => {
                            ends += 1;
                            assert_eq!(end.frames, 2048);
                        }
                        _ => panic!("unexpected event in upstream valid stream"),
                    },
                )
                .unwrap_or_else(|error| panic!("{name}: {error}"));
                assert_eq!((starts, ends), (1, 1));
                let mut difference = support::Difference::default();
                for (native, reference) in native.iter().zip(&reference) {
                    assert_eq!(native.len(), 2048);
                    for (&a, &b) in native.iter().zip(reference) {
                        difference.add(a, b);
                    }
                }
                difference.check(&name);
                total.merge(&difference);
                let mut seek = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
                let id = seek.selected().info.id;
                for target in [2048, 2047, 1, 1023, 0] {
                    seek.seek(id, target).unwrap();
                    let mut position = target as usize;
                    while seek
                        .read_packet(|event| {
                            if let OggEvent::Pcm(block) = event {
                                assert_eq!(block.start_frame as usize, position);
                                for (expected, plane) in native.iter().zip(block.pcm.planes()) {
                                    assert_eq!(
                                        plane,
                                        &expected[position..position + block.pcm.frames]
                                    );
                                }
                                position += block.pcm.frames;
                            }
                        })
                        .unwrap()
                    {}
                    assert_eq!(position, 2048);
                }
                let mut stream = OggStreamDecoder::default();
                let mut position = 0;
                for input in bytes.chunks(17) {
                    let mut consumed = 0;
                    while consumed < input.len() {
                        consumed += stream
                            .push(&input[consumed..], |event| {
                                if let OggEvent::Pcm(block) = event {
                                    for (expected, plane) in native.iter().zip(block.pcm.planes()) {
                                        assert_eq!(
                                            plane,
                                            &expected[position..position + block.pcm.frames]
                                        );
                                    }
                                    position += block.pcm.frames;
                                }
                            })
                            .unwrap();
                    }
                }
                stream.finish().unwrap();
                assert_eq!(position, 2048);
                writeln!(
                    &mut report,
                    "{name}\t{channels}\t{rate}\t{quality:.1}\t2048\t{}\t{:.12e}\t{:.12e}\t{:.9}",
                    difference.samples,
                    difference.peak_error,
                    difference.rms(),
                    difference.peak_reference
                )
                .unwrap();
            }
        }
    }
    total.check("all 528 pinned Xiph upstream vectors");
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_UPSTREAM_REPORT") {
        fs::write(path, report).unwrap();
    }
}
