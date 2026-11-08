//! External Ogg mapping gates; opt in with the pinned C oracle and corpus.
use deadlib_audio_decode_flac::*;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
};
use symphonia::core::{
    audio::{Audio, GenericAudioBufferRef},
    codecs::audio::AudioDecoderOptions,
    formats::{FormatOptions, TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

fn symphonia(data: &[u8], expected: &[i32], bits: u8, channels: u8) -> Result<(), String> {
    let mut hint = Hint::new();
    hint.with_extension("oga");
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(Box::new(Cursor::new(data.to_vec())), Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| e.to_string())?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or("no audio track")?;
    let id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(
            track
                .codec_params
                .as_ref()
                .and_then(|p| p.audio())
                .ok_or("no audio parameters")?,
            &AudioDecoderOptions::default(),
        )
        .map_err(|e| e.to_string())?;
    let mut pos = 0;
    while let Some(packet) = format.next_packet().map_err(|e| e.to_string())? {
        if packet.track_id != id {
            return Err("unexpected track".into());
        }
        let GenericAudioBufferRef::S32(audio) =
            decoder.decode(&packet).map_err(|e| e.to_string())?
        else {
            return Err("unexpected PCM type".into());
        };
        for ch in 0..channels as usize {
            let plane = audio.plane(ch).ok_or("missing channel")?;
            for (i, &value) in plane.iter().enumerate() {
                let position = (pos + i) * channels as usize + ch;
                let value = value >> (32 - bits);
                if expected.get(position) != Some(&value) {
                    return Err(format!(
                        "PCM channel={ch} sample={} expected={:?} actual={value}",
                        pos + i,
                        expected.get(position)
                    ));
                }
            }
        }
        pos += audio.frames();
    }
    if pos * channels as usize != expected.len() {
        return Err("sample count".into());
    }
    Ok(())
}
fn oracle(path: &Path, ogg: bool) -> Vec<(u32, u8, Vec<Vec<i32>>)> {
    let mut command =
        Command::new(std::env::var_os("DEADLIB_FLAC_ORACLE").expect("set DEADLIB_FLAC_ORACLE"));
    if ogg {
        command.arg("ogg");
    }
    let output = command.arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut data = output.stdout.as_slice();
    fn word(data: &mut &[u8]) -> u32 {
        let result = u32::from_le_bytes(data[..4].try_into().unwrap());
        *data = &data[4..];
        result
    }
    let mut frames = Vec::new();
    while !data.is_empty() {
        let rate = word(&mut data);
        let channels = word(&mut data);
        let bits = word(&mut data) as u8;
        let n = word(&mut data) as usize;
        assert!((1..=8).contains(&channels) && (1..=65535).contains(&n));
        frames.push((
            rate,
            bits,
            (0..channels)
                .map(|_| (0..n).map(|_| word(&mut data) as i32).collect())
                .collect(),
        ));
    }
    frames
}
fn interleave(planes: &[Vec<i32>]) -> Vec<i32> {
    (0..planes[0].len())
        .flat_map(|i| planes.iter().map(move |p| p[i]))
        .collect()
}
fn verify(data: &[u8], expected: &[i32], channels: u8) {
    let mut reader = OggReader::new(data, OggOptions::default());
    let mut pcm = Vec::new();
    while reader
        .read_next(|e| {
            if let OggEvent::Pcm(frame) = e {
                for i in 0..frame.pcm.samples() {
                    for ch in 0..channels as usize {
                        pcm.push(frame.pcm.plane(ch)[i]);
                    }
                }
            }
        })
        .unwrap()
    {}
    assert_eq!(pcm, expected);
    let mut reader = OggSeekReader::new(Cursor::new(data), OggSeekOptions::default()).unwrap();
    let id = reader.streams()[0].id;
    let n = expected.len() / channels as usize;
    for sample in [n, 0, 1, n / 2, n - 1] {
        reader.seek(id, sample as u64).unwrap();
        pcm.clear();
        while let Some(frame) = reader.read_next().unwrap() {
            for i in 0..frame.pcm.samples() {
                for ch in 0..channels as usize {
                    pcm.push(frame.pcm.plane(ch)[i]);
                }
            }
        }
        assert_eq!(pcm, expected[sample * channels as usize..]);
    }
}

#[test]
#[ignore = "requires Ogg-enabled libFLAC oracle; see OGG.md"]
fn generated_ogg_matrix() {
    let root = PathBuf::from(
        std::env::var_os("DEADLIB_FLAC_GENERATED").expect("set DEADLIB_FLAC_GENERATED"),
    )
    .join("ogg");
    fs::create_dir_all(&root).unwrap();
    let oracle_exe = std::env::var_os("DEADLIB_FLAC_ORACLE").unwrap();
    let mut report =
        String::from("bits\tchannels\tlevel\tbytes\tlibFLAC_bytes\tPCM\tseeks\tSymphonia\n");
    let mut differences = Vec::new();
    for bits in 4..=32 {
        for channels in 1..=8 {
            for level in [0, 5, 8] {
                let n = 1025;
                let mut seed = 31337u32;
                let input: Vec<i32> = (0..n * channels as usize)
                    .map(|i| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let value = if level == 0 {
                            seed as i32
                        } else {
                            ((i / channels as usize) as f64 * 0.37)
                                .sin()
                                .mul_add(1e9, (seed as i32 >> 8) as f64)
                                as i32
                        };
                        value >> (32 - bits)
                    })
                    .collect();
                let options = EncodeOptions {
                    bits_per_sample: bits,
                    channels,
                    block_size: 256,
                    compression_level: level,
                    variable_block_size: level == 8,
                    ..Default::default()
                };
                let mut encoder =
                    OggEncoder::new(Vec::new(), options, OggWriteOptions::default()).unwrap();
                encoder.write_interleaved(&input).unwrap();
                let data = encoder.finish().unwrap().output;
                let native = root.join("native.oga");
                fs::write(&native, &data).unwrap();
                let frames = oracle(&native, true);
                let actual: Vec<i32> = frames.iter().flat_map(|(_, _, p)| interleave(p)).collect();
                assert_eq!(
                    actual, input,
                    "libFLAC bits={bits} channels={channels} level={level}"
                );
                let raw = root.join("input.i32le");
                fs::write(
                    &raw,
                    input
                        .iter()
                        .flat_map(|x| x.to_le_bytes())
                        .collect::<Vec<_>>(),
                )
                .unwrap();
                let reference = root.join("reference.oga");
                let status = Command::new(&oracle_exe)
                    .arg("encode-ogg")
                    .arg(raw)
                    .arg(&reference)
                    .args([
                        options.sample_rate.to_string(),
                        channels.to_string(),
                        bits.to_string(),
                        level.to_string(),
                        "256".into(),
                    ])
                    .output()
                    .unwrap();
                assert!(
                    status.status.success(),
                    "{}",
                    String::from_utf8_lossy(&status.stderr)
                );
                let reference = fs::read(reference).unwrap();
                verify(&reference, &input, channels);
                let sym = symphonia(&data, &input, bits, channels).err();
                if let Some(error) = &sym {
                    differences.push(format!(
                        "bits={bits} channels={channels} level={level}: {error}"
                    ));
                }
                report.push_str(&format!(
                    "{bits}\t{channels}\t{level}\t{}\t{}\texact both directions\t5\t{}\n",
                    data.len(),
                    reference.len(),
                    sym.as_deref().unwrap_or("exact")
                ));
            }
        }
    }
    fs::write(
        std::env::var_os("DEADLIB_FLAC_OGG_REPORT").expect("set DEADLIB_FLAC_OGG_REPORT"),
        report,
    )
    .unwrap();
    assert!(
        differences.is_empty(),
        "Symphonia differences: {differences:#?}"
    );
}

#[test]
#[ignore = "requires official corpus and Ogg-enabled libFLAC oracle; see OGG.md"]
fn official_corpus_remux_and_seek() {
    let corpus =
        PathBuf::from(std::env::var_os("DEADLIB_FLAC_CORPUS").expect("set DEADLIB_FLAC_CORPUS"));
    let root = PathBuf::from(
        std::env::var_os("DEADLIB_FLAC_GENERATED").expect("set DEADLIB_FLAC_GENERATED"),
    )
    .join("ogg-corpus");
    fs::create_dir_all(&root).unwrap();
    let mut report = String::from("group\tfile\tframes\tsamples\tlibFLAC\tseeks\tintegrity\n");
    let mut count = 0;
    for group in ["subset", "uncommon"] {
        let mut paths: Vec<_> = fs::read_dir(corpus.join(group))
            .unwrap()
            .map(|p| p.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "flac"))
            .collect();
        paths.sort();
        for path in paths {
            let native = fs::read(&path).unwrap();
            if !native.starts_with(b"fLaC") {
                continue;
            }
            eprintln!("Ogg remux {}", path.display());
            let metadata = Metadata::parse(
                &native,
                Limits {
                    metadata_bytes: 128 << 20,
                    ..Default::default()
                },
            )
            .unwrap();
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
            let data = muxer.finish().unwrap().output;
            let output = root.join("remux.oga");
            fs::write(&output, &data).unwrap();
            let expected = oracle(&path, false);
            assert_eq!(oracle(&output, true), expected);
            let mut options = OggSeekOptions::default();
            options.mapping.decode.limits.metadata_bytes = 128 << 20;
            options.mapping.max_metadata_bytes = 256 << 20;
            let mut reader = OggSeekReader::new(Cursor::new(&data), options).unwrap();
            let id = reader.streams()[0].id;
            let total = reader.streams()[0].samples;
            let mut starts = Vec::new();
            let mut start = 0;
            for (rate, bits, planes) in &expected {
                starts.push(start);
                let frame = reader.read_next().unwrap().unwrap();
                assert_eq!(frame.start_sample, start);
                assert_eq!(frame.header.sample_rate, *rate);
                assert_eq!(frame.header.bits_per_sample, *bits);
                for (a, b) in frame.pcm.planes().zip(planes) {
                    assert_eq!(a, b);
                }
                start += planes[0].len() as u64;
            }
            assert_eq!(start, total);
            assert!(reader.read_next().unwrap().is_none());
            let integrity = reader.integrity();
            let mut positions = vec![total, 0, total - 1, total / 2, 1];
            let mut seed = 41u64;
            for _ in 0..16 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                positions.push(seed % total);
            }
            for sample in positions {
                reader.seek(id, sample).unwrap();
                if sample == total {
                    assert!(reader.read_next().unwrap().is_none());
                    continue;
                }
                let i = starts.partition_point(|&s| s <= sample) - 1;
                let skip = (sample - starts[i]) as usize;
                let frame = reader.read_next().unwrap().unwrap();
                assert_eq!(frame.start_sample, sample);
                for (a, b) in frame.pcm.planes().zip(&expected[i].2) {
                    assert_eq!(a, &b[skip..]);
                }
            }
            report.push_str(&format!(
                "{group}\t{}\t{}\t{total}\texact\t21\t{integrity:?}\n",
                path.file_name().unwrap().to_string_lossy(),
                expected.len()
            ));
            count += 1;
        }
    }
    fs::write(
        std::env::var_os("DEADLIB_FLAC_OGG_CORPUS_REPORT")
            .expect("set DEADLIB_FLAC_OGG_CORPUS_REPORT"),
        report,
    )
    .unwrap();
    assert_eq!(count, 73);
}
