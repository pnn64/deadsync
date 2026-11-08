mod support;
use deadlib_audio_decode_vorbis as codec;
#[path = "support/eos_vectors.rs"]
#[allow(dead_code)]
mod vectors;
use deadlib_audio_decode_ogg::PacketWriter;
use deadlib_audio_decode_vorbis::*;
use std::io::Cursor;
use vectors::Config;
const STEREO: &[u8] = include_bytes!("fixtures/stereo.ogg");
fn repack(bytes: &[u8], eos: u64) -> Vec<u8> {
    let packets = support::packets(bytes);
    let mut d = support::decoder(&packets);
    let mut writer = PacketWriter::new(Vec::new(), 193);
    for h in &packets[..3] {
        writer.write_packet(h, 0, false).unwrap();
    }
    writer.flush_page().unwrap();
    for (i, p) in packets[3..].iter().enumerate() {
        let b = d.decode(p).unwrap().unwrap();
        let last = i + 4 == packets.len();
        writer
            .write_packet(
                p,
                if last {
                    eos
                } else {
                    b.start_frame + b.frames as u64
                },
                last,
            )
            .unwrap();
        writer.flush_page().unwrap();
    }
    writer.into_inner()
}
#[test]
#[ignore = "requires external libvorbisfile oracle"]
fn reference_eos_clamping_witness() {
    let bytes = repack(STEREO, 1);
    let output = support::file_oracle("oggpcm", &bytes);
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_EOS_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(std::path::Path::new(&root).join("eos-baseline.ogg"), &bytes).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join("eos-baseline.out"),
            &output,
        )
        .unwrap();
    }
    let mut wire = output.as_slice();
    assert_eq!(support::u32_in(&mut wire), 1);
    assert_eq!(support::u32_in(&mut wire), 193);
    let channels = support::u32_in(&mut wire) as usize;
    assert_eq!(support::u32_in(&mut wire), 48000);
    let declared = u64::from_le_bytes(support::take(&mut wire, 8).try_into().unwrap());
    assert_eq!(declared, 1);
    let mut actual = 0;
    while !wire.is_empty() {
        assert_eq!(support::u32_in(&mut wire), 0);
        let n = support::u32_in(&mut wire) as usize;
        actual += n;
        support::take(&mut wire, n * channels * 4);
    }
    assert_eq!(actual, 3648);
    let mut frames = 0;
    let result = decode_reader(
        bytes.as_slice(),
        OggOptions {
            eos_behavior: EosBehavior::Libvorbis,
            ..OggOptions::default()
        },
        |e| {
            if let OggEvent::Pcm(b) = e {
                frames += b.pcm.frames;
            }
        },
    );
    assert!(
        result.is_ok(),
        "reference emits {actual} frames with declared {declared}; native emitted {frames} then {result:?}"
    );
    assert_eq!(frames, actual);
}

#[derive(Default)]
struct Output {
    planes: Vec<Vec<f32>>,
    end: Option<StreamEnd>,
}
fn options() -> OggOptions {
    OggOptions {
        eos_behavior: EosBehavior::Libvorbis,
        ..OggOptions::default()
    }
}
fn collect(out: &mut Output, e: OggEvent<'_>) {
    match e {
        OggEvent::StreamStart(info) => {
            out.planes = vec![Vec::new(); info.identification.channels as usize]
        }
        OggEvent::Pcm(b) => {
            assert_eq!(b.start_frame as usize, out.planes[0].len());
            for (p, q) in out.planes.iter_mut().zip(b.pcm.planes()) {
                p.extend(q);
            }
        }
        OggEvent::StreamEnd(e) => {
            assert_eq!(e.emitted_frames as usize, out.planes[0].len());
            out.end = Some(e);
        }
        _ => {}
    }
}
fn native(bytes: &[u8]) -> Output {
    let mut out = Output::default();
    decode_reader(bytes, options(), |e| collect(&mut out, e)).unwrap();
    out
}
fn reference_stream(bytes: &[u8], label: &str) -> Output {
    let raw = support::file_oracle("oggstream", bytes);
    save(label, bytes, &raw);
    parse_stream(&raw)
}
fn parse_stream(mut w: &[u8]) -> Output {
    let ch = support::u32_in(&mut w) as usize;
    let rate = support::u32_in(&mut w);
    let mut out = Output {
        planes: vec![Vec::new(); ch],
        end: None,
    };
    loop {
        let n = support::u32_in(&mut w) as usize;
        if n == 0 {
            break;
        }
        assert_eq!(support::u32_in(&mut w), 0);
        assert_eq!(support::u32_in(&mut w) as usize, ch);
        assert_eq!(support::u32_in(&mut w), rate);
        for p in &mut out.planes {
            for _ in 0..n {
                p.push(f32::from_bits(support::u32_in(&mut w)));
            }
        }
    }
    assert!(w.is_empty());
    out
}

fn same_bits(a: &Output, b: &Output) {
    assert_eq!(a.end, b.end);
    assert_eq!(a.planes.len(), b.planes.len());
    for (a, b) in a.planes.iter().zip(&b.planes) {
        assert_eq!(
            a.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            b.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
        );
    }
}
fn replay(bytes: &[u8], expected: &Output) {
    let mut pull = OggReader::with_options(bytes, options());
    let mut out = Output::default();
    while pull.read_next(|e| collect(&mut out, e)).unwrap() {}
    same_bits(&out, expected);
    for chunk in [1, 17, bytes.len().max(1)] {
        let mut d = OggStreamDecoder::new(options());
        for _ in 0..2 {
            let mut out = Output::default();
            for b in bytes.chunks(chunk) {
                let mut at = 0;
                while at < b.len() {
                    let n = d.push(&b[at..], |e| collect(&mut out, e)).unwrap();
                    assert!(n > 0);
                    at += n;
                }
            }
            d.finish().unwrap();
            same_bits(&out, expected);
            d.reset();
        }
    }
}
#[test]
fn eos_stream_pull_push_and_reset_preserve_counts_and_bits() {
    for c in configs() {
        let bytes = c.bytes();
        let expected = native(&bytes);
        replay(&bytes, &expected);
    }
}
#[test]
fn eos_reference_is_explicit_and_separate_from_indexed_endpoint_trimming() {
    let bytes = repack(STEREO, 1);
    assert!(decode_reader(bytes.as_slice(), OggOptions::default(), |_| {}).is_err());
    let mut indexed = OggSeekReader::with_options(
        Cursor::new(&bytes),
        SeekOptions {
            trim_to_final_granule: true,
            ..SeekOptions::default()
        },
    )
    .unwrap();
    let mut n = 0;
    while indexed
        .read_next(|e| {
            if let OggEvent::Pcm(b) = e {
                n += b.pcm.frames;
            }
        })
        .unwrap()
    {}
    assert_eq!(n, 1);
    assert_eq!(native(&bytes).end.unwrap().emitted_frames, 3648);
    assert!(matches!(
        VorbisIndex::read(
            &mut Cursor::new(&bytes),
            SeekOptions {
                trim_to_final_granule: true,
                eos_behavior: EosBehavior::Libvorbis,
                ..SeekOptions::default()
            }
        ),
        Err(Error::Invalid("conflicting Vorbis end-trim policies"))
    ));
    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert!(matches!(
        decode_reader(corrupt.as_slice(), options(), |_| {}),
        Err(Error::Container(_))
    ));
}
fn golden_configs() -> Vec<Config> {
    let all = configs();
    [
        0, 45, 90, 91, 94, 115, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 198, 225,
        270, 314, 405, 495, 810, 1125,
    ]
    .into_iter()
    .map(|i| all[i])
    .collect()
}
fn put_blob(out: &mut Vec<u8>, b: &[u8]) {
    out.extend((b.len() as u64).to_le_bytes());
    out.extend(b);
}
fn get_blob<'a>(w: &mut &'a [u8]) -> &'a [u8] {
    let n = u64::from_le_bytes(support::take(w, 8).try_into().unwrap()) as usize;
    support::take(w, n)
}
#[test]
#[ignore = "manual external-oracle golden regeneration"]
fn regenerate_eos_stream_golden() {
    let all = golden_configs();
    let mut out = b"DEOSOG01".to_vec();
    out.extend((all.len() as u32).to_le_bytes());
    for (i, c) in all.into_iter().enumerate() {
        let b = c.bytes();
        let pcm = support::file_oracle("oggstream", &b);
        let seek = seek_reference(&b, &[0], &format!("eos-golden-{i}"));
        let declared = &seek[16..24];
        put_blob(&mut out, &b);
        put_blob(&mut out, &pcm);
        out.extend(declared);
    }
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/eos-stream-v1.libvorbis-pcm"),
        out,
    )
    .unwrap();
}
#[test]
fn eos_stream_pcm_and_declared_duration_golden() {
    let mut w = include_bytes!("fixtures/eos-stream-v1.libvorbis-pcm").as_slice();
    assert_eq!(support::take(&mut w, 8), b"DEOSOG01");
    let n = support::u32_in(&mut w);
    for i in 0..n {
        let bytes = get_blob(&mut w);
        let r = parse_stream(get_blob(&mut w));
        let declared = u64::from_le_bytes(support::take(&mut w, 8).try_into().unwrap());
        let a = native(bytes);
        assert_eq!(a.end.unwrap().frames, declared);
        compare_pcm(&format!("EOS stream golden {i}"), &a, &r);
        replay(bytes, &a);
        let end = a.end.unwrap();
        for packet_preroll in [false, true] {
            let mut reader = OggSeekReader::with_options(
                Cursor::new(bytes),
                SeekOptions {
                    eos_behavior: EosBehavior::Libvorbis,
                    packet_preroll,
                    ..SeekOptions::default()
                },
            )
            .unwrap();
            let id = reader.selected().info.id;
            for target in [
                end.frames,
                0,
                end.frames / 2,
                0,
                end.emitted_frames.min(end.frames),
                end.frames.saturating_sub(1),
                0,
            ] {
                reader.seek(id, target).unwrap();
                let position = reader.position();
                let mut actual = Output {
                    planes: vec![Vec::new(); a.planes.len()],
                    end: None,
                };
                while reader
                    .read_packet(|e| {
                        if let OggEvent::Pcm(b) = e {
                            assert_eq!(b.start_frame, position + actual.planes[0].len() as u64);
                            for (p, q) in actual.planes.iter_mut().zip(b.pcm.planes()) {
                                p.extend(q);
                            }
                        }
                    })
                    .unwrap()
                {}
                for (p, q) in actual.planes.iter().zip(&a.planes) {
                    assert_eq!(
                        p.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        q[(position as usize).min(q.len())..]
                            .iter()
                            .map(|x| x.to_bits())
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
    assert!(w.is_empty());
}
#[test]
#[ignore = "requires pinned historical corpus and libvorbisfile oracle"]
fn real_song_and_ffmpeg_eos_counts_match_reference_without_repair() {
    let root = std::path::PathBuf::from(
        std::env::var_os("DEADLIB_VORBIS_EXTERNAL_CORPUS").expect("set corpus"),
    );
    for (name, declared, emitted) in [
        ("itgmania/mecha-tribe-assault.ogg", 4630500, 4762304),
        ("ffmpeg/1sec.ogg", 44608, 44608),
    ] {
        // corpus.json owns the relative file name and pinned digest.
        let path = root.join(name);
        let bytes = std::fs::read(&path).unwrap();
        let a = native(&bytes);
        let r = reference_stream(&bytes, &format!("eos-real-{}", declared));
        let end = a.end.unwrap();
        assert_eq!((end.frames, end.emitted_frames), (declared, emitted));
        compare_pcm(name, &a, &r);
        let raw = seek_reference(&bytes, &[0], &format!("eos-real-metadata-{declared}"));
        assert_eq!(
            u64::from_le_bytes(raw[16..24].try_into().unwrap()),
            declared
        );
    }
}
fn configs() -> Vec<Config> {
    vectors::configurations()
}
fn compare_pcm(label: &str, a: &Output, b: &Output) {
    assert_eq!(a.planes.len(), b.planes.len());
    let mut d = support::Difference::default();
    for (p, q) in a.planes.iter().zip(&b.planes) {
        assert_eq!(p.len(), q.len(), "{label}");
        for (&x, &y) in p.iter().zip(q) {
            d.add(x, y);
        }
    }
    d.check_with_bounds(label, 3e-6, 5e-7);
}
fn save(label: &str, bytes: &[u8], raw: &[u8]) {
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_EOS_INPUTS") {
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            std::path::Path::new(&root).join(format!("{label}.ogg")),
            bytes,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&root).join(format!("{label}.out")),
            raw,
        )
        .unwrap();
    }
}
#[test]
#[ignore = "requires unseekable libvorbisfile oracle"]
fn bounded_eos_layouts_and_discarded_packets_match_reference() {
    let all = configs();
    for (i, c) in all.iter().copied().enumerate() {
        let b = c.bytes();
        let a = native(&b);
        let r = reference_stream(&b, &format!("eos-stream-{i}"));
        compare_pcm(&format!("eos-stream-{i}/{c:?}"), &a, &r);
    }
    eprintln!("EOS stream cases: {}", all.len());
}
fn seek_reference(bytes: &[u8], targets: &[u64], label: &str) -> Vec<u8> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "deadlib-eos-seek-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let p = base.with_extension("ogg");
    let q = base.with_extension("bin");
    let mut wire = (targets.len() as u64).to_le_bytes().to_vec();
    for &t in targets {
        wire.extend(t.to_le_bytes());
        wire.extend(100000u64.to_le_bytes());
    }
    std::fs::write(&p, bytes).unwrap();
    std::fs::write(&q, &wire).unwrap();
    let r = std::process::Command::new(std::env::var_os("DEADLIB_LIBVORBIS_ORACLE").unwrap())
        .arg("oggseekstates")
        .arg(&p)
        .arg(&q)
        .output()
        .unwrap();
    std::fs::remove_file(p).unwrap();
    std::fs::remove_file(q).unwrap();
    assert!(
        r.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    save(label, bytes, &r.stdout);
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_EOS_INPUTS") {
        std::fs::write(
            std::path::Path::new(&root).join(format!("{label}.bin")),
            wire,
        )
        .unwrap();
    }
    r.stdout
}
#[test]
#[ignore = "requires real libvorbisfile seek status oracle"]
fn bounded_eos_metadata_and_seek_difference_inventory() {
    let all = configs();
    let mut seeks = 0;
    let mut differences = Vec::new();
    for (i, c) in all.iter().copied().enumerate() {
        let bytes = c.bytes();
        let full = native(&bytes);
        let end = full.end.unwrap();
        let targets: std::collections::BTreeSet<_> = [
            0,
            1,
            end.frames / 2,
            end.frames.saturating_sub(1),
            end.frames,
            end.emitted_frames.saturating_sub(1),
            end.emitted_frames,
            end.emitted_frames + 1,
        ]
        .into_iter()
        .filter(|&x| x <= end.frames)
        .collect();
        let targets: Vec<_> = targets.into_iter().collect();
        let raw = seek_reference(&bytes, &targets, &format!("eos-seek-{i}"));
        let mut w = raw.as_slice();
        assert_eq!(support::u32_in(&mut w), 1);
        assert_eq!(support::u32_in(&mut w), 193 + c.fixture as u32);
        assert_eq!(support::u32_in(&mut w) as usize, full.planes.len());
        assert_eq!(
            support::u32_in(&mut w),
            Identification::parse(&vectors::packets(vectors::FILES[c.fixture])[0])
                .unwrap()
                .sample_rate
        );
        let declared = u64::from_le_bytes(support::take(&mut w, 8).try_into().unwrap());
        assert_eq!(declared, end.frames, "{c:?}");
        let mut expected = Vec::new();
        for &t in &targets {
            let status = support::u32_in(&mut w) as i32;
            let tell = u64::from_le_bytes(support::take(&mut w, 8).try_into().unwrap());
            let mut out = Output {
                planes: vec![Vec::new(); full.planes.len()],
                end: None,
            };
            loop {
                let n = support::u32_in(&mut w) as usize;
                if n == 0 {
                    break;
                }
                assert_ne!(n, u32::MAX as usize, "{c:?}, read error");
                assert_eq!(support::u32_in(&mut w), 0);
                for p in &mut out.planes {
                    for _ in 0..n {
                        p.push(f32::from_bits(support::u32_in(&mut w)));
                    }
                }
            }
            expected.push((t, status, tell, out));
        }
        assert!(w.is_empty());
        let mut reference_reader = OggSeekReader::with_options(
            Cursor::new(&bytes),
            SeekOptions {
                seek_behavior: SeekBehavior::Libvorbis,
                eos_behavior: EosBehavior::Libvorbis,
                ..SeekOptions::default()
            },
        )
        .unwrap();
        let reference_id = reference_reader.selected().info.id;
        for (target, status, tell, reference) in &expected {
            reference_reader
                .seek(reference_id, *target)
                .unwrap_or_else(|e| panic!("reference case {i}/{c:?}, target {target}: {e:?}"));
            assert_eq!(*status, 0);
            assert_eq!(
                reference_reader.position(),
                *tell,
                "reference case {i}/{c:?}, target {target}"
            );
            let mut actual = Output {
                planes: vec![Vec::new(); full.planes.len()],
                end: None,
            };
            while reference_reader
                .read_packet(|e| {
                    if let OggEvent::Pcm(b) = e {
                        for (p, q) in actual.planes.iter_mut().zip(b.pcm.planes()) {
                            p.extend(q);
                        }
                    }
                })
                .unwrap()
            {}
            compare_pcm(
                &format!("reference case {i}/{c:?}, target {target}"),
                &actual,
                reference,
            );
        }
        for packet_preroll in [false, true] {
            let mut native = OggSeekReader::with_options(
                Cursor::new(&bytes),
                SeekOptions {
                    eos_behavior: EosBehavior::Libvorbis,
                    packet_preroll,
                    ..SeekOptions::default()
                },
            )
            .unwrap();
            assert_eq!(native.selected().end, end);
            let id = native.selected().info.id;
            for (target, status, tell, reference) in &expected {
                let result = native.seek(id, *target);
                assert!(result.is_ok(), "{c:?}, target {target}: {result:?}");
                let position = native.position();
                let mut out = Output {
                    planes: vec![Vec::new(); full.planes.len()],
                    end: None,
                };
                let mut step = 0;
                while if step % 2 == 0 {
                    native.read_next(|e| {
                        if let OggEvent::Pcm(b) = e {
                            assert_eq!(b.start_frame, position + out.planes[0].len() as u64);
                            for (p, q) in out.planes.iter_mut().zip(b.pcm.planes()) {
                                p.extend(q);
                            }
                        }
                    })
                } else {
                    native.read_packet(|e| {
                        if let OggEvent::Pcm(b) = e {
                            assert_eq!(b.start_frame, position + out.planes[0].len() as u64);
                            for (p, q) in out.planes.iter_mut().zip(b.pcm.planes()) {
                                p.extend(q);
                            }
                        }
                    })
                }
                .unwrap_or_else(|e| {
                    panic!("{c:?}, target {target}, preroll {packet_preroll}: {e:?}")
                }) {
                    step += 1;
                }
                // Native seeks address the retained sequential PCM. Keep the
                // remaining vorbisfile positioning differences visible.
                for (a, b) in out.planes.iter().zip(&full.planes) {
                    assert_eq!(
                        a.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        b[(position as usize).min(b.len())..]
                            .iter()
                            .map(|x| x.to_bits())
                            .collect::<Vec<_>>(),
                        "{c:?}, target {target}, preroll {packet_preroll}"
                    );
                }
                if *status != 0
                    || position != *tell
                    || out.planes[0].len() != reference.planes[0].len()
                {
                    if !packet_preroll {
                        differences.push(format!(
                            "{i}\t{target}\t{status}\t{tell}\t{}\t{position}\t{}",
                            reference.planes[0].len(),
                            out.planes[0].len()
                        ));
                    }
                } else {
                    compare_pcm(
                        &format!("eos-seek-{i}/target-{target}/preroll-{packet_preroll}"),
                        &out,
                        reference,
                    );
                }
                seeks += 1;
            }
        }
    }
    eprintln!("EOS seek cases: {}, native seeks {seeks}", all.len());
    if let Some(root) = std::env::var_os("DEADLIB_VORBIS_EOS_INPUTS") {
        std::fs::write(
            std::path::Path::new(&root).join("known-seek-differences.tsv"),
            differences.join("\n") + "\n",
        )
        .unwrap();
    }
    eprintln!(
        "Unresolved vorbisfile seek-position/count differences: {}",
        differences.len()
    );
    assert_eq!(
        differences,
        include_str!("fixtures/eos-seek-differences-v1.tsv")
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
}
