//! Test-only fuzz oracles, isolated from the application workspace.
use deadlib_audio_decode_ogg as ogg;
use deadlib_audio_decode_vorbis as vorbis;
use std::ops::ControlFlow;
use std::{cell::RefCell, io::Cursor};

pub const MAX_INPUT: usize = 128 << 10;
const MAX_PAGES: usize = 128;

pub fn limits() -> vorbis::Limits {
    vorbis::Limits {
        max_packet_bytes: MAX_INPUT,
        max_comment_bytes: MAX_INPUT,
        max_comments: 1024,
        max_codebook_entries: 65536,
        max_codebook_values: 262144,
        max_huffman_nodes: 131072,
        max_setup_memory: 8 << 20,
        max_decode_memory: 8 << 20,
    }
}
fn container_limits() -> ogg::Limits {
    ogg::Limits {
        max_packet_bytes: MAX_INPUT,
        max_pending_bytes: MAX_INPUT * 2,
        max_streams: 16,
    }
}
fn validation(control: u8) -> ogg::ValidationOptions {
    ogg::ValidationOptions {
        allow_zero_span_granules: control & 1 != 0,
        allow_decreasing_eos_granule: control & 2 != 0,
    }
}

/// Recompute CRCs only for complete, structurally delimited capture candidates.
/// This is a fuzz mutation transform, never production recovery behavior.
pub fn repair_checksums(data: &mut [u8]) {
    let mut offset = 0;
    while offset + 27 <= data.len() {
        if &data[offset..offset + 4] != b"OggS" {
            offset += 1;
            continue;
        }
        let body = offset + 27 + data[offset + 26] as usize;
        if body > data.len() {
            offset += 1;
            continue;
        }
        let end = body
            + data[offset + 27..body]
                .iter()
                .map(|&n| n as usize)
                .sum::<usize>();
        if end > data.len() {
            offset += 1;
            continue;
        }
        data[offset + 22..offset + 26].fill(0);
        let crc = ogg::crc32(&data[offset..end]);
        data[offset + 22..offset + 26].copy_from_slice(&crc.to_le_bytes());
        offset = end;
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
struct Digest {
    hash: u64,
    units: u64,
}
impl Digest {
    fn bytes(&mut self, bytes: &[u8]) {
        // An order-sensitive bounded summary avoids storing untrusted PCM.
        for &byte in bytes {
            self.hash = (self.hash ^ byte as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn number(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn packet(&mut self, packet: ogg::Packet<'_>) {
        self.number(packet.info.serial as u64);
        self.number(packet.info.number);
        self.number(packet.info.granule_position.unwrap_or(u64::MAX));
        self.number(packet.data.len() as u64);
        self.bytes(packet.data);
        self.units += 1;
    }
    fn pcm(&mut self, pcm: &vorbis::PcmBlock<'_>) {
        self.number(pcm.start_frame);
        self.number(pcm.frames as u64);
        self.number(pcm.channels as u64);
        self.number(pcm.truncated as u64);
        for plane in pcm.planes() {
            for &sample in plane {
                assert!(sample.is_finite());
                self.bytes(&sample.to_bits().to_le_bytes());
            }
        }
        self.units += pcm.frames as u64;
    }
    fn event(&mut self, event: vorbis::OggEvent<'_>) {
        use vorbis::OggEvent;
        match event {
            OggEvent::StreamStart(info) => {
                self.number(0);
                self.number(info.id.serial as u64);
                self.number(info.id.chain as u64);
                self.number(info.identification.channels as u64);
                self.number(info.identification.sample_rate as u64);
                self.bytes(info.metadata.vendor());
                for value in info.metadata.comments() {
                    self.number(value.len() as u64);
                    self.bytes(value);
                }
            }
            OggEvent::Pcm(pcm) => {
                self.number(1);
                self.number(pcm.stream.serial as u64);
                self.number(pcm.stream.chain as u64);
                self.number(pcm.start_frame);
                self.number(pcm.start_granule);
                self.pcm(&pcm.pcm);
            }
            OggEvent::StreamEnd(end) => {
                self.number(2);
                self.number(end.stream.serial as u64);
                self.number(end.frames);
                self.number(end.start_granule);
                self.number(end.end_granule);
            }
            OggEvent::IgnoredStream(id) => {
                self.number(3);
                self.number(id.serial as u64);
            }
            OggEvent::CommentError { stream, .. } => {
                self.number(4);
                self.number(stream.serial as u64);
            }
        }
    }
}

pub fn ogg_container(input: &[u8]) {
    let Some((&control, data)) = input.split_first() else {
        return;
    };
    if data.len() > MAX_INPUT {
        return;
    }
    let options = validation(control);
    let mut pull = ogg::PageReader::new(data);
    let mut demux = ogg::Demuxer::with_validation(container_limits(), options);
    let mut expected = Digest::default();
    let pull_result = (|| {
        while let Some(page) = pull.next_page()? {
            demux.push_page(page.page, page.offset, |p| expected.packet(p))?;
        }
        demux.finish()
    })();
    let mut pages = ogg::PageDecoder::new();
    let mut push_demux = ogg::Demuxer::with_validation(container_limits(), options);
    let mut actual = Digest::default();
    let push_result = (|| {
        let mut offset = 0;
        for chunk in data.chunks(1 + (control >> 2) as usize) {
            let mut consumed = 0;
            while consumed < chunk.len() {
                let progress = pages.push(&chunk[consumed..])?;
                assert!(progress.consumed > 0);
                consumed += progress.consumed;
                if let Some(page) = progress.page {
                    push_demux.push_page(page, offset, |p| actual.packet(p))?;
                    offset += page.bytes().len() as u64;
                }
            }
        }
        pages.finish()?;
        push_demux.finish()
    })();
    assert_eq!(pull_result, push_result);
    assert_eq!(expected, actual);
    assert_eq!(demux.compatibility(), push_demux.compatibility());
    // The recovering transport must terminate and only expose CRC-valid pages.
    let mut recover = ogg::PageReader::with_mode(data, ogg::SyncMode::Recover);
    while let Ok(Some(page)) = recover.next_page() {
        assert!(ogg::Page::parse(page.page.bytes()).is_ok());
    }
    if pull_result.is_ok() {
        let mut indexed = Digest::default();
        let result = ogg::OggIndex::read_with_events(
            data,
            ogg::IndexOptions {
                container_limits: container_limits(),
                max_pages: MAX_PAGES,
                validation: options,
            },
            |event| {
                if let ogg::IndexEvent::Packet { packet, .. } = event {
                    indexed.packet(packet);
                }
                Ok::<_, ogg::Error>(())
            },
        );
        if let Ok(index) = result {
            assert_eq!(indexed, expected);
            assert_eq!(index.compatibility(), demux.compatibility());
            for stream in index.streams() {
                let entries = index.pages(stream.serial).unwrap();
                for entry in [entries.first().unwrap(), entries.last().unwrap()] {
                    let mut previous = None;
                    for paused in [false, true] {
                        let mut capture = ogg::PacketCapture::at_page_with_validation(
                            entry,
                            container_limits(),
                            options,
                        );
                        let mut digest = Digest::default();
                        let mut reader = ogg::PageReader::new(&data[entry.offset as usize..]);
                        while let Some(page) = reader.next_page().unwrap() {
                            loop {
                                let complete = capture
                                    .push_page_until(page.page, page.offset + entry.offset, |p| {
                                        assert!(p.info.number.is_some());
                                        digest.number(p.info.number.unwrap());
                                        digest.number(p.info.start_page_offset);
                                        digest.number(p.info.end_page_offset);
                                        digest.number(p.info.granule_position.unwrap_or(u64::MAX));
                                        digest.bytes(p.data);
                                        if paused {
                                            ControlFlow::Break(())
                                        } else {
                                            ControlFlow::Continue(())
                                        }
                                    })
                                    .unwrap();
                                if complete {
                                    break;
                                }
                            }
                            if capture.is_ended() {
                                break;
                            }
                        }
                        capture.finish().unwrap();
                        let outcome = (digest, capture.compatibility());
                        if let Some(previous) = &previous {
                            assert_eq!(previous, &outcome);
                        }
                        previous = Some(outcome);
                    }
                }
            }
        }
    }
}

pub fn vorbis_headers(input: &[u8]) {
    let Some((&kind, data)) = input.split_first() else {
        return;
    };
    if data.len() > MAX_INPUT {
        return;
    }
    match kind % 3 {
        0 => {
            if let Ok(id) = vorbis::Identification::parse(data) {
                assert_eq!(
                    vorbis::Identification::parse(&id.encode().unwrap()).unwrap(),
                    id
                );
            }
        }
        1 => {
            if let Ok(comments) = vorbis::Comments::parse(data, &limits()) {
                let encoded = comments.encode(&limits()).unwrap();
                assert_eq!(
                    vorbis::Comments::parse(&encoded, &limits()).unwrap(),
                    comments
                );
                let owned = comments.to_owned().unwrap();
                assert_eq!(owned.vendor(), comments.vendor);
                assert_eq!(owned.comments().collect::<Vec<_>>(), comments.comments);
            }
        }
        _ => {
            if data.len() < 30 {
                return;
            }
            if let Ok(id) = vorbis::Identification::parse(&data[..30])
                && let Ok(setup) = vorbis::Setup::parse(&data[30..], &id, &limits())
                && let Ok(mut decoder) = vorbis::PacketDecoder::new(setup)
            {
                for packet in [&[][..], &[0][..], &[0xff; 16][..]] {
                    if let Ok(Some(pcm)) = decoder.decode(packet) {
                        Digest::default().pcm(&pcm);
                    }
                }
            }
        }
    }
}

pub const FIXTURES: &[&[u8]] = &[
    include_bytes!("../../vorbis/tests/fixtures/stereo.ogg"),
    include_bytes!("../../vorbis/tests/fixtures/mono.ogg"),
    include_bytes!("../../vorbis/tests/fixtures/surround.ogg"),
    include_bytes!("../../vorbis/tests/fixtures/eight-channel.ogg"),
];
pub fn packets(data: &[u8]) -> Vec<Vec<u8>> {
    let mut reader = ogg::PageReader::new(data);
    let mut demux = ogg::Demuxer::with_validation(
        ogg::Limits::default(),
        ogg::ValidationOptions {
            allow_zero_span_granules: true,
            ..ogg::ValidationOptions::default()
        },
    );
    let mut packets = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        demux
            .push_page(page.page, page.offset, |p| packets.push(p.data.to_vec()))
            .unwrap();
    }
    demux.finish().unwrap();
    packets
}
struct Fixture {
    decoder: vorbis::PacketDecoder,
    packets: Vec<Vec<u8>>,
    golden: Digest,
}
fn replay(decoder: &mut vorbis::PacketDecoder, packets: &[Vec<u8>]) -> Digest {
    decoder.reset();
    let mut digest = Digest::default();
    for packet in &packets[3..] {
        if let Some(pcm) = decoder.decode(packet).unwrap() {
            digest.pcm(&pcm);
        }
    }
    digest
}
thread_local! {
    static DECODERS: RefCell<Vec<Fixture>> = RefCell::new({
        let mut files: Vec<Vec<u8>> = FIXTURES.iter().map(|bytes| bytes.to_vec()).collect();
        if let Some(path) = std::env::var_os("DEADLIB_VORBIS_FLOOR0") {
            files.push(std::fs::read(path).expect("read optional floor-zero fuzz fixture"));
        }
        files.iter().map(|bytes| {
            let packets = packets(bytes);
            let id = vorbis::Identification::parse(&packets[0]).unwrap();
            let setup = vorbis::Setup::parse(&packets[2], &id, &limits()).unwrap();
            let mut decoder = vorbis::PacketDecoder::new(setup).unwrap();
            // Keep a short known-good replay, even for the external long file.
            let packets: Vec<_> = packets.into_iter().take(12).collect();
            let golden = replay(&mut decoder, &packets);
            Fixture { decoder, packets, golden }
        }).collect()
    });
}
pub fn vorbis_packets(input: &[u8]) {
    let Some((&control, data)) = input.split_first() else {
        return;
    };
    if data.len() > MAX_INPUT {
        return;
    }
    DECODERS.with_borrow_mut(|fixtures| {
        let count = fixtures.len();
        let fixture = &mut fixtures[(control & 0x7f) as usize % count];
        let run = |decoder: &mut vorbis::PacketDecoder| {
            decoder.reset_at(if control & 0x80 != 0 {
                u64::MAX - 8192
            } else {
                0
            });
            let mut data = data;
            let mut digest = Digest::default();
            let mut outcomes = Vec::new();
            for _ in 0..32 {
                if data.len() < 2 {
                    break;
                }
                let size = u16::from_le_bytes([data[0], data[1]]) as usize;
                data = &data[2..];
                let size = size.min(data.len());
                let result = decoder.decode(&data[..size]).map(|pcm| {
                    if let Some(pcm) = pcm {
                        digest.pcm(&pcm);
                        true
                    } else {
                        false
                    }
                });
                outcomes.push(result);
                data = &data[size..];
            }
            (digest, outcomes, decoder.position())
        };
        let first = run(&mut fixture.decoder);
        assert_eq!(first, run(&mut fixture.decoder));
        assert_eq!(
            replay(&mut fixture.decoder, &fixture.packets),
            fixture.golden
        );
    });
}

pub fn vorbis_stream(input: &[u8]) {
    let Some((&control, data)) = input.split_first() else {
        return;
    };
    if data.len() > MAX_INPUT {
        return;
    }
    let options = vorbis::OggOptions {
        limits: limits(),
        container_limits: container_limits(),
        max_active_streams: 4,
        max_stream_memory: 16 << 20,
        allow_zero_span_granules: control & 1 != 0,
    };
    let mut reader = vorbis::OggReader::with_options(data, options);
    let mut expected = Digest::default();
    let pull = (|| {
        for _ in 0..MAX_PAGES {
            if !reader.read_next(|e| expected.event(e))? {
                return Ok(true);
            }
        }
        Ok::<_, vorbis::Error>(false)
    })();
    let mut stream = vorbis::OggStreamDecoder::new(options);
    for _ in 0..2 {
        stream.reset();
        let mut actual = Digest::default();
        let push = (|| {
            let mut pages = 0;
            // Count pages independently so push and pull share the work fence.
            let mut framing = ogg::PageDecoder::new();
            for chunk in data.chunks(1 + (control >> 1) as usize) {
                let mut offset = 0;
                while offset < chunk.len() {
                    let used = stream.push(&chunk[offset..], |e| actual.event(e))?;
                    assert!(used > 0);
                    let progress = framing.push(&chunk[offset..offset + used]).unwrap();
                    assert_eq!(progress.consumed, used);
                    if progress.page.is_some() {
                        pages += 1;
                    }
                    offset += used;
                    if pages == MAX_PAGES {
                        return Ok(false);
                    }
                }
            }
            stream.finish()?;
            Ok::<_, vorbis::Error>(true)
        })();
        assert_eq!(pull, push);
        assert_eq!(expected, actual);
        assert_eq!(reader.compatibility(), stream.compatibility());
    }
    // Prefix indexing deliberately does not synthesize payloads. An accepted
    // index may therefore encounter a payload error later; replay must return
    // the same error/PCM prefix and recover after another explicit seek.
    {
        let seek_options = vorbis::SeekOptions {
            limits: limits(),
            container_limits: container_limits(),
            allow_zero_span_granules: control & 1 != 0,
            trim_to_final_granule: control & 2 != 0,
            packet_preroll: control & 4 == 0,
            max_index_pages: MAX_PAGES,
            max_index_memory: 8 << 20,
            max_stream_memory: 16 << 20,
        };
        if let Ok(mut seek) = vorbis::OggSeekReader::with_options(Cursor::new(data), seek_options) {
            let id = seek.selected().info.id;
            let end = seek.selected().end.frames;
            for target in [end, end / 2, 0, end.saturating_sub(1)] {
                let mut previous = None;
                for mode in 0..3 {
                    seek.seek(id, target).unwrap();
                    let mut position = target;
                    let mut digest = Digest::default();
                    let result = (|| {
                        let mut step = 0;
                        loop {
                            let packet = mode == 1 || (mode == 2 && step % 3 != 2);
                            let mut blocks = 0;
                            let emit = |event: vorbis::OggEvent<'_>| {
                                if let vorbis::OggEvent::Pcm(pcm) = event {
                                    blocks += 1;
                                    assert_eq!(pcm.start_frame, position);
                                    position += pcm.pcm.frames as u64;
                                    digest.pcm(&pcm.pcm);
                                }
                            };
                            let more = if packet {
                                seek.read_packet(emit)
                            } else {
                                seek.read_next(emit)
                            }?;
                            if packet {
                                assert!(blocks <= 1);
                            }
                            if !more {
                                break;
                            }
                            step += 1;
                        }
                        Ok::<_, vorbis::Error>(())
                    })();
                    if result.is_ok() {
                        assert_eq!(position, end);
                    }
                    let outcome = (result, position, digest);
                    if let Some(previous) = &previous {
                        assert_eq!(previous, &outcome);
                    }
                    previous = Some(outcome);
                }
            }
        }
    }
}

/// Encode two identical streams through different input layouts/chunks, with a
/// reset between them, then validate framing, exact trimming and finite PCM.
pub fn vorbis_encode(input: &[u8]) {
    if input.len() < 6 || input.len() > MAX_INPUT + 1 {
        return;
    }
    let channels = [1, 2, 3, 4, 5, 6, 7, 8, 16, 255][input[0] as usize % 10];
    let channels_usize = channels as usize;
    let options = vorbis::EncodeOptions {
        channels,
        block_size: 64 << (input[1] & 7),
        residue_passes: 1 + (input[2] & 3),
        sample_rate: [1, 8000, 22050, 44100, 48000, 192000, u32::MAX][input[3] as usize % 7],
        max_memory: 8 << 20,
        max_packet_bytes: 2 << 20,
        max_frames: 2048,
    };
    let Ok(mut encoder) = vorbis::PacketEncoder::new(options) else {
        return; // Some extreme channel/block combinations exceed the work fence.
    };
    let mut pcm: Vec<f32> = if input[4] & 7 == 7 {
        input[6..]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    } else {
        input[6..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b) as f32 / 8192.0)
            .collect()
    };
    pcm.truncate((pcm.len() / channels_usize).min(2048) * channels_usize);
    let frames = pcm.len() / channels_usize;
    if pcm.iter().any(|x| !x.is_finite() || x.abs() > 4.0) {
        assert!(
            encoder
                .push_interleaved(&pcm, |_| panic!("invalid input emitted a packet"))
                .is_err()
        );
        assert_eq!(encoder.position(), 0);
        encoder.reset();
        return;
    }
    let comments = vorbis::Comments {
        vendor: b"native fuzz",
        comments: vec![b"TITLE=a", b"TITLE=b"],
    };
    let header = encoder.identification().encode().unwrap();
    let metadata = comments.encode(&limits()).unwrap();
    let mut expected = None;
    for planar in [false, true] {
        encoder.reset();
        let mut pages = ogg::PacketWriter::with_options(
            Vec::new(),
            123,
            ogg::WriteOptions {
                target_page_body: 1 + input[5] as usize * 257,
            },
        );
        pages.write_packet(&header, 0, false).unwrap();
        pages.write_packet(&metadata, 0, false).unwrap();
        pages
            .write_packet(encoder.setup_packet(), 0, false)
            .unwrap();
        pages.flush_page().unwrap();
        let mut number = 3;
        let mut end = false;
        let mut emit = |packet: vorbis::EncodedPacket<'_>| {
            assert_eq!(packet.number, number);
            assert!(!end);
            assert!(packet.granule_position <= frames as u64);
            number += 1;
            end = packet.last_in_stream;
            pages.write_packet(packet.data, packet.granule_position, packet.last_in_stream)?;
            Ok(())
        };
        if planar {
            for chunk in pcm.chunks((1 + input[5] as usize) * channels_usize) {
                let planes: Vec<Vec<_>> = (0..channels_usize)
                    .map(|ch| chunk.chunks_exact(channels_usize).map(|f| f[ch]).collect())
                    .collect();
                let slices: Vec<_> = planes.iter().map(Vec::as_slice).collect();
                encoder.push_planar(&slices, &mut emit).unwrap();
            }
        } else {
            encoder.push_interleaved(&pcm, &mut emit).unwrap();
        }
        assert_eq!(encoder.position(), frames as u64);
        encoder.finish(&mut emit).unwrap();
        encoder.finish(&mut emit).unwrap();
        assert!(end);
        assert!(encoder.push_interleaved(&[], |_| Ok(())).is_err());
        let bytes = pages.into_inner();
        if let Some(expected) = &expected {
            assert_eq!(expected, &bytes);
        } else {
            expected = Some(bytes);
        }
    }
    let bytes = expected.unwrap();
    let mut position = 0;
    let mut starts = 0;
    let mut ends = 0;
    let decode_options = vorbis::OggOptions {
        limits: vorbis::Limits {
            max_packet_bytes: 2 << 20,
            max_decode_memory: 32 << 20,
            ..limits()
        },
        ..vorbis::OggOptions::default()
    };
    vorbis::decode_reader(bytes.as_slice(), decode_options, |event| match event {
        vorbis::OggEvent::StreamStart(info) => {
            starts += 1;
            assert_eq!(info.identification, *encoder.identification());
        }
        vorbis::OggEvent::Pcm(block) => {
            assert_eq!(block.start_frame, position);
            position += block.pcm.frames as u64;
            Digest::default().pcm(&block.pcm);
        }
        vorbis::OggEvent::StreamEnd(info) => {
            ends += 1;
            assert_eq!(info.frames, frames as u64);
        }
        _ => panic!("unexpected event from native encoding"),
    })
    .unwrap();
    assert_eq!(position, frames as u64);
    assert_eq!((starts, ends), (1, 1));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_and_truncated_seeds_reach_every_oracle() {
        for control in 0..80 {
            let mut bytes = vec![control, control, control, control, control, control];
            bytes.extend((0..2048).map(|v| (v * 79) as u8));
            vorbis_encode(&bytes);
        }
        for (fixture, bytes) in FIXTURES.iter().enumerate() {
            for length in [0, 1, 27, bytes.len() / 2, bytes.len()] {
                let mut input = vec![17];
                input.extend_from_slice(&bytes[..length]);
                ogg_container(&input);
                vorbis_stream(&input);
            }
            let packets = packets(bytes);
            for kind in 0..3 {
                let mut input = vec![kind];
                if kind == 2 {
                    input.extend_from_slice(&packets[0]);
                }
                input.extend_from_slice(&packets[kind as usize]);
                vorbis_headers(&input);
            }
            let mut input = vec![fixture as u8];
            for packet in &packets[3..] {
                input.extend_from_slice(&(packet.len() as u16).to_le_bytes());
                input.extend_from_slice(packet);
            }
            vorbis_packets(&input);
            input[0] |= 0x80;
            vorbis_packets(&input);
        }
    }
}
