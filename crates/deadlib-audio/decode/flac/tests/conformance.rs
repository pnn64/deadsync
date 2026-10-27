//! External corpus tests are explicit, reproducible gates, never silently skipped.
use deadlib_audio_decode_flac::*;
use std::{fs, io::Cursor, path::Path, process::Command};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_FLAC},
    packet::PacketRef,
    units::{Duration, Timestamp},
};

struct ReferenceFrame {
    start: u64,
    rate: u32,
    bits: u8,
    planes: Vec<Vec<i32>>,
}
impl ReferenceFrame {
    fn samples(&self) -> usize {
        self.planes[0].len()
    }
}
fn word(data: &mut &[u8]) -> u32 {
    let (a, b) = data.split_at(4);
    *data = b;
    u32::from_le_bytes(a.try_into().unwrap())
}
fn reference(path: &Path) -> Vec<ReferenceFrame> {
    let oracle = std::env::var_os("DEADLIB_FLAC_ORACLE").expect("set DEADLIB_FLAC_ORACLE");
    let out = Command::new(oracle).arg(path).output().unwrap();
    assert!(
        out.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let mut data = out.stdout.as_slice();
    let mut result = Vec::new();
    let mut start = 0;
    while !data.is_empty() {
        let rate = word(&mut data);
        let channels = word(&mut data);
        let bits = word(&mut data) as u8;
        let samples = word(&mut data) as usize;
        assert!((1..=8).contains(&channels));
        assert!((1..=65535).contains(&samples));
        let planes = (0..channels)
            .map(|_| (0..samples).map(|_| word(&mut data) as i32).collect())
            .collect();
        result.push(ReferenceFrame {
            start,
            rate,
            bits,
            planes,
        });
        start += samples as u64;
    }
    result
}
fn options() -> DecodeOptions {
    let mut result = DecodeOptions::default();
    result.limits.metadata_bytes = 128 << 20;
    result
}
fn compare(header: FrameHeader, pcm: Pcm<'_>, expected: &ReferenceFrame, skip: usize) {
    assert_eq!(header.sample_rate, expected.rate);
    assert_eq!(header.bits_per_sample, expected.bits);
    assert_eq!(pcm.channels() as usize, expected.planes.len());
    for (ch, plane) in pcm.planes().enumerate() {
        assert_eq!(plane, &expected.planes[ch][skip..]);
    }
}
fn symphonia(
    data: &[u8],
    info: StreamInfo,
    offset: usize,
    frames: &[ReferenceFrame],
) -> Result<(), String> {
    let mut params = AudioCodecParameters::new();
    params
        .for_codec(CODEC_ID_FLAC)
        .with_extra_data(info.encode().unwrap().to_vec().into_boxed_slice());
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut native = FrameDecoder::new(Some(info)).unwrap();
    let mut offset = offset;
    for expected in frames {
        let frame = native.decode(&data[offset..]).unwrap();
        let packet = PacketRef::new(
            0,
            Timestamp::ZERO,
            Duration::ZERO,
            &data[offset..offset + frame.bytes],
        );
        offset += frame.bytes;
        let audio = decoder.decode_ref(&packet).map_err(|e| e.to_string())?;
        let GenericAudioBufferRef::S32(audio) = audio else {
            return Err("unexpected reference PCM format".into());
        };
        if audio.frames() != expected.samples() {
            return Err("MISMATCH frame sample count".into());
        }
        for (ch, plane) in expected.planes.iter().enumerate() {
            let other = audio.plane(ch).ok_or("MISMATCH channel count")?;
            if let Some((sample, (&a, &b))) = plane
                .iter()
                .zip(other)
                .enumerate()
                .find(|(_, (a, b))| **a != **b >> (32 - expected.bits))
            {
                return Err(format!(
                    "MISMATCH channel={ch} sample={} assignment={} expected={a} actual={}",
                    expected.start + sample as u64,
                    frame.header.channel_assignment,
                    b >> (32 - expected.bits)
                ));
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires pinned official corpus and test-only libFLAC oracle; see COVERAGE.md"]
fn official_corpus_against_libflac_and_symphonia() {
    let root = std::env::var_os("DEADLIB_FLAC_CORPUS").expect("set DEADLIB_FLAC_CORPUS");
    let root = Path::new(&root);
    let mut report = String::from("group\tfile\tsamples\tlibFLAC\tSymphonia\tintegrity\tseeks\n");
    let mut count = 0;
    let mut symphonia_passes = 0;
    let mut mismatches = Vec::new();
    let mut rejections = Vec::new();
    for group in ["subset", "uncommon"] {
        let mut paths: Vec<_> = fs::read_dir(root.join(group))
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "flac"))
            .collect();
        paths.sort();
        assert!(!paths.is_empty());
        for path in paths {
            eprintln!("checking {}", path.display());
            let data = fs::read(&path).unwrap();
            let expected = reference(&path);
            assert!(!expected.is_empty());
            let total = expected.last().unwrap().start + expected.last().unwrap().samples() as u64;
            let info;
            let audio_offset;
            let integrity;
            let seeks;
            if data.starts_with(b"fLaC") {
                let mut reader = Reader::with_options(Cursor::new(&data), options()).unwrap();
                info = reader.stream_info();
                audio_offset = reader.metadata().unwrap().audio_offset;
                for frame in &expected {
                    let native = reader.read_next().unwrap().unwrap();
                    assert_eq!(native.start_sample, frame.start);
                    compare(native.header, native.pcm, frame, 0);
                }
                assert!(reader.read_next().unwrap().is_none());
                integrity = format!("{:?}", reader.integrity());
                let mut decoder = Decoder::with_options(&data, options()).unwrap();
                let mut positions = vec![total, 0, total - 1, total / 2, 1];
                let mut seed = 41u64;
                for _ in 0..16 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    positions.push(seed % total);
                }
                for sample in &positions {
                    decoder.seek(*sample).unwrap();
                    assert_eq!(decoder.position(), *sample);
                    if *sample == total {
                        assert!(decoder.read_next().unwrap().is_none());
                        continue;
                    }
                    let i = expected.partition_point(|f| f.start <= *sample) - 1;
                    let frame = &expected[i];
                    let native = decoder.read_next().unwrap().unwrap();
                    assert_eq!(native.start_sample, *sample);
                    compare(
                        native.header,
                        native.pcm,
                        frame,
                        (*sample - frame.start) as usize,
                    );
                }
                seeks = positions.len();
            } else {
                let mut decoder = FrameDecoder::new(None).unwrap();
                let (skip, first) = decoder.find_frame(&data).unwrap();
                audio_offset = skip;
                info = StreamInfo {
                    min_block_size: 16,
                    max_block_size: 65535,
                    min_frame_size: 0,
                    max_frame_size: 0,
                    sample_rate: first.header.sample_rate,
                    channels: first.header.channels,
                    bits_per_sample: first.header.bits_per_sample,
                    total_samples: 0,
                    md5: [0; 16],
                };
                let mut offset = skip;
                for frame in &expected {
                    let native = decoder.decode(&data[offset..]).unwrap();
                    compare(native.header, native.pcm, frame, 0);
                    offset += native.bytes;
                }
                assert_eq!(offset, data.len());
                integrity = "headerless CRCs".into();
                seeks = 0;
            }
            let sym = match symphonia(&data, info, audio_offset, &expected) {
                Ok(()) => {
                    symphonia_passes += 1;
                    "exact".to_string()
                }
                Err(e) => {
                    if e.starts_with("MISMATCH") {
                        mismatches.push(format!(
                            "{group}/{}: {e}",
                            path.file_name().unwrap().to_string_lossy()
                        ));
                    } else {
                        rejections.push(format!(
                            "{group}/{}: {e}",
                            path.file_name().unwrap().to_string_lossy()
                        ));
                    }
                    format!("reference difference: {e}")
                }
            };
            report.push_str(&format!(
                "{group}\t{}\t{total}\texact\t{sym}\t{integrity}\t{seeks}\n",
                path.file_name().unwrap().to_string_lossy()
            ));
            count += 1;
        }
    }
    let dest = std::env::var_os("DEADLIB_FLAC_REPORT").expect("set DEADLIB_FLAC_REPORT");
    fs::write(dest, report).unwrap();
    eprintln!("Symphonia differences: {mismatches:#?}");
    // A pinned reference regression, not a native PCM tolerance: every native
    // sample above must equal libFLAC, and this stream's MD5 must verify. Keep
    // the first differing Symphonia sample explicit so new differences fail.
    assert_eq!(
        mismatches,
        [
            "uncommon/05 - 32bps audio.flac: MISMATCH channel=0 sample=16384 assignment=10 expected=1340429 actual=1446772"
        ]
    );
    assert_eq!(count, 75);
    assert_eq!(symphonia_passes, 72);
    assert_eq!(
        rejections,
        [
            "uncommon/02 - increasing number of channels.flac: malformed stream: flac: unexpected channel assignment",
            "uncommon/06 - samplerate 768kHz.flac: malformed stream: flac: stream sample rate out of bounds",
        ]
    );
    eprintln!("{count} exact libFLAC comparisons, {symphonia_passes} exact Symphonia comparisons");
}

#[test]
#[ignore = "requires pinned official corpus"]
fn faulty_corpus_finishes_without_panic_or_unbounded_reads() {
    let root = std::env::var_os("DEADLIB_FLAC_CORPUS").expect("set DEADLIB_FLAC_CORPUS");
    let mut count = 0;
    for path in fs::read_dir(Path::new(&root).join("faulty"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "flac"))
    {
        let bytes = fs::read(&path).unwrap();
        let result = (|| -> Result<Integrity, Error> {
            let mut r = Reader::new(Cursor::new(&bytes))?;
            while r.read_next()?.is_some() {}
            Ok(r.integrity())
        })();
        let name = path.file_name().unwrap().to_string_lossy();
        if ["02 -", "03 -", "04 -"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            assert_eq!(result, Ok(Integrity::Verified));
        } else {
            assert!(result.is_err(), "unexpected acceptance of {name}");
        }
        // RFC 9639 leaves malformed behavior unspecified. Incorrect advisory
        // frame sizes, channel count and depth are safely superseded by frames.
        eprintln!("{}: {result:?}", path.display());
        count += 1;
    }
    assert_eq!(count, 11);
}
