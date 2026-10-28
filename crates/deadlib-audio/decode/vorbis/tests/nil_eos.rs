mod support;
use deadlib_audio_decode_vorbis as codec;
#[path = "support/eos_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use deadlib_audio_decode_ogg::{PageReader, encode_page};
use deadlib_audio_decode_vorbis::*;
use std::io::Cursor;

const SOURCE: &[u8] = include_bytes!("fixtures/unknown-nil-eos-v1.ogg");
fn with_eos(bytes: &[u8], value: Option<u64>) -> Vec<u8> {
    let mut pages = PageReader::new(bytes);
    let mut bytes = Vec::new();
    let mut previous = 0;
    while let Some(page) = pages.next_page().unwrap() {
        let mut header = page.page.header();
        if header.is_eos() {
            header.granule_position = value.unwrap_or(previous);
        } else if header.granule_position != u64::MAX {
            previous = header.granule_position;
        }
        bytes.extend(encode_page(header, page.page.lacing(), page.page.body()).unwrap());
    }
    bytes
}
fn decode(bytes: &[u8], eos_behavior: EosBehavior) -> (Vec<Vec<f32>>, StreamEnd) {
    let mut planes = Vec::new();
    let mut end = None;
    decode_reader(
        bytes,
        OggOptions {
            eos_behavior,
            ..Default::default()
        },
        |e| match e {
            OggEvent::StreamStart(info) => {
                planes = vec![Vec::new(); info.identification.channels as usize]
            }
            OggEvent::Pcm(b) => {
                for (p, q) in planes.iter_mut().zip(b.pcm.planes()) {
                    p.extend(q);
                }
            }
            OggEvent::StreamEnd(value) => end = Some(value),
            _ => {}
        },
    )
    .unwrap();
    (planes, end.unwrap())
}
#[test]
fn empty_eos_without_timestamp_uses_the_completed_audio_landmark() {
    let expected = decode(&with_eos(SOURCE, None), EosBehavior::Vorbis);
    for eos_behavior in [EosBehavior::Vorbis, EosBehavior::Libvorbis] {
        let actual = decode(SOURCE, eos_behavior);
        assert_eq!(actual.0, expected.0);
        assert_eq!(actual.1.emitted_frames, 4672);
        assert_eq!(
            actual.1.frames,
            if eos_behavior == EosBehavior::Vorbis {
                4672
            } else {
                0
            }
        );
        for seek_behavior in [SeekBehavior::Retained, SeekBehavior::Libvorbis] {
            let mut reader = OggSeekReader::with_options(
                Cursor::new(SOURCE),
                SeekOptions {
                    eos_behavior,
                    seek_behavior,
                    ..Default::default()
                },
            )
            .unwrap();
            let id = reader.selected().info.id;
            let targets =
                if eos_behavior == EosBehavior::Vorbis && seek_behavior == SeekBehavior::Retained {
                    vec![0, 1, 2336, 4671, 4672]
                } else {
                    vec![0]
                };
            for target in targets {
                reader.seek(id, target).unwrap();
                let mut planes = vec![Vec::<f32>::new(); 2];
                while reader
                    .read_packet(|e| {
                        if let OggEvent::Pcm(b) = e {
                            for (p, q) in planes.iter_mut().zip(b.pcm.planes()) {
                                p.extend(q);
                            }
                        }
                    })
                    .unwrap()
                {}
                for (p, q) in planes.iter().zip(&expected.0) {
                    assert_eq!(p, &q[target as usize..]);
                }
            }
        }
    }
}

#[test]
fn unknown_nil_eos_empty_priming_and_audio_streams_preserve_fragmented_reset_output() {
    for fixture in 0..4 {
        for count in [0, 1, 2, usize::MAX] {
            let bytes = vectors::Config {
                fixture,
                count,
                group: 1,
                tail: vectors::Tail::Nil,
                end: 0,
            }
            .bytes();
            let bytes = with_eos(&bytes, Some(u64::MAX));
            let expected = decode(&with_eos(&bytes, None), EosBehavior::Vorbis);
            for eos_behavior in [EosBehavior::Vorbis, EosBehavior::Libvorbis] {
                let options = OggOptions {
                    eos_behavior,
                    ..Default::default()
                };
                let mut decoder = OggStreamDecoder::new(options);
                for chunk in [1, 17, 4093] {
                    decoder.reset();
                    let mut actual = Vec::<Vec<f32>>::new();
                    let mut end = None;
                    for data in bytes.chunks(chunk) {
                        let mut offset = 0;
                        while offset < data.len() {
                            let n = decoder
                                .push(&data[offset..], |e| match e {
                                    OggEvent::StreamStart(info) => {
                                        actual =
                                            vec![Vec::new(); info.identification.channels as usize]
                                    }
                                    OggEvent::Pcm(b) => {
                                        for (p, q) in actual.iter_mut().zip(b.pcm.planes()) {
                                            p.extend(q);
                                        }
                                    }
                                    OggEvent::StreamEnd(value) => end = Some(value),
                                    _ => {}
                                })
                                .unwrap();
                            assert!(n > 0);
                            offset += n;
                        }
                    }
                    decoder.finish().unwrap();
                    assert_eq!(actual.len(), expected.0.len());
                    for (p, q) in actual.iter().zip(&expected.0) {
                        assert_eq!(p.len(), q.len());
                        for (a, b) in p.iter().zip(q) {
                            assert_eq!(a.to_bits(), b.to_bits());
                        }
                    }
                    let end = end.unwrap();
                    assert_eq!(end.emitted_frames, expected.1.emitted_frames);
                    assert_eq!(
                        end.frames,
                        if eos_behavior == EosBehavior::Vorbis {
                            expected.1.frames
                        } else {
                            0
                        }
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires independent libvorbis unseekable and duration oracle"]
fn unknown_nil_eos_pcm_matches_reference_despite_zero_declared_duration() {
    let (native, _) = decode(SOURCE, EosBehavior::Vorbis);
    let raw = support::file_oracle("oggstream", SOURCE);
    let mut wire = raw.as_slice();
    assert_eq!(support::u32_in(&mut wire), 2);
    assert_eq!(support::u32_in(&mut wire), 48000);
    let mut difference = support::Difference::default();
    let mut position = 0;
    loop {
        let frames = support::u32_in(&mut wire) as usize;
        if frames == 0 {
            break;
        }
        assert_eq!(support::u32_in(&mut wire), 0);
        assert_eq!(support::u32_in(&mut wire), 2);
        assert_eq!(support::u32_in(&mut wire), 48000);
        for plane in &native {
            for &value in &plane[position..position + frames] {
                difference.add(value, f32::from_bits(support::u32_in(&mut wire)));
            }
        }
        position += frames;
    }
    assert_eq!(position, 4672);
    assert!(wire.is_empty());
    difference.check("unknown nil EOS");
    let raw = support::file_oracle("oggpcm", SOURCE);
    assert_eq!(raw.len(), 24);
    assert_eq!(u64::from_le_bytes(raw[16..24].try_into().unwrap()), 0);
}
