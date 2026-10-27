mod support;
use deadlib_audio_decode_ogg::{PacketWriter, PageReader, encode_page};
use deadlib_audio_decode_vorbis::*;
use std::{collections::BTreeMap, io::Cursor};

const STEREO: &[u8] = include_bytes!("fixtures/stereo.ogg");
const MONO: &[u8] = include_bytes!("fixtures/mono.ogg");
const SURROUND: &[u8] = include_bytes!("fixtures/surround.ogg");

#[derive(Default)]
struct Audio {
    info: Option<StreamInfo>,
    planes: Vec<Vec<f32>>,
    end: Option<StreamEnd>,
    first_raw: Option<u64>,
}
fn full(bytes: &[u8]) -> BTreeMap<(u32, u32), Audio> {
    let mut out: BTreeMap<(u32, u32), Audio> = BTreeMap::new();
    decode_reader(bytes, OggOptions::default(), |event| match event {
        OggEvent::StreamStart(info) => {
            let audio = out.entry((info.id.chain, info.id.serial)).or_default();
            audio
                .planes
                .resize(info.identification.channels as usize, Vec::new());
            audio.info = Some(info.clone());
        }
        OggEvent::Pcm(pcm) => {
            let audio = out.get_mut(&(pcm.stream.chain, pcm.stream.serial)).unwrap();
            audio.first_raw.get_or_insert(pcm.pcm.start_frame);
            assert_eq!(pcm.start_frame as usize, audio.planes[0].len());
            for (out, plane) in audio.planes.iter_mut().zip(pcm.pcm.planes()) {
                out.extend_from_slice(plane);
            }
        }
        OggEvent::StreamEnd(end) => {
            out.get_mut(&(end.stream.chain, end.stream.serial))
                .unwrap()
                .end = Some(end);
        }
        _ => {}
    })
    .unwrap();
    out
}
fn suffix<R: std::io::Read + std::io::Seek>(
    reader: &mut OggSeekReader<R>,
    target: u64,
    expected: &Audio,
) {
    suffix_with_mode(reader, target, expected, 0);
}
fn suffix_with_mode<R: std::io::Read + std::io::Seek>(
    reader: &mut OggSeekReader<R>,
    target: u64,
    expected: &Audio,
    mode: usize,
) {
    let id = expected.info.as_ref().unwrap().id;
    reader.seek(id, target).unwrap();
    assert_eq!(reader.position(), target);
    let mut position = target as usize;
    let mut starts = 0;
    let mut ends = 0;
    let mut step = 0;
    loop {
        let packet = mode == 1 || (mode == 2 && step % 3 != 2);
        let mut blocks = 0;
        let emit = |event: OggEvent<'_>| match event {
            OggEvent::StreamStart(info) => {
                assert_eq!(info, expected.info.as_ref().unwrap());
                starts += 1;
            }
            OggEvent::Pcm(pcm) => {
                blocks += 1;
                assert_eq!(pcm.start_frame as usize, position);
                assert_eq!(
                    pcm.start_granule,
                    expected.end.unwrap().start_granule + position as u64
                );
                assert_eq!(pcm.stream, id);
                assert_eq!(
                    pcm.pcm.start_frame,
                    expected.first_raw.unwrap() + position as u64
                );
                for (reference, plane) in expected.planes.iter().zip(pcm.pcm.planes()) {
                    // Native synthesis is exactly reproducible after pre-roll.
                    assert_eq!(&reference[position..position + pcm.pcm.frames], plane);
                }
                position += pcm.pcm.frames;
            }
            OggEvent::StreamEnd(end) => {
                assert_eq!(Some(end), expected.end);
                ends += 1;
            }
            _ => {}
        };
        let more = if packet {
            reader.read_packet(emit)
        } else {
            reader.read_next(emit)
        }
        .unwrap();
        if packet {
            assert!(blocks <= 1);
        }
        if !more {
            break;
        }
        step += 1;
    }
    assert_eq!(position as u64, expected.end.unwrap().frames);
    assert_eq!(reader.position() as usize, position);
    assert_eq!((starts, ends), (1, 1));
    assert!(!reader.read_next(|_| panic!("events after EOF")).unwrap());
    assert!(
        !reader
            .read_packet(|_| panic!("packet events after EOF"))
            .unwrap()
    );
}

#[test]
fn packet_reads_and_mixed_page_reads_preserve_exact_suffixes_and_reset_pauses() {
    for bytes in [
        STEREO.to_vec(),
        MONO.to_vec(),
        SURROUND.to_vec(),
        repack(STEREO, 96, -37, 29, 70, false, true),
        repack(STEREO, 97, 900, 33, 70, true, false),
    ] {
        let expected = full(&bytes);
        let audio = expected.values().next().unwrap();
        let mut reader = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
        let id = reader.selected().info.id;
        let frames = audio.end.unwrap().frames;
        for mode in [1, 2] {
            for target in [0, 1, 127, frames / 2, frames - 1, frames] {
                suffix_with_mode(&mut reader, target, audio, mode);
                // Interrupt a buffered page, then seek elsewhere without
                // draining it. The next suffix must have no stale cursor/PCM.
                reader.seek(id, 0).unwrap();
                assert!(reader.read_packet(|_| {}).unwrap());
            }
        }
    }
}

fn repack(
    bytes: &[u8],
    serial: u32,
    origin: i64,
    tail: u64,
    repeat: usize,
    nil_eos: bool,
    discarded: bool,
) -> Vec<u8> {
    let packets = support::packets(bytes);
    let mut decoder = support::decoder(&packets);
    let mut audio = packets[3..].to_vec();
    if repeat != 0 {
        let packet = packets[3..]
            .iter()
            .find(|packet| {
                let header = decoder
                    .setup()
                    .read_audio_header(&mut bitpack::BitReader::new(packet))
                    .unwrap();
                header.previous_long
                    && header.next_long
                    && header.block_size == decoder.setup().block_sizes()[1]
            })
            .unwrap()
            .clone();
        audio.extend(std::iter::repeat_n(packet, repeat));
    }
    let mut writer = PacketWriter::new(Vec::new(), serial);
    for packet in &packets[..3] {
        writer.write_packet(packet, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    for (number, packet) in audio.iter().enumerate() {
        let block = decoder.decode(packet).unwrap().unwrap();
        let last = number + 1 == audio.len();
        let gp = if number == 0 {
            0
        } else {
            (block.start_frame as i128 + block.frames as i128 + origin as i128
                - if last { tail as i128 } else { 0 }) as u64
        };
        writer.write_packet(packet, gp, last && !nil_eos).unwrap();
        if number == 1 {
            writer.flush_page().unwrap();
        }
        if discarded && !last {
            writer.write_packet(&[], gp, false).unwrap();
            writer.write_packet(b"\x01ignored", gp, false).unwrap();
        }
    }
    if nil_eos {
        writer.finish().unwrap();
    }
    writer.into_inner()
}

#[test]
fn every_frame_of_short_fixtures_matches_sequential_output() {
    for bytes in [
        STEREO,
        MONO,
        SURROUND,
        include_bytes!("fixtures/eight-channel.ogg").as_slice(),
    ] {
        let expected = full(bytes);
        let audio = expected.values().next().unwrap();
        let mut reader = OggSeekReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.index().streams().len(), 1);
        assert_eq!(reader.selected().info, *audio.info.as_ref().unwrap());
        assert_eq!(reader.selected().end, audio.end.unwrap());
        // Release exhausts every retained frame; debug samples every 97th.
        // Compare every block of the first output page, plus complete suffixes.
        let step = if cfg!(debug_assertions) { 97 } else { 1 };
        for frame in (0..audio.end.unwrap().frames).step_by(step) {
            reader.seek(audio.info.as_ref().unwrap().id, frame).unwrap();
            let mut found = false;
            let mut position = frame as usize;
            while !found {
                assert!(
                    reader
                        .read_next(|event| {
                            if let OggEvent::Pcm(pcm) = event {
                                assert_eq!(pcm.start_frame as usize, position);
                                for (expected, plane) in audio.planes.iter().zip(pcm.pcm.planes()) {
                                    assert_eq!(
                                        plane,
                                        &expected[position..position + pcm.pcm.frames]
                                    );
                                }
                                position += pcm.pcm.frames;
                                found = true;
                            }
                        })
                        .unwrap()
                );
            }
        }
        for target in [
            audio.end.unwrap().frames,
            0,
            1,
            audio.end.unwrap().frames / 2,
            audio.end.unwrap().frames - 1,
        ] {
            suffix(&mut reader, target, audio);
        }
    }
}

#[test]
fn edited_offsets_long_spans_discarded_packets_and_nil_eos_seek_exactly() {
    for (origin, tail, nil, discarded) in [
        (-37, 29, false, false),
        (-576, 0, false, false),
        (900, 33, false, false),
        ((1i64 << 54) + 731, 17, false, false),
        (0, 0, true, false),
        (0, 17, false, true),
    ] {
        let repeat = if cfg!(debug_assertions) { 40 } else { 280 };
        let bytes = repack(STEREO, 71, origin, tail, repeat, nil, discarded);
        let expected = full(&bytes);
        let audio = expected.values().next().unwrap();
        let mut reader = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(reader.selected().end, audio.end.unwrap());
        let frames = audio.end.unwrap().frames;
        for frame in [
            frames,
            frames - 1,
            0,
            1,
            538,
            539,
            575,
            576,
            577,
            10000,
            frames / 2,
        ] {
            suffix(&mut reader, frame, audio);
        }
    }
}

fn pages(bytes: &[u8], serial: u32) -> Vec<Vec<u8>> {
    let mut reader = PageReader::new(bytes);
    let mut pages = Vec::new();
    while let Some(page) = reader.next_page().unwrap() {
        let mut header = page.page.header();
        header.serial = serial;
        pages.push(encode_page(header, page.page.lacing(), page.page.body()).unwrap());
    }
    pages
}
#[test]
fn chained_multiplexed_streams_can_be_selected_in_arbitrary_order() {
    let a = pages(STEREO, 81);
    let b = pages(MONO, 82);
    let c = pages(SURROUND, 83);
    let mut unknown = PacketWriter::new(Vec::new(), 84);
    unknown.write_packet(b"future codec", 0, true).unwrap();
    let mut bytes = Vec::new();
    bytes.extend(&a[0]);
    bytes.extend(&b[0]);
    bytes.extend(unknown.into_inner());
    for number in 1..a.len().max(b.len()) {
        if let Some(page) = a.get(number) {
            bytes.extend(page);
        }
        if let Some(page) = b.get(number) {
            bytes.extend(page);
        }
    }
    bytes.extend(c.concat());
    let expected = full(&bytes);
    let mut reader = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.index().streams().len(), 3);
    assert_eq!(reader.index().container().streams().len(), 4);
    for _ in 0..3 {
        for audio in expected.values().rev() {
            for target in [audio.end.unwrap().frames, 0, audio.end.unwrap().frames / 2] {
                suffix(&mut reader, target, audio);
                suffix_with_mode(&mut reader, target, audio, 1);
                suffix_with_mode(&mut reader, target, audio, 2);
            }
        }
    }
}

#[test]
fn empty_stream_end_seeks_and_argument_and_resource_errors() {
    let packets = support::packets(STEREO);
    let mut writer = PacketWriter::new(Vec::new(), 91);
    for (number, packet) in packets[..3].iter().enumerate() {
        writer.write_packet(packet, 0, number == 2).unwrap();
    }
    let bytes = writer.into_inner();
    let expected = full(&bytes);
    let mut reader = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
    suffix(&mut reader, 0, expected.values().next().unwrap());
    assert!(reader.seek(reader.selected().info.id, 1).is_err());

    let expected = full(STEREO);
    let audio = expected.values().next().unwrap();
    let mut reader = OggSeekReader::new(Cursor::new(STEREO)).unwrap();
    assert!(
        reader
            .seek(
                StreamId {
                    serial: 99,
                    chain: 99
                },
                0
            )
            .is_err()
    );
    assert!(reader.seek(reader.selected().info.id, u64::MAX).is_err());
    suffix(&mut reader, 17, audio);
    let (source, index) = reader.into_parts();
    let mut reader = OggSeekReader::with_index(source, index).unwrap();
    suffix(&mut reader, 23, audio);
    for options in [
        SeekOptions {
            max_index_pages: 0,
            ..SeekOptions::default()
        },
        SeekOptions {
            max_index_memory: 0,
            ..SeekOptions::default()
        },
        SeekOptions {
            max_stream_memory: 0,
            ..SeekOptions::default()
        },
    ] {
        assert!(matches!(
            OggSeekReader::with_options(Cursor::new(STEREO), options),
            Err(Error::LimitExceeded(_)
                | Error::Container(deadlib_audio_decode_ogg::Error::LimitExceeded(_)))
        ));
    }
    assert!(OggSeekReader::new(Cursor::new(&STEREO[..STEREO.len() - 1])).is_err());
    assert!(OggSeekReader::new(Cursor::new(&[])).is_err());
}

fn one_sample_stream() -> Vec<u8> {
    let packets = support::packets(STEREO);
    let mut writer = PacketWriter::new(Vec::new(), 92);
    for packet in &packets[..3] {
        writer.write_packet(packet, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    writer.write_packet(&packets[3], 0, false).unwrap();
    writer.write_packet(&packets[4], 1, true).unwrap();
    writer.into_inner()
}
#[test]
fn one_sample_stream_has_exact_seek_boundaries() {
    let bytes = one_sample_stream();
    let expected = full(&bytes);
    let audio = expected.values().next().unwrap();
    assert_eq!(audio.end.unwrap().frames, 1);
    let mut reader = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
    for target in [1, 0, 1, 0] {
        suffix(&mut reader, target, audio);
    }
}

#[test]
fn terminal_io_errors_recover_by_seek_including_after_stream_switch() {
    use std::{
        cell::Cell,
        io::{self, Read, Seek, SeekFrom},
        rc::Rc,
    };
    struct Faults {
        bytes: Cursor<Vec<u8>>,
        read: Rc<Cell<bool>>,
        seek: Rc<Cell<bool>>,
    }
    impl Read for Faults {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.read.get() {
                return Err(io::Error::other("injected read error"));
            }
            self.bytes.read(out)
        }
    }
    impl Seek for Faults {
        fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
            if self.seek.get() {
                return Err(io::Error::other("injected seek error"));
            }
            self.bytes.seek(from)
        }
    }
    let mut bytes = pages(STEREO, 111).concat();
    bytes.extend(pages(MONO, 112).concat());
    let expected = full(&bytes);
    let read = Rc::new(Cell::new(false));
    let seek = Rc::new(Cell::new(false));
    let mut reader = OggSeekReader::new(Faults {
        bytes: Cursor::new(bytes),
        read: read.clone(),
        seek: seek.clone(),
    })
    .unwrap();
    let a = &expected[&(0, 111)];
    let b = &expected[&(1, 112)];
    read.set(true);
    assert!(matches!(reader.read_next(|_| {}), Err(Error::Container(_))));
    assert!(matches!(reader.read_next(|_| {}), Err(Error::Failed)));
    read.set(false);
    suffix(&mut reader, 17, a);
    seek.set(true);
    assert!(reader.seek(b.info.as_ref().unwrap().id, 1).is_err());
    assert!(matches!(reader.read_next(|_| {}), Err(Error::Failed)));
    seek.set(false);
    // A failed switch must not leave the old stream paired with new setup.
    suffix(&mut reader, 13, a);
    suffix(&mut reader, 19, b);
}

#[test]
fn raw_packet_seek_preroll_preserves_absolute_untrimmed_positions() {
    for bytes in [STEREO, MONO, SURROUND] {
        let packets = support::packets(bytes);
        let mut decoder = support::decoder(&packets);
        let mut blocks = Vec::new();
        for packet in &packets[3..] {
            let block = decoder.decode(packet).unwrap().unwrap();
            blocks.push((
                block.start_frame,
                block.frames,
                block.planes().map(<[f32]>::to_vec).collect::<Vec<_>>(),
            ));
        }
        for prime in (0..blocks.len()).rev() {
            decoder.reset_at(blocks[prime].0 + blocks[prime].1 as u64);
            assert_eq!(
                decoder.decode(&packets[3 + prime]).unwrap().unwrap().frames,
                0
            );
            for (packet, expected) in packets[4 + prime..].iter().zip(&blocks[prime + 1..]) {
                let actual = decoder.decode(packet).unwrap().unwrap();
                assert_eq!(
                    (actual.start_frame, actual.frames),
                    (expected.0, expected.1)
                );
                for (actual, expected) in actual.planes().zip(&expected.2) {
                    assert_eq!(actual, expected);
                }
            }
        }
        decoder.reset_at(u64::MAX);
        assert_eq!(decoder.decode(&packets[3]).unwrap().unwrap().frames, 0);
        assert!(matches!(
            decoder.decode(&packets[4]),
            Err(Error::LimitExceeded("PCM frame position"))
        ));
    }
}

#[test]
fn seeking_preserves_nonfatal_and_spanning_metadata() {
    let big = vec![b'x'; 70000];
    let good = Comments {
        vendor: b"seek fixture",
        comments: vec![&big, b"ARTIST=one", b"artist=two"],
    }
    .encode(&Limits::default())
    .unwrap();
    for comment in [good.as_slice(), b"\x03vorbis\0"] {
        let packets = support::packets(STEREO);
        let mut decoder = support::decoder(&packets);
        let mut writer = PacketWriter::new(Vec::new(), 141);
        writer.write_packet(&packets[0], 0, false).unwrap();
        writer.write_packet(comment, 0, false).unwrap();
        writer.write_packet(&packets[2], 0, false).unwrap();
        writer.flush_page().unwrap();
        for (number, packet) in packets[3..].iter().enumerate() {
            let block = decoder.decode(packet).unwrap().unwrap();
            writer
                .write_packet(
                    packet,
                    block.start_frame + block.frames as u64,
                    number + 4 == packets.len(),
                )
                .unwrap();
        }
        let bytes = writer.into_inner();
        let expected = full(&bytes);
        let mut reader = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(
            reader.selected().comment_error.is_some(),
            comment != good.as_slice()
        );
        for frame in [0, 17, 0] {
            suffix(&mut reader, frame, expected.values().next().unwrap());
        }
    }
}

fn seek_oracle(bytes: &[u8], requests: &[(u64, usize)]) -> Vec<u8> {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let executable =
        std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set DEADLIB_LIBVORBIS_ORACLE");
    let name = std::env::temp_dir().join(format!(
        "deadlib-vorbis-seek-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let audio_path = name.with_extension("ogg");
    let request_path = name.with_extension("bin");
    fs::write(&audio_path, bytes).unwrap();
    let mut wire = (requests.len() as u64).to_le_bytes().to_vec();
    for &(global, count) in requests {
        wire.extend(global.to_le_bytes());
        wire.extend((count as u64).to_le_bytes());
    }
    fs::write(&request_path, wire).unwrap();
    let reference = Command::new(executable)
        .arg("oggseek")
        .arg(&audio_path)
        .arg(&request_path)
        .output()
        .unwrap();
    fs::remove_file(audio_path).unwrap();
    fs::remove_file(request_path).unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    reference.stdout
}
fn compare_libvorbis_seeks(bytes: &[u8]) -> (support::Difference, usize) {
    let mut native = OggSeekReader::new(Cursor::new(bytes)).unwrap();
    let streams: Vec<_> = native
        .index()
        .streams()
        .iter()
        .map(|s| {
            (
                s.info.id,
                s.info.identification.channels,
                s.info.identification.sample_rate,
                s.end.frames,
            )
        })
        .collect();
    let mut requests = Vec::new();
    let mut base = 0;
    let mut seed = 17u64;
    for &(id, _, _, frames) in &streams {
        let mut targets = vec![
            0,
            frames,
            frames.saturating_sub(1),
            frames / 2,
            frames / 2 + 1,
            frames / 3,
        ];
        targets.extend(
            [1, 127, 128, 255, 256, 575, 576, 577, 1023, 1024, 2047, 2048]
                .into_iter()
                .map(|p| p.min(frames)),
        );
        for _ in 0..32 {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            targets.push(seed % (frames + 1));
        }
        for frame in targets {
            let frame = frame.min(frames);
            requests.push((id, frame, (frames - frame).min(4096) as usize, base + frame));
        }
        base += frames;
    }
    let reference = seek_oracle(
        bytes,
        &requests.iter().map(|r| (r.3, r.2)).collect::<Vec<_>>(),
    );
    let mut input = reference.as_slice();
    fn u64_in(input: &mut &[u8]) -> u64 {
        u64::from_le_bytes(support::take(input, 8).try_into().unwrap())
    }
    assert_eq!(support::u32_in(&mut input) as usize, streams.len());
    for &(id, channels, rate, frames) in &streams {
        assert_eq!(support::u32_in(&mut input), id.serial);
        assert_eq!(support::u32_in(&mut input), channels as u32);
        assert_eq!(support::u32_in(&mut input), rate);
        assert_eq!(u64_in(&mut input), frames);
    }
    let mut difference = support::Difference::default();
    for &(id, frame, wanted, global) in &requests {
        assert_eq!(u64_in(&mut input), global);
        let link = streams.iter().position(|s| s.0 == id).unwrap();
        let channels = streams[link].1 as usize;
        let mut expected = vec![Vec::new(); channels];
        loop {
            let count = support::u32_in(&mut input) as usize;
            if count == 0 {
                break;
            }
            assert_eq!(support::u32_in(&mut input) as usize, link);
            for plane in &mut expected {
                for _ in 0..count {
                    plane.push(f32::from_bits(support::u32_in(&mut input)));
                }
            }
        }
        assert_eq!(expected[0].len(), wanted);
        native.seek(id, frame).unwrap();
        let mut position = 0;
        while position < wanted {
            let mut progress = false;
            assert!(
                native
                    .read_packet(|event| {
                        if let OggEvent::Pcm(pcm) = event {
                            if position == wanted {
                                return;
                            }
                            assert_eq!(pcm.stream, id);
                            assert_eq!(pcm.start_frame, frame + position as u64);
                            let count = pcm.pcm.frames.min(wanted - position);
                            for (plane, reference) in pcm.pcm.planes().zip(&expected) {
                                for (&a, &b) in plane[..count]
                                    .iter()
                                    .zip(&reference[position..position + count])
                                {
                                    difference.add(a, b);
                                }
                            }
                            position += count;
                            progress = true;
                        }
                    })
                    .unwrap()
            );
            if progress {
                assert!(position <= wanted);
            }
        }
    }
    assert!(input.is_empty());
    (difference, requests.len())
}

#[test]
#[ignore = "requires independently built libvorbisfile oracle"]
fn libvorbisfile_repeated_seeks_fixtures_edits_and_mixed_rate_chains() {
    for bytes in [STEREO, MONO, SURROUND] {
        compare_libvorbis_seeks(bytes)
            .0
            .check("libvorbisfile/seek fixture");
    }
    compare_libvorbis_seeks(&one_sample_stream())
        .0
        .check("libvorbisfile/seek one sample");
    for (origin, tail, nil) in [(-37, 29, false), (0, 0, true)] {
        let bytes = repack(STEREO, 71, origin, tail, 280, nil, false);
        compare_libvorbis_seeks(&bytes)
            .0
            .check("libvorbisfile/seek edited");
    }
    let mut chain = pages(STEREO, 91).concat();
    chain.extend(pages(MONO, 92).concat());
    compare_libvorbis_seeks(&chain)
        .0
        .check("libvorbisfile/seek chain");
}

#[test]
#[ignore = "requires independently built libvorbisfile oracle"]
fn libvorbisfile_1_3_7_broadcast_seek_reports_incorrect_position() {
    let bytes = repack(STEREO, 71, 900, 33, 280, false, false);
    let reference = seek_oracle(&bytes, &[(1, 32)]);
    let mut input = reference.as_slice();
    assert_eq!(support::u32_in(&mut input), 1);
    support::take(&mut input, 4 + 4 + 4 + 8); // Link metadata.
    let actual = u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap());
    assert_eq!(actual, 900); // Requested retained frame 1; libvorbisfile reports 900.
    let mut native = OggSeekReader::new(Cursor::new(&bytes)).unwrap();
    native.seek(native.selected().info.id, 1).unwrap();
    assert_eq!(native.position(), 1);
    let expected = full(&bytes);
    suffix(&mut native, 1, expected.values().next().unwrap());
}

#[test]
fn symphonia_accurate_seek_with_packet_preroll_and_timestamp_discard() {
    use symphonia::core::{
        audio::{Audio, GenericAudioBufferRef},
        codecs::audio::AudioDecoderOptions,
        formats::{FormatOptions, SeekMode, SeekTo, TrackType, probe::Hint},
        io::MediaSourceStream,
        meta::MetadataOptions,
        units::Timestamp,
    };
    for bytes in [
        STEREO.to_vec(),
        MONO.to_vec(),
        repack(STEREO, 78, -37, 29, 40, false, false),
        repack(STEREO, 79, 900, 33, 40, false, false),
    ] {
        let expected = full(&bytes);
        let audio = expected.values().next().unwrap();
        let mut hint = Hint::new();
        hint.with_extension("ogg");
        let mut format = symphonia::default::get_probe()
            .probe(
                &hint,
                MediaSourceStream::new(Box::new(Cursor::new(bytes.clone())), Default::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .unwrap();
        let track = format.default_track(TrackType::Audio).unwrap();
        let track_id = track.id;
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                track.codec_params.as_ref().unwrap().audio().unwrap(),
                &AudioDecoderOptions::default(),
            )
            .unwrap();
        let frames = audio.end.unwrap().frames;
        let mut difference = support::Difference::default();
        for target in [0, 1, 127, 255, 575, 576, frames / 2, frames - 1] {
            if target >= frames {
                continue;
            }
            // FormatReader seeks to a packet, not an initialized codec state.
            // Request a full maximum block earlier, then prime and discard by
            // timestamp to the actual requested sample (the documented API).
            let preroll = audio.info.as_ref().unwrap().identification.block_sizes[1] as u64;
            let ts = Timestamp::new(
                (audio.end.unwrap().start_granule + target.saturating_sub(preroll)) as i64,
            );
            let wanted_ts = (audio.end.unwrap().start_granule + target) as i64;
            let seeked = format
                .seek(SeekMode::Accurate, SeekTo::Timestamp { ts, track_id })
                .unwrap_or_else(|error| {
                    panic!("seek target {target}, timestamp {}: {error:?}", ts.get())
                });
            assert_eq!(seeked.required_ts, ts);
            assert!(seeked.actual_ts <= ts);
            decoder.reset();
            let wanted = (frames - target).min(256) as usize;
            let mut position = 0;
            while position < wanted {
                let packet = format.next_packet().unwrap().unwrap();
                let GenericAudioBufferRef::F32(data) = decoder.decode(&packet).unwrap() else {
                    panic!("expected f32")
                };
                if data.frames() == 0 {
                    continue;
                }
                let start = packet.pts.get() + packet.trim_start.get() as i64;
                let next = wanted_ts + position as i64;
                assert!(start <= next, "seek missed pre-roll: {start} > {next}");
                let skip = (next - start).min(data.frames() as i64) as usize;
                let count = (data.frames() - skip).min(wanted - position);
                for (channel, expected) in audio.planes.iter().enumerate() {
                    for (&a, &b) in expected
                        [target as usize + position..target as usize + position + count]
                        .iter()
                        .zip(&data.plane(channel).unwrap()[skip..skip + count])
                    {
                        difference.add(a, b);
                    }
                }
                position += count;
            }
        }
        difference.check("Symphonia/accurate seek");
    }
}

#[test]
#[ignore = "requires libvorbisfile oracle and DEADLIB_VORBIS_CORPUS"]
fn libvorbisfile_local_corpus_repeated_sample_accurate_seeks() {
    use std::{fmt::Write, fs, path::Path};
    let root = std::env::var_os("DEADLIB_VORBIS_CORPUS").expect("set DEADLIB_VORBIS_CORPUS");
    let paths = support::paths(Path::new(&root));
    assert!(!paths.is_empty());
    let mut total = support::Difference::default();
    let mut count = 0;
    let mut report =
        String::from("path\tseeks\tsamples\tpeak_abs_error\trms_error\treference_peak\n");
    for path in &paths {
        eprintln!("{}", path.display());
        let (difference, seeks) = compare_libvorbis_seeks(&fs::read(path).unwrap());
        difference.check("libvorbisfile/seek");
        writeln!(
            &mut report,
            "{}\t{seeks}\t{}\t{:.12e}\t{:.12e}\t{:.9}",
            path.strip_prefix(&root).unwrap().display(),
            difference.samples,
            difference.peak_error,
            difference.rms(),
            difference.peak_reference
        )
        .unwrap();
        total.merge(&difference);
        count += seeks;
    }
    total.check(&format!("{} files, {count} seeks", paths.len()));
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_SEEK_REPORT") {
        fs::write(path, report).unwrap();
    }
}

#[test]
#[ignore = "requires external historical floor-zero file and libvorbisfile oracle"]
fn historical_floor0_repacketized_into_strict_ogg_seeks_correctly() {
    let path = std::env::var_os("DEADLIB_VORBIS_FLOOR0").expect("set DEADLIB_VORBIS_FLOOR0");
    let mut input = ogg::PacketReader::new(std::fs::File::open(path).unwrap());
    let mut packets = Vec::new();
    while let Some(packet) = input.read_packet().unwrap() {
        packets.push(packet.data);
    }
    // The historical source has invalid granules on packet-span pages. This
    // test isolates floor-zero seek synthesis by repacketizing unchanged codec
    // packets into a strict, zero-origin container. It adds no legacy exception.
    let mut decoder = support::decoder(&packets);
    let mut writer = PacketWriter::new(Vec::new(), 121);
    for packet in &packets[..3] {
        writer.write_packet(packet, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    for (number, packet) in packets[3..].iter().enumerate() {
        let block = decoder.decode(packet).unwrap().unwrap();
        writer
            .write_packet(
                packet,
                block.start_frame + block.frames as u64,
                number + 4 == packets.len(),
            )
            .unwrap();
        if number == 1 {
            writer.flush_page().unwrap();
        }
    }
    let (difference, count) = compare_libvorbis_seeks(&writer.into_inner());
    difference.check_with_bounds(&format!("floor-zero/{count} seeks"), 5e-5, 3e-6);
}
