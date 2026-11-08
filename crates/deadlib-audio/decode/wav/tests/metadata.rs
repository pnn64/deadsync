use deadlib_audio_decode_wav::{
    Endian, Error, Limits, RawChunk, SampleFormat, Spec, Wave, WriteOptions, Writer, metadata::*,
};
use std::io::Cursor;
fn u32b(v: u32, e: Endian) -> [u8; 4] {
    if e == Endian::Little {
        v.to_le_bytes()
    } else {
        v.to_be_bytes()
    }
}

#[test]
fn list_code_pages_padding_and_associated_text_are_borrowed() {
    for e in [Endian::Little, Endian::Big] {
        let mut raw = b"INFOINAM".to_vec();
        raw.extend(u32b(3, e));
        raw.extend([b'X', 0xe9, 0, 0xcd]);
        raw.extend(b"ZZZZ");
        raw.extend(u32b(1, e));
        raw.extend([42, 0xab]);
        let list = List::parse(&raw, e, Limits::default()).unwrap();
        assert_eq!(list.kind(), *b"INFO");
        let entries: Vec<_> = list.entries().map(Result::unwrap).collect();
        assert_eq!(entries[0].text_bytes(), [b'X', 0xe9]);
        assert!(std::str::from_utf8(entries[0].text_bytes()).is_err());
        assert_eq!(entries[0].padding, [0xcd]);
        assert_eq!(entries[1].padding, [0xab]);
        assert_eq!(list.raw().as_ptr(), raw.as_ptr());
        for n in 0..raw.len() {
            if n != 4 && n != 16 {
                assert!(
                    List::parse(&raw[..n], e, Limits::default()).is_err(),
                    "prefix {n}"
                );
            }
        }
        assert!(matches!(
            List::parse(
                &raw,
                e,
                Limits {
                    chunks: 1,
                    ..Limits::default()
                }
            ),
            Err(Error::Limit(_))
        ));
        let mut text = [0; 64];
        let n = write_associated_text(123, b"marker", e, &mut text).unwrap();
        let fields = ListEntry {
            id: *b"labl",
            data: &text[..n],
            padding: &[],
        }
        .associated(e)
        .unwrap();
        assert_eq!(fields.cue_id, 123);
        assert_eq!(nul_terminated(fields.text), b"marker");
        assert_eq!(fields.sample_length, None);
        let n = write_region_text(17, 999, *b"rgn ", [44, 9, 1, 1252], b"region", e, &mut text)
            .unwrap();
        let fields = ListEntry {
            id: *b"ltxt",
            data: &text[..n],
            padding: &[],
        }
        .associated(e)
        .unwrap();
        assert_eq!(fields.locale, Some([44, 9, 1, 1252]));
        assert_eq!(fields.sample_length, Some(999));
        assert_eq!(fields.purpose, Some(*b"rgn "));
        let mut list_out = [0; 128];
        let n = write_list(
            *b"adtl",
            &[RawChunk {
                id: *b"ltxt",
                data: &text[..n],
            }],
            e,
            &mut list_out,
        )
        .unwrap();
        assert_eq!(
            List::parse(&list_out[..n], e, Limits::default())
                .unwrap()
                .entries()
                .count(),
            1
        );
    }
}

#[test]
fn cue_and_sampler_gold_layouts_survive_both_byte_orders() {
    for e in [Endian::Little, Endian::Big] {
        let point = CuePoint {
            id: 7,
            position: 100,
            data_chunk: *b"data",
            chunk_start: 0,
            block_start: 13,
            sample_offset: u32::MAX,
        };
        let mut gold = Vec::new();
        gold.extend(u32b(1, e));
        gold.extend(u32b(7, e));
        gold.extend(u32b(100, e));
        gold.extend(b"data");
        for v in [0, 13, u32::MAX] {
            gold.extend(u32b(v, e));
        }
        let cues = Cues::parse(&gold, e, Limits::default()).unwrap();
        assert_eq!(cues.iter().collect::<Vec<_>>(), [point]);
        let mut out = [0; 128];
        let n = write_cues(&[point], e, &mut out).unwrap();
        assert_eq!(&out[..n], gold);
        for n in 0..gold.len() {
            assert!(Cues::parse(&gold[..n], e, Limits::default()).is_err());
        }
        let header = SamplerHeader {
            manufacturer: 0x01000013,
            product: 9,
            sample_period: 22675,
            midi_unity_note: 60,
            midi_pitch_fraction: 0x80000000,
            smpte_format: 25,
            smpte_offset: 0xff010203,
        };
        let loop_info = SampleLoop {
            cue_id: 7,
            kind: 1,
            start: 3,
            end: 99,
            fraction: 0x40000000,
            play_count: 0,
        };
        let mut gold = Vec::new();
        for v in [
            0x01000013, 9, 22675, 60, 0x80000000, 25, 0xff010203, 1, 3, 7, 1, 3, 99, 0x40000000, 0,
        ] {
            gold.extend(u32b(v, e));
        }
        gold.extend([9, 8, 7]);
        let sampler = Sampler::parse(&gold, e, Limits::default()).unwrap();
        assert_eq!(sampler.header(), header);
        assert_eq!(sampler.loops().collect::<Vec<_>>(), [loop_info]);
        assert_eq!(sampler.specific_data(), [9, 8, 7]);
        let n = write_sampler(header, &[loop_info], &[9, 8, 7], e, &mut out).unwrap();
        assert_eq!(&out[..n], gold);
        for n in 0..gold.len() {
            assert!(Sampler::parse(&gold[..n], e, Limits::default()).is_err());
        }
        assert!(Cues::parse(&[255; 4], e, Limits::default()).is_err());
        assert!(Sampler::parse(&[255; 36], e, Limits::default()).is_err());
    }
}

#[test]
fn bext_v0_v1_v2_layout_precision_and_reserved_retention() {
    for version in 0..=2u16 {
        let mut gold = vec![0u8; 606];
        gold[..5].copy_from_slice(b"audio");
        gold[256..262].copy_from_slice(b"studio");
        gold[288..291].copy_from_slice(b"ref");
        gold[320..330].copy_from_slice(b"2024-02-29");
        gold[330..338].copy_from_slice(b"23:59:59");
        gold[338..346].copy_from_slice(&0xfedcba9876543210u64.to_le_bytes());
        gold[346..348].copy_from_slice(&version.to_le_bytes());
        if version >= 1 {
            gold[348] = 0x77;
            gold[411] = 0x99;
        }
        let loud = Loudness {
            integrated: -2300,
            range: 1250,
            true_peak: -100,
            momentary: -1900,
            short_term: -2100,
        };
        if version >= 2 {
            for (row, v) in gold[412..422]
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip([-2300i16, 1250, -100, -1900, -2100])
            {
                *row = v.to_le_bytes();
            }
        }
        gold[602..].copy_from_slice(b"ok\r\n");
        let view = Bext::parse(&gold).unwrap();
        assert_eq!(view.description(), b"audio");
        assert_eq!(view.originator(), b"studio");
        assert_eq!(view.originator_reference(), b"ref");
        assert_eq!(view.version(), version);
        assert_eq!(view.time_reference(), 0xfedcba9876543210);
        assert_eq!(view.coding_history(), b"ok\r\n");
        assert_eq!(view.umid().is_some(), version >= 1);
        assert_eq!(
            view.loudness(),
            if version == 2 { Some(loud) } else { None }
        );
        assert_eq!(view.raw().as_ptr(), gold.as_ptr());
        let spec = BextSpec {
            description: b"audio",
            originator: b"studio",
            originator_reference: b"ref",
            date: *b"2024-02-29",
            time: *b"23:59:59",
            time_reference: 0xfedcba9876543210,
            version,
            umid: if version >= 1 {
                *view.umid().unwrap()
            } else {
                [0; 64]
            },
            loudness: if version >= 2 {
                loud
            } else {
                Loudness::UNKNOWN
            },
            coding_history: b"ok\r\n",
        };
        let mut output = vec![0xaa; 606];
        assert_eq!(spec.write(&mut output).unwrap(), 606);
        assert_eq!(output, gold);
        let reserved_start = if version == 0 {
            348
        } else if version == 1 {
            412
        } else {
            422
        };
        gold[reserved_start] = 0xab;
        assert_eq!(Bext::parse(&gold).unwrap().reserved()[0], 0xab);
        for n in 0..602 {
            assert!(Bext::parse(&gold[..n]).is_err());
        }
    }
    let malformed = Loudness {
        integrated: -10000,
        range: -1,
        true_peak: 10000,
        momentary: i16::MAX,
        short_term: -9999,
    };
    assert_eq!(malformed.values(), [None, None, None, None, Some(-9999)]);
}

#[test]
fn metadata_serialization_errors_leave_caller_storage_untouched() {
    let mut output = [0xab; 1024];
    for spec in [
        BextSpec {
            description: &[0xff],
            ..BextSpec::default()
        },
        BextSpec {
            date: *b"2023-02-29",
            ..BextSpec::default()
        },
        BextSpec {
            time: *b"24:00:00",
            ..BextSpec::default()
        },
        BextSpec {
            version: 0,
            umid: [1; 64],
            ..BextSpec::default()
        },
        BextSpec {
            loudness: Loudness {
                range: -1,
                ..Loudness::UNKNOWN
            },
            ..BextSpec::default()
        },
        BextSpec {
            coding_history: b"missing newline",
            ..BextSpec::default()
        },
    ] {
        assert!(spec.write(&mut output).is_err());
        assert_eq!(output, [0xab; 1024]);
    }
    assert!(BextSpec::default().write(&mut output[..601]).is_err());
    assert_eq!(output, [0xab; 1024]);
    assert!(
        write_cues(
            &[CuePoint {
                id: 0,
                position: 0,
                data_chunk: *b"data",
                chunk_start: 0,
                block_start: 0,
                sample_offset: 0
            }],
            Endian::Little,
            &mut output[..27]
        )
        .is_err()
    );
    assert_eq!(output, [0xab; 1024]);
    let malformed = [1, 0, 0, 0, b'x'];
    assert!(
        ListEntry {
            id: *b"labl",
            data: &malformed,
            padding: &[]
        }
        .associated(Endian::Little)
        .is_err()
    );
}

#[test]
fn ancillary_chunks_roundtrip_through_native_writer_without_pcm_changes() {
    let mut bext = [0; 602];
    BextSpec {
        description: b"test",
        ..BextSpec::default()
    }
    .write(&mut bext)
    .unwrap();
    let mut text = [0; 64];
    let n = write_list(
        *b"INFO",
        &[
            RawChunk {
                id: *b"INAM",
                data: b"title\0",
            },
            RawChunk {
                id: *b"ABCD",
                data: &[0, 255, 1],
            },
        ],
        Endian::Little,
        &mut text,
    )
    .unwrap();
    let mut writer = Writer::new(
        Cursor::new(Vec::new()),
        Spec::new(SampleFormat::I16, 1, 44100),
        WriteOptions::default(),
        &[
            RawChunk {
                id: *b"bext",
                data: &bext,
            },
            RawChunk {
                id: *b"LIST",
                data: &text[..n],
            },
        ],
    )
    .unwrap();
    writer.write_i16(&[i16::MIN, 0, i16::MAX]).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let wave = Wave::parse(&bytes).unwrap();
    let mut pcm = [0; 3];
    wave.pcm().unwrap().read_i16(0, &mut pcm).unwrap();
    assert_eq!(pcm, [i16::MIN, 0, i16::MAX]);
    for chunk in wave.chunks().unwrap() {
        let chunk = chunk.unwrap();
        if chunk.id == *b"bext" {
            assert_eq!(Bext::parse(chunk.data).unwrap().description(), b"test");
        }
        if chunk.id == *b"LIST" {
            assert_eq!(
                List::parse(chunk.data, Endian::Little, Limits::default())
                    .unwrap()
                    .entries()
                    .count(),
                2
            );
        }
    }
}
