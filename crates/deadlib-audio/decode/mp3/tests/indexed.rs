mod support;
use deadlib_audio_decode_mp3::*;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use support::{frame, info, protect};

fn raw_pcm(data: &[u8]) -> Vec<Vec<f32>> {
    let mut stream = StreamDecoder::new(Cursor::new(data));
    let mut result = Vec::<Vec<f32>>::new();
    while let Some(out) = stream.next_frame().unwrap() {
        if result.is_empty() {
            result.resize_with(out.pcm.channels(), Vec::new);
        }
        for (samples, plane) in result.iter_mut().zip(out.pcm.planes()) {
            samples.extend_from_slice(plane);
        }
    }
    result
}
fn collect(reader: &mut IndexedDecoder<Cursor<&[u8]>>) -> Vec<Vec<f32>> {
    let mut result = Vec::<Vec<f32>>::new();
    let mut pos = reader.position();
    while let Some(out) = reader.next_frame().unwrap() {
        assert_eq!(out.position, pos);
        pos += out.pcm.samples() as u64;
        if result.is_empty() {
            result.resize_with(out.pcm.channels(), Vec::new);
        }
        for (samples, plane) in result.iter_mut().zip(out.pcm.planes()) {
            samples.extend_from_slice(plane);
        }
    }
    assert_eq!(pos, reader.samples());
    result
}
#[test]
fn indexed_seeks_match_linear_raw_pcm_with_exact_positions() {
    assert_eq!(std::mem::size_of::<IndexEntry>(), 32);
    for version in [3, 2, 0] {
        for channels in [1, 2] {
            let mut bytes = Vec::new();
            for n in 0..16 {
                bytes.extend(frame(version, 0, channels, 190 + n, n % 2 == 0, 0));
            }
            let expected = raw_pcm(&bytes);
            let length = expected[0].len() as u64;
            let mut reader = IndexedDecoder::with_options(
                Cursor::new(bytes.as_slice()),
                FileOptions {
                    samples: SampleMode::Raw,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(reader.samples(), length);
            assert_eq!(collect(&mut reader), expected);
            let mut positions = vec![0, 1, 31, 32, 528, 529, 575, 576, 577, length - 1, length];
            for entry in reader.index().frames() {
                let p = entry.raw_position();
                for offset in [0, 1] {
                    if p >= offset {
                        positions.push(p - offset);
                    }
                }
            }
            let mut random = 173u64;
            for _ in 0..32 {
                random ^= random << 13;
                random ^= random >> 7;
                random ^= random << 17;
                positions.push(random % (length + 1));
            }
            positions.reverse();
            positions.extend([length, 0, length / 2, 0, length]);
            for position in positions {
                reader.seek_sample(position).unwrap();
                assert_eq!(reader.position(), position);
                if position == length {
                    assert!(reader.next_frame().unwrap().is_none());
                    continue;
                }
                let mut offset = position as usize;
                for _ in 0..2 {
                    let Some(out) = reader.next_frame().unwrap() else {
                        break;
                    };
                    assert_eq!(out.position, offset as u64);
                    assert_eq!(out.raw_position, offset as u64);
                    for (c, plane) in out.pcm.planes().enumerate() {
                        assert_eq!(plane, &expected[c][offset..offset + plane.len()]);
                    }
                    offset += out.pcm.samples();
                }
            }
            let before = reader.position();
            assert!(reader.seek_sample(length + 1).is_err());
            assert_eq!(reader.position(), before);
        }
    }
}

#[test]
fn tagged_gapless_counts_ranges_and_seeks_follow_delay_and_padding() {
    for version in [3, 2, 0] {
        for channels in [1, 2] {
            let granule = if version == 3 { 1152 } else { 576 };
            let audio_frames = 9;
            let delay = 576;
            let padding = 700;
            let mut bytes = info(version, 0, channels, audio_frames, delay, padding);
            for n in 0..audio_frames {
                bytes.extend(frame(version, 0, channels, 196 + n as u8, n % 2 == 0, 0));
            }
            let raw = raw_pcm(&bytes);
            let start = granule + delay as usize + DECODER_DELAY as usize;
            let length = audio_frames as usize * granule - delay as usize - padding as usize;
            let expected: Vec<_> = raw
                .iter()
                .map(|p| p[start..start + length].to_vec())
                .collect();
            let mut reader = IndexedDecoder::new(Cursor::new(bytes.as_slice())).unwrap();
            let range = reader.sample_range();
            assert_eq!(range.raw_start(), start as u64);
            assert_eq!(range.samples(), length as u64);
            assert_eq!(range.to_raw(0), Some(start as u64));
            assert_eq!(range.to_raw(length as u64), Some((start + length) as u64));
            assert_eq!(range.from_raw(start as u64 - 1), None);
            assert_eq!(range.from_raw((start + length) as u64 + 1), None);
            assert_eq!(collect(&mut reader), expected);
            for position in [
                0,
                1,
                47,
                48,
                529,
                575,
                576,
                length - 1,
                length / 2,
                0,
                length,
            ] {
                reader.seek_sample(position as u64).unwrap();
                if position == length {
                    assert!(reader.next_frame().unwrap().is_none());
                    continue;
                }
                let out = reader.next_frame().unwrap().unwrap();
                assert_eq!(out.position, position as u64);
                assert_eq!(out.raw_position, (start + position) as u64);
                for (c, plane) in out.pcm.planes().enumerate() {
                    assert_eq!(plane, &expected[c][position..position + plane.len()]);
                }
            }
        }
    }
}

#[test]
fn short_padding_drains_dsp_and_seeking_into_the_tail_is_exact() {
    for version in [3, 2, 0] {
        let mut bytes = info(version, 0, 2, 4, 576, 0);
        for n in 0..4 {
            bytes.extend(frame(version, 0, 2, 190 + n, false, 0));
        }
        let raw_count = (5 * if version == 3 { 1152 } else { 576 }) as u64;
        let mut zero = frame(version, 0, 2, 0, false, 0);
        zero[4..].fill(0);
        let mut extended = bytes.clone();
        extended.extend(zero);
        let raw = raw_pcm(&extended);
        let mut reader = IndexedDecoder::new(Cursor::new(bytes.as_slice())).unwrap();
        let range = reader.sample_range();
        assert_eq!(range.raw_end(), raw_count + DECODER_DELAY);
        let expected: Vec<_> = raw
            .iter()
            .map(|p| p[range.raw_start() as usize..range.raw_end() as usize].to_vec())
            .collect();
        assert_eq!(collect(&mut reader), expected);
        let tail = raw_count - range.raw_start();
        for position in [tail - 1, tail, tail + 1, range.samples() - 1, tail] {
            reader.seek_sample(position).unwrap();
            let out = reader.next_frame().unwrap().unwrap();
            assert_eq!(out.position, position);
            assert_eq!(out.byte_offset.is_none(), position >= tail);
            for (c, plane) in out.pcm.planes().enumerate() {
                assert_eq!(
                    plane,
                    &expected[c][position as usize..position as usize + plane.len()]
                );
            }
        }
    }
}

#[test]
fn index_limits_metadata_mismatches_and_source_origin_are_checked() {
    let audio = frame(3, 0, 1, 206, false, 0).repeat(4);
    let mut prefixed = vec![42; 137];
    prefixed.extend(&audio);
    let mut source = Cursor::new(prefixed.as_slice());
    source.seek(SeekFrom::Start(137)).unwrap();
    let mut reader = IndexedDecoder::new(source).unwrap();
    assert_eq!(reader.index().origin(), 137);
    assert_eq!(reader.index().frames()[0].byte_offset(), 137);
    assert_eq!(reader.next_frame().unwrap().unwrap().byte_offset, Some(137));
    let mut options = FileOptions::default();
    options.stream.decode.limits.index_frames = 3;
    assert!(matches!(
        IndexedDecoder::with_options(Cursor::new(audio.as_slice()), options),
        Err(Error::LimitExceeded(_))
    ));
    let bad = [info(3, 0, 1, 5, 576, 700), audio.clone()].concat();
    assert!(matches!(
        IndexedDecoder::new(Cursor::new(bad.as_slice())),
        Err(Error::Invalid(_))
    ));
    let raw = FileOptions {
        samples: SampleMode::Raw,
        ..Default::default()
    };
    assert!(IndexedDecoder::with_options(Cursor::new(bad.as_slice()), raw).is_ok());
    let impossible = [info(3, 0, 1, 1, 4095, 4095), frame(3, 0, 1, 206, false, 0)].concat();
    assert!(matches!(
        IndexedDecoder::new(Cursor::new(impossible.as_slice())),
        Err(Error::Invalid(_))
    ));
    assert!(IndexedDecoder::new(Cursor::new(&[][..])).is_err());
}

#[test]
fn pcm_slices_keep_stereo_stride_and_reject_invalid_ranges() {
    let bytes = frame(3, 0, 2, 206, false, 0);
    let mut decoder = FrameDecoder::new();
    let out = decoder.decode(&bytes, None).unwrap();
    for range in [0..0, 1..47, 511..576, 1152..1152] {
        let sliced = out.pcm.slice(range.clone()).unwrap();
        assert_eq!(sliced.samples(), range.len());
        for (c, p) in sliced.planes().enumerate() {
            assert_eq!(p, &out.pcm.plane(c).unwrap()[range.clone()]);
        }
    }
    assert!(out.pcm.slice(0..1153).is_none());
    let nested = out.pcm.slice(1..100).unwrap().slice(5..9).unwrap();
    for c in 0..2 {
        assert_eq!(nested.plane(c), Some(&out.pcm.plane(c).unwrap()[6..10]));
    }
    assert!(
        out.pcm
            .slice(std::ops::Range { start: 8, end: 3 })
            .is_none()
    );
}

#[test]
fn indexed_recovery_and_seek_preserve_reservoir_spans_and_discontinuities() {
    let mut warm = protect(frame(3, 0, 1, 206, false, 0));
    let header = FrameHeader::parse(&warm).unwrap();
    warm[header.main_data_offset()..].fill(0x40);
    let mut bad = warm.clone();
    bad[header.main_data_offset()..].fill(0);
    bad[4] ^= 1;
    let continuous = [
        warm.clone(),
        bad,
        frame(3, 0, 1, 206, false, 511),
        frame(3, 0, 1, 196, true, 0),
    ]
    .concat();
    let gap = [
        warm,
        b"unframed bytes".to_vec(),
        frame(2, 0, 1, 198, true, 0),
        frame(2, 0, 1, 199, false, 0),
    ]
    .concat();
    let stream_options = StreamOptions {
        recovery: Recovery::Conceal,
        ..Default::default()
    };
    for data in [&continuous, &gap] {
        let mut stream = StreamDecoder::with_options(Cursor::new(data.as_slice()), stream_options);
        let mut expected = Vec::new();
        while let Some(out) = stream.next_frame().unwrap() {
            expected.extend_from_slice(out.pcm.plane(0).unwrap());
        }
        let options = FileOptions {
            stream: stream_options,
            samples: SampleMode::Raw,
        };
        let mut indexed =
            IndexedDecoder::with_options(Cursor::new(data.as_slice()), options).unwrap();
        assert_eq!(collect(&mut indexed), [expected.clone()]);
        for position in [0, 1, 1152, 2304, 2305, expected.len() - 1, 0] {
            if position >= expected.len() {
                continue;
            }
            indexed.seek_sample(position as u64).unwrap();
            let out = indexed.next_frame().unwrap().unwrap();
            assert_eq!(
                out.pcm.plane(0).unwrap(),
                &expected[position..position + out.pcm.samples()]
            );
        }
    }
}

#[test]
fn empty_tagged_audio_and_unknown_encoder_metadata_have_explicit_ranges() {
    let empty = info(3, 0, 1, 0, 0, 0);
    let mut reader = IndexedDecoder::new(Cursor::new(empty.as_slice())).unwrap();
    assert_eq!(reader.samples(), 0);
    assert!(reader.next_frame().unwrap().is_none());
    reader.seek_sample(0).unwrap();
    let mut first = info(3, 0, 1, 4, 576, 700);
    let header = FrameHeader::parse(&first).unwrap();
    let name = 4 + header.side_info_bytes() + 12;
    first[name..name + 4].copy_from_slice(b"????");
    let bytes = [first, frame(3, 0, 1, 206, false, 0).repeat(4)].concat();
    let reader = IndexedDecoder::new(Cursor::new(bytes.as_slice())).unwrap();
    assert_eq!(reader.sample_range().raw_start(), 1152);
    assert_eq!(reader.samples(), 4608);
    let mixed = [
        info(3, 0, 1, 3, 576, 700),
        frame(3, 0, 1, 206, false, 0),
        frame(2, 0, 2, 206, false, 0),
        frame(3, 0, 1, 206, false, 0),
    ]
    .concat();
    assert!(matches!(
        IndexedDecoder::new(Cursor::new(mixed.as_slice())),
        Err(Error::Unsupported(_))
    ));
    let raw = FileOptions {
        samples: SampleMode::Raw,
        ..Default::default()
    };
    assert!(IndexedDecoder::with_options(Cursor::new(mixed.as_slice()), raw).is_ok());
}

#[test]
fn caller_supplied_trim_maps_untagged_audio_without_inventing_metadata() {
    let bytes = frame(2, 0, 2, 206, false, 0).repeat(12);
    let raw = raw_pcm(&bytes);
    let trim = tags::Gapless {
        encoder_delay: 576,
        encoder_padding: 1088,
    };
    let options = FileOptions {
        samples: SampleMode::Trim(trim),
        ..Default::default()
    };
    let mut reader = IndexedDecoder::with_options(Cursor::new(bytes.as_slice()), options).unwrap();
    assert!(reader.metadata().vbr().unwrap().is_none());
    let range = reader.sample_range();
    assert_eq!(range.raw_start(), 576 + DECODER_DELAY);
    assert_eq!(range.samples(), 12 * 576 - 576 - 1088);
    let expected: Vec<_> = raw
        .iter()
        .map(|p| p[range.raw_start() as usize..range.raw_end() as usize].to_vec())
        .collect();
    assert_eq!(collect(&mut reader), expected);
    reader.seek_sample(1234).unwrap();
    let out = reader.next_frame().unwrap().unwrap();
    for (c, p) in out.pcm.planes().enumerate() {
        assert_eq!(p, &expected[c][1234..1234 + p.len()]);
    }
}

#[test]
fn short_reads_interrupted_io_and_recovery_after_source_changes_are_checked() {
    use std::{cell::RefCell, rc::Rc};
    struct Source {
        data: Rc<RefCell<Cursor<Vec<u8>>>>,
        calls: usize,
    }
    impl Read for Source {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls.is_multiple_of(5) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = bytes.len().min(1);
            self.data.borrow_mut().read(&mut bytes[..count])
        }
    }
    impl Seek for Source {
        fn seek(&mut self, where_: SeekFrom) -> io::Result<u64> {
            self.data.borrow_mut().seek(where_)
        }
    }
    let bytes = frame(3, 0, 2, 206, false, 0).repeat(5);
    let expected = raw_pcm(&bytes);
    let shared = Rc::new(RefCell::new(Cursor::new(bytes)));
    let mut reader = IndexedDecoder::new(Source {
        data: shared.clone(),
        calls: 0,
    })
    .unwrap();
    reader.seek_sample(1197).unwrap();
    let out = reader.next_frame().unwrap().unwrap();
    for (c, p) in out.pcm.planes().enumerate() {
        assert_eq!(p, &expected[c][1197..1197 + p.len()]);
    }
    reader.seek_sample(0).unwrap();
    shared.borrow_mut().get_mut()[0] ^= 1;
    assert!(matches!(reader.next_frame(), Err(Error::Invalid(_))));
    assert!(reader.next_frame().is_err());
    shared.borrow_mut().get_mut()[0] ^= 1;
    reader.seek_sample(0).unwrap();
    let out = reader.next_frame().unwrap().unwrap();
    assert_eq!(out.pcm.plane(0).unwrap(), &expected[0][..out.pcm.samples()]);
}
