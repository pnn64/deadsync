use deadlib_audio_decode_flac::{bitstream::BitReader, metadata::SeekTable, *};
use std::io::{Cursor, Write};

fn pcm(bits: u8, channels: u8, n: usize, pattern: u8) -> Vec<Vec<i32>> {
    let mut seed = 123456789u32;
    (0..channels)
        .map(|ch| {
            (0..n)
                .map(|i| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let value = match pattern {
                        0 => i32::MIN,
                        1 => {
                            if i % 2 == 0 {
                                i32::MIN
                            } else {
                                i32::MAX
                            }
                        }
                        2 => (i as i32 - 100).wrapping_mul(128123),
                        3 => seed as i32,
                        _ => {
                            ((i as f64 * 0.37 + f64::from(ch) * 0.001).sin() * 1e9
                                + (i as f64 * 0.83).sin() * 5e8) as i32
                        }
                    };
                    value >> (32 - bits)
                })
                .collect()
        })
        .collect()
}
fn encode(planes: &[Vec<i32>], options: EncodeOptions, chunk: usize, slots: u16) -> Vec<u8> {
    let mut encoder = Encoder::with_metadata(Cursor::new(Vec::new()), options, &[], slots).unwrap();
    let mut pos = 0;
    while pos < planes[0].len() {
        let end = (pos + chunk).min(planes[0].len());
        let refs: Vec<_> = planes.iter().map(|p| &p[pos..end]).collect();
        encoder.write_planar(&refs).unwrap();
        pos = end;
    }
    let stream = encoder.finish_seekable().unwrap();
    assert!(stream.metadata_finalized);
    stream.output.into_inner()
}
fn verify(data: &[u8], planes: &[Vec<i32>]) {
    let mut decoder = Decoder::new(data).unwrap();
    let mut pos = 0;
    while let Some(frame) = decoder.read_next().unwrap() {
        assert_eq!(frame.start_sample, pos as u64);
        for (ch, p) in frame.pcm.planes().enumerate() {
            assert_eq!(p, &planes[ch][pos..pos + p.len()]);
        }
        pos += frame.pcm.samples();
    }
    assert_eq!(pos, planes[0].len());
    assert_eq!(decoder.integrity(), Integrity::Verified);
}

fn verify_symphonia(data: &[u8], planes: &[Vec<i32>]) -> Result<(), String> {
    use symphonia::core::{
        audio::{Audio, GenericAudioBufferRef},
        codecs::audio::{AudioCodecParameters, AudioDecoderOptions, well_known::CODEC_ID_FLAC},
        packet::PacketRef,
        units::{Duration, Timestamp},
    };
    let metadata = Metadata::parse(data, Limits::default()).unwrap();
    let info = metadata.stream_info;
    let mut parameters = AudioCodecParameters::new();
    parameters
        .for_codec(CODEC_ID_FLAC)
        .with_extra_data(info.encode().unwrap().to_vec().into_boxed_slice());
    let mut reference = symphonia::default::get_codecs()
        .make_audio_decoder(&parameters, &AudioDecoderOptions::default())
        .map_err(|e| e.to_string())?;
    let mut native = FrameDecoder::new(Some(info)).unwrap();
    let mut offset = metadata.audio_offset;
    let mut position = 0;
    while offset < data.len() {
        let frame = native.decode(&data[offset..]).unwrap();
        let bytes = &data[offset..offset + frame.bytes];
        offset += frame.bytes;
        let packet = PacketRef::new(0, Timestamp::ZERO, Duration::ZERO, bytes);
        let decoded = reference.decode_ref(&packet).map_err(|e| e.to_string())?;
        let GenericAudioBufferRef::S32(decoded) = decoded else {
            return Err("reference sample format".into());
        };
        if decoded.frames() != frame.pcm.samples() {
            return Err("reference sample count".into());
        }
        for (ch, plane) in planes.iter().enumerate() {
            let actual = decoded.plane(ch).ok_or("reference channel count")?;
            for (i, (&a, &b)) in plane[position..position + frame.pcm.samples()]
                .iter()
                .zip(actual)
                .enumerate()
            {
                if a != b >> (32 - info.bits_per_sample) {
                    return Err(format!(
                        "PCM channel={ch} sample={} expected={a} actual={}",
                        position + i,
                        b >> (32 - info.bits_per_sample)
                    ));
                }
            }
        }
        position += frame.pcm.samples();
    }
    if position != planes[0].len() {
        return Err("reference total samples".into());
    }
    Ok(())
}

#[test]
fn exact_roundtrips_all_depths_channels_levels() {
    for bits in 4..=32 {
        for channels in 1..=8 {
            for level in [0, 5, 8] {
                let options = EncodeOptions {
                    bits_per_sample: bits,
                    channels,
                    compression_level: level,
                    block_size: 32,
                    variable_block_size: level == 8,
                    ..EncodeOptions::default()
                };
                let planes = pcm(bits, channels, 67, (bits + channels + level) % 5);
                let data = encode(&planes, options, 11, 3);
                verify(&data, &planes);
            }
        }
    }
    for level in 0..=8 {
        for pattern in 0..=4 {
            let options = EncodeOptions {
                compression_level: level,
                block_size: 128,
                ..EncodeOptions::default()
            };
            let planes = pcm(16, 2, 267, pattern);
            verify(&encode(&planes, options, 19, 0), &planes);
        }
    }
}

#[test]
fn streaming_chunks_interleaving_seektable_and_nonzero_origin() {
    let options = EncodeOptions {
        bits_per_sample: 24,
        channels: 3,
        block_size: 32,
        ..EncodeOptions::default()
    };
    let planes = pcm(24, 3, 1045, 4);
    let baseline = encode(&planes, options, 1045, 5);
    let metadata = Metadata::parse(&baseline, Limits::default()).unwrap();
    let table = SeekTable::parse(metadata.blocks[1].data).unwrap();
    assert!(table.points().filter(|p| p.sample != u64::MAX).count() >= 3);
    for point in table.points().filter(|p| p.sample != u64::MAX) {
        let header = FrameHeader::parse(
            &baseline[metadata.audio_offset + point.offset as usize..],
            Some(&metadata.stream_info),
        )
        .unwrap();
        assert_eq!(header.number * 32, point.sample);
        assert_eq!(header.block_size, point.samples as usize);
    }
    for chunk in [1, 2, 7, 31, 32, 33, 257, 1045] {
        assert_eq!(encode(&planes, options, chunk, 5), baseline);
        let mut writer = Cursor::new(b"prefix".to_vec());
        writer.set_position(6);
        let mut encoder = Encoder::with_metadata(writer, options, &[], 5).unwrap();
        let interleaved: Vec<_> = (0..1045)
            .flat_map(|i| planes.iter().map(move |p| p[i]))
            .collect();
        for part in interleaved.chunks(chunk * 3) {
            encoder.write_interleaved(part).unwrap();
        }
        assert_eq!(encoder.samples_written(), 1045);
        let result = encoder.finish_seekable().unwrap();
        assert_eq!(result.output.position(), (baseline.len() + 6) as u64);
        assert_eq!(&result.output.into_inner()[6..], baseline);
    }
}

#[test]
fn lpc_compression_and_raw_frame_number_boundaries() {
    let planes = pcm(24, 2, 2048, 4);
    let refs: Vec<_> = planes.iter().map(Vec::as_slice).collect();
    let options = EncodeOptions {
        bits_per_sample: 24,
        compression_level: 8,
        block_size: 2048,
        variable_block_size: true,
        ..EncodeOptions::default()
    };
    let mut encoder = FrameEncoder::new(options).unwrap();
    let info = options.stream_info().unwrap();
    let mut decoder = FrameDecoder::new(Some(info)).unwrap();
    for number in [
        0,
        127,
        128,
        2048,
        65536,
        1 << 21,
        1 << 26,
        1 << 31,
        (1 << 36) - 1,
    ] {
        let data = encoder.encode(&refs, number).unwrap();
        let frame = decoder.decode(data).unwrap();
        assert_eq!(frame.header.number, number);
        for (ch, p) in frame.pcm.planes().enumerate() {
            assert_eq!(p, planes[ch]);
        }
        assert!(
            data.len() < 2048 * 2 * 3 / 3,
            "predictable audio should compress"
        );
        let mut r = BitReader::new(&data[frame.header.header_bytes..]);
        r.read(1).unwrap();
        assert!(r.read(6).unwrap() >= 32, "expected an LPC proposal to win");
    }
}

#[test]
fn all_four_stereo_choices_are_selected_by_cost() {
    let options = EncodeOptions {
        block_size: 1024,
        ..EncodeOptions::default()
    };
    let mut encoder = FrameEncoder::new(options).unwrap();
    let mut seed = 91u32;
    let noise: Vec<i32> = (0..1024)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as i32) >> 18
        })
        .collect();
    let unrelated: Vec<i32> = (0..1024)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as i32) >> 18
        })
        .collect();
    let ramp: Vec<i32> = (0..1024).map(|i| i - 512).collect();
    let perturbed: Vec<i32> = ramp
        .iter()
        .zip(&noise)
        .map(|(&a, &b)| a + (b >> 10))
        .collect();
    let opposite: Vec<i32> = noise
        .iter()
        .zip(&unrelated)
        .map(|(&a, &b)| -a + (b >> 10))
        .collect();
    for (left, right, assignment) in [
        (&noise, &unrelated, 1),
        (&ramp, &perturbed, 8),
        (&perturbed, &ramp, 9),
        (&noise, &opposite, 10),
    ] {
        let data = encoder.encode(&[left, right], 0).unwrap();
        assert_eq!(
            FrameHeader::parse(data, Some(&options.stream_info().unwrap()))
                .unwrap()
                .channel_assignment,
            assignment
        );
    }
}

#[test]
fn all_block_sizes_rates_empty_stream_and_metadata() {
    for (n, rate) in [
        (1, 0),
        (15, 1),
        (16, 65535),
        (192, 255000),
        (256, 655350),
        (257, 768000),
        (65535, 1048575),
    ] {
        let options = EncodeOptions {
            sample_rate: rate,
            channels: 1,
            block_size: (n as u16).max(16),
            ..EncodeOptions::default()
        };
        let planes = vec![vec![-123; n]];
        verify(&encode(&planes, options, n, 0), &planes);
    }
    let options = EncodeOptions {
        channels: 1,
        ..EncodeOptions::default()
    };
    let planes = vec![Vec::new()];
    verify(&encode(&planes, options, 1, 1), &planes);
    let blocks = [
        MetadataBlock {
            kind: 2,
            data: b"TEST\0\xff",
        },
        MetadataBlock {
            kind: 126,
            data: b"opaque",
        },
    ];
    let mut encoder = Encoder::with_metadata(Cursor::new(Vec::new()), options, &blocks, 0).unwrap();
    encoder.write_planar(&[&[-1, 2, 3]]).unwrap();
    let out = encoder.finish_seekable().unwrap().output.into_inner();
    let meta = Metadata::parse(&out, Limits::default()).unwrap();
    assert_eq!(&meta.blocks[1..], blocks);
}

#[test]
fn forward_only_and_write_failures_have_explicit_integrity() {
    let options = EncodeOptions {
        channels: 1,
        block_size: 16,
        ..EncodeOptions::default()
    };
    let mut encoder = Encoder::new(Vec::new(), options).unwrap();
    encoder.write_interleaved(&[-1; 35]).unwrap();
    let result = encoder.finish().unwrap();
    assert_eq!(result.stream_info.total_samples, 35);
    assert!(!result.metadata_finalized);
    let mut decoder = Decoder::new(&result.output).unwrap();
    while decoder.read_next().unwrap().is_some() {}
    assert_eq!(decoder.integrity(), Integrity::Unavailable);
    struct Fails {
        remaining: usize,
    }
    impl Write for Fails {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("injected"));
            }
            let n = bytes.len().min(self.remaining);
            self.remaining -= n;
            Ok(n)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut encoder = Encoder::new(Fails { remaining: 44 }, options).unwrap();
    assert!(encoder.write_planar(&[&[0; 16]]).is_err());
    assert!(encoder.write_planar(&[&[0; 16]]).is_err());
    assert!(encoder.finish().is_err());
    let mut encoder = Encoder::new(Vec::new(), options).unwrap();
    assert!(encoder.write_interleaved(&[i32::MAX]).is_err());
    assert!(encoder.finish().is_err());
}

#[test]
#[ignore = "requires libFLAC oracle and a generated-corpus directory; see ENCODING.md"]
fn generated_matrix_against_libflac() {
    use std::{fs, path::PathBuf, process::Command};
    let oracle = std::env::var_os("DEADLIB_FLAC_ORACLE").expect("set DEADLIB_FLAC_ORACLE");
    let root = PathBuf::from(
        std::env::var_os("DEADLIB_FLAC_GENERATED").expect("set DEADLIB_FLAC_GENERATED"),
    );
    fs::create_dir_all(&root).unwrap();
    let mut report = String::from(
        "bits\tchannels\tlevel\tvariable\tsamples\tpattern\tnative_bytes\tlibFLAC_bytes\tnative_frame_bytes\tlibFLAC_frame_bytes\tPCM\tSymphonia\n",
    );
    let mut count = 0;
    let mut symphonia_differences = Vec::new();
    for bits in 4..=32 {
        for channels in 1..=8 {
            for level in [0, 5, 8] {
                let options = EncodeOptions {
                    bits_per_sample: bits,
                    channels,
                    compression_level: level,
                    block_size: 256,
                    variable_block_size: level == 8,
                    ..EncodeOptions::default()
                };
                let pattern = (bits + channels + level) % 5;
                let planes = pcm(bits, channels, 1025, pattern);
                let encoded = encode(&planes, options, 117, 4);
                let native_path = root.join("native.flac");
                fs::write(&native_path, &encoded).unwrap();
                let output = Command::new(&oracle).arg(&native_path).output().unwrap();
                assert!(
                    output.status.success(),
                    "bits={bits} channels={channels} level={level}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let mut bytes = output.stdout.as_slice();
                let mut position = 0;
                fn word(bytes: &mut &[u8]) -> u32 {
                    let result = u32::from_le_bytes(bytes[..4].try_into().unwrap());
                    *bytes = &bytes[4..];
                    result
                }
                while !bytes.is_empty() {
                    assert_eq!(word(&mut bytes), options.sample_rate);
                    assert_eq!(word(&mut bytes), u32::from(channels));
                    assert_eq!(word(&mut bytes), u32::from(bits));
                    let n = word(&mut bytes) as usize;
                    for plane in &planes {
                        for &sample in &plane[position..position + n] {
                            assert_eq!(
                                word(&mut bytes) as i32,
                                sample,
                                "oracle PCM bits={bits} channels={channels} level={level}"
                            );
                        }
                    }
                    position += n;
                }
                assert_eq!(position, planes[0].len());
                let raw: Vec<u8> = (0..1025)
                    .flat_map(|i| planes.iter().flat_map(move |p| p[i].to_le_bytes()))
                    .collect();
                let raw_path = root.join("input.i32le");
                fs::write(&raw_path, raw).unwrap();
                let reference_path = root.join("reference.flac");
                let result = Command::new(&oracle)
                    .arg("encode")
                    .arg(&raw_path)
                    .arg(&reference_path)
                    .args([
                        options.sample_rate.to_string(),
                        channels.to_string(),
                        bits.to_string(),
                        level.to_string(),
                        options.block_size.to_string(),
                    ])
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "libFLAC encoder bits={bits} channels={channels} level={level}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let reference = fs::read(&reference_path).unwrap();
                verify(&reference, &planes);
                let sym = match verify_symphonia(&encoded, &planes) {
                    Ok(()) => "exact".to_string(),
                    Err(error) => {
                        symphonia_differences.push(format!(
                            "bits={bits} channels={channels} level={level}: {error}"
                        ));
                        error
                    }
                };
                let native_audio = encoded.len()
                    - Metadata::parse(&encoded, Limits::default())
                        .unwrap()
                        .audio_offset;
                let reference_audio = reference.len()
                    - Metadata::parse(&reference, Limits::default())
                        .unwrap()
                        .audio_offset;
                report.push_str(&format!("{bits}\t{channels}\t{level}\t{}\t1025\t{pattern}\t{}\t{}\t{native_audio}\t{reference_audio}\texact both directions\t{sym}\n",options.variable_block_size,encoded.len(),reference.len()));
                count += 1;
            }
        }
    }
    let destination =
        std::env::var_os("DEADLIB_FLAC_ENCODING_REPORT").expect("set DEADLIB_FLAC_ENCODING_REPORT");
    fs::write(destination, report).unwrap();
    assert_eq!(count, 696);
    // Same pinned 32-bit stereo reference limitation as the official corpus.
    // Native PCM must still match both the input and libFLAC exactly.
    assert_eq!(
        symphonia_differences,
        [
            "bits=32 channels=2 level=0: PCM channel=1 sample=0 expected=999999 actual=1",
            "bits=32 channels=2 level=5: PCM channel=0 sample=0 expected=0 actual=999999",
        ]
    );
    eprintln!("{count} generated streams verified in both directions against libFLAC");
}
