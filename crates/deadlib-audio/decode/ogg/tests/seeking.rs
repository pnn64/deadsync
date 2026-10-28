use deadlib_audio_decode_ogg::*;
use std::io::Cursor;
use std::ops::ControlFlow;

fn fixture(serial: u32, nil_eos: bool) -> Vec<u8> {
    let sizes = [4, 0, 255, 65024, 130050, 17, 0, 1, 65025, 510, 19];
    let mut writer = PacketWriter::new(Vec::new(), serial);
    for (number, size) in sizes.into_iter().enumerate() {
        let packet: Vec<u8> = (0..size).map(|n| (n * 17 + number * 37) as u8).collect();
        writer
            .write_packet(
                &packet,
                number as u64 * 100,
                number + 1 == sizes.len() && !nil_eos,
            )
            .unwrap();
        if number % 3 == 1 {
            writer.flush_page().unwrap();
        }
    }
    if nil_eos {
        writer.finish().unwrap();
    }
    writer.into_inner()
}
fn pages(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut reader = PageReader::new(bytes);
    let mut pages = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        pages.push(page.page.bytes().to_vec());
    }
    pages
}
fn multiplexed_and_chained() -> Vec<u8> {
    let a = pages(&fixture(21, false));
    let b = pages(&fixture(22, true));
    let mut out = Vec::new();
    for number in 0..a.len().max(b.len()) {
        if let Some(page) = a.get(number) {
            out.extend(page);
        }
        if let Some(page) = b.get(number) {
            out.extend(page);
        }
    }
    out.extend(fixture(23, false));
    out
}
fn packets(bytes: &[u8]) -> Vec<(Vec<u8>, PacketInfo)> {
    let mut reader = PageReader::new(bytes);
    let mut demux = Demuxer::default();
    let mut out = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        demux
            .push_page(page.page, page.offset, |packet| {
                out.push((packet.data.to_vec(), packet.info))
            })
            .unwrap();
    }
    demux.finish().unwrap();
    out
}

#[test]
fn index_events_share_exact_demux_packets_and_finish_every_page_in_physical_order() {
    let bytes = multiplexed_and_chained();
    let mut output = Vec::new();
    let mut observed_pages = Vec::new();
    let index = OggIndex::read_with_events(bytes.as_slice(), IndexOptions::default(), |event| {
        match event {
            IndexEvent::Packet { page, packet } => {
                assert_eq!(packet.info.serial, page.header().serial);
                assert!(packet.info.start_page_offset <= packet.info.end_page_offset);
                output.push((packet.data.to_vec(), packet.info));
            }
            IndexEvent::Page { page, offset } => {
                let completed = output
                    .iter()
                    .filter(|(_, info)| info.end_page_offset == offset)
                    .count();
                assert_eq!(completed, page.completed_packets());
                observed_pages.push((offset, page.header(), page.bytes().len()));
            }
        }
        Ok::<_, Error>(())
    })
    .unwrap();
    assert_eq!(output, packets(&bytes));
    assert!(observed_pages.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert!(
        observed_pages
            .iter()
            .any(|(_, header, length)| header.is_eos() && *length == 27)
    );
    assert_eq!(index.length(), bytes.len() as u64);
    for stream in index.streams() {
        let actual: Vec<_> = index
            .pages(stream.serial)
            .unwrap()
            .iter()
            .map(|entry| (entry.offset, entry.header, entry.length as usize))
            .collect();
        let expected: Vec<_> = observed_pages
            .iter()
            .copied()
            .filter(|(_, header, _)| header.serial == stream.serial)
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn index_callback_errors_stop_observations_and_crc_failure_leaves_only_provisional_events() {
    #[derive(Debug, PartialEq)]
    enum Failure {
        Container(Error),
        Stop,
    }
    impl From<Error> for Failure {
        fn from(value: Error) -> Self {
            Self::Container(value)
        }
    }
    let mut bytes = fixture(1, false);
    let mut packets = 0;
    let result = OggIndex::read_with_events(bytes.as_slice(), IndexOptions::default(), |event| {
        if matches!(event, IndexEvent::Packet { .. }) {
            packets += 1;
            if packets == 3 {
                return Err(Failure::Stop);
            }
        }
        Ok(())
    });
    assert!(matches!(result, Err(Failure::Stop)));
    assert_eq!(packets, 3);
    *bytes.last_mut().unwrap() ^= 1;
    let mut pages = 0;
    let result = OggIndex::read_with_events(bytes.as_slice(), IndexOptions::default(), |event| {
        if matches!(event, IndexEvent::Page { .. }) {
            pages += 1;
        }
        Ok::<_, Failure>(())
    });
    assert!(matches!(
        result,
        Err(Failure::Container(Error::Checksum { .. }))
    ));
    assert!(pages > 0);
}

#[test]
fn paused_capture_matches_demux_from_every_page_including_spans_and_mux() {
    let bytes = multiplexed_and_chained();
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let expected = packets(&bytes);
    for stream in index.streams() {
        for entry in index.pages(stream.serial).unwrap() {
            for mixed in [false, true] {
                let mut reader = PageReader::new(Cursor::new(&bytes));
                reader.seek(entry.offset).unwrap();
                let mut capture = PacketCapture::at_page(entry, Limits::default());
                let mut output = Vec::new();
                let mut pauses = 0;
                while let Some(page) = reader.next_page().unwrap() {
                    loop {
                        let mut emitted = 0;
                        let complete = capture
                            .push_page_until(page.page, page.offset, |packet| {
                                output.push((packet.data.to_vec(), packet.info));
                                emitted += 1;
                                ControlFlow::Break(())
                            })
                            .unwrap();
                        assert!(emitted <= 1);
                        if complete {
                            break;
                        }
                        assert_eq!(emitted, 1);
                        pauses += 1;
                        if mixed && pauses % 3 == 0 {
                            capture
                                .push_page(page.page, page.offset, |packet| {
                                    output.push((packet.data.to_vec(), packet.info));
                                })
                                .unwrap();
                            break;
                        }
                    }
                    if capture.is_ended() {
                        break;
                    }
                }
                capture.finish().unwrap();
                let suffix: Vec<_> = expected
                    .iter()
                    .filter(|(_, info)| {
                        info.serial == stream.serial && info.start_page_offset >= entry.offset
                    })
                    .collect();
                assert_eq!(output.len(), suffix.len());
                for ((data, info), &(reference, expected)) in output.iter().zip(&suffix) {
                    assert_eq!(data, reference);
                    assert_eq!(info.number, Some(expected.number));
                    assert_eq!(info.granule_position, expected.granule_position);
                    assert_eq!(info.first_in_stream, expected.first_in_stream);
                    assert_eq!(info.last_in_stream, expected.last_in_stream);
                    assert_eq!(info.start_page_offset, expected.start_page_offset);
                    assert_eq!(info.end_page_offset, expected.end_page_offset);
                }
            }
        }
    }
}

#[test]
fn paused_capture_rejects_changed_pages_and_validates_tail_before_output() {
    let header = PageHeader {
        flags: EOS,
        granule_position: 7,
        serial: 91,
        sequence: 4,
    };
    let bytes = encode_page(header, &[1, 1], b"ab").unwrap();
    let changed = encode_page(header, &[1, 1], b"ac").unwrap();
    let page = Page::parse(&bytes).unwrap().0;
    let mut capture = PacketCapture::new(91, Limits::default());
    assert!(
        !capture
            .push_page_until(page, 17, |_| ControlFlow::Break(()))
            .unwrap()
    );
    assert!(!capture.is_ended());
    assert!(
        capture
            .push_page(Page::parse(&changed).unwrap().0, 17, |_| {})
            .is_err()
    );
    assert_eq!(
        capture.push_page(page, 17, |_| {}).unwrap_err(),
        Error::Failed
    );
    capture.reset();
    capture.push_page(page, 17, |_| {}).unwrap();
    capture.finish().unwrap();

    for (lacing, body, limits) in [
        (
            vec![1, 4],
            vec![0; 5],
            Limits {
                max_packet_bytes: 3,
                ..Limits::default()
            },
        ),
        (vec![1, 255], vec![0; 256], Limits::default()),
    ] {
        let bytes = encode_page(header, &lacing, &body).unwrap();
        let mut capture = PacketCapture::new(91, limits);
        assert!(
            capture
                .push_page_until(Page::parse(&bytes).unwrap().0, 0, |_| {
                    panic!("malformed/resource-invalid page emitted a prefix")
                })
                .is_err()
        );
    }
}

#[test]
fn paused_capture_counts_compatibility_once_at_page_completion() {
    let validation = ValidationOptions {
        allow_decreasing_eos_granule: true,
        ..ValidationOptions::default()
    };
    let mut capture = PacketCapture::with_validation(92, Limits::default(), validation);
    let first = encode_page(
        PageHeader {
            flags: BOS,
            granule_position: 10,
            serial: 92,
            sequence: 0,
        },
        &[1],
        b"a",
    )
    .unwrap();
    capture
        .push_page(Page::parse(&first).unwrap().0, 0, |_| {})
        .unwrap();
    let last = encode_page(
        PageHeader {
            flags: EOS,
            granule_position: 5,
            serial: 92,
            sequence: 1,
        },
        &[1, 1],
        b"bc",
    )
    .unwrap();
    let page = Page::parse(&last).unwrap().0;
    assert!(
        !capture
            .push_page_until(page, first.len() as u64, |_| ControlFlow::Break(()))
            .unwrap()
    );
    assert_eq!(capture.compatibility().decreasing_eos_granules, 0);
    assert!(
        capture
            .push_page_until(page, first.len() as u64, |_| ControlFlow::Break(()))
            .unwrap()
    );
    assert_eq!(capture.compatibility().decreasing_eos_granules, 1);
    capture.finish().unwrap();
}

#[test]
fn current_page_reborrows_verified_storage_and_is_cleared_on_seek_or_eof() {
    let bytes = fixture(93, true);
    let mut reader = PageReader::new(Cursor::new(&bytes));
    assert!(reader.current_page().is_none());
    let (offset, expected) = {
        let page = reader.next_page().unwrap().unwrap();
        (page.offset, page.page.bytes().to_vec())
    };
    for _ in 0..3 {
        let page = reader.current_page().unwrap();
        assert_eq!(page.offset, offset);
        assert_eq!(page.page.bytes(), expected);
    }
    reader.seek(0).unwrap();
    assert!(reader.current_page().is_none());
    while reader.next_page().unwrap().is_some() {}
    assert!(reader.current_page().is_none());
}

#[test]
fn indexed_granules_and_packet_boundaries_match_complete_demux() {
    for bytes in [
        fixture(19, false),
        fixture(20, true),
        multiplexed_and_chained(),
    ] {
        let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
        assert_eq!(index.length(), bytes.len() as u64);
        let packets = packets(&bytes);
        let count: usize = index
            .streams()
            .iter()
            .map(|stream| index.pages(stream.serial).unwrap().len())
            .sum();
        assert_eq!(count, pages(&bytes).len());
        assert!(index.storage_bytes() >= count * std::mem::size_of::<IndexedPage>());
        for (number, stream) in index.streams().iter().enumerate() {
            assert_eq!(stream.chain, if number == 2 { 1 } else { 0 });
            let pages = index.pages(stream.serial).unwrap();
            assert_eq!(pages.first().unwrap().offset, stream.bos_offset);
            assert_eq!(pages.last().unwrap().offset, stream.eos_offset);
            assert_eq!(
                pages.last().unwrap().offset + pages.last().unwrap().length as u64,
                stream.end_offset
            );
            let expected: Vec<_> = packets
                .iter()
                .filter(|(_, info)| info.serial == stream.serial)
                .collect();
            assert_eq!(stream.packets, expected.len() as u64);
            for (_, info) in &expected {
                let page = index.packet_start_page(stream.serial, info.number).unwrap();
                assert_eq!(page.offset, info.start_page_offset);
                assert_eq!(page.stream, number);
            }
            assert!(
                index
                    .packet_start_page(stream.serial, stream.packets)
                    .is_none()
            );
            for target in (0..=1100).chain([u64::MAX - 1, u64::MAX]) {
                let expected = pages.iter().rev().find(|page| {
                    page.header.granule_position != UNKNOWN_GRANULE_POSITION
                        && page.header.granule_position <= target
                });
                let actual = index.granule_at_or_before(stream.serial, target);
                assert_eq!(actual.map(|p| p.offset), expected.map(|p| p.offset));
            }
            for page in pages {
                assert_eq!(
                    index
                        .page_at_or_before(stream.serial, page.offset)
                        .unwrap()
                        .offset,
                    page.offset
                );
                assert_eq!(
                    index
                        .page_at_or_before(stream.serial, page.offset + page.length as u64 - 1)
                        .unwrap()
                        .offset,
                    page.offset
                );
            }
        }
        assert!(index.pages(0xdeadbeef).is_none());
        assert!(index.granule_at_or_before(0xdeadbeef, 0).is_none());
    }
}

#[test]
fn index_carries_packet_origins_across_nil_and_mixed_continuation_pages() {
    let mut bytes = Vec::new();
    let mut offsets = Vec::new();
    for (sequence, (flags, granule, lacing, body)) in [
        (BOS, 0, vec![1], vec![37]),
        (0, UNKNOWN_GRANULE_POSITION, vec![255], vec![37; 255]),
        (CONTINUED, UNKNOWN_GRANULE_POSITION, vec![], vec![]),
        (CONTINUED, 2, vec![0, 1, 255], vec![37; 256]),
        (CONTINUED, UNKNOWN_GRANULE_POSITION, vec![], vec![]),
        (
            CONTINUED,
            UNKNOWN_GRANULE_POSITION,
            vec![255],
            vec![37; 255],
        ),
        (CONTINUED | EOS, 3, vec![0], vec![]),
    ]
    .into_iter()
    .enumerate()
    {
        offsets.push(bytes.len() as u64);
        bytes.extend(
            encode_page(
                PageHeader {
                    flags,
                    granule_position: granule,
                    serial: 42,
                    sequence: sequence as u32,
                },
                &lacing,
                &body,
            )
            .unwrap(),
        );
    }
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let indexed = index.pages(42).unwrap();
    assert_eq!(indexed.len(), offsets.len());
    for (entry, expected) in indexed.iter().zip([0, 1, 1, 1, 3, 3, 3]) {
        assert_eq!(entry.first_packet_offset, offsets[expected]);
    }
    for (packet, expected) in [0, 1, 3, 3].into_iter().enumerate() {
        assert_eq!(
            index.packet_start_page(42, packet as u64).unwrap().offset,
            offsets[expected]
        );
    }
    assert_eq!(index.stream(42).unwrap().packets, 4);
}

#[test]
fn every_indexed_page_capture_and_repeated_seek_returns_exact_suffix() {
    let bytes = multiplexed_and_chained();
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let expected = packets(&bytes);
    let mut reader = PageReader::new(Cursor::new(&bytes));
    let mut capture = PacketCapture::new(0, Limits::default());
    // Reverse order exercises transport/assembly reset after EOS, as well as
    // leading continuations, fully spanned pages and nil EOS captures.
    for stream in index.streams().iter().rev() {
        for entry in index.pages(stream.serial).unwrap().iter().rev() {
            reader.seek(entry.offset).unwrap();
            capture.reset_at(entry);
            let mut output = Vec::new();
            while let Some(page) = reader.next_page().unwrap() {
                capture
                    .push_page(page.page, page.offset, |packet| {
                        output.push((packet.data.to_vec(), packet.info))
                    })
                    .unwrap();
                if capture.is_ended() {
                    break;
                }
            }
            capture.finish().unwrap();
            let suffix: Vec<_> = expected
                .iter()
                .filter(|(_, info)| {
                    info.serial == stream.serial && info.start_page_offset >= entry.offset
                })
                .collect();
            assert_eq!(output.len(), suffix.len(), "offset {}", entry.offset);
            for ((data, info), &(expected_data, expected_info)) in output.iter().zip(&suffix) {
                assert_eq!(data, expected_data);
                assert_eq!(info.number, Some(expected_info.number));
                assert_eq!(info.first_in_stream, expected_info.first_in_stream);
                assert_eq!(info.last_in_stream, expected_info.last_in_stream);
                assert_eq!(info.granule_position, expected_info.granule_position);
                assert_eq!(info.start_page_offset, expected_info.start_page_offset);
                assert_eq!(info.end_page_offset, expected_info.end_page_offset);
            }
        }
    }
}

#[test]
fn unindexed_capture_discards_only_unavailable_initial_fragment() {
    let bytes = fixture(31, false);
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let expected = packets(&bytes);
    for entry in index.pages(31).unwrap() {
        let mut capture = PacketCapture::new(31, Limits::default());
        let mut reader = PageReader::new(Cursor::new(&bytes));
        reader.seek(entry.offset).unwrap();
        let mut output = Vec::new();
        while let Some(page) = reader.next_page().unwrap() {
            capture
                .push_page(page.page, page.offset, |packet| {
                    output.push((packet.data.to_vec(), packet.info.number))
                })
                .unwrap();
        }
        capture.finish().unwrap();
        let suffix: Vec<_> = expected
            .iter()
            .filter(|(_, info)| info.start_page_offset >= entry.offset)
            .collect();
        assert_eq!(output.len(), suffix.len());
        for ((data, number), &(expected, info)) in output.iter().zip(&suffix) {
            assert_eq!(data, expected);
            assert_eq!(*number, entry.header.is_bos().then_some(info.number));
        }
    }
}

#[test]
fn capture_at_empty_continued_page_keeps_skipping_until_packet_end() {
    let mut bytes = Vec::new();
    for (sequence, flags, gp, lacing, body) in [
        (0, BOS, 0, vec![1], vec![b'h']),
        (1, 0, UNKNOWN_GRANULE_POSITION, vec![255], vec![b'a'; 255]),
        (2, CONTINUED, UNKNOWN_GRANULE_POSITION, vec![], vec![]),
        (3, CONTINUED | EOS, 9, vec![0, 1], vec![b'x']),
    ] {
        bytes.extend(
            encode_page(
                PageHeader {
                    flags,
                    granule_position: gp,
                    serial: 32,
                    sequence,
                },
                &lacing,
                &body,
            )
            .unwrap(),
        );
    }
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let pages = index.pages(32).unwrap();
    let mut capture = PacketCapture::at_page(&pages[2], Limits::default());
    let mut reader = PageReader::new(Cursor::new(&bytes));
    reader.seek(pages[2].offset).unwrap();
    let mut output = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        capture
            .push_page(page.page, page.offset, |packet| {
                output.push((packet.data.to_vec(), packet.info.number))
            })
            .unwrap();
    }
    capture.finish().unwrap();
    assert_eq!(output, vec![(b"x".to_vec(), Some(2))]);
    assert_eq!(
        index.packet_start_page(32, 1).unwrap().offset,
        pages[1].offset
    );
    assert_eq!(
        index.packet_start_page(32, 2).unwrap().offset,
        pages[3].offset
    );
}

#[test]
fn index_and_capture_reject_corruption_truncation_holes_and_limits() {
    let bytes = fixture(41, false);
    assert!(OggIndex::read(&bytes[..bytes.len() - 1], IndexOptions::default()).is_err());
    assert!(OggIndex::read(&[][..], IndexOptions::default()).is_err());
    let options = IndexOptions {
        max_pages: 1,
        ..IndexOptions::default()
    };
    assert!(matches!(
        OggIndex::read(bytes.as_slice(), options),
        Err(Error::LimitExceeded("index pages"))
    ));
    let mut broken = bytes.clone();
    broken[28] ^= 1;
    assert!(matches!(
        OggIndex::read(broken.as_slice(), IndexOptions::default()),
        Err(Error::Checksum { .. })
    ));
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let entries = index.pages(41).unwrap();
    let mut capture = PacketCapture::at_page(&entries[0], Limits::default());
    let page = Page::parse(&bytes[entries[0].offset as usize..]).unwrap().0;
    capture.push_page(page, 0, |_| {}).unwrap();
    let page = Page::parse(&bytes[entries[2].offset as usize..]).unwrap().0;
    assert!(matches!(
        capture.push_page(page, entries[2].offset, |_| {}),
        Err(Error::Sequence { .. })
    ));
    assert!(matches!(
        capture.push_page(page, entries[2].offset, |_| {}),
        Err(Error::Failed)
    ));
    capture.reset_at(&entries[2]);
    capture.push_page(page, entries[2].offset, |_| {}).unwrap();
    assert_eq!(capture.finish(), Err(Error::Truncated));
    let mut capture = PacketCapture::new(
        41,
        Limits {
            max_packet_bytes: 1000,
            ..Limits::default()
        },
    );
    let mut reader = PageReader::new(bytes.as_slice());
    let error = loop {
        let page = reader.next_page().unwrap().unwrap();
        if let Err(error) = capture.push_page(page.page, page.offset, |_| {}) {
            break error;
        }
    };
    assert_eq!(error, Error::LimitExceeded("packet size"));
    let mut capture = PacketCapture::new(
        41,
        Limits {
            max_pending_bytes: 1000,
            ..Limits::default()
        },
    );
    let mut reader = PageReader::new(bytes.as_slice());
    let error = loop {
        let page = reader.next_page().unwrap().unwrap();
        if let Err(error) = capture.push_page(page.page, page.offset, |_| {}) {
            break error;
        }
    };
    assert_eq!(error, Error::LimitExceeded("pending packet bytes"));

    let first = Page::parse(&bytes).unwrap().0;
    let second = Page::parse(&bytes[entries[1].offset as usize..]).unwrap().0;
    let mut header = first.header();
    header.granule_position = 200;
    let first_bytes = encode_page(header, first.lacing(), first.body()).unwrap();
    let mut capture = PacketCapture::new(41, Limits::default());
    capture
        .push_page(Page::parse(&first_bytes).unwrap().0, 0, |_| {})
        .unwrap();
    assert!(matches!(
        capture.push_page(second, entries[1].offset, |_| {}),
        Err(Error::Invalid("decreasing captured granule"))
    ));
    capture.reset();
    capture.push_page(first, 0, |_| {}).unwrap();
    let mut header = second.header();
    header.flags |= CONTINUED;
    let second_bytes = encode_page(header, second.lacing(), second.body()).unwrap();
    assert!(matches!(
        capture.push_page(
            Page::parse(&second_bytes).unwrap().0,
            entries[1].offset,
            |_| {}
        ),
        Err(Error::Invalid("captured packet continuation flag"))
    ));
}

fn reference_capture(bytes: &[u8], entry: &IndexedPage) -> usize {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let executable = std::env::var_os("DEADLIB_LIBOGG_ORACLE").expect("set DEADLIB_LIBOGG_ORACLE");
    let file = std::env::temp_dir().join(format!(
        "deadlib-ogg-capture-{}-{}.ogg",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut reader = PageReader::new(Cursor::new(bytes));
    reader.seek(entry.offset).unwrap();
    let mut suffix = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        if page.page.header().serial == entry.header.serial {
            suffix.extend_from_slice(page.page.bytes());
            if page.page.header().is_eos() {
                break;
            }
        }
    }
    std::fs::write(&file, &suffix).unwrap();
    let reference = Command::new(executable)
        .arg("capture")
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    fn take<'a>(input: &mut &'a [u8], n: usize) -> &'a [u8] {
        let (a, b) = input.split_at(n);
        *input = b;
        a
    }
    fn u32_in(input: &mut &[u8]) -> u32 {
        u32::from_le_bytes(take(input, 4).try_into().unwrap())
    }
    fn u64_in(input: &mut &[u8]) -> u64 {
        u64::from_le_bytes(take(input, 8).try_into().unwrap())
    }
    let mut input = reference.stdout.as_slice();
    reader.seek(entry.offset).unwrap();
    let mut capture = PacketCapture::at_page(entry, Limits::default());
    let mut count = 0;
    while let Some(page) = reader.next_page().unwrap() {
        let header = page.page.header();
        if header.serial != entry.header.serial {
            continue;
        }
        assert_eq!(take(&mut input, 1), b"G");
        assert_eq!(u32_in(&mut input), header.serial);
        assert_eq!(u32_in(&mut input), header.sequence);
        assert_eq!(u64_in(&mut input), header.granule_position);
        assert_eq!(take(&mut input, 1)[0], header.flags);
        assert_eq!(u32_in(&mut input) as usize, page.page.bytes().len());
        capture
            .push_page(page.page, page.offset, |packet| {
                assert_eq!(take(&mut input, 1), b"P");
                assert_eq!(u32_in(&mut input), packet.info.serial);
                assert_eq!(
                    u64_in(&mut input),
                    packet
                        .info
                        .granule_position
                        .unwrap_or(UNKNOWN_GRANULE_POSITION)
                );
                assert_eq!(take(&mut input, 1)[0] != 0, packet.info.first_in_stream);
                assert_eq!(take(&mut input, 1)[0] != 0, packet.info.last_in_stream);
                let n = u64_in(&mut input) as usize;
                assert_eq!(take(&mut input, n), packet.data);
                count += 1;
            })
            .unwrap();
        if header.is_eos() {
            break;
        }
    }
    capture.finish().unwrap();
    assert!(input.is_empty());
    count
}

#[test]
#[ignore = "requires independently built libogg oracle"]
fn libogg_capture_at_every_generated_page_boundary() {
    let bytes = multiplexed_and_chained();
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    for stream in index.streams() {
        for entry in index.pages(stream.serial).unwrap() {
            reference_capture(&bytes, entry);
        }
    }
}

#[test]
#[ignore = "requires libogg oracle and DEADLIB_OGG_CORPUS"]
fn libogg_local_corpus_indexed_captures() {
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    fn collect(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for file in fs::read_dir(root).unwrap() {
            let file = file.unwrap();
            if file.file_type().unwrap().is_dir() {
                out.extend(collect(&file.path()));
            } else if file
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ogg"))
            {
                out.push(file.path());
            }
        }
        out.sort();
        out
    }
    let root = std::env::var_os("DEADLIB_OGG_CORPUS").expect("set DEADLIB_OGG_CORPUS");
    let paths = collect(Path::new(&root));
    assert!(!paths.is_empty());
    let mut counts = (0, 0, 0);
    for path in &paths {
        let bytes = fs::read(path).unwrap();
        let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
        let packets = packets(&bytes);
        for stream in index.streams() {
            let pages = index.pages(stream.serial).unwrap();
            counts.0 += pages.len();
            for (_, packet) in packets.iter().filter(|(_, p)| p.serial == stream.serial) {
                assert_eq!(
                    index
                        .packet_start_page(stream.serial, packet.number)
                        .unwrap()
                        .offset,
                    packet.start_page_offset
                );
            }
            for number in [0, pages.len() / 2, pages.len() - 1] {
                let gp = pages[number].header.granule_position;
                let target = if gp == UNKNOWN_GRANULE_POSITION {
                    0
                } else {
                    gp
                };
                let expected = index
                    .granule_at_or_before(stream.serial, target)
                    .map(|p| p.offset);
                let actual = find_granule(
                    &mut Cursor::new(&bytes),
                    0..bytes.len() as u64,
                    stream.serial,
                    target,
                )
                .unwrap()
                .map(|p| p.offset);
                assert_eq!(actual, expected, "{} / {target}", path.display());
                counts.1 += 1;
                counts.2 += reference_capture(&bytes, &pages[number]);
            }
        }
    }
    eprintln!(
        "libogg indexed captures: {} files, {} indexed pages, {} seeks, {} returned packets",
        paths.len(),
        counts.0,
        counts.1,
        counts.2
    );
}

#[test]
fn indexless_bisection_matches_linear_landmarks_in_all_streams() {
    let bytes = multiplexed_and_chained();
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let mut reader = Cursor::new(&bytes);
    for stream in index.streams() {
        for target in (0..=1100).step_by(53).chain([1000, u64::MAX - 1, u64::MAX]) {
            let expected = index.granule_at_or_before(stream.serial, target);
            // Whole physical file, including competing serials and chains.
            let actual =
                find_granule(&mut reader, 0..bytes.len() as u64, stream.serial, target).unwrap();
            assert_eq!(
                actual.map(|p| p.offset),
                expected.map(|p| p.offset),
                "{} / {target}",
                stream.serial
            );
            // The same search fenced to just this stream's physical extent.
            let actual = find_granule(
                &mut reader,
                stream.bos_offset..stream.end_offset,
                stream.serial,
                target,
            )
            .unwrap();
            assert_eq!(actual.map(|p| p.offset), expected.map(|p| p.offset));
        }
    }
    assert!(
        find_granule(&mut reader, 0..bytes.len() as u64, 99, 1000)
            .unwrap()
            .is_none()
    );
    assert!(find_granule(&mut reader, 0..0, 21, 0).unwrap().is_none());
    assert!(matches!(
        find_granule(&mut reader, std::ops::Range { start: 2, end: 1 }, 21, 0),
        Err(Error::Invalid(_))
    ));
}

#[test]
fn indexless_search_uses_absolute_64_bit_offsets_and_fenced_reads() {
    use std::io::{self, Read, Seek, SeekFrom};
    struct OffsetSource<'a> {
        inner: Cursor<&'a [u8]>,
        base: u64,
        end: u64,
    }
    impl Read for OffsetSource<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            assert!(
                self.base + self.inner.position() + out.len() as u64 <= self.end,
                "read escaped fence"
            );
            self.inner.read(out)
        }
    }
    impl Seek for OffsetSource<'_> {
        fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
            let SeekFrom::Start(offset) = from else {
                panic!("expected absolute seek")
            };
            assert!(offset >= self.base && offset <= self.end);
            self.inner.set_position(offset - self.base);
            Ok(offset)
        }
    }
    let bytes = fixture(51, true);
    let index = OggIndex::read(bytes.as_slice(), IndexOptions::default()).unwrap();
    let base = (1u64 << 54) + 731;
    let end = base + bytes.len() as u64;
    let mut reader = OffsetSource {
        inner: Cursor::new(&bytes),
        base,
        end,
    };
    for target in [0, 200, 300, 500, 1000, u64::MAX] {
        let expected = index.granule_at_or_before(51, target).unwrap();
        let actual = find_granule(&mut reader, base..end, 51, target)
            .unwrap()
            .unwrap();
        assert_eq!(actual.offset, base + expected.offset);
        assert_eq!(actual.header, expected.header);
    }
    // A minimum granule above the target has no predecessor.
    let first = index.granule_at_or_before(51, 500).unwrap().offset;
    assert!(
        find_granule(&mut reader, base + first..end, 51, 0)
            .unwrap()
            .is_none()
    );
}
