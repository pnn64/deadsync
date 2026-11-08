use deadlib_audio_decode_wav::{Chunk, Endian, Error, Limits, metadata::*};

fn id<'a>(track: u16, uid: &'a [u8; 12], reference: &'a [u8; 14]) -> AudioId<'a> {
    AudioId {
        track_index: track,
        uid,
        track_ref: reference,
        pack_ref: b"AP_00010002",
        padding: 0,
    }
}

#[test]
fn chna_matches_itu_examples_and_keeps_unused_slots_and_shared_tracks() {
    let ids = [
        id(1, b"ATU_00000001", b"AT_00010001_01"),
        id(2, b"ATU_00000002", b"AC_00010002_00"),
    ];
    let mut gold = Vec::new();
    gold.extend(2u16.to_le_bytes());
    gold.extend(2u16.to_le_bytes());
    for value in ids {
        gold.extend(value.track_index.to_le_bytes());
        gold.extend(value.uid);
        gold.extend(value.track_ref);
        gold.extend(value.pack_ref);
        gold.push(0);
    }
    let chna = Chna::parse(&gold, Limits::default()).unwrap();
    assert_eq!(chna.num_tracks(), 2);
    assert_eq!(chna.num_uids(), 2);
    assert_eq!(chna.ids().collect::<Vec<_>>(), ids);
    chna.validate_assignments(2, &mut [0; 1]).unwrap();
    let mut out = [0xaa; 204];
    let n = write_chna(2, &ids, 2, &mut out).unwrap();
    assert_eq!(&out[..n], gold);
    assert_eq!(out[n], 0xaa);
    let extended = [
        ids[0],
        AudioId::UNUSED,
        id(1, b"ATU_00000003", b"AT_00031001_01"),
        ids[1],
    ];
    let n = write_chna(2, &extended, 5, &mut out).unwrap();
    let chna = Chna::parse(&out[..n], Limits::default()).unwrap();
    assert_eq!(chna.num_uids(), 3);
    assert_eq!(chna.ids().len(), 5);
    chna.validate_assignments(2, &mut [0; 1]).unwrap();
    assert!(chna.validate_assignments(1, &mut [0; 1]).is_err());
    assert!(chna.validate_assignments(2, &mut []).is_err());
    assert!(matches!(
        Chna::parse(
            &out[..n],
            Limits {
                chunks: 4,
                ..Limits::default()
            }
        ),
        Err(Error::Limit(_))
    ));
    for n in 0..gold.len() {
        assert!(Chna::parse(&gold[..n], Limits::default()).is_err());
    }
    let mut bad = gold.clone();
    bad[2..4].copy_from_slice(&1u16.to_le_bytes());
    assert!(Chna::parse(&bad, Limits::default()).is_err());
    let mut invalid = ids[0];
    invalid.track_ref = b"AC_00010001_01";
    let mut out = [0xab; 84];
    assert!(write_chna(2, &[invalid, ids[1]], 2, &mut out).is_err());
    assert_eq!(out, [0xab; 84]);
}

#[test]
fn bxml_and_sxml_envelopes_preserve_packed_odd_payloads_and_64bit_alignment() {
    let mut out = [0xab; 128];
    let n = write_bxml(1, &[0x1f, 0x8b, 0, 255], &mut out).unwrap();
    assert_eq!(&out[..n], [1, 0, 0x1f, 0x8b, 0, 255]);
    let v = Bxml::parse(&out[..n]).unwrap();
    assert_eq!(v.compression(), 1);
    assert_eq!(v.data(), [0x1f, 0x8b, 0, 255]);
    let points = [
        AlignmentPoint {
            byte_offset: 14,
            sample_count: 0,
        },
        AlignmentPoint {
            byte_offset: 27,
            sample_count: (1u64 << 32) + 48000,
        },
    ];
    let mut gold = Vec::new();
    gold.extend(0u16.to_le_bytes());
    gold.extend(29u64.to_le_bytes());
    gold.extend(2u32.to_le_bytes());
    gold.extend(5u32.to_le_bytes());
    gold.extend(48000u32.to_le_bytes());
    gold.extend(b"<a />");
    gold.extend(4u32.to_le_bytes());
    gold.extend(96000u32.to_le_bytes());
    gold.extend(b"<b/>");
    gold.extend(2u32.to_le_bytes());
    for p in points {
        gold.extend(p.byte_offset.to_le_bytes());
        gold.extend(p.sample_count.to_le_bytes());
    }
    let n = write_sxml(0, &[(48000, b"<a />"), (96000, b"<b/>")], &points, &mut out).unwrap();
    assert_eq!(&out[..n], gold);
    let view = Sxml::parse(&gold, Limits::default()).unwrap();
    assert_eq!(view.compression(), 0);
    let subs: Vec<_> = view.subchunks().collect();
    assert_eq!(subs[0].data, b"<a />");
    assert_eq!(subs[1].byte_offset, 27);
    assert_eq!(view.subchunk_at(27).unwrap().samples, 96000);
    assert!(view.subchunk_at(26).is_none());
    assert_eq!(view.alignment_points().collect::<Vec<_>>(), points);
    for len in 0..gold.len() {
        if len != 39 {
            assert!(
                Sxml::parse(&gold[..len], Limits::default()).is_err(),
                "prefix {len}"
            );
        }
    }
    assert_eq!(
        Sxml::parse(&gold[..39], Limits::default())
            .unwrap()
            .alignment_points()
            .count(),
        0
    );
    for (range, value) in [
        (2..10, u64::MAX.to_le_bytes().to_vec()),
        (10..14, u32::MAX.to_le_bytes().to_vec()),
        (14..18, u32::MAX.to_le_bytes().to_vec()),
        (39..43, u32::MAX.to_le_bytes().to_vec()),
    ] {
        let mut bad = gold.clone();
        bad[range].copy_from_slice(&value);
        assert!(Sxml::parse(&bad, Limits::default()).is_err());
    }
    assert!(matches!(
        Sxml::parse(
            &gold,
            Limits {
                chunks: 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit(_))
    ));
    let before = out;
    assert!(write_sxml(0, &[(0, b"large payload")], &[], &mut out[..14]).is_err());
    assert_eq!(out, before);
}

#[test]
fn rf64_marker_rows_have_no_count_prefix_and_retain_all_words() {
    let mut label = [0; 256];
    label[..7].copy_from_slice("Caf\u{e9} x".as_bytes());
    let guid = [0xa5; 16];
    let value = Marker {
        flags: 1 | 2 | 4 | 16,
        sample_offset: 0x1122334455667788,
        byte_offset: 0xffeeddccbbaa9988,
        intra_sample_words: [0x12345678, 0x9abcdef0],
        label: &label,
        label_chunk_id: 17,
        vendor_product_guid: &guid,
        user_data: [1, 2, 3, 4],
    };
    let mut gold = Vec::new();
    gold.extend(value.flags.to_le_bytes());
    gold.extend(value.sample_offset.to_le_bytes());
    gold.extend(value.byte_offset.to_le_bytes());
    for v in value.intra_sample_words {
        gold.extend(v.to_le_bytes());
    }
    gold.extend(label);
    gold.extend(17u32.to_le_bytes());
    gold.extend(guid);
    for v in [1u32, 2, 3, 4] {
        gold.extend(v.to_le_bytes());
    }
    assert_eq!(gold.len(), 320);
    let view = Markers::parse(&gold, Limits::default()).unwrap();
    let marker = view.iter().next().unwrap();
    assert_eq!(marker, value);
    assert!(marker.is_valid());
    assert_eq!(marker.valid_byte_offset(), Some(value.byte_offset));
    assert_eq!(marker.inline_label(), Some("Caf\u{e9} x".as_bytes()));
    let mut out = [0xab; 640];
    assert_eq!(write_markers(&[value], &mut out).unwrap(), 320);
    assert_eq!(&out[..320], gold);
    assert_eq!(out[320], 0xab);
    for n in 1..320 {
        assert!(Markers::parse(&gold[..n], Limits::default()).is_err());
    }
    let bad_label = [255; 256];
    let bad = Marker {
        label: &bad_label,
        ..value
    };
    let mut out = [0xab; 320];
    assert!(write_markers(&[bad], &mut out).is_err());
    assert_eq!(out, [0xab; 320]);
    let external = Marker {
        flags: 1 | 8,
        label: &bad_label,
        ..value
    };
    write_markers(&[external], &mut out).unwrap();
    assert_eq!(
        Markers::parse(&out, Limits::default())
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .inline_label(),
        None
    );
}

#[test]
fn explicit_metadata_dispatch_keeps_unknown_chunk_bytes_and_padding() {
    let chunk = Chunk {
        id: *b"ZZZZ",
        offset: 42,
        data: &[1, 2, 3],
        padding: &[0xab],
    };
    let result = chunk.metadata(Endian::Little, Limits::default()).unwrap();
    assert_eq!(result.raw(), [1, 2, 3]);
    assert!(matches!(result,Metadata::Opaque(c) if c.padding==[0xab]&&c.offset==42));
    let chunk = Chunk {
        id: *b"axml",
        data: b"<audioFormatExtended/>",
        ..chunk
    };
    assert!(matches!(
        chunk.metadata(Endian::Little, Limits::default()).unwrap(),
        Metadata::Axml(_)
    ));
    let chunk = Chunk {
        id: *b"chna",
        data: &[0; 4],
        ..chunk
    };
    assert!(matches!(
        chunk.metadata(Endian::Little, Limits::default()).unwrap(),
        Metadata::Chna(_)
    ));
    assert!(matches!(
        chunk.metadata(Endian::Big, Limits::default()),
        Err(Error::Unsupported(_))
    ));
}
