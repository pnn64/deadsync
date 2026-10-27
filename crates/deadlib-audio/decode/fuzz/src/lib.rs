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
        max_codebook_extent_bits: 24,
        max_codebook_entries: 131072,
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
    allow_nonfinite: bool,
    hash: u64,
    units: u64,
}
impl Digest {
    fn with_nonfinite(allow_nonfinite: bool) -> Self {
        Self {
            allow_nonfinite,
            ..Self::default()
        }
    }
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
                assert!(self.allow_nonfinite || sample.is_finite());
                let bits = if sample.is_nan() {
                    f32::NAN.to_bits()
                } else {
                    sample.to_bits()
                };
                self.bytes(&bits.to_le_bytes());
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
                self.number(end.emitted_frames);
                self.number(end.emitted_start_granule);
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

fn bitpacking(input: &[u8]) {
    fn wire(input: &[u8], position: usize, width: u32) -> Option<u32> {
        if width > 32 || position + width as usize > input.len() * 8 {
            return None;
        }
        Some((0..width).fold(0, |value, bit| {
            let index = position + bit as usize;
            value | (((input[index / 8] >> (index % 8)) & 1) as u32) << bit
        }))
    }
    let mut bits = vorbis::bitpack::BitReader::new(input);
    let mut position = 0;
    let mut exhausted = false;
    for step in 0..128 {
        let control = input.get(step % input.len().max(1)).copied().unwrap_or(0) as u32;
        let width = control * 13 % 34;
        let expected = if exhausted {
            None
        } else {
            wire(input, position, width)
        };
        assert_eq!(bits.peek(width), expected);
        assert_eq!(bits.peek(33), None);
        assert_eq!(
            bits.read(0),
            if exhausted {
                Err(vorbis::Error::EndOfPacket)
            } else {
                Ok(0)
            }
        );
        let width = 1 + control % 33;
        let expected = if width > 32 {
            Err(vorbis::Error::Invalid("integer width exceeds 32"))
        } else if exhausted {
            Err(vorbis::Error::EndOfPacket)
        } else {
            wire(input, position, width).ok_or(vorbis::Error::EndOfPacket)
        };
        assert_eq!(bits.read(width), expected);
        match expected {
            Ok(_) => position += width as usize,
            Err(vorbis::Error::EndOfPacket) => exhausted = true,
            Err(_) => {}
        }
        assert_eq!(bits.position(), position);
        assert_eq!(bits.remaining(), input.len() * 8 - position);
        assert_eq!(bits.is_exhausted(), exhausted);
    }
}

pub fn vorbis_headers(input: &[u8]) {
    if input.len() > MAX_INPUT + 1 {
        return;
    }
    // Exercise the wire-bit contract even when arbitrary bytes cannot parse
    // as headers. This oracle does not use BitWriter or the decoder cache.
    bitpacking(input);
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
            let setup_limits = vorbis::Limits {
                max_codebook_extent_bits: if kind & 16 != 0 { 40 } else { 24 },
                ..limits()
            };
            if let Ok(id) = vorbis::Identification::parse(&data[..30])
                && let Ok(setup) = vorbis::Setup::parse_with_options(
                    &data[30..],
                    &id,
                    vorbis::ParseOptions {
                        limits: setup_limits,
                        allow_empty_codebooks: kind & 64 != 0,
                        residue_behavior: if kind & 128 != 0 {
                            vorbis::ResidueBehavior::Libvorbis
                        } else {
                            vorbis::ResidueBehavior::Vorbis
                        },
                        codebook_behavior: if kind & 4 != 0 {
                            vorbis::CodebookBehavior::Libvorbis
                        } else {
                            vorbis::CodebookBehavior::Vorbis
                        },
                    },
                )
                && let Ok(mut decoder) = vorbis::PacketDecoder::with_options(
                    setup,
                    vorbis::DecodeOptions {
                        limits: setup_limits,
                        allow_nonfinite: kind & 32 != 0,
                        floor1_behavior: if kind & 8 != 0 {
                            vorbis::Floor1Behavior::Libvorbis
                        } else {
                            vorbis::Floor1Behavior::Vorbis
                        },
                        floor0_behavior: if kind & 8 != 0 {
                            vorbis::Floor0Behavior::Libvorbis
                        } else {
                            vorbis::Floor0Behavior::Vorbis
                        },
                        residue2_layout: if kind & 0x80 != 0 {
                            vorbis::Residue2Layout::Libvorbis
                        } else {
                            vorbis::Residue2Layout::Interleaved
                        },
                    },
                )
            {
                for packet in [&[][..], &[0][..], &[0xff; 16][..]] {
                    if let Ok(Some(pcm)) = decoder.decode(packet) {
                        Digest::with_nonfinite(kind & 32 != 0).pcm(&pcm);
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

struct SynthesisFixture {
    finite: [vorbis::SynthesisDecoder; 2],
    ieee: [vorbis::SynthesisDecoder; 2],
}
thread_local! {
    static SYNTHESIS_DECODERS: RefCell<Vec<SynthesisFixture>> = RefCell::new({
        let mut files: Vec<Vec<u8>>=FIXTURES.iter().map(|b|b.to_vec()).collect();
        if let Some(path)=std::env::var_os("DEADLIB_VORBIS_FLOOR0") {files.push(std::fs::read(path).expect("floor-zero fixture"));}
        files.iter().map(|bytes| {
            let p=packets(bytes);let id=vorbis::Identification::parse(&p[0]).unwrap();
            let create=|allow_nonfinite|{
                let setup=vorbis::Setup::parse_with_options(&p[2],&id,vorbis::ParseOptions{limits:limits(),allow_empty_codebooks:true,codebook_behavior:vorbis::CodebookBehavior::Libvorbis,residue_behavior:vorbis::ResidueBehavior::Libvorbis}).unwrap();
                vorbis::SynthesisDecoder::with_options(setup,vorbis::DecodeOptions{limits:limits(),floor0_behavior:vorbis::Floor0Behavior::Libvorbis,floor1_behavior:vorbis::Floor1Behavior::Libvorbis,residue2_layout:vorbis::Residue2Layout::Libvorbis,allow_nonfinite}).unwrap()
            };
            SynthesisFixture{finite:std::array::from_fn(|_|create(false)),ieee:std::array::from_fn(|_|create(true))}
        }).collect()
    });
}
#[derive(Clone, Copy)]
enum SynthesisOp<'a> {
    Reset,
    Packet {
        track: bool,
        data: &'a [u8],
        timing: vorbis::PacketTiming,
    },
    Read(usize),
    Drain,
}
fn synthesis_digest(d: &vorbis::SynthesisDecoder, ieee: bool) -> Digest {
    let mut digest = Digest::with_nonfinite(ieee);
    let state = d.state();
    for value in [
        state.packet_number.unwrap_or(u64::MAX),
        state.granule_position.unwrap_or(-1) as u64,
        state.sample_count.unwrap_or(-1) as u64,
        state.block_size as u64,
        state.queued_frames as u64,
        state.end_of_stream as u64,
        d.position(),
    ] {
        digest.number(value);
    }
    digest.pcm(&d.pcm());
    digest
}
fn synthesis_apply(
    d: &mut vorbis::SynthesisDecoder,
    op: SynthesisOp<'_>,
) -> Result<u8, vorbis::Error> {
    match op {
        SynthesisOp::Reset => {
            let size = d.state().block_size;
            d.reset();
            assert_eq!(d.state().block_size, size);
            assert_eq!(d.state().queued_frames, 0);
            assert_eq!(d.position(), 0);
            Ok(2)
        }
        SynthesisOp::Packet {
            track,
            data,
            timing,
        } => if track {
            d.track(data, timing)
        } else {
            d.submit(data, timing)
        }
        .map(u8::from),
        SynthesisOp::Read(n) => d.consume(n).map(|_| 3),
        SynthesisOp::Drain => {
            let n = d.pcm().frames;
            d.consume(n).map(|_| 4)
        }
    }
}
/// Independent decoder pairs preserve identical restart histories. A restart
/// intentionally retains PCM storage, so fresh/reset replay is not assumed to
/// expose the same stale samples under arbitrary track/read scheduling.
pub fn vorbis_synthesis(input: &[u8]) {
    let Some((&control, mut wire)) = input.split_first() else {
        return;
    };
    if wire.len() > MAX_INPUT {
        return;
    }
    SYNTHESIS_DECODERS.with_borrow_mut(|fixtures| {
        let count = fixtures.len();
        let fixture = &mut fixtures[(control & 63) as usize % count];
        let ieee = control & 64 != 0;
        let pair = if ieee {
            &mut fixture.ieee
        } else {
            &mut fixture.finite
        };
        pair[0].reset();
        pair[1].reset();
        for _ in 0..32 {
            let Some((&opcode, rest)) = wire.split_first() else {
                break;
            };
            wire = rest;
            let op = match opcode % 6 {
                0 => SynthesisOp::Reset,
                1 | 2 => {
                    if wire.len() < 19 {
                        break;
                    }
                    let n = u16::from_le_bytes(wire[..2].try_into().unwrap()) as usize;
                    let sequence = u64::from_le_bytes(wire[2..10].try_into().unwrap());
                    let gp = i64::from_le_bytes(wire[10..18].try_into().unwrap());
                    let eos = wire[18] & 1 != 0;
                    wire = &wire[19..];
                    let n = n.min(wire.len());
                    let data = &wire[..n];
                    wire = &wire[n..];
                    SynthesisOp::Packet {
                        track: opcode % 6 == 2,
                        data,
                        timing: vorbis::PacketTiming {
                            packet_number: sequence,
                            granule_position: Some(gp),
                            end_of_stream: eos,
                        },
                    }
                }
                3 => {
                    if wire.len() < 4 {
                        break;
                    }
                    let n = u32::from_le_bytes(wire[..4].try_into().unwrap()) & 0x7fffffff;
                    wire = &wire[4..];
                    SynthesisOp::Read(n as usize)
                }
                4 => SynthesisOp::Drain,
                _ => {
                    let Some((&choice, rest)) = wire.split_first() else {
                        break;
                    };
                    wire = rest;
                    let frames = pair[0].pcm().frames;
                    let n = [0, 1, frames / 2, frames, frames + 1, i32::MAX as usize]
                        [choice as usize % 6];
                    SynthesisOp::Read(n)
                }
            };
            let mut first = None;
            for decoder in pair.iter_mut() {
                let before = decoder.state();
                let previous = synthesis_digest(decoder, ieee);
                let result = synthesis_apply(decoder, op);
                if result.is_err() || result == Ok(0) {
                    assert_eq!(decoder.state(), before);
                    assert_eq!(synthesis_digest(decoder, ieee), previous);
                }
                let outcome = (
                    result,
                    decoder.state(),
                    decoder.position(),
                    synthesis_digest(decoder, ieee),
                );
                if let Some(first) = &first {
                    assert_eq!(first, &outcome);
                } else {
                    first = Some(outcome);
                }
            }
        }
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
        limits: vorbis::Limits {
            max_codebook_extent_bits: if control & 64 != 0 { 40 } else { 24 },
            ..limits()
        },
        container_limits: container_limits(),
        max_active_streams: 4,
        max_stream_memory: 16 << 20,
        allow_zero_span_granules: control & 1 != 0,
        eos_behavior: if control & 128 != 0 {
            vorbis::EosBehavior::Libvorbis
        } else {
            vorbis::EosBehavior::Vorbis
        },
        residue2_layout: if control & 8 != 0 {
            vorbis::Residue2Layout::Libvorbis
        } else {
            vorbis::Residue2Layout::Interleaved
        },
        allow_empty_codebooks: control & 16 != 0,
        residue_behavior: if control & 128 != 0 {
            vorbis::ResidueBehavior::Libvorbis
        } else {
            vorbis::ResidueBehavior::Vorbis
        },
        codebook_behavior: if control & 128 != 0 {
            vorbis::CodebookBehavior::Libvorbis
        } else {
            vorbis::CodebookBehavior::Vorbis
        },
        allow_nonfinite: control & 32 != 0,
        floor1_behavior: if control & 128 != 0 {
            vorbis::Floor1Behavior::Libvorbis
        } else {
            vorbis::Floor1Behavior::Vorbis
        },
        floor0_behavior: if control & 128 != 0 {
            vorbis::Floor0Behavior::Libvorbis
        } else {
            vorbis::Floor0Behavior::Vorbis
        },
    };
    let mut reader = vorbis::OggReader::with_options(data, options);
    let mut expected = Digest::with_nonfinite(options.allow_nonfinite);
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
        let mut actual = Digest::with_nonfinite(options.allow_nonfinite);
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
            limits: options.limits,
            container_limits: container_limits(),
            allow_zero_span_granules: control & 1 != 0,
            trim_to_final_granule: control & 2 != 0
                && options.eos_behavior == vorbis::EosBehavior::Vorbis,
            eos_behavior: options.eos_behavior,
            residue2_layout: options.residue2_layout,
            allow_empty_codebooks: options.allow_empty_codebooks,
            codebook_behavior: options.codebook_behavior,
            residue_behavior: options.residue_behavior,
            allow_nonfinite: options.allow_nonfinite,
            floor0_behavior: options.floor0_behavior,
            floor1_behavior: options.floor1_behavior,
            packet_preroll: control & 4 == 0,
            max_index_pages: MAX_PAGES,
            max_index_memory: 8 << 20,
            max_stream_memory: 16 << 20,
        };
        if let Ok(mut seek) = vorbis::OggSeekReader::with_options(Cursor::new(data), seek_options) {
            let id = seek.selected().info.id;
            let end = seek.selected().end.frames;
            let emitted = seek.selected().end.emitted_frames;
            for target in [end, end / 2, 0, end.saturating_sub(1)] {
                let mut previous = None;
                for mode in 0..3 {
                    seek.seek(id, target).unwrap();
                    let initial_position = seek.position();
                    let mut position = initial_position;
                    let mut digest = Digest::with_nonfinite(options.allow_nonfinite);
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
                        assert_eq!(position, emitted.max(initial_position));
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
            assert_eq!(info.emitted_frames, frames as u64);
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
    fn bitpacking_oracle_covers_mixed_widths_and_packet_tails() {
        let input: Vec<_> = (0..128).map(|v| (v * 79) as u8).collect();
        for length in 0..=input.len() {
            bitpacking(&input[..length]);
        }
    }
    #[test]
    fn synthesis_schedules_cover_finite_ieee_pending_and_restart_states() {
        for (fixture, bytes) in FIXTURES.iter().enumerate() {
            let p = packets(bytes);
            for profile in [0, 64] {
                let mut wire = vec![fixture as u8 | profile, 0, 3];
                wire.extend(i32::MAX.to_le_bytes());
                for (i, p) in p[3..].iter().enumerate() {
                    for kind in [1, 2] {
                        wire.push(kind);
                        wire.extend((p.len() as u16).to_le_bytes());
                        wire.extend((i as u64).to_le_bytes());
                        wire.extend((if i % 2 == 0 { i64::MIN } else { i64::MAX }).to_le_bytes());
                        wire.push((i % 3 == 0) as u8);
                        wire.extend(p);
                        wire.extend([5, 1, 4]);
                    }
                    if i % 2 == 0 {
                        wire.push(0);
                    }
                }
                for n in [0, 1, 20, wire.len() / 2, wire.len()] {
                    vorbis_synthesis(&wire[..n]);
                }
            }
        }
    }
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
