use deadlib_audio_decode_ogg::*;
use std::io::Cursor;

fn fixture(span: u64, end: u64, hole: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (sequence, flags, gp, laces, body) in [
        (0, BOS, 0, vec![1], vec![b'h']),
        (1, 0, 100, vec![1], vec![b'a']),
        (2, 0, span, vec![255], vec![0; 255]),
        (
            if hole { 8 } else { 3 },
            CONTINUED,
            200,
            vec![0, 1],
            vec![b'x'],
        ),
        (4, EOS, end, vec![1], vec![b'e']),
    ] {
        bytes.extend(
            encode_page(
                PageHeader {
                    flags,
                    granule_position: gp,
                    serial: 7,
                    sequence,
                },
                &laces,
                &body,
            )
            .unwrap(),
        );
    }
    bytes
}
fn demux(bytes: &[u8], options: ValidationOptions) -> Result<CompatibilityStats, Error> {
    let mut reader = PageReader::new(bytes);
    let mut demux = Demuxer::with_validation(Limits::default(), options);
    while let Some(page) = reader.next_page()? {
        demux.push_page(page.page, page.offset, |_| {})?;
    }
    demux.finish()?;
    Ok(demux.compatibility())
}
#[test]
fn zero_span_granule_is_explicit_and_never_updates_the_timeline() {
    let bytes = fixture(0, 300, false);
    assert_eq!(
        demux(&bytes, ValidationOptions::default()),
        Err(Error::Invalid("granule on page without a completed packet"))
    );
    let options = ValidationOptions {
        allow_zero_span_granules: true,
        ..ValidationOptions::default()
    };
    assert_eq!(
        demux(&bytes, options).unwrap(),
        CompatibilityStats {
            zero_span_granules: 1,
            decreasing_eos_granules: 0
        }
    );
    assert!(demux(&fixture(7, 300, false), options).is_err());
    assert!(matches!(
        demux(&fixture(0, 300, true), options),
        Err(Error::Sequence { .. })
    ));
    let mut corrupt = bytes.clone();
    corrupt[28] ^= 1;
    assert!(matches!(
        demux(&corrupt, options),
        Err(Error::Checksum { .. })
    ));
    // A completed page going backward is still invalid with only this flag.
    assert!(demux(&fixture(0, 150, false), options).is_err());
    let index = OggIndex::read(
        bytes.as_slice(),
        IndexOptions {
            validation: options,
            ..IndexOptions::default()
        },
    )
    .unwrap();
    assert_eq!(index.compatibility().zero_span_granules, 1);
    assert_eq!(
        index.granule_at_or_before(7, 50).unwrap().header.sequence,
        0
    );
    assert_eq!(
        find_granule(&mut Cursor::new(&bytes), 0..bytes.len() as u64, 7, 50)
            .unwrap()
            .unwrap()
            .header
            .sequence,
        0
    );
}
#[test]
fn final_decrease_is_explicit_and_excluded_from_ordered_landmarks() {
    let bytes = fixture(UNKNOWN_GRANULE_POSITION, 150, false);
    assert_eq!(
        demux(&bytes, ValidationOptions::default()),
        Err(Error::Invalid("decreasing granule position"))
    );
    let options = ValidationOptions {
        allow_decreasing_eos_granule: true,
        ..ValidationOptions::default()
    };
    let index = OggIndex::read(
        bytes.as_slice(),
        IndexOptions {
            validation: options,
            ..IndexOptions::default()
        },
    )
    .unwrap();
    assert_eq!(index.compatibility().decreasing_eos_granules, 1);
    assert_eq!(
        index
            .pages(7)
            .unwrap()
            .last()
            .unwrap()
            .header
            .granule_position,
        150
    );
    assert_eq!(
        index
            .granule_at_or_before(7, 150)
            .unwrap()
            .header
            .granule_position,
        100
    );
    assert_eq!(
        index
            .granule_at_or_before(7, 999)
            .unwrap()
            .header
            .granule_position,
        200
    );
    assert!(demux(&fixture(0, 150, false), options).is_err()); // Does not enable the other quirk.
    let last = index.pages(7).unwrap().last().unwrap();
    let page = Page::parse(&bytes[last.offset as usize..]).unwrap().0;
    let mut header = page.header();
    header.flags &= !EOS;
    let mut not_final = bytes[..last.offset as usize].to_vec();
    not_final.extend(encode_page(header, page.lacing(), page.body()).unwrap());
    assert_eq!(
        demux(&not_final, options),
        Err(Error::Invalid("decreasing granule position"))
    );
}
#[test]
fn capture_and_reset_preserve_the_selected_validation_flags() {
    let bytes = fixture(0, 150, false);
    let options = ValidationOptions {
        allow_zero_span_granules: true,
        allow_decreasing_eos_granule: true,
    };
    let index = OggIndex::read(
        bytes.as_slice(),
        IndexOptions {
            validation: options,
            ..IndexOptions::default()
        },
    )
    .unwrap();
    let mut capture = PacketCapture::at_page_with_validation(
        &index.pages(7).unwrap()[0],
        Limits::default(),
        options,
    );
    let mut demux = Demuxer::with_validation(Limits::default(), options);
    for _ in 0..3 {
        let mut reader = PageReader::new(bytes.as_slice());
        let mut packets = Vec::new();
        let mut whole = Vec::new();
        while let Some(page) = reader.next_page().unwrap() {
            capture
                .push_page(page.page, page.offset, |p| packets.push(p.data.to_vec()))
                .unwrap();
            demux
                .push_page(page.page, page.offset, |p| whole.push(p.data.to_vec()))
                .unwrap();
        }
        capture.finish().unwrap();
        demux.finish().unwrap();
        assert_eq!(packets, whole);
        assert_eq!(packets.len(), 5);
        assert_eq!(capture.compatibility(), index.compatibility());
        assert_eq!(demux.compatibility(), index.compatibility());
        capture.reset();
        demux.reset();
        assert_eq!(capture.compatibility(), CompatibilityStats::default());
        assert_eq!(demux.compatibility(), CompatibilityStats::default());
    }
    let mut limited = Demuxer::with_validation(
        Limits {
            max_packet_bytes: 128,
            ..Limits::default()
        },
        options,
    );
    let mut reader = PageReader::new(bytes.as_slice());
    let error = loop {
        let page = reader.next_page().unwrap().unwrap();
        if let Err(error) = limited.push_page(page.page, page.offset, |_| {}) {
            break error;
        }
    };
    assert_eq!(error, Error::LimitExceeded("packet size"));
}

#[test]
fn unknown_spans_retain_near_maximum_granules_and_reset_clears_them() {
    for allow_decreasing_eos_granule in [false, true] {
        for end in [u64::MAX - 1, u64::MAX - 3, 0, UNKNOWN_GRANULE_POSITION] {
            let options = ValidationOptions {
                allow_decreasing_eos_granule,
                ..ValidationOptions::default()
            };
            let frames: Vec<_> = [
                (BOS, u64::MAX - 2, vec![1], vec![b'h']),
                (0, UNKNOWN_GRANULE_POSITION, vec![255], vec![b'a'; 255]),
                (CONTINUED | EOS, end, vec![1], vec![b'b']),
            ]
            .into_iter()
            .enumerate()
            .map(|(sequence, (flags, granule_position, lacing, body))| {
                encode_page(
                    PageHeader {
                        flags,
                        granule_position,
                        serial: 7,
                        sequence: sequence as u32,
                    },
                    &lacing,
                    &body,
                )
                .unwrap()
            })
            .collect();
            let mut demux = Demuxer::with_validation(Limits::default(), options);
            let mut capture = PacketCapture::with_validation(7, Limits::default(), options);
            let mut offset = 0;
            for (number, bytes) in frames.iter().enumerate() {
                let page = Page::parse(bytes).unwrap().0;
                let mut whole = Vec::new();
                let mut selected = Vec::new();
                let a = demux.push_page(page, offset, |p| {
                    whole.push((p.data.to_vec(), p.info.granule_position));
                });
                let b = capture.push_page(page, offset, |p| {
                    selected.push((p.data.to_vec(), p.info.granule_position));
                });
                assert_eq!(whole, selected);
                if number == 2 && end == UNKNOWN_GRANULE_POSITION {
                    let error = Error::Invalid("unknown granule on page with completed packets");
                    assert_eq!(a, Err(error.clone()));
                    assert_eq!(b, Err(error));
                    assert!(whole.is_empty());
                } else if number == 2 && end < u64::MAX - 2 && !allow_decreasing_eos_granule {
                    assert_eq!(a, Err(Error::Invalid("decreasing granule position")));
                    assert_eq!(b, Err(Error::Invalid("decreasing captured granule")));
                    assert!(whole.is_empty());
                } else {
                    a.unwrap();
                    b.unwrap();
                    if number == 2 {
                        assert_eq!(
                            whole,
                            vec![([vec![b'a'; 255], vec![b'b']].concat(), Some(end))]
                        );
                        assert_eq!(
                            demux.compatibility().decreasing_eos_granules,
                            u64::from(end < u64::MAX - 2)
                        );
                        assert_eq!(capture.compatibility(), demux.compatibility());
                    }
                }
                offset += bytes.len() as u64;
            }
            demux.reset();
            capture.reset();
            let bytes = encode_page(
                PageHeader {
                    flags: BOS | EOS,
                    granule_position: 0,
                    serial: 7,
                    sequence: 0,
                },
                &[0],
                &[],
            )
            .unwrap();
            let page = Page::parse(&bytes).unwrap().0;
            demux.push_page(page, 0, |_| {}).unwrap();
            capture.push_page(page, 0, |_| {}).unwrap();
            demux.finish().unwrap();
            capture.finish().unwrap();
            assert_eq!(demux.compatibility(), CompatibilityStats::default());
            assert_eq!(capture.compatibility(), CompatibilityStats::default());
        }
    }
}
