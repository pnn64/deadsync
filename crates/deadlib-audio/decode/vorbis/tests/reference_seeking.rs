#[path = "support/raw_cases.rs"]
#[allow(dead_code)]
mod raw_cases;
mod support;
#[path = "support/synthetic.rs"]
mod synthetic;
use deadlib_audio_decode_vorbis::*;
use std::io::Cursor;

fn options() -> SeekOptions {
    SeekOptions {
        seek_behavior: SeekBehavior::Libvorbis,
        eos_behavior: EosBehavior::Libvorbis,
        codebook_behavior: CodebookBehavior::Libvorbis,
        residue_behavior: ResidueBehavior::Libvorbis,
        residue2_layout: Residue2Layout::Libvorbis,
        floor0_behavior: Floor0Behavior::Libvorbis,
        floor1_behavior: Floor1Behavior::Libvorbis,
        allow_empty_codebooks: true,
        allow_nonfinite: true,
        allow_zero_span_granules: true,
        ..Default::default()
    }
}
fn u64_in(b: &mut &[u8]) -> u64 {
    u64::from_le_bytes(support::take(b, 8).try_into().unwrap())
}
fn compare(bytes: &[u8], requests: &[u8], response: &[u8], label: &str) {
    let mut reader = OggSeekReader::with_options(Cursor::new(bytes), options()).unwrap();
    let mut q = requests;
    let mut r = response;
    let links = support::u32_in(&mut r) as usize;
    assert_eq!(links, reader.index().streams().len());
    let mut streams = Vec::new();
    let mut base = 0;
    for stream in reader.index().streams() {
        assert_eq!(support::u32_in(&mut r), stream.info.id.serial);
        assert_eq!(
            support::u32_in(&mut r),
            stream.info.identification.channels as u32
        );
        assert_eq!(
            support::u32_in(&mut r),
            stream.info.identification.sample_rate
        );
        assert_eq!(u64_in(&mut r), stream.end.frames);
        streams.push((
            stream.info.id,
            base,
            stream.end.frames,
            stream.info.identification.channels as usize,
        ));
        base += stream.end.frames;
    }
    let count = u64_in(&mut q);
    // Match the established seeking suite: aggregate all returned samples for
    // this stream/case. A one-frame crop must not replace the source block's
    // normalization with a near-zero sample. Raw tests independently enforce
    // the original bounds on every complete overlap block.
    let mut total_difference = support::Difference::default();
    for index in 0..count {
        let target = u64_in(&mut q);
        let wanted = u64_in(&mut q) as usize;
        let link = streams.iter().rposition(|s| target >= s.1).unwrap();
        let (id, base, _, channels) = streams[link];
        let status = support::u32_in(&mut r) as i32;
        assert_eq!(status, 0, "{label}/{index} target {target}");
        let tell = u64_in(&mut r);
        let mut expected = vec![Vec::new(); channels];
        loop {
            let frames = support::u32_in(&mut r) as usize;
            if frames == 0 {
                break;
            }
            assert_ne!(frames, u32::MAX as usize);
            assert_eq!(support::u32_in(&mut r) as usize, link);
            for plane in &mut expected {
                for _ in 0..frames {
                    plane.push(f32::from_bits(support::u32_in(&mut r)));
                }
            }
        }
        reader
            .seek(id, target - base)
            .unwrap_or_else(|e| panic!("{label}/{index} target {target}: {e:?}"));
        assert_eq!(
            reader.position() + base,
            tell,
            "{label}/{index} target {target}"
        );
        let mut actual = vec![Vec::new(); channels];
        while actual[0].len() < wanted {
            let progress = reader
                .read_packet(|e| {
                    if let OggEvent::Pcm(b) = e {
                        let keep = b.pcm.frames.min(wanted - actual[0].len());
                        for (p, q) in actual.iter_mut().zip(b.pcm.planes()) {
                            p.extend(&q[..keep]);
                        }
                    }
                })
                .unwrap_or_else(|e| panic!("{label}/{index} target {target}, read: {e:?}"));
            if !progress {
                break;
            }
        }
        let mut difference = support::Difference::default();
        for (a, b) in actual.iter().zip(&expected) {
            assert_eq!(a.len(), b.len(), "{label}/{index} target {target}, counts");
            for (&a, &b) in a.iter().zip(b) {
                difference.add(a, b);
            }
        }
        total_difference.merge(&difference);
    }
    assert!(q.is_empty());
    assert!(r.is_empty());
    total_difference.check(label);
}

#[test]
fn captured_eos_and_broadcast_seeks_match_reference_positions_counts_and_pcm() {
    for (label, bytes, requests, response) in [
        (
            "eos-115",
            include_bytes!("../validation/2026-10-27-synthesis-state-capture-eos-115-input.ogg")
                .as_slice(),
            include_bytes!("../validation/2026-10-27-synthesis-state-capture-eos-115-requests.bin")
                .as_slice(),
            include_bytes!("../validation/2026-10-27-synthesis-state-capture-eos-115-file.out")
                .as_slice(),
        ),
        (
            "origin-900",
            include_bytes!("../validation/2026-10-27-synthesis-state-capture-origin-900-input.ogg")
                .as_slice(),
            include_bytes!(
                "../validation/2026-10-27-synthesis-state-capture-origin-900-requests.bin"
            )
            .as_slice(),
            include_bytes!("../validation/2026-10-27-synthesis-state-capture-origin-900-file.out")
                .as_slice(),
        ),
        (
            "truncated-first-long",
            include_bytes!("fixtures/truncated-first-long-v1.ogg").as_slice(),
            include_bytes!("fixtures/truncated-first-long-v1.bin").as_slice(),
            include_bytes!("fixtures/truncated-first-long-v1.out").as_slice(),
        ),
    ] {
        compare(bytes, requests, response, label);
    }
}

fn oracle(bytes: &[u8], targets: &[(u64, usize)]) -> (Vec<u8>, Vec<u8>) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "deadlib-reference-seek-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let input = base.with_extension("ogg");
    let requests = base.with_extension("bin");
    let mut wire = (targets.len() as u64).to_le_bytes().to_vec();
    for &(target, length) in targets {
        wire.extend(target.to_le_bytes());
        wire.extend((length as u64).to_le_bytes());
    }
    std::fs::write(&input, bytes).unwrap();
    std::fs::write(&requests, &wire).unwrap();
    let result = std::process::Command::new(
        std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").expect("set oracle"),
    )
    .arg("oggseekstates")
    .arg(&input)
    .arg(&requests)
    .output()
    .unwrap();
    std::fs::remove_file(input).unwrap();
    std::fs::remove_file(requests).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    (wire, result.stdout)
}

#[test]
#[ignore = "requires independent libvorbisfile seek oracle"]
fn reference_seeks_cover_spanning_packets_large_files_and_repeated_partial_reads() {
    for bytes in [
        include_bytes!("fixtures/stereo.ogg").as_slice(),
        include_bytes!("fixtures/mono.ogg").as_slice(),
        include_bytes!("fixtures/surround.ogg").as_slice(),
        include_bytes!("fixtures/spanning-prime-v1.ogg").as_slice(),
    ] {
        check_file(bytes, "fixture");
    }
    let mut chain = include_bytes!("fixtures/stereo.ogg").to_vec();
    chain.extend(include_bytes!("fixtures/mono.ogg"));
    chain.extend(include_bytes!("fixtures/surround.ogg"));
    check_file(&chain, "mixed-rate-chain");
    // Trailing packet padding is ignored by Vorbis. Make a legal, non-silent
    // page-spanning priming packet small enough for the bounded fuzz harness.
    let packets = support::packets(include_bytes!("fixtures/stereo.ogg"));
    let mut writer = deadlib_audio_decode_ogg::PacketWriter::new(Vec::new(), 519);
    for header in &packets[..3] {
        writer.write_packet(header, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    let mut decoder = support::decoder(&packets);
    for (i, packet) in packets[3..].iter().enumerate() {
        let block = decoder.decode(packet).unwrap().unwrap();
        let last = i + 4 == packets.len();
        let mut padded = packet.clone();
        if i == 0 {
            padded.resize(70000, 0);
        }
        writer
            .write_packet(
                &padded,
                if last {
                    3840
                } else {
                    block.start_frame + block.frames as u64
                },
                last,
            )
            .unwrap();
        writer.flush_page().unwrap();
    }
    let bytes = writer.into_inner();
    assert!(bytes.len() < 131072);
    check_file(&bytes, "padded-prime");
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_REFERENCE_SEEK_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("padded-prime.ogg"), bytes).unwrap();
    }
}

fn check_file(bytes: &[u8], label: &str) {
    check_file_capture(bytes, label, false);
}
fn check_file_capture(bytes: &[u8], label: &str, capture: bool) {
    let reader = OggSeekReader::with_options(Cursor::new(bytes), options()).unwrap();
    let mut requests = Vec::new();
    let mut base = 0;
    for stream in reader.index().streams() {
        let frames = stream.end.frames;
        for target in [
            frames / 2,
            0,
            frames,
            1.min(frames),
            frames.saturating_sub(1),
            frames / 3,
            0,
            frames / 2,
        ] {
            requests.push((base + target, (frames - target).min(8193) as usize));
            requests.push((base + target, (frames - target).min(1) as usize));
        }
        base += frames;
    }
    // Return to earlier links after other setups have replaced synthesis state.
    base = 0;
    for stream in reader.index().streams() {
        requests.push((base, stream.end.frames.min(4097) as usize));
        base += stream.end.frames;
    }
    let (wire, output) = oracle(bytes, &requests);
    compare(bytes, &wire, &output, label);
    if capture && let Some(root) = std::env::var_os("DEADLIB_VORBIS_REFERENCE_SEEK_INPUTS") {
        let root = std::path::Path::new(&root);
        std::fs::create_dir_all(root).unwrap();
        for (extension, data) in [
            ("ogg", bytes),
            ("bin", wire.as_slice()),
            ("out", output.as_slice()),
        ] {
            std::fs::write(root.join(format!("truncated-first-long.{extension}")), data).unwrap();
        }
    }
}

#[test]
#[ignore = "requires repository asset corpus and independent libvorbisfile oracle"]
fn reference_seeks_match_asset_corpus() {
    let root =
        std::path::PathBuf::from(std::env::var_os("DEADLIB_VORBIS_CORPUS").expect("set corpus"));
    let paths = support::paths(&root);
    assert!(!paths.is_empty());
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        check_file(&bytes, &path.to_string_lossy());
    }
}

#[test]
#[ignore = "requires pinned upstream coded grid and independent libvorbisfile oracle"]
fn reference_seeks_match_upstream_rate_channel_quality_grid() {
    let root = std::path::PathBuf::from(
        std::env::var_os("DEADLIB_VORBIS_UPSTREAM_CORPUS").expect("set upstream corpus"),
    );
    let paths = support::paths(&root);
    assert_eq!(paths.len(), 528);
    for path in paths {
        let bytes = std::fs::read(&path).unwrap();
        check_file(&bytes, &path.to_string_lossy());
    }
}

#[test]
#[ignore = "requires independent libvorbisfile seek oracle"]
fn reference_seeks_match_mode_tables_residues_submaps_and_truncated_preroll() {
    for modes in [1, 2, 3, 5, 17, 33, 63, 64] {
        for first_long in [false, true] {
            for kind in 0..=2 {
                for empty_first_submap in [false, true] {
                    for first_truncated in [false, true] {
                        let config = synthetic::Config {
                            modes,
                            first_long,
                            kind,
                            empty_first_submap,
                            ..Default::default()
                        };
                        let headers = config.headers();
                        let mut decoder = support::decoder(&headers);
                        let mut writer =
                            deadlib_audio_decode_ogg::PacketWriter::new(Vec::new(), 779);
                        for header in &headers {
                            writer.write_packet(header, 0, false).unwrap();
                        }
                        writer.flush_page().unwrap();
                        let mut end = 0;
                        let mut packets = Vec::new();
                        for i in 0..64 {
                            let mode = (i % modes.min(2) as u32) as u8;
                            let mut packet = config.audio(mode, 3, i, None);
                            if (first_truncated && i < 2)
                                || (11..=12).contains(&i)
                                || (31..=32).contains(&i)
                            {
                                packet.truncate(1);
                            }
                            if let Some(block) = decoder.decode(&packet).unwrap() {
                                end = block.start_frame + block.frames as u64;
                            }
                            writer.write_packet(&packet, end, i == 63).unwrap();
                            packets.push(packet);
                        }
                        let label =
                            format!("synthetic/{config:?}/first-truncated-{first_truncated}");
                        let cases = [raw_cases::Case {
                            name: label.clone(),
                            packets,
                        }];
                        let reference =
                            raw_cases::reference("reference-seek-synthetic", &headers, &cases);
                        raw_cases::report(&label, raw_cases::compare(&headers, &cases, &reference));
                        let capture = modes == 33
                            && first_long
                            && kind == 0
                            && !empty_first_submap
                            && first_truncated;
                        check_file_capture(&writer.into_inner(), &label, capture);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires hash-pinned historical/song corpus and independent libvorbisfile oracle"]
fn reference_seeks_match_historical_and_shipped_songs() {
    let root = std::path::PathBuf::from(
        std::env::var_os("DEADLIB_VORBIS_EXTERNAL_CORPUS").expect("set external corpus"),
    );
    for name in [
        "floor0/vorbis_floor_type_0.ogg",
        "legacy/Lumme-Badloop.ogg",
        "itgmania/goin-under.ogg",
        "itgmania/mecha-tribe-assault.ogg",
        "ffmpeg/coyote.ogg",
        "ffmpeg/test6.ogg",
        "ffmpeg/1sec.ogg",
    ] {
        let bytes = std::fs::read(root.join(name)).unwrap();
        check_file(&bytes, name);
    }
}

#[test]
fn reference_seek_budgets_invalid_arguments_and_io_recovery() {
    use std::{
        cell::Cell,
        io::{Read, Seek, SeekFrom},
        rc::Rc,
    };
    struct Source {
        data: Cursor<&'static [u8]>,
        fail: Rc<Cell<bool>>,
    }
    impl Read for Source {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = out.len().min(17);
            self.data.read(&mut out[..n])
        }
    }
    impl Seek for Source {
        fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
            if self.fail.get() {
                return Err(std::io::Error::other("injected seek failure"));
            }
            self.data.seek(from)
        }
    }
    const BYTES: &[u8] = include_bytes!("fixtures/stereo.ogg");
    let fail = Rc::new(Cell::new(false));
    let mut reader = OggSeekReader::with_options(
        Source {
            data: Cursor::new(BYTES),
            fail: fail.clone(),
        },
        options(),
    )
    .unwrap();
    let id = reader.selected().info.id;
    let frames = reader.selected().end.frames;
    let bytes = reader.decoder_storage_bytes();
    assert!(matches!(
        OggSeekReader::with_options(
            Cursor::new(BYTES),
            SeekOptions {
                max_stream_memory: bytes - 1,
                ..options()
            }
        ),
        Err(Error::LimitExceeded("selected stream memory"))
    ));
    assert!(matches!(
        OggSeekReader::with_options(
            Cursor::new(BYTES),
            SeekOptions {
                trim_to_final_granule: true,
                ..options()
            }
        ),
        Err(Error::Invalid(_))
    ));
    reader.seek(id, frames / 2).unwrap();
    let position = reader.position();
    assert!(reader.seek(id, frames + 1).is_err());
    assert!(
        reader
            .seek(
                StreamId {
                    serial: id.serial.wrapping_add(1),
                    chain: id.chain
                },
                0
            )
            .is_err()
    );
    assert_eq!(reader.position(), position);
    let mut before = Vec::new();
    reader
        .read_packet(|e| {
            if let OggEvent::Pcm(b) = e {
                before.extend(b.pcm.plane(0).unwrap().iter().map(|x| x.to_bits()));
            }
        })
        .unwrap();
    fail.set(true);
    assert!(reader.seek(id, frames / 2).is_err());
    assert_eq!(reader.read_next(|_| {}), Err(Error::Failed));
    fail.set(false);
    reader.seek(id, frames / 2).unwrap();
    let mut after = Vec::new();
    reader
        .read_next(|e| {
            if let OggEvent::Pcm(b) = e {
                after.extend(b.pcm.plane(0).unwrap().iter().map(|x| x.to_bits()));
            }
        })
        .unwrap();
    assert!(!before.is_empty());
    assert_eq!(before, after);
}
