use crate::{MAX_INPUT, OwnedFrame, options};
use deadlib_audio_decode_flac::*;
use deadlib_audio_decode_ogg as container;
use std::io::Cursor;

fn mapping() -> OggOptions {
    OggOptions {
        decode: options(),
        max_active_streams: 8,
        max_metadata_bytes: MAX_INPUT * 2,
        container_limits: container::Limits {
            max_packet_bytes: MAX_INPUT,
            max_pending_bytes: MAX_INPUT * 2,
            max_streams: 16,
        },
    }
}
#[derive(Debug, PartialEq)]
enum Event {
    Start(StreamId, Vec<u8>),
    Audio(StreamId, u64, OwnedFrame),
    End(StreamId, u64, u64, Integrity),
    Ignored(StreamId),
}
fn event(value: OggEvent<'_>) -> Event {
    match value {
        OggEvent::StreamStart { stream, metadata } => {
            Event::Start(stream, metadata.encode().unwrap())
        }
        OggEvent::Pcm(f) => Event::Audio(
            f.stream,
            f.start_granule,
            OwnedFrame::new(f.start_sample, f.header, f.pcm),
        ),
        OggEvent::StreamEnd {
            stream,
            samples,
            start_granule,
            integrity,
        } => Event::End(stream, samples, start_granule, integrity),
        OggEvent::IgnoredStream(id) => Event::Ignored(id),
    }
}
// Bound declared PCM work even when thousands of tiny constant frames occur on
// a page. This is a harness budget, separate from the production format limits.
fn bounded(data: &[u8]) -> bool {
    let mut pages = container::PageReader::new(data);
    let mut demux = container::Demuxer::with_validation(
        mapping().container_limits,
        container::ValidationOptions {
            allow_zero_span_granules: true,
            ..Default::default()
        },
    );
    let info = EncodeOptions::default().stream_info().unwrap();
    let mut work = 0;
    let mut packets = 0;
    for _ in 0..256 {
        let Ok(Some(page)) = pages.next_page() else {
            return true;
        };
        let result = demux.push_page(page.page, page.offset, |packet| {
            packets += 1;
            if let Ok(header) = FrameHeader::parse(packet.data, Some(&info)) {
                work += header.block_size * header.channels as usize;
            }
        });
        if work > 1 << 20 || packets > 512 {
            return false;
        }
        if result.is_err() {
            return true;
        }
    }
    false
}
pub fn ogg(data: &[u8]) {
    if data.len() > MAX_INPUT || !bounded(data) {
        return;
    }
    let mut reader = OggReader::new(data, mapping());
    let mut expected = Vec::new();
    let success = loop {
        match reader.read_next(|e| expected.push(event(e))) {
            Ok(true) => (),
            Ok(false) => break true,
            Err(_) => break false,
        }
    };
    let mut decoder = OggStreamDecoder::new(mapping());
    for pass in 0..2 {
        let mut actual = Vec::new();
        let mut failed = false;
        let chunk = 1 + (data.last().copied().unwrap_or(0) as usize * (pass + 1));
        'input: for chunk in data.chunks(chunk) {
            let mut pos = 0;
            while pos < chunk.len() {
                match decoder.push(&chunk[pos..], |e| actual.push(event(e))) {
                    Ok(n) => {
                        assert!(n > 0);
                        pos += n;
                    }
                    Err(_) => {
                        failed = true;
                        break 'input;
                    }
                }
            }
        }
        let ok = !failed && decoder.finish().is_ok();
        assert_eq!(ok, success);
        assert_eq!(actual, expected);
        decoder.reset();
    }
    if !success {
        return;
    }
    let Ok(mut seek) = OggSeekReader::new(
        Cursor::new(data),
        OggSeekOptions {
            mapping: mapping(),
            max_pages: 256,
            max_frames: 512,
        },
    ) else {
        // A physically valid file may contain only ignored non-FLAC streams.
        assert!(!expected.iter().any(|e| matches!(e, Event::Start(..))));
        return;
    };
    let streams: Vec<_> = seek.streams().iter().map(|s| (s.id, s.samples)).collect();
    for (id, total) in streams {
        for sample in [total, 0, total / 2, total.saturating_sub(1)] {
            seek.seek(id, sample).unwrap();
            if sample == total {
                assert!(seek.read_next().unwrap().is_none());
                continue;
            }
            let expected = expected
                .iter()
                .find_map(|event| match event {
                    Event::Audio(stream, granule, frame)
                        if *stream == id
                            && sample >= frame.start
                            && sample < frame.start + frame.header.block_size as u64 =>
                    {
                        Some((*granule, frame))
                    }
                    _ => None,
                })
                .unwrap();
            let actual = seek.read_next().unwrap().unwrap();
            assert_eq!(actual.start_sample, sample);
            assert_eq!(actual.start_granule, expected.0 + sample - expected.1.start);
            expected.1.compare(sample, actual.header, actual.pcm);
        }
    }
}
pub fn repair_ogg(data: &mut [u8]) {
    let mut pos = 0;
    while data.len() - pos >= 27 && &data[pos..pos + 4] == b"OggS" {
        let segments = data[pos + 26] as usize;
        if data.len() - pos < 27 + segments {
            return;
        }
        let size: usize = data[pos + 27..pos + 27 + segments]
            .iter()
            .map(|&v| v as usize)
            .sum();
        let end = pos + 27 + segments + size;
        if end > data.len() {
            return;
        }
        // Repair complete in-page raw frames, leaving spans for normal mutation.
        let mut start = pos + 27 + segments;
        let mut offset = start;
        let mut continued = data[pos + 5] & 1 != 0;
        for i in 0..segments {
            let len = data[pos + 27 + i] as usize;
            offset += len;
            if len != 255 {
                if !continued {
                    crate::repair_frame(&mut data[start..offset]);
                }
                start = offset;
                continued = false;
            }
        }
        data[pos + 22..pos + 26].fill(0);
        let crc = container::crc32(&data[pos..end]).to_le_bytes();
        data[pos + 22..pos + 26].copy_from_slice(&crc);
        pos = end;
    }
}
