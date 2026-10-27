mod support;
use deadlib_audio_decode_ogg::{PacketWriter, Page, PageReader, encode_page};
use deadlib_audio_decode_vorbis::*;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read},
    path::Path,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const STEREO: &[u8] = include_bytes!("fixtures/stereo.ogg");
const MONO: &[u8] = include_bytes!("fixtures/mono.ogg");
const SURROUND: &[u8] = include_bytes!("fixtures/surround.ogg");
#[derive(Default, Debug)]
struct Audio {
    info: Option<StreamInfo>,
    planes: Vec<Vec<f32>>,
    end: Option<StreamEnd>,
    warnings: usize,
    chunks: usize,
    first_granule: Option<u64>,
}
#[derive(Default, Debug)]
struct Output {
    streams: BTreeMap<(u32, u32), Audio>,
    ignored: Vec<StreamId>,
}
impl Output {
    fn event(&mut self, event: OggEvent<'_>) {
        match event {
            OggEvent::StreamStart(info) => {
                let stream = self
                    .streams
                    .entry((info.id.chain, info.id.serial))
                    .or_default();
                assert!(stream.info.is_none());
                stream
                    .planes
                    .resize(info.identification.channels as usize, Vec::new());
                stream.info = Some(info.clone());
            }
            OggEvent::Pcm(data) => {
                let stream = self
                    .streams
                    .get_mut(&(data.stream.chain, data.stream.serial))
                    .expect("stream starts before PCM");
                assert_eq!(stream.planes[0].len() as u64, data.start_frame);
                let origin = *stream.first_granule.get_or_insert(data.start_granule);
                assert_eq!(origin + data.start_frame, data.start_granule);
                assert_eq!(
                    stream.info.as_ref().unwrap().identification.sample_rate,
                    data.sample_rate
                );
                assert_eq!(stream.planes.len(), data.pcm.channels);
                assert!(data.pcm.frames != 0);
                for (out, plane) in stream.planes.iter_mut().zip(data.pcm.planes()) {
                    out.extend_from_slice(plane);
                }
                stream.chunks += 1;
            }
            OggEvent::StreamEnd(end) => {
                let stream = self
                    .streams
                    .get_mut(&(end.stream.chain, end.stream.serial))
                    .expect("start before end");
                assert!(stream.end.is_none());
                assert_eq!(stream.planes[0].len() as u64, end.frames);
                assert_eq!(end.end_granule - end.start_granule, end.frames);
                if let Some(start) = stream.first_granule {
                    assert_eq!(start, end.start_granule);
                }
                stream.end = Some(end);
            }
            OggEvent::IgnoredStream(id) => self.ignored.push(id),
            OggEvent::CommentError { stream, .. } => {
                self.streams
                    .entry((stream.chain, stream.serial))
                    .or_default()
                    .warnings += 1
            }
        }
    }
}
fn decode(bytes: &[u8]) -> Output {
    let mut out = Output::default();
    decode_reader(bytes, OggOptions::default(), |event| out.event(event)).unwrap();
    out
}
fn push(bytes: &[u8], chunk: usize) -> Output {
    let mut decoder = OggStreamDecoder::default();
    let mut out = Output::default();
    for input in bytes.chunks(chunk) {
        let mut position = 0;
        while position < input.len() {
            let consumed = decoder
                .push(&input[position..], |event| out.event(event))
                .unwrap();
            assert!(consumed != 0);
            position += consumed;
        }
    }
    decoder.finish().unwrap();
    out
}
fn raw_audio(bytes: &[u8]) -> (Vec<Vec<f32>>, Vec<Vec<u8>>, Vec<u64>) {
    let packets = support::packets(bytes);
    let mut decoder = support::decoder(&packets);
    let mut pcm = vec![Vec::new(); decoder.setup().channels() as usize];
    let mut granules = Vec::new();
    for packet in &packets[3..] {
        let block = decoder.decode(packet).unwrap().unwrap();
        granules.push(block.start_frame + block.frames as u64);
        for (out, plane) in pcm.iter_mut().zip(block.planes()) {
            out.extend(plane);
        }
    }
    (pcm, packets, granules)
}
fn repack(
    bytes: &[u8],
    id: u32,
    origin: i64,
    tail: u64,
    nil_eos: bool,
    comment: Option<&[u8]>,
) -> Vec<u8> {
    let (_, packets, ends) = raw_audio(bytes);
    let mut writer = PacketWriter::new(Vec::new(), id);
    writer.write_packet(&packets[0], 0, false).unwrap();
    writer
        .write_packet(comment.unwrap_or(&packets[1]), 0, false)
        .unwrap();
    writer.write_packet(&packets[2], 0, false).unwrap();
    writer.flush_page().unwrap();
    for (i, packet) in packets[3..].iter().enumerate() {
        let last = i + 1 == ends.len();
        let gp = if i == 0 {
            0
        } else {
            (ends[i] as i128 + origin as i128 - if last { tail as i128 } else { 0 }) as u64
        };
        writer.write_packet(packet, gp, last && !nil_eos).unwrap();
        // Offset edits require the second audio packet to finish its page.
        if i == 1 {
            writer.flush_page().unwrap();
        }
    }
    if nil_eos {
        writer.flush_page().unwrap();
        writer.finish().unwrap();
    }
    writer.into_inner()
}
fn one(out: &Output) -> &Audio {
    assert_eq!(out.streams.len(), 1);
    out.streams.values().next().unwrap()
}

#[test]
fn original_fixtures_trim_to_exact_source_sample_counts() {
    for (bytes, frames) in [
        (STEREO, 3840),
        (MONO, 640),
        (SURROUND, 2880),
        (include_bytes!("fixtures/eight-channel.ogg").as_slice(), 882),
    ] {
        let out = decode(bytes);
        let stream = one(&out);
        assert_eq!(stream.planes[0].len(), frames);
        assert_eq!(stream.end.unwrap().frames, frames as u64);
        let (raw, _, _) = raw_audio(bytes);
        assert_eq!(
            stream.planes,
            raw.iter().map(|p| p[..frames].to_vec()).collect::<Vec<_>>()
        );
    }
}
#[test]
fn initial_trim_broadcast_offset_and_final_trim_preserve_correct_pcm() {
    let (raw, _, _) = raw_audio(STEREO);
    for origin in [-37, 0, 900] {
        for tail in [0, 29, 1024] {
            let bytes = repack(STEREO, 71, origin, tail, false, None);
            let out = decode(&bytes);
            let stream = one(&out);
            let start = (-origin).max(0) as usize;
            let end = raw[0].len() - tail as usize;
            assert_eq!(
                stream.planes,
                raw.iter()
                    .map(|plane| plane[start..end].to_vec())
                    .collect::<Vec<_>>()
            );
            assert_eq!(stream.end.unwrap().start_granule, origin.max(0) as u64);
        }
    }
    let bytes = repack(STEREO, 72, 0, 0, true, None);
    assert_eq!(one(&decode(&bytes)).planes, raw);
}

#[test]
fn invalid_mapping_headers_and_granules_are_terminal() {
    let (_, packets, _) = raw_audio(STEREO);
    // Identification padding violates the exact 58-byte Vorbis BOS mapping.
    let mut writer = PacketWriter::new(Vec::new(), 3);
    let mut id = packets[0].clone();
    id.push(0);
    writer.write_packet(&id, 0, false).unwrap();
    assert!(
        decode_reader(
            writer.into_inner().as_slice(),
            OggOptions::default(),
            |_| {}
        )
        .is_err()
    );
    // The setup packet must finish its page, even if the first audio granule is zero.
    let mut writer = PacketWriter::new(Vec::new(), 4);
    writer.write_packet(&packets[0], 0, false).unwrap();
    writer.write_packet(&packets[1], 0, false).unwrap();
    writer.write_packet(&packets[2], 0, false).unwrap();
    writer.write_packet(&packets[3], 0, true).unwrap();
    let bad = writer.into_inner();
    let mut decoder = OggStreamDecoder::default();
    let mut position = 0;
    while position < bad.len() {
        match decoder.push(&bad[position..], |_| {}) {
            Ok(n) => position += n,
            Err(_) => break,
        }
    }
    assert!(matches!(decoder.push(&[], |_| {}), Err(Error::Failed)));
    // Rewrite page CRCs so these tests exercise mapping validation, not CRC rejection.
    let original = pages(STEREO, 5);
    let mut bad = Vec::new();
    for (index, bytes) in original.iter().enumerate() {
        let page = Page::parse(bytes).unwrap().0;
        let mut header = page.header();
        if index == 0 {
            header.granule_position = 1;
        }
        bad.extend(encode_page(header, page.lacing(), page.body()).unwrap());
    }
    assert!(matches!(
        decode_reader(bad.as_slice(), OggOptions::default(), |_| {}),
        Err(Error::Invalid(_))
    ));
    let mut bad = Vec::new();
    for bytes in original {
        let page = Page::parse(&bytes).unwrap().0;
        let mut header = page.header();
        if header.is_eos() {
            header.granule_position += 10000;
        }
        bad.extend(encode_page(header, page.lacing(), page.body()).unwrap());
    }
    assert!(matches!(
        decode_reader(bad.as_slice(), OggOptions::default(), |_| {}),
        Err(Error::Invalid(_))
    ));
}

fn short_stream(audio_packets: usize, end_granule: u64) -> Vec<u8> {
    let packets = support::packets(STEREO);
    let mut writer = PacketWriter::new(Vec::new(), 79);
    writer.write_packet(&packets[0], 0, false).unwrap();
    writer.write_packet(&packets[1], 0, false).unwrap();
    writer
        .write_packet(&packets[2], 0, audio_packets == 0)
        .unwrap();
    writer.flush_page().unwrap();
    for (index, packet) in packets[3..3 + audio_packets].iter().enumerate() {
        let last = index + 1 == audio_packets;
        writer
            .write_packet(packet, if last { end_granule } else { 0 }, last)
            .unwrap();
    }
    writer.into_inner()
}

#[test]
fn empty_and_single_page_audio_have_unambiguous_tail_counts() {
    let (raw, _, ends) = raw_audio(STEREO);
    for (packets, end, frames, origin) in [
        (0, 0, 0, 0),
        (1, 0, 0, 0),
        (2, 0, 0, 0),
        (2, 1, 1, 0),
        (2, ends[1], ends[1], 0),
        (2, ends[1] + 900, ends[1], 900),
    ] {
        let output = decode(&short_stream(packets, end));
        let audio = one(&output);
        assert_eq!(audio.end.unwrap().frames, frames);
        assert_eq!(audio.end.unwrap().start_granule, origin);
        for (plane, raw) in audio.planes.iter().zip(&raw) {
            assert_eq!(plane, &raw[..frames as usize]);
        }
    }
    // The second audio packet's overlap can be discarded completely at start.
    let output = decode(&repack(STEREO, 80, -(ends[1] as i64), 0, false, None));
    assert_eq!(one(&output).planes[0], raw[0][ends[1] as usize..]);
    // Retraction of output returned before the EOS page is invalid.
    let bytes = repack(
        STEREO,
        81,
        -1,
        ends.last().unwrap() - ends[1] + 1,
        false,
        None,
    );
    assert!(decode_reader(bytes.as_slice(), OggOptions::default(), |_| {}).is_err());
    for tail in [2048, 4096] {
        let bytes = repack(STEREO, 82, 0, tail, false, None);
        assert!(decode_reader(bytes.as_slice(), OggOptions::default(), |_| {}).is_err());
    }
}
#[test]
fn push_boundaries_and_reset_match_pull_output() {
    let bytes = repack(STEREO, 81, -17, 33, false, None);
    let expected = decode(&bytes);
    for chunk in [1, 2, 3, 26, 27, 28, 255, 4096, bytes.len()] {
        assert_eq!(one(&push(&bytes, chunk)).planes, one(&expected).planes);
    }
    let mut decoder = OggStreamDecoder::default();
    assert!(
        decoder
            .push(b"not an Ogg page but long enough", |_| {})
            .is_err()
    );
    assert!(matches!(decoder.push(&bytes, |_| {}), Err(Error::Failed)));
    decoder.reset();
    let mut position = 0;
    while position < bytes.len() {
        position += decoder.push(&bytes[position..], |_| {}).unwrap();
    }
    decoder.finish().unwrap();
}
fn pages(bytes: &[u8], id: u32) -> Vec<Vec<u8>> {
    let mut reader = PageReader::new(Cursor::new(bytes));
    let mut out = Vec::new();
    while let Some(located) = reader.next_page().unwrap() {
        let page = located.page;
        let mut header = page.header();
        header.serial = id;
        out.push(encode_page(header, page.lacing(), page.body()).unwrap());
    }
    out
}
#[test]
fn chains_and_multiplexing_keep_independent_rates_layouts_and_overlaps() {
    let a = pages(STEREO, 101);
    let b = pages(MONO, 102);
    let mut unknown = PacketWriter::new(Vec::new(), 103);
    unknown.write_packet(b"future codec", 0, true).unwrap();
    let mut mux = a[0].clone();
    mux.extend(&b[0]);
    mux.extend(unknown.into_inner());
    for index in 1..a.len().max(b.len()) {
        if let Some(page) = a.get(index) {
            mux.extend(page);
        }
        if let Some(page) = b.get(index) {
            mux.extend(page);
        }
    }
    let out = decode(&mux);
    assert_eq!(out.streams.len(), 2);
    assert_eq!(
        out.ignored,
        [StreamId {
            serial: 103,
            chain: 0
        }]
    );
    assert_eq!(out.streams[&(0, 101)].planes, one(&decode(STEREO)).planes);
    assert_eq!(out.streams[&(0, 102)].planes, one(&decode(MONO)).planes);
    let mut chain = a.concat();
    chain.extend(b.concat());
    let out = decode(&chain);
    assert_eq!(
        out.streams[&(0, 101)]
            .info
            .as_ref()
            .unwrap()
            .identification
            .sample_rate,
        48000
    );
    assert_eq!(
        out.streams[&(1, 102)]
            .info
            .as_ref()
            .unwrap()
            .identification
            .sample_rate,
        8000
    );
    let mut decoder = OggDecoder::default();
    let mut reader = PageReader::new(Cursor::new(&mux));
    while let Some(located) = reader.next_page().unwrap() {
        decoder
            .push_page(located.page, located.offset, |_| {})
            .unwrap();
    }
    decoder.finish().unwrap();
    assert_eq!(decoder.storage_bytes(), 0);
    let mut peak = 0;
    for bytes in [STEREO, MONO] {
        let mut decoder = OggDecoder::default();
        let mut reader = PageReader::new(Cursor::new(bytes));
        while let Some(located) = reader.next_page().unwrap() {
            decoder
                .push_page(located.page, located.offset, |_| {})
                .unwrap();
            peak = peak.max(decoder.storage_bytes());
        }
    }
    let options = OggOptions {
        max_stream_memory: peak,
        ..OggOptions::default()
    };
    // One live codec fits. Its storage is released and reused by a new chain;
    // concurrent codecs exceed the same aggregate budget.
    decode_reader(chain.as_slice(), options, |_| {}).unwrap();
    assert!(matches!(
        decode_reader(mux.as_slice(), options, |_| {}),
        Err(Error::LimitExceeded(_))
    ));
    let options = OggOptions {
        max_active_streams: 1,
        ..OggOptions::default()
    };
    decode_reader(chain.as_slice(), options, |_| {}).unwrap();
    assert!(matches!(
        decode_reader(mux.as_slice(), options, |_| {}),
        Err(Error::LimitExceeded(_))
    ));
}
#[test]
fn comment_truncation_is_nonfatal_and_large_comments_span_pages() {
    let bad = b"\x03vorbis\0";
    let bytes = repack(STEREO, 51, 0, 0, false, Some(bad));
    let out = decode(&bytes);
    assert_eq!(one(&out).warnings, 1);
    assert!(
        one(&out)
            .info
            .as_ref()
            .unwrap()
            .metadata
            .vendor()
            .is_empty()
    );
    let field = vec![b'x'; 70000];
    let comments = Comments {
        vendor: b"test",
        comments: vec![&field, b"ARTIST=one", b"artist=two"],
    }
    .encode(&Limits::default())
    .unwrap();
    let bytes = repack(STEREO, 52, 0, 0, false, Some(&comments));
    let out = push(&bytes, 17);
    let metadata = &one(&out).info.as_ref().unwrap().metadata;
    assert_eq!(metadata.vendor(), b"test");
    assert_eq!(metadata.comments().next().unwrap(), field);
    assert_eq!(
        metadata.values(b"Artist").collect::<Vec<_>>(),
        [b"one", b"two"]
    );
}
struct Fragmented<R> {
    inner: R,
    interrupt: bool,
}
impl<R: Read> Read for Fragmented<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.interrupt = !self.interrupt;
        if self.interrupt {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let length = output.len().min(7);
        self.inner.read(&mut output[..length])
    }
}
#[test]
fn fragmented_io_truncation_terminal_errors_and_limits() {
    let mut out = Output::default();
    decode_reader(
        Fragmented {
            inner: Cursor::new(STEREO),
            interrupt: false,
        },
        OggOptions::default(),
        |event| out.event(event),
    )
    .unwrap();
    assert_eq!(one(&out).planes[0].len(), 3840);
    for end in [0, 1, 27, 58, STEREO.len() - 1] {
        assert!(decode_reader(&STEREO[..end], OggOptions::default(), |_| {}).is_err());
    }
    for options in [
        OggOptions {
            max_stream_memory: 0,
            ..OggOptions::default()
        },
        OggOptions {
            max_active_streams: 0,
            ..OggOptions::default()
        },
    ] {
        assert!(matches!(
            decode_reader(STEREO, options, |_| {}),
            Err(Error::LimitExceeded(_))
        ));
    }
    let mut reader = OggReader::new(&STEREO[..STEREO.len() - 1]);
    while matches!(reader.read_next(|_| {}), Ok(true)) {}
    assert!(matches!(reader.read_next(|_| {}), Err(Error::Failed)));
}

fn oracle(bytes: &[u8]) -> Output {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let executable =
        std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set DEADLIB_LIBVORBIS_ORACLE");
    let file = std::env::temp_dir().join(format!(
        "deadlib-vorbis-file-{}-{}.ogg",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&file, bytes).unwrap();
    let reference = Command::new(executable)
        .arg("oggpcm")
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let mut input = reference.stdout.as_slice();
    let count = support::u32_in(&mut input);
    let mut order = Vec::new();
    let mut out = Output::default();
    for link in 0..count {
        let id = support::u32_in(&mut input);
        let channels = support::u32_in(&mut input);
        let rate = support::u32_in(&mut input);
        let frames = u64::from_le_bytes(support::take(&mut input, 8).try_into().unwrap());
        order.push((link, id));
        out.streams.insert(
            (link, id),
            Audio {
                info: Some(StreamInfo {
                    id: StreamId {
                        serial: id,
                        chain: link,
                    },
                    identification: Identification {
                        channels: channels as u8,
                        sample_rate: rate,
                        bitrate_maximum: 0,
                        bitrate_minimum: 0,
                        bitrate_nominal: 0,
                        block_sizes: [64, 64],
                    },
                    metadata: Metadata::default(),
                }),
                planes: vec![Vec::new(); channels as usize],
                end: Some(StreamEnd {
                    stream: StreamId {
                        serial: id,
                        chain: link,
                    },
                    frames,
                    emitted_frames: frames,
                    emitted_start_granule: 0,
                    start_granule: 0,
                    end_granule: frames,
                }),
                ..Audio::default()
            },
        );
    }
    while !input.is_empty() {
        let link = support::u32_in(&mut input) as usize;
        let frames = support::u32_in(&mut input) as usize;
        let stream = out.streams.get_mut(&order[link]).unwrap();
        for plane in &mut stream.planes {
            for _ in 0..frames {
                plane.push(f32::from_bits(support::u32_in(&mut input)));
            }
        }
    }
    for stream in out.streams.values() {
        assert_eq!(stream.planes[0].len() as u64, stream.end.unwrap().frames);
    }
    out
}
fn compare(bytes: &[u8]) -> support::Difference {
    let native = decode(bytes);
    let reference = oracle(bytes);
    assert_eq!(native.streams.len(), reference.streams.len());
    let mut difference = support::Difference::default();
    for (id, native) in &native.streams {
        let reference = &reference.streams[id];
        assert_eq!(native.end.unwrap().frames, reference.end.unwrap().frames);
        assert_eq!(native.planes.len(), reference.planes.len());
        for (a, b) in native.planes.iter().zip(&reference.planes) {
            assert_eq!(a.len(), b.len());
            for (&a, &b) in a.iter().zip(b) {
                difference.add(a, b);
            }
        }
    }
    difference
}

fn compare_symphonia(bytes: &[u8]) -> support::Difference {
    compare_symphonia_with_undiscarded_prefix(bytes, 0)
}
// The nonzero argument is reserved for the named reference-discrepancy test.
// All ordinary parity checks require exact counts and use zero.
fn compare_symphonia_with_undiscarded_prefix(bytes: &[u8], prefix: usize) -> support::Difference {
    use symphonia::core::{
        audio::{Audio, GenericAudioBufferRef},
        codecs::audio::AudioDecoderOptions,
        formats::{FormatOptions, TrackType, probe::Hint},
        io::MediaSourceStream,
        meta::MetadataOptions,
    };
    let native = decode(bytes);
    let native = one(&native);
    // These fixtures/corpus files are mono/stereo; dependent multichannel
    // coupling is checked independently against libvorbis (see COVERAGE.md).
    assert!(native.planes.len() <= 2);
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .unwrap();
    let track = format.default_track(TrackType::Audio).unwrap();
    let track_id = track.id;
    let parameters = track.codec_params.as_ref().unwrap().audio().unwrap();
    assert_eq!(
        parameters.sample_rate,
        Some(native.info.as_ref().unwrap().identification.sample_rate)
    );
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .unwrap();
    let mut position = 0;
    let mut difference = support::Difference::default();
    while let Some(packet) = format.next_packet().unwrap() {
        assert_eq!(packet.track_id, track_id);
        let GenericAudioBufferRef::F32(audio) = decoder.decode(&packet).unwrap() else {
            panic!("expected f32")
        };
        let frames = audio.frames();
        if frames != 0 {
            assert_eq!(
                packet.pts.get() + packet.trim_start.get() as i64,
                native.end.unwrap().start_granule as i64 + position as i64 - prefix as i64
            );
            assert_eq!(packet.dur.get(), frames as u64);
        }
        let skip = prefix.saturating_sub(position).min(frames);
        let start = position.saturating_sub(prefix);
        for (channel, plane) in native.planes.iter().enumerate() {
            for (&native, &reference) in plane[start..start + frames - skip]
                .iter()
                .zip(&audio.plane(channel).unwrap()[skip..])
            {
                difference.add(native, reference);
            }
        }
        position += frames;
    }
    assert_eq!(position as u64, native.end.unwrap().frames + prefix as u64);
    difference
}

#[test]
fn symphonia_full_file_counts_pcm_and_timestamps() {
    for bytes in [STEREO, MONO] {
        compare_symphonia(bytes).check("Symphonia/full file");
    }
    for origin in [0, 900] {
        compare_symphonia(&repack(STEREO, 78, origin, 33, false, None)).check("Symphonia/edited");
    }
}

#[test]
fn symphonia_0_6_1_retains_negative_timestamp_initial_samples() {
    // Appendix A requires samples before time zero to be discarded. Symphonia
    // 0.6.1 returns these 37 extra samples at negative PTS with trim_start=0.
    // Native counts/PCM match libvorbisfile in the separate external test.
    compare_symphonia_with_undiscarded_prefix(&repack(STEREO, 78, -37, 33, false, None), 37)
        .check("Symphonia/known undiscarded prefix");
}
#[test]
#[ignore = "requires independently built libvorbisfile oracle"]
fn libvorbisfile_original_edited_and_chained_streams() {
    for bytes in [STEREO, MONO, SURROUND] {
        compare(bytes).check("Vorbisfile/original");
    }
    for origin in [-37, 0, 900] {
        for tail in [0, 29, 1024] {
            compare(&repack(STEREO, 71, origin, tail, false, None)).check("Vorbisfile/edited");
        }
    }
    compare(&repack(STEREO, 73, 0, 0, true, None)).check("Vorbisfile/nil EOS");
    let mut chain = pages(STEREO, 91).concat();
    chain.extend(pages(MONO, 92).concat());
    compare(&chain).check("Vorbisfile/chain");
    let (_, _, ends) = raw_audio(STEREO);
    for (packets, end) in [
        (0, 0),
        (1, 0),
        (2, 0),
        (2, 1),
        (2, ends[1]),
        (2, ends[1] + 900),
    ] {
        compare(&short_stream(packets, end)).check("Vorbisfile/short");
    }
    compare(&repack(STEREO, 80, -(ends[1] as i64), 0, false, None))
        .check("Vorbisfile/full initial trim");
}
#[test]
#[ignore = "requires libvorbisfile oracle and DEADLIB_VORBIS_CORPUS"]
fn libvorbisfile_local_corpus_final_counts_and_pcm() {
    let root = std::env::var_os("DEADLIB_VORBIS_CORPUS").expect("set DEADLIB_VORBIS_CORPUS");
    let paths = support::paths(Path::new(&root));
    assert!(!paths.is_empty());
    let mut total = support::Difference::default();
    let mut total_symphonia = support::Difference::default();
    let mut report =
        String::from("path\tdecoder\tsamples\tpeak_abs_error\trms_error\treference_peak\n");
    for path in &paths {
        eprintln!("{}", path.display());
        let bytes = fs::read(path).unwrap();
        let difference = compare(&bytes);
        difference.check("Vorbisfile");
        total.merge(&difference);
        let symphonia = compare_symphonia(&bytes);
        symphonia.check("Symphonia");
        total_symphonia.merge(&symphonia);
        for (name, result) in [
            ("libvorbisfile-1.3.7", &difference),
            ("Symphonia-0.6.1", &symphonia),
        ] {
            use std::fmt::Write;
            writeln!(
                &mut report,
                "{}\t{name}\t{}\t{:.12e}\t{:.12e}\t{:.9}",
                path.strip_prefix(&root).unwrap().display(),
                result.samples,
                result.peak_error,
                result.rms(),
                result.peak_reference
            )
            .unwrap();
        }
    }
    total.check(&format!("all {} files", paths.len()));
    total_symphonia.check(&format!("all {} files/Symphonia", paths.len()));
    if let Some(path) = std::env::var_os("DEADLIB_VORBIS_MAPPING_REPORT") {
        fs::write(path, report).unwrap();
    }
}
