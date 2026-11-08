use deadlib_audio_decode_flac::*;
use deadlib_audio_decode_ogg::{self as container, PacketWriter, Page, PageReader, WriteOptions};
use std::collections::BTreeMap;
use std::io::Cursor;

fn options(bits: u8, channels: u8) -> EncodeOptions {
    EncodeOptions {
        bits_per_sample: bits,
        channels,
        block_size: 32,
        compression_level: 0,
        ..Default::default()
    }
}
fn pcm(bits: u8, channels: u8, n: usize) -> Vec<i32> {
    let mut seed = 193u32;
    (0..n * channels as usize)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as i32) >> (32 - bits)
        })
        .collect()
}
fn encoded(
    input: &[i32],
    options: EncodeOptions,
    serial: u32,
    origin: u64,
    page_body: usize,
) -> Vec<u8> {
    let mut encoder = OggEncoder::new(
        Vec::new(),
        options,
        OggWriteOptions {
            serial,
            granule_offset: origin,
            container: WriteOptions {
                target_page_body: page_body,
            },
        },
    )
    .unwrap();
    for chunk in input.chunks(options.channels as usize * 17) {
        encoder.write_interleaved(chunk).unwrap();
    }
    encoder.finish().unwrap().output
}
fn collect(
    event: OggEvent<'_>,
    audio: &mut BTreeMap<u32, Vec<i32>>,
    origin: u64,
    ends: &mut Vec<(StreamId, u64, Integrity)>,
) {
    match event {
        OggEvent::Pcm(frame) => {
            let output = audio.entry(frame.stream.serial).or_default();
            assert_eq!(
                frame.start_sample,
                output.len() as u64 / u64::from(frame.header.channels)
            );
            assert_eq!(frame.start_granule, origin + frame.start_sample);
            for i in 0..frame.pcm.samples() {
                for plane in frame.pcm.planes() {
                    output.push(plane[i]);
                }
            }
        }
        OggEvent::StreamEnd {
            stream,
            samples,
            integrity,
            ..
        } => ends.push((stream, samples, integrity)),
        _ => (),
    }
}
fn pages(data: &[u8]) -> Vec<Vec<u8>> {
    let mut reader = PageReader::new(data);
    let mut pages = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        pages.push(page.page.bytes().to_vec());
    }
    pages
}
fn rewrite(data: &[u8], change: impl FnOnce(&mut container::PageHeader, &mut Vec<u8>)) -> Vec<u8> {
    let page = Page::parse(data).unwrap().0;
    let mut header = page.header();
    let mut body = page.body().to_vec();
    change(&mut header, &mut body);
    container::encode_page(header, page.lacing(), &body).unwrap()
}
fn decode(data: &[u8]) -> Result<(), Error> {
    let mut reader = OggReader::new(data, OggOptions::default());
    while reader.read_next(|_| {})? {}
    Ok(())
}

#[test]
fn byte_boundaries_multiframe_first_page_and_broadcast_origin() {
    let opts = options(16, 2);
    let input = pcm(16, 2, 193);
    let bytes = encoded(&input, opts, 51, 9000, 60000);
    for split in 0..=bytes.len() {
        let mut decoder = OggStreamDecoder::new(OggOptions::default());
        let mut audio = BTreeMap::new();
        let mut ends = Vec::new();
        for part in [&bytes[..split], &bytes[split..]] {
            let mut pos = 0;
            while pos < part.len() {
                let used = decoder
                    .push(&part[pos..], |e| collect(e, &mut audio, 9000, &mut ends))
                    .unwrap();
                assert!(used > 0);
                pos += used;
            }
        }
        decoder.finish().unwrap();
        assert_eq!(audio[&51], input);
        assert_eq!(
            ends,
            [(
                StreamId {
                    serial: 51,
                    chain: 0
                },
                193,
                Integrity::Unavailable
            )]
        );
    }
}

#[test]
fn every_depth_and_channel_count_roundtrip() {
    for bits in 4..=32 {
        for channels in 1..=8 {
            let input = pcm(bits, channels, 67);
            let bytes = encoded(&input, options(bits, channels), 9, 0, 500);
            let mut reader = OggReader::new(bytes.as_slice(), OggOptions::default());
            let mut audio = BTreeMap::new();
            let mut ends = Vec::new();
            while reader
                .read_next(|e| collect(e, &mut audio, 0, &mut ends))
                .unwrap()
            {}
            assert_eq!(audio[&9], input);
            assert_eq!(ends[0].1, 67);
        }
    }
}

#[test]
fn native_remux_preserves_metadata_md5_and_unknown_blocks() {
    let opts = options(24, 2);
    let input = pcm(24, 2, 145);
    let extra = [MetadataBlock {
        kind: 126,
        data: b"keep me",
    }];
    let mut encoder = Encoder::with_metadata(Cursor::new(Vec::new()), opts, &extra, 4).unwrap();
    encoder.write_interleaved(&input).unwrap();
    let native = encoder.finish_seekable().unwrap().output.into_inner();
    let metadata = Metadata::parse(&native, Limits::default()).unwrap();
    let mut muxer = OggMuxer::new(
        Vec::new(),
        metadata.stream_info,
        &metadata.blocks[1..],
        OggWriteOptions::default(),
    )
    .unwrap();
    let mut decoder = FrameDecoder::new(Some(metadata.stream_info)).unwrap();
    let mut pos = metadata.audio_offset;
    while pos < native.len() {
        let frame = decoder.decode(&native[pos..]).unwrap();
        muxer.write_frame(&native[pos..pos + frame.bytes]).unwrap();
        pos += frame.bytes;
    }
    let bytes = muxer.finish().unwrap().output;
    let mut reader = OggReader::new(bytes.as_slice(), OggOptions::default());
    let mut audio = BTreeMap::new();
    let mut ends = Vec::new();
    let mut kept = false;
    while reader
        .read_next(|e| {
            if let OggEvent::StreamStart { ref metadata, .. } = e {
                kept = metadata.blocks.iter().any(|b| *b == extra[0]);
            }
            collect(e, &mut audio, 0, &mut ends)
        })
        .unwrap()
    {}
    assert!(kept);
    assert_eq!(audio[&0], input);
    assert_eq!(ends[0].2, Integrity::Verified);
    let mut seek = OggSeekReader::new(Cursor::new(&bytes), OggSeekOptions::default()).unwrap();
    while seek.read_next().unwrap().is_some() {}
    assert_eq!(seek.integrity(), Integrity::Verified);
    for sample in [144, 0, 31, 32, 33, 145, 1, 90] {
        seek.seek(
            StreamId {
                serial: 0,
                chain: 0,
            },
            sample,
        )
        .unwrap();
        if sample == 145 {
            assert!(seek.read_next().unwrap().is_none());
            continue;
        }
        let frame = seek.read_next().unwrap().unwrap();
        assert_eq!(frame.start_sample, sample);
        for (ch, plane) in frame.pcm.planes().enumerate() {
            for (i, &value) in plane.iter().enumerate() {
                assert_eq!(value, input[(sample as usize + i) * 2 + ch]);
            }
        }
    }
}

#[test]
fn spanning_metadata_and_audio_packets() {
    let padding = vec![0; 140000];
    let opts = EncodeOptions {
        block_size: 65535,
        ..options(32, 8)
    };
    let input = pcm(32, 8, 65535);
    let mut encoder = OggEncoder::with_metadata(
        Vec::new(),
        opts,
        &[MetadataBlock {
            kind: 1,
            data: &padding,
        }],
        OggWriteOptions::default(),
    )
    .unwrap();
    encoder.write_interleaved(&input).unwrap();
    let bytes = encoder.finish().unwrap().output;
    let all = pages(&bytes);
    let mut zero_headers = 0;
    let mut unknown_audio = 0;
    for data in &all {
        let page = Page::parse(data).unwrap().0;
        if page.completed_packets() == 0 && !page.lacing().is_empty() {
            if page.header().granule_position == 0 {
                zero_headers += 1;
            } else {
                assert_eq!(page.header().granule_position, u64::MAX);
                unknown_audio += 1;
            }
        }
    }
    assert!(zero_headers >= 1 && unknown_audio >= 1);
    let mut decoder = OggStreamDecoder::new(OggOptions::default());
    let mut audio = BTreeMap::new();
    let mut ends = Vec::new();
    for chunk in bytes.chunks(997) {
        let mut pos = 0;
        while pos < chunk.len() {
            pos += decoder
                .push(&chunk[pos..], |e| collect(e, &mut audio, 0, &mut ends))
                .unwrap();
        }
    }
    decoder.finish().unwrap();
    assert_eq!(audio[&0], input);
    // Also accept generic Ogg's unknown granule on unfinished header pages.
    let alternate: Vec<u8> = all
        .iter()
        .flat_map(|data| {
            rewrite(data, |h, _| {
                let page = Page::parse(data).unwrap().0;
                if page.completed_packets() == 0
                    && !page.lacing().is_empty()
                    && h.granule_position == 0
                {
                    h.granule_position = u64::MAX;
                }
            })
        })
        .collect();
    decode(&alternate).unwrap();
    let mut seek = OggSeekReader::new(Cursor::new(&bytes), OggSeekOptions::default()).unwrap();
    seek.seek(
        StreamId {
            serial: 0,
            chain: 0,
        },
        65534,
    )
    .unwrap();
    let frame = seek.read_next().unwrap().unwrap();
    for ch in 0..8 {
        assert_eq!(frame.pcm.plane(ch), &input[65534 * 8 + ch..][..1]);
    }
    assert!(seek.read_next().unwrap().is_none());
}

#[test]
fn multiplex_chains_unknown_codecs_and_empty_streams() {
    let a = pcm(8, 1, 97);
    let b = pcm(24, 2, 111);
    let ap = pages(&encoded(&a, options(8, 1), 1, 0, 100));
    let bp = pages(&encoded(&b, options(24, 2), 2, 0, 100));
    let mut mixed = Vec::new();
    for i in 0..ap.len().max(bp.len()) {
        if let Some(p) = ap.get(i) {
            mixed.extend(p);
        }
        if let Some(p) = bp.get(i) {
            mixed.extend(p);
        }
    }
    let mut reader = OggReader::new(mixed.as_slice(), OggOptions::default());
    let mut audio = BTreeMap::new();
    let mut ends = Vec::new();
    while reader
        .read_next(|e| collect(e, &mut audio, 0, &mut ends))
        .unwrap()
    {}
    assert_eq!(audio[&1], a);
    assert_eq!(audio[&2], b);
    let mut seek = OggSeekReader::new(Cursor::new(&mixed), OggSeekOptions::default()).unwrap();
    for pass in 0..2 {
        for (serial, channels, expected) in [(1, 1, &a), (2, 2, &b)] {
            let count = expected.len() / channels;
            for sample in (0..=count).rev() {
                let id = StreamId { serial, chain: 0 };
                seek.seek(id, sample as u64).unwrap();
                let mut output = Vec::new();
                while let Some(frame) = seek.read_next().unwrap() {
                    for i in 0..frame.pcm.samples() {
                        for ch in 0..channels {
                            output.push(frame.pcm.plane(ch)[i]);
                        }
                    }
                }
                assert_eq!(
                    output,
                    expected[sample * channels..],
                    "pass {pass} serial {serial} sample {sample}"
                );
            }
        }
    }
    assert!(ends.iter().all(|e| e.0.chain == 0));
    let chained: Vec<u8> = ap.iter().chain(&bp).flatten().copied().collect();
    let mut reader = OggReader::new(chained.as_slice(), OggOptions::default());
    let mut ends = Vec::new();
    while reader
        .read_next(|e| {
            if let OggEvent::StreamEnd { stream, .. } = e {
                ends.push(stream);
            }
        })
        .unwrap()
    {}
    assert_eq!(
        ends,
        [
            StreamId {
                serial: 1,
                chain: 0
            },
            StreamId {
                serial: 2,
                chain: 1
            }
        ]
    );
    let mut unknown = PacketWriter::new(Vec::new(), 88);
    unknown.write_packet(b"unknown codec", 0, true).unwrap();
    let mut data = unknown.into_inner();
    data.extend(encoded(&[], options(16, 1), 12, 0, 100));
    let mut ignored = false;
    let mut ended = false;
    let mut reader = OggReader::new(data.as_slice(), OggOptions::default());
    while reader
        .read_next(|e| match e {
            OggEvent::IgnoredStream(_) => ignored = true,
            OggEvent::StreamEnd { samples, .. } => {
                assert_eq!(samples, 0);
                ended = true;
            }
            _ => (),
        })
        .unwrap()
    {}
    assert!(ignored && ended);
    let mut seek = OggSeekReader::new(Cursor::new(&data), OggSeekOptions::default()).unwrap();
    assert_eq!(seek.streams()[0].id.chain, 1);
    assert!(seek.read_next().unwrap().is_none());
}

#[test]
fn mapping_errors_crc_truncation_limits_and_terminal_state() {
    let data = encoded(&pcm(16, 1, 80), options(16, 1), 3, 0, 100);
    let original = pages(&data);
    for n in 0..data.len() {
        assert!(decode(&data[..n]).is_err(), "prefix {n}");
    }
    for (page, offset) in [(0, 5), (0, 7), (0, 9), (1, 0)] {
        let mut bad = original.clone();
        bad[page] = rewrite(&bad[page], |_, body| body[offset] ^= 1);
        assert!(decode(&bad.concat()).is_err());
    }
    let mut bad = original.clone();
    let last = bad.len() - 1;
    bad[last] = rewrite(&bad[last], |h, _| h.granule_position += 1);
    assert!(decode(&bad.concat()).is_err());
    let mut bad = data.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(decode(&bad).is_err());
    let options = OggOptions {
        max_metadata_bytes: 1,
        ..Default::default()
    };
    let mut reader = OggReader::new(data.as_slice(), options);
    assert!(reader.read_next(|_| {}).is_err());
    assert!(reader.read_next(|_| {}).is_err());
    // Unknown header count is valid; the last-block flag still terminates headers.
    let mut unknown = original.clone();
    unknown[0] = rewrite(&unknown[0], |_, body| body[7..9].fill(0));
    decode(&unknown.concat()).unwrap();
    let limits = OggSeekOptions {
        max_frames: 1,
        ..Default::default()
    };
    assert!(OggSeekReader::new(Cursor::new(&data), limits).is_err());
    let limits = OggSeekOptions {
        max_pages: 1,
        ..Default::default()
    };
    assert!(OggSeekReader::new(Cursor::new(&data), limits).is_err());
}

#[test]
fn indexed_seeks_keep_nonzero_granule_origin() {
    let input = pcm(32, 2, 73);
    let data = encoded(&input, options(32, 2), 15, 1_000_000, 65500);
    let mut seek = OggSeekReader::new(Cursor::new(&data), OggSeekOptions::default()).unwrap();
    assert_eq!(seek.streams()[0].start_granule, 1_000_000);
    for sample in 0..73 {
        seek.seek(
            StreamId {
                serial: 15,
                chain: 0,
            },
            sample,
        )
        .unwrap();
        let frame = seek.read_next().unwrap().unwrap();
        assert_eq!(frame.start_sample, sample);
        assert_eq!(frame.start_granule, 1_000_000 + sample);
        assert_eq!(frame.pcm.plane(0)[0], input[sample as usize * 2]);
    }
}

#[test]
fn partial_broadcast_numbering_nil_pages_and_integrity() {
    for variable in [false, true] {
        let opts = EncodeOptions {
            variable_block_size: variable,
            ..options(16, 1)
        };
        let input = pcm(16, 1, 97);
        let mut native = Encoder::new(Cursor::new(Vec::new()), opts).unwrap();
        native.write_interleaved(&input).unwrap();
        let native = native.finish_seekable().unwrap().output.into_inner();
        let meta = Metadata::parse(&native, Limits::default()).unwrap();
        let mut frame = FrameDecoder::new(Some(meta.stream_info)).unwrap();
        let first = frame.decode(&native[meta.audio_offset..]).unwrap().bytes;
        let mut position = meta.audio_offset + first;
        let mut muxer = OggMuxer::new(
            Vec::new(),
            meta.stream_info,
            &[],
            OggWriteOptions {
                granule_offset: 5000,
                ..Default::default()
            },
        )
        .unwrap();
        while position < native.len() {
            let n = frame.decode(&native[position..]).unwrap().bytes;
            muxer.write_frame(&native[position..position + n]).unwrap();
            position += n;
        }
        let data = muxer.finish().unwrap().output;
        let mut all = pages(&data);
        let mut nil = Page::parse(&all[2]).unwrap().0.header();
        nil.flags = 0;
        nil.granule_position = 0;
        let nil = container::encode_page(nil, &[], &[]).unwrap();
        for page in &mut all[2..] {
            *page = rewrite(page, |h, _| h.sequence += 1);
        }
        all.insert(2, nil);
        let data = all.concat();
        let mut reader = OggReader::new(data.as_slice(), OggOptions::default());
        let mut output = Vec::new();
        let mut integrity = Integrity::Pending;
        while reader
            .read_next(|event| match event {
                OggEvent::Pcm(frame) => {
                    assert_eq!(frame.start_granule, 5000 + output.len() as u64);
                    output.extend_from_slice(frame.pcm.plane(0));
                }
                OggEvent::StreamEnd {
                    integrity: value, ..
                } => integrity = value,
                _ => (),
            })
            .unwrap()
        {}
        assert_eq!(output, input[32..]);
        assert_eq!(integrity, Integrity::Partial);
        let mut seek = OggSeekReader::new(Cursor::new(&data), OggSeekOptions::default()).unwrap();
        assert!(seek.streams()[0].partial);
        seek.seek(
            StreamId {
                serial: 0,
                chain: 0,
            },
            2,
        )
        .unwrap();
        let frame = seek.read_next().unwrap().unwrap();
        assert_eq!(frame.start_granule, 5002);
        assert_eq!(frame.pcm.plane(0)[0], input[34]);
        assert_eq!(seek.integrity(), Integrity::Partial);
    }
}
