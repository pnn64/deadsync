use deadlib_audio_decode_mp3::{bits::BitWriter, checksum, *};
use std::io::{self, Cursor, Read};

struct Split<'a> {
    data: &'a [u8],
    position: usize,
    split: usize,
    chunk: usize,
    calls: usize,
    fail: usize,
}
impl<'a> Split<'a> {
    fn new(data: &'a [u8], split: usize, chunk: usize) -> Self {
        Self {
            data,
            position: 0,
            split,
            chunk,
            calls: 0,
            fail: usize::MAX,
        }
    }
}
impl Read for Split<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls.is_multiple_of(5) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.position >= self.fail {
            return Err(io::ErrorKind::ConnectionReset.into());
        }
        let boundary = if self.position < self.split {
            self.split
        } else {
            self.data.len()
        };
        let count = out
            .len()
            .min(self.chunk)
            .min(boundary - self.position)
            .min(self.fail - self.position);
        out[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
fn frame(version: u32, channels: usize, crc: bool, pad: bool, back: u32) -> Vec<u8> {
    let word = 0xffe00000
        | version << 19
        | 1 << 17
        | u32::from(!crc) << 16
        | 9 << 12
        | u32::from(pad) << 9
        | if channels == 1 { 3 << 6 } else { 0 };
    let h = FrameHeader::parse(&word.to_be_bytes()).unwrap();
    let mut side = BitWriter::new();
    side.write(back, if version == 3 { 9 } else { 8 }).unwrap();
    side.write(
        0,
        match (version == 3, channels) {
            (true, 1) => 5,
            (true, _) => 3,
            (false, 1) => 1,
            _ => 2,
        },
    )
    .unwrap();
    if version == 3 {
        for _ in 0..channels {
            side.write(0, 4).unwrap();
        }
    }
    for granule in 0..h.granules() {
        for _ in 0..channels {
            for (value, width) in [
                (if granule == 0 { 3 } else { 0 }, 12),
                (u32::from(granule == 0), 9),
                (206, 8),
                (0, if version == 3 { 4 } else { 9 }),
                (0, 1),
            ] {
                side.write(value, width).unwrap();
            }
            for _ in 0..3 {
                side.write(1, 5).unwrap();
            }
            side.write(0, 4).unwrap();
            side.write(0, 3).unwrap();
            if version == 3 {
                side.write(0, 1).unwrap();
            }
            side.write(0, 1).unwrap();
            side.write(0, 1).unwrap();
        }
    }
    let mut data = vec![0; h.frame_bytes().unwrap()];
    data[..4].copy_from_slice(&word.to_be_bytes());
    let start = 4 + usize::from(crc) * 2;
    data[start..h.main_data_offset()].copy_from_slice(side.as_bytes());
    let mut main = BitWriter::new();
    for _ in 0..channels {
        main.write(2, 3).unwrap();
    }
    data[h.main_data_offset()..h.main_data_offset() + main.as_bytes().len()]
        .copy_from_slice(main.as_bytes());
    if crc {
        let sum = checksum::bytes(checksum::bytes(0xffff, &data[2..4]), side.as_bytes());
        data[4..6].copy_from_slice(&sum.to_be_bytes());
    }
    data
}
fn tag(size: usize, footer: bool) -> Vec<u8> {
    let mut data = b"ID3\x04\0\0\0\0\0\0".to_vec();
    data[5] = if footer { 16 } else { 0 };
    for (n, b) in data[6..10].iter_mut().enumerate() {
        *b = ((size >> (7 * (3 - n))) & 127) as u8;
    }
    data.extend((0..size).map(|n| [0xff, 0xfb, 0x90, 0xc0][n % 4]));
    if footer {
        data.extend_from_slice(b"3DI");
        data.extend_from_within(3..10);
    }
    data
}
fn packets<R: Read>(reader: &mut PacketReader<R>) -> Result<Vec<(u64, Vec<u8>)>, Error> {
    let mut result = Vec::new();
    while let Some(packet) = reader.next_packet()? {
        result.push((packet.byte_offset, packet.data.to_vec()));
    }
    assert!(reader.next_packet()?.is_none());
    Ok(result)
}

#[test]
fn eof_discovered_during_free_format_probe_does_not_change_resynchronization() {
    let mut free = frame(3, 1, false, false, 0);
    free[2] &= 0x0f;
    free.resize(1306, 0);
    let valid = frame(3, 1, false, false, 0);
    let mut invalid = valid.clone();
    let h = FrameHeader::parse(&invalid).unwrap();
    invalid[4..h.main_data_offset()].fill(0xff); // Invalid big_values, etc.
    let data = [free.repeat(3), vec![0], invalid, vec![0], valid.repeat(2)].concat();
    let expected = packets(&mut PacketReader::new(Cursor::new(&data))).unwrap();
    assert_eq!(expected.len(), 5);
    assert_eq!(expected[3].0, (3 * 1306 + 1 + valid.len() + 1) as u64);
    // Bulk reads have not discovered EOF during inference. Tiny reads need
    // another fill to infer the same size, and that fill discovers EOF early.
    for chunk in [1, 4, 17, 127, 8192] {
        let actual = packets(&mut PacketReader::new(Split::new(&data, 0, chunk))).unwrap();
        assert_eq!(actual, expected, "chunk={chunk}");
    }
}

#[test]
fn every_read_split_preserves_packets_tags_and_positions() {
    let a = frame(3, 1, true, false, 0);
    let b = frame(3, 1, false, true, 0);
    let id3 = tag(1600, true);
    let middle = tag(13, false);
    let mut data = b"noise".to_vec();
    data.extend_from_slice(&id3);
    let a_offset = data.len();
    data.extend_from_slice(&a);
    data.extend_from_slice(&middle);
    let b_offset = data.len();
    data.extend_from_slice(&b);
    let mut tail = [0; 128];
    tail[..3].copy_from_slice(b"TAG");
    tail[3..8].copy_from_slice(b"title");
    data.extend_from_slice(&tail);
    for split in 0..=data.len() {
        let mut reader = PacketReader::new(Split::new(&data, split, usize::MAX));
        let found = packets(&mut reader).unwrap();
        assert_eq!(
            found,
            [(a_offset as u64, a.clone()), (b_offset as u64, b.clone())]
        );
        assert_eq!(reader.byte_position(), data.len() as u64);
        assert_eq!(reader.skipped_bytes(), 5);
        let meta = reader.metadata();
        assert_eq!(meta.id3v2().len(), 2);
        assert_eq!(meta.id3v2()[0].data(), id3);
        assert_eq!(meta.id3v2()[0].byte_offset, 5);
        assert_eq!(meta.id3v2()[1].data(), middle);
        assert_eq!(&meta.id3v1().unwrap().title[..5], b"title");
    }
    // A tag after a single frame must not cause acquisition to discard it.
    let mut one = a.clone();
    one.extend_from_slice(&tail);
    assert_eq!(
        packets(&mut PacketReader::new(Cursor::new(one)))
            .unwrap()
            .len(),
        1
    );
    let mut reader = PacketReader::new(Cursor::new(data));
    reader.next_packet().unwrap().unwrap();
    let (mut source, unread) = reader.into_parts();
    let mut remaining = unread;
    source.read_to_end(&mut remaining).unwrap();
    assert!(remaining.starts_with(&middle));
}

#[test]
fn raw_stream_matches_linear_pcm_for_tiny_reads_and_format_changes() {
    let mut encoded = Vec::new();
    let mut expected = Vec::new();
    let mut raw = FrameDecoder::new();
    let mut pos = 0;
    for (version, channels) in [(3, 1), (3, 2), (2, 2), (2, 1), (0, 1), (0, 2)] {
        for pad in [false, true, false] {
            let bytes = frame(version, channels, true, pad, 0);
            let decoded = raw.decode(&bytes, None).unwrap();
            expected.push((
                pos,
                encoded.len() as u64,
                decoded.header,
                decoded
                    .pcm
                    .planes()
                    .map(<[f32]>::to_vec)
                    .collect::<Vec<_>>(),
            ));
            pos += decoded.pcm.samples() as u64;
            encoded.extend_from_slice(&bytes);
        }
    }
    for chunk in [1, 2, 3, 4, 7, 31, 128, 417, 8192] {
        let mut stream = StreamDecoder::new(Split::new(&encoded, encoded.len() / 2, chunk));
        for (position, offset, header, planes) in &expected {
            let frame = stream.next_frame().unwrap().unwrap();
            assert_eq!(frame.position, *position);
            assert_eq!(frame.byte_offset, *offset);
            assert_eq!(frame.header, *header);
            assert_eq!(frame.status, FrameStatus::Decoded);
            assert_eq!(
                frame.pcm.planes().collect::<Vec<_>>(),
                planes.iter().map(Vec::as_slice).collect::<Vec<_>>()
            );
        }
        assert!(stream.next_frame().unwrap().is_none());
        assert_eq!(stream.stats().raw_samples, pos);
        assert_eq!(stream.stats().decoded_frames, expected.len() as u64);
        stream.reset(Split::new(&encoded, 0, chunk));
        assert_eq!(stream.next_frame().unwrap().unwrap().position, 0);
    }
}

#[test]
fn metadata_larger_than_the_packet_buffer_is_retained_without_hunting_inside_it() {
    let id3 = tag(300_000, true);
    let audio = frame(3, 1, false, false, 0);
    let data = [id3.clone(), audio.clone()].concat();
    let mut reader = PacketReader::new(Split::new(&data, 123_456, 511));
    let packet = reader.next_packet().unwrap().unwrap();
    assert_eq!(packet.byte_offset, id3.len() as u64);
    assert_eq!(packet.skipped_bytes, 0);
    assert_eq!(packet.data, audio);
    assert_eq!(reader.metadata().id3v2()[0].data(), id3);
    assert!(reader.next_packet().unwrap().is_none());
}

#[test]
fn free_format_inference_padding_and_explicit_short_streams() {
    let mut data = Vec::new();
    let mut expected = Vec::new();
    for pad in [false, true, false, true] {
        let mut bytes = frame(3, 1, false, pad, 0);
        bytes[2] &= 15;
        expected.push((data.len() as u64, bytes.clone()));
        data.extend(bytes);
    }
    for chunk in [1, 7, 128, 8192] {
        let mut reader = PacketReader::new(Split::new(&data, 500, chunk));
        assert_eq!(packets(&mut reader).unwrap(), expected);
    }
    let single = &data[..417];
    assert!(matches!(
        PacketReader::new(Cursor::new(single)).next_packet(),
        Err(Error::Unsupported(_))
    ));
    let size = FreeFormat::new(417, Limits::default()).unwrap();
    let options = ReaderOptions {
        free_format: Some(size),
        ..Default::default()
    };
    assert_eq!(
        packets(&mut PacketReader::with_options(
            Cursor::new(single),
            options
        ))
        .unwrap()
        .len(),
        1
    );
    let mut stream = StreamDecoder::with_options(
        Cursor::new(single),
        StreamOptions {
            free_format: Some(size),
            ..Default::default()
        },
    );
    assert_eq!(stream.next_frame().unwrap().unwrap().pcm.samples(), 1152);
}

#[test]
fn every_final_audio_prefix_and_metadata_truncation_has_explicit_policy() {
    let complete = frame(3, 1, false, false, 0);
    let mut bytes = complete.repeat(3);
    for final_bytes in 0..complete.len() {
        let source = &bytes[..2 * complete.len() + final_bytes];
        let mut strict = PacketReader::new(Cursor::new(source));
        strict.next_packet().unwrap().unwrap();
        strict.next_packet().unwrap().unwrap();
        if final_bytes == 0 {
            assert!(strict.next_packet().unwrap().is_none());
        } else {
            assert!(matches!(strict.next_packet(), Err(Error::Truncated)));
            assert!(strict.next_packet().is_err());
        }
        let options = ReaderOptions {
            truncated: TruncatedPolicy::Discard,
            ..Default::default()
        };
        let mut discard = PacketReader::with_options(Cursor::new(source), options);
        assert_eq!(packets(&mut discard).unwrap().len(), 2);
        assert_eq!(discard.discarded_tail_bytes(), final_bytes);
    }
    bytes.truncate(2 * complete.len());
    bytes.extend_from_slice(b"ID3\x04\0\0\0\0\0\x20");
    for policy in [TruncatedPolicy::Error, TruncatedPolicy::Discard] {
        let mut reader = PacketReader::with_options(
            Cursor::new(&bytes),
            ReaderOptions {
                truncated: policy,
                ..Default::default()
            },
        );
        assert!(matches!(packets(&mut reader), Err(Error::Truncated)));
    }
}

#[test]
fn false_headers_limits_and_io_errors_do_not_silently_consume_audio() {
    let frame = frame(3, 1, false, false, 0);
    let mut false_header = vec![0xff, 0xfb, 0xe0, 0xc0];
    false_header.extend_from_slice(b"junk");
    false_header.extend_from_slice(&frame);
    let found = packets(&mut PacketReader::new(Cursor::new(&false_header))).unwrap();
    assert_eq!(found, [(8, frame.clone())]);
    let mut bad_side = vec![0xff, 0xfb, 0x90, 0xc0];
    bad_side.extend([0xff; 32]);
    bad_side.extend(frame.repeat(3));
    let found = packets(&mut PacketReader::new(Cursor::new(bad_side))).unwrap();
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].0, 36);
    let mut data = b"-----".to_vec();
    data.extend(frame.repeat(3));
    let mut options = ReaderOptions::default();
    options.limits.sync_bytes = 4;
    assert!(matches!(
        packets(&mut PacketReader::with_options(Cursor::new(&data), options)),
        Err(Error::LimitExceeded(_))
    ));
    options = ReaderOptions::default();
    options.limits.frame_bytes = 100;
    assert!(matches!(
        packets(&mut PacketReader::with_options(
            Cursor::new(&frame),
            options
        )),
        Err(Error::LimitExceeded(_))
    ));
    let tags = [tag(1, false), tag(1, false), frame.clone()].concat();
    options = ReaderOptions::default();
    options.limits.metadata_bytes = 21;
    assert!(matches!(
        packets(&mut PacketReader::with_options(Cursor::new(tags), options)),
        Err(Error::LimitExceeded(_))
    ));
    let data = frame.repeat(3);
    let mut source = Split::new(&data, 0, 19);
    source.fail = 2 * frame.len() + 4;
    let mut reader = PacketReader::new(source);
    assert_eq!(reader.next_packet().unwrap().unwrap().data, frame);
    assert_eq!(reader.next_packet().unwrap().unwrap().data, frame);
    assert!(matches!(reader.next_packet(), Err(Error::Io(_))));
    reader.reset(Split::new(&data, 0, 19));
    assert_eq!(packets(&mut reader).unwrap().len(), 3);
}

#[test]
fn strict_errors_and_concealment_preserve_known_frame_timing_and_reservoir() {
    let mut first = frame(3, 1, true, false, 0);
    *first.last_mut().unwrap() = 0x40; // Next frame's Huffman (1,0), positive.
    first[4] ^= 1;
    let second = frame(3, 1, false, false, 1);
    let third = frame(3, 1, false, false, 0);
    let bytes = [first, second, third.clone()].concat();
    let mut strict = StreamDecoder::new(Cursor::new(&bytes));
    assert!(matches!(
        strict.next_frame(),
        Err(Error::CrcMismatch { .. })
    ));
    assert!(strict.next_frame().is_err());
    let mut unchecked = StreamDecoder::with_options(
        Cursor::new(&bytes),
        StreamOptions {
            decode: DecodeOptions {
                verify_crc: false,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(
        unchecked.next_frame().unwrap().unwrap().status,
        FrameStatus::Decoded
    );
    let options = StreamOptions {
        recovery: Recovery::Conceal,
        ..Default::default()
    };
    let mut stream = StreamDecoder::with_options(Cursor::new(&bytes), options);
    let first = stream.next_frame().unwrap().unwrap();
    assert_eq!(first.status, FrameStatus::Corrupt);
    assert!(first.pcm.plane(0).unwrap().iter().all(|&v| v == 0.0));
    for n in 1..3 {
        let out = stream.next_frame().unwrap().unwrap();
        assert_eq!(out.position, n * 1152);
        assert_eq!(out.status, FrameStatus::Decoded);
    }
    assert!(stream.next_frame().unwrap().is_none());
    assert_eq!(stream.stats().raw_samples, 3456);
    assert_eq!(stream.stats().concealed_frames, 1);
    assert!(matches!(
        stream.last_recovered_error(),
        Some(Error::CrcMismatch { .. })
    ));
    let missing = [frame(3, 1, false, false, 511), third.clone(), third.clone()].concat();
    let mut stream = StreamDecoder::with_options(Cursor::new(missing), options);
    assert_eq!(
        stream.next_frame().unwrap().unwrap().status,
        FrameStatus::MissingReservoir
    );
    assert_eq!(
        stream.next_frame().unwrap().unwrap().status,
        FrameStatus::Decoded
    );
    let gap = [third.clone(), b"junk bytes".to_vec(), third.clone(), third].concat();
    let mut strict = StreamDecoder::new(Cursor::new(&gap));
    strict.next_frame().unwrap().unwrap();
    assert!(matches!(strict.next_frame(), Err(Error::Invalid(_))));
    let mut recovery = StreamDecoder::with_options(Cursor::new(&gap), options);
    recovery.next_frame().unwrap().unwrap();
    let after_gap = recovery.next_frame().unwrap().unwrap();
    assert_eq!(after_gap.skipped_bytes, 10);
    assert_eq!(after_gap.position, 1152);
    let mut broken = frame(3, 1, false, false, 0);
    broken[6] &= 0xc0;
    broken[7] &= 3; // Zero part2_3 length, but one big-values pair.
    broken.extend_from_slice(&bytes[bytes.len() - 417..]);
    let mut recovering = StreamDecoder::with_options(Cursor::new(broken), options);
    assert_eq!(
        recovering.next_frame().unwrap().unwrap().status,
        FrameStatus::Corrupt
    );
    assert_eq!(
        recovering.next_frame().unwrap().unwrap().status,
        FrameStatus::Decoded
    );
    let unsupported = frame(0, 1, false, false, 0);
    let mut disabled = StreamDecoder::with_options(
        Cursor::new(unsupported),
        StreamOptions {
            decode: DecodeOptions {
                allow_mpeg25: false,
                ..Default::default()
            },
            ..options
        },
    );
    assert!(matches!(disabled.next_frame(), Err(Error::Unsupported(_))));
    let mut warm = frame(3, 1, true, false, 0);
    let header = FrameHeader::parse(&warm).unwrap();
    warm[header.main_data_offset()..].fill(0x40);
    for entropy_error in [false, true] {
        let mut bad = warm.clone();
        bad[header.main_data_offset()..].fill(0);
        if entropy_error {
            bad[8] &= 0xc0;
            bad[9] &= 3;
            let sum = checksum::bytes(
                checksum::bytes(0xffff, &bad[2..4]),
                &bad[6..header.main_data_offset()],
            );
            bad[4..6].copy_from_slice(&sum.to_be_bytes());
        } else {
            bad[4] ^= 1;
        }
        // The back-pointer starts in the older warm packet. Duplicating the bad
        // payload or discarding that older history changes the next codeword.
        let history = [warm.clone(), bad, frame(3, 1, false, false, 511)].concat();
        let mut recovery = StreamDecoder::with_options(Cursor::new(history), options);
        assert_eq!(
            recovery.next_frame().unwrap().unwrap().status,
            FrameStatus::Decoded
        );
        assert_eq!(
            recovery.next_frame().unwrap().unwrap().status,
            FrameStatus::Corrupt
        );
        let recovered = recovery.next_frame().unwrap().unwrap();
        assert_eq!(recovered.status, FrameStatus::Decoded);
        let mut reference = FrameDecoder::new();
        let encoded = frame(3, 1, false, false, 0);
        let expected = reference.decode(&encoded, None).unwrap();
        assert_eq!(recovered.pcm.plane(0), expected.pcm.plane(0));
    }
}
