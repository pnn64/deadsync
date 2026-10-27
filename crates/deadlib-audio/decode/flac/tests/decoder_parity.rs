//! Exact PCM comparisons with independent public C decoder APIs.
use deadlib_audio_decode_flac::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
#[path = "support/decoder_vectors.rs"]
mod vectors;

#[derive(Debug, PartialEq)]
struct ReferenceFrame {
    rate: u32,
    bits: u8,
    planes: Vec<Vec<i32>>,
}
fn word(data: &mut &[u8]) -> u32 {
    let value = u32::from_le_bytes(data[..4].try_into().unwrap());
    *data = &data[4..];
    value
}
fn parse(data: &[u8]) -> Vec<ReferenceFrame> {
    let mut data = data;
    let mut frames = Vec::new();
    while !data.is_empty() {
        let rate = word(&mut data);
        let channels = word(&mut data);
        let bits = word(&mut data) as u8;
        let n = word(&mut data) as usize;
        assert!((1..=8).contains(&channels) && (1..=65535).contains(&n));
        let planes = (0..channels)
            .map(|_| (0..n).map(|_| word(&mut data) as i32).collect())
            .collect();
        frames.push(ReferenceFrame { rate, bits, planes });
    }
    frames
}
fn ffmpeg(path: &Path) -> Output {
    ffmpeg_command().arg(oracle_path(path)).output().unwrap()
}
fn oracle_path(path: &Path) -> PathBuf {
    if std::env::var_os("DEADLIB_FLAC_FFMPEG_WSL").is_some() {
        let path = fs::canonicalize(path)
            .unwrap()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\\', "/");
        assert!(
            path.as_bytes().get(1) == Some(&b':'),
            "WSL oracle expects an absolute Windows drive path"
        );
        PathBuf::from(format!(
            "/mnt/{}{}",
            path[..1].to_ascii_lowercase(),
            &path[2..]
        ))
    } else {
        path.to_path_buf()
    }
}
fn ffmpeg_command() -> Command {
    let oracle =
        std::env::var_os("DEADLIB_FLAC_FFMPEG_ORACLE").expect("set DEADLIB_FLAC_FFMPEG_ORACLE");
    if let Some(distro) = std::env::var_os("DEADLIB_FLAC_FFMPEG_WSL") {
        let mut command = Command::new("wsl");
        command.arg("-d").arg(distro).arg("--").arg(oracle);
        command
    } else {
        Command::new(oracle)
    }
}
fn verify_ffmpeg_version() {
    let output = ffmpeg_command().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "9.0.2");
}

#[test]
fn explicit_wire_families_decode_exactly() {
    let cases = vectors::cases();
    assert_eq!(cases.len(), 1279);
    for case in cases {
        let mut reader = Reader::new(case.data.as_slice()).unwrap();
        let frame = reader.read_next().unwrap().unwrap();
        for (a, b) in frame.pcm.planes().zip(&case.planes) {
            assert_eq!(a, b, "{}", case.name);
        }
        assert!(reader.read_next().unwrap().is_none());
        assert_eq!(reader.integrity(), Integrity::Verified);
    }
}

#[test]
#[ignore = "requires libFLAC and batch-enabled FFmpeg C oracles; see PARITY.md"]
fn explicit_wire_families_match_independent_decoders() {
    compare_cases(vectors::cases(), "wire-parity", "DEADLIB_FLAC_WIRE_REPORT");
}
#[test]
#[ignore = "requires retained raw-frame fuzz corpus and C oracles; see PARITY.md"]
fn accepted_fuzz_frames_match_independent_decoders() {
    let corpus = PathBuf::from(
        std::env::var_os("DEADLIB_FLAC_RAW_FUZZ_CORPUS").expect("set DEADLIB_FLAC_RAW_FUZZ_CORPUS"),
    );
    let mut paths: Vec<_> = fs::read_dir(corpus)
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect();
    paths.sort();
    let mut seen = std::collections::BTreeSet::new();
    let mut cases = Vec::new();
    for path in paths {
        if let Some(case) = vectors::fuzz_case(
            path.file_name().unwrap().to_string_lossy().into_owned(),
            &fs::read(&path).unwrap(),
        ) {
            // A zero sample rate denotes non-audio data in RFC 9639. FFmpeg's
            // audio demux API cannot represent that case; native tests cover it.
            if Metadata::parse(&case.data, Limits::default())
                .unwrap()
                .stream_info
                .sample_rate
                == 0
            {
                continue;
            }
            if seen.insert(case.data.clone()) {
                cases.push(case);
            }
        }
    }
    assert!(
        cases.len() >= 500,
        "unexpectedly small accepted corpus: {}",
        cases.len()
    );
    eprintln!("{} unique accepted frame mutations", cases.len());
    compare_cases(cases, "fuzz-frame-parity", "DEADLIB_FLAC_FUZZ_FRAME_REPORT");
}
fn compare_cases(cases: Vec<vectors::Case>, directory: &str, report_env: &str) {
    verify_ffmpeg_version();
    let root = PathBuf::from(
        std::env::var_os("DEADLIB_FLAC_GENERATED").expect("set DEADLIB_FLAC_GENERATED"),
    )
    .join(directory);
    fs::create_dir_all(&root).unwrap();
    let mut paths = String::new();
    let mut report = String::from("case\tbits\tchannels\tsamples\tlibFLAC\tFFmpeg\n");
    for (i, case) in cases.iter().enumerate() {
        let path = root.join(format!("{i}.flac"));
        fs::write(&path, &case.data).unwrap();
        let frames = libflac(&path);
        assert_eq!(frames.len(), 1, "{}", case.name);
        let info = Metadata::parse(&case.data, Limits::default())
            .unwrap()
            .stream_info;
        assert_eq!(frames[0].bits, info.bits_per_sample);
        assert_eq!(frames[0].rate, info.sample_rate);
        assert_eq!(frames[0].planes, case.planes, "libFLAC {}", case.name);
        paths.push_str(&oracle_path(&path).to_string_lossy());
        paths.push('\n');
    }
    let list = root.join("files.txt");
    fs::write(&list, paths).unwrap();
    let output = ffmpeg_command()
        .arg("--batch")
        .arg(oracle_path(&list))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut bytes = output.stdout.as_slice();
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(word(&mut bytes), u32::MAX);
        assert_eq!(word(&mut bytes), i as u32);
        let rate = word(&mut bytes);
        let channels = word(&mut bytes) as usize;
        let bits = word(&mut bytes);
        let n = word(&mut bytes) as usize;
        assert_eq!(
            rate,
            Metadata::parse(&case.data, Limits::default())
                .unwrap()
                .stream_info
                .sample_rate
        );
        assert_eq!(
            bits,
            Metadata::parse(&case.data, Limits::default())
                .unwrap()
                .stream_info
                .bits_per_sample as u32
        );
        assert_eq!(channels, case.planes.len());
        assert_eq!(n, case.planes[0].len());
        for plane in &case.planes {
            for &sample in plane {
                assert_eq!(word(&mut bytes) as i32, sample, "FFmpeg {}", case.name);
            }
        }
        assert_eq!(word(&mut bytes), u32::MAX - 1);
        assert_eq!(word(&mut bytes), 0);
        report.push_str(&format!(
            "{}\t{bits}\t{channels}\t{n}\texact\texact\n",
            case.name
        ));
    }
    assert!(bytes.is_empty());
    fs::write(
        std::env::var_os(report_env).expect("set parity report output"),
        report,
    )
    .unwrap();
}
fn libflac(path: &Path) -> Vec<ReferenceFrame> {
    let output =
        Command::new(std::env::var_os("DEADLIB_FLAC_ORACLE").expect("set DEADLIB_FLAC_ORACLE"))
            .arg(path)
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    parse(&output.stdout)
}
fn compare_ffmpeg(output: &Output, expected: &[ReferenceFrame], path: &Path) -> &'static str {
    let decoded = parse(&output.stdout);
    if output.status.success() {
        assert_eq!(decoded, expected, "{}", path.display());
        "exact"
    } else {
        assert_eq!(path.file_name().unwrap(), "04 - changing bitdepth.flac");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("switching bps mid-stream is not supported"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            decoded,
            expected[..decoded.len()],
            "reference accepted prefix {}",
            path.display()
        );
        "reference rejects mid-stream bit-depth changes"
    }
}

#[test]
#[ignore = "requires official corpus, libFLAC and FFmpeg C oracles; see PARITY.md"]
fn official_ffmpeg_exact_pcm_and_ogg_remux() {
    verify_ffmpeg_version();
    let corpus =
        PathBuf::from(std::env::var_os("DEADLIB_FLAC_CORPUS").expect("set DEADLIB_FLAC_CORPUS"));
    let generated = PathBuf::from(
        std::env::var_os("DEADLIB_FLAC_GENERATED").expect("set DEADLIB_FLAC_GENERATED"),
    )
    .join("decoder-parity");
    fs::create_dir_all(&generated).unwrap();
    let mut report = String::from("group\tfile\tframes\tsamples\tnative_FFmpeg\tOgg_FFmpeg\n");
    let mut count = 0;
    for group in ["subset", "uncommon"] {
        let mut paths: Vec<_> = fs::read_dir(corpus.join(group))
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "flac"))
            .collect();
        paths.sort();
        for path in paths {
            eprintln!("FFmpeg comparison {}", path.display());
            let expected = libflac(&path);
            let output = ffmpeg(&path);
            let native_result = compare_ffmpeg(&output, &expected, &path);
            let data = fs::read(&path).unwrap();
            let mut ogg = "headerless (raw native gate)";
            if data.starts_with(b"fLaC") {
                let meta = Metadata::parse(
                    &data,
                    Limits {
                        metadata_bytes: 128 << 20,
                        ..Default::default()
                    },
                )
                .unwrap();
                let mut muxer = OggMuxer::new(
                    Vec::new(),
                    meta.stream_info,
                    &meta.blocks[1..],
                    OggWriteOptions::default(),
                )
                .unwrap();
                let mut decoder = FrameDecoder::new(Some(meta.stream_info)).unwrap();
                let mut offset = meta.audio_offset;
                while offset < data.len() {
                    let frame = decoder.decode(&data[offset..]).unwrap();
                    muxer
                        .write_frame(&data[offset..offset + frame.bytes])
                        .unwrap();
                    offset += frame.bytes;
                }
                let target = generated.join("remux.oga");
                fs::write(&target, muxer.finish().unwrap().output).unwrap();
                let output = ffmpeg(&target);
                ogg = compare_ffmpeg(&output, &expected, &path);
            }
            let samples: usize = expected.iter().map(|f| f.planes[0].len()).sum();
            report.push_str(&format!(
                "{group}\t{}\t{}\t{samples}\t{native_result}\t{ogg}\n",
                path.file_name().unwrap().to_string_lossy(),
                expected.len()
            ));
            count += 1;
        }
    }
    fs::write(
        std::env::var_os("DEADLIB_FLAC_FFMPEG_REPORT").expect("set DEADLIB_FLAC_FFMPEG_REPORT"),
        report,
    )
    .unwrap();
    assert_eq!(count, 75);
}
