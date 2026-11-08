use deadlib_image_png::{
    AncillaryChunk, AnimationPolicy, ChunkPosition, DecodeEvent, DecodeOptions, DecodeStatus,
    EncodeOptions, Error, Limits, Metadata, Png, StreamDecoder, decode, decode_reader,
    decode_with_options, encode, encode_writer,
};
use std::io::{self, Read};
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/hdr.rs"]
mod hdr;
use hdr::Hdr;

struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.0.len().min(out.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}
fn expected(h: &Hdr, color: u8, depth: u8, interlaced: bool) -> Png {
    let mut png = Png::new(hdr::pixel(color, depth));
    png.interlaced = interlaced;
    if color == 3 {
        png.palette = vec![[0, 0, 0], [255, 255, 255]];
    }
    png.ancillary = h
        .payloads()
        .into_iter()
        .map(|(kind, data)| AncillaryChunk {
            kind,
            data,
            position: ChunkPosition::BeforePalette,
        })
        .collect();
    png
}
fn reference(bytes: &[u8], h: &Hdr, color: u8, depth: u8) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let info = reader.info();
    let c = info.coding_independent_code_points.as_ref().unwrap();
    assert_eq!(
        (
            c.color_primaries,
            c.transfer_function,
            c.matrix_coefficients,
            c.is_video_full_range_image
        ),
        (9, 16, 0, true)
    );
    let m = info.mastering_display_color_volume.as_ref().unwrap();
    let v = &m.chromaticities;
    let actual = [
        v.red.0, v.red.1, v.green.0, v.green.1, v.blue.0, v.blue.1, v.white.0, v.white.1,
    ]
    .map(|v| v.into_scaled());
    assert_eq!(actual, h.chromaticities.map(|v| u32::from(v) * 2));
    assert_eq!([m.max_luminance, m.min_luminance], h.luminance);
    let l = info.content_light_level.as_ref().unwrap();
    assert_eq!(
        [l.max_content_light_level, l.max_frame_average_light_level],
        h.light
    );
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(
        (
            info.width,
            info.height,
            info.color_type as u8,
            info.bit_depth as u8
        ),
        (1, 1, color, depth)
    );
    assert_eq!(&output[..info.buffer_size()], hdr::packed(color, depth));
    reader.finish().unwrap();
}
fn push_metadata(bytes: &[u8]) -> Result<Vec<Metadata>, Error> {
    let mut decoder = StreamDecoder::new(Default::default());
    let (mut offset, mut result) = (0, Vec::new());
    loop {
        let end = bytes.len().min(offset + 1);
        let p = decoder.process(&bytes[offset..end])?;
        offset += p.consumed;
        assert_eq!(offset, decoder.total_in());
        if let Some(DecodeEvent::Metadata { value, .. }) = decoder.event() {
            result.push(value.clone());
        }
        if p.status == DecodeStatus::Done {
            assert_eq!(offset, bytes.len());
            decoder.finish()?;
            return Ok(result);
        }
        if p.status == DecodeStatus::NeedInput && offset == bytes.len() {
            decoder.finish()?;
            panic!("truncated input accepted");
        }
    }
}

#[test]
fn literal_hdr_units_white_point_erratum_all_formats_and_writers() {
    let bt2020 = Hdr::default();
    // D65 x=0.3127 -> 15635 -> 0x3d13 (W3C erratum, not 0x3c05).
    assert_eq!(
        bt2020.payloads()[1].1,
        [
            0x8a, 0x48, 0x39, 0x08, 0x21, 0x34, 0x9b, 0xaa, 0x19, 0x96, 0x08, 0xfc, 0x3d, 0x13,
            0x40, 0x42, 0x02, 0x62, 0x5a, 0x00, 0, 0, 0, 5,
        ]
    );
    assert_eq!(
        bt2020.payloads()[2].1,
        [0, 0x98, 0x96, 0x80, 0, 0x26, 0x25, 0xa0]
    );
    let profiles = [
        bt2020,
        Hdr {
            chromaticities: [0; 8],
            luminance: [0; 2],
            light: [0; 2],
        },
        Hdr {
            chromaticities: [65535; 8],
            luminance: [0x7fff_ffff; 2],
            light: [0x7fff_ffff; 2],
        },
        Hdr {
            chromaticities: [1, 255, 256, 32767, 32768, 50000, 50001, 65534],
            luminance: [1, 0],
            light: [0, 1],
        },
    ];
    for h in profiles {
        for (color, depth) in hdr::FORMATS {
            for interlaced in [false, true] {
                let png = expected(&h, color, depth, interlaced);
                let options = EncodeOptions {
                    interlaced,
                    ..Default::default()
                };
                let wire = h.fixture(color, depth, interlaced);
                assert_eq!(decode(&wire).unwrap(), png);
                let mut streamed = Vec::new();
                encode_writer(&png, &options, &mut streamed).unwrap();
                for bytes in [wire, encode(&png, &options).unwrap(), streamed] {
                    assert_eq!(decode(&bytes).unwrap(), png);
                    assert_eq!(
                        decode_reader(Fragments(&bytes, 1), &Default::default()).unwrap(),
                        png
                    );
                    assert_eq!(push_metadata(&bytes).unwrap(), h.metadata());
                    reference(&bytes, &h, color, depth);
                }
            }
        }
    }
}

#[test]
fn all_hdr_field_bytes_integer_boundaries_and_native_reference_values() {
    let (mut valid, mut invalid) = (0, 0);
    let recovery = DecodeOptions {
        animation_policy: AnimationPolicy::RecoverDefault,
        ..Default::default()
    };
    for (label, h) in hdr::byte_cases().chain(hdr::boundary_cases()) {
        let bytes = h.fixture(2, 8, false);
        if h.valid_wire_integers() {
            let png = expected(&h, 2, 8, false);
            assert_eq!(decode(&bytes).unwrap(), png, "{label}");
            assert_eq!(
                decode_reader(Fragments(&bytes, 7), &Default::default()).unwrap(),
                png,
                "{label}"
            );
            assert_eq!(
                decode_with_options(&bytes, &recovery).unwrap(),
                png,
                "{label}"
            );
            assert_eq!(push_metadata(&bytes).unwrap(), h.metadata(), "{label}");
            reference(&bytes, &h, 2, 8);
            valid += 1;
        } else {
            assert!(matches!(decode(&bytes), Err(Error::Invalid(_))), "{label}");
            assert!(
                matches!(
                    decode_reader(Fragments(&bytes, 1), &Default::default()),
                    Err(Error::Invalid(_))
                ),
                "{label}"
            );
            assert!(
                matches!(
                    decode_with_options(&bytes, &recovery),
                    Err(Error::Invalid(_))
                ),
                "{label}"
            );
            assert!(
                matches!(push_metadata(&bytes), Err(Error::Invalid(_))),
                "{label}"
            );
            let png = expected(&h, 2, 8, false);
            assert!(encode(&png, &Default::default()).is_err());
            let mut output = Vec::new();
            assert!(encode_writer(&png, &Default::default(), &mut output).is_err());
            assert!(output.is_empty());
            invalid += 1;
        }
    }
    assert_eq!((valid, invalid), (7728, 524));
}

#[test]
fn hdr_chunk_lengths_dependency_order_duplicates_and_resource_budget() {
    let h = Hdr::default();
    let payloads = h.payloads();
    for target in [1, 2] {
        for len in 0..=65 {
            let mut data = payloads.clone();
            data[target].1.resize(len, 0);
            let extra: Vec<_> = data.iter().map(|(k, d)| chunks::chunk(*k, d)).collect();
            let bytes = hdr::fixture(3, 2, false, &extra);
            let valid = len == payloads[target].1.len();
            assert_eq!(decode(&bytes).is_ok(), valid, "{target}/{len}");
            assert_eq!(
                decode_reader(Fragments(&bytes, 1), &Default::default()).is_ok(),
                valid
            );
            let mut png = expected(&h, 3, 2, false);
            png.ancillary[target].data = data[target].1.clone();
            assert_eq!(encode(&png, &Default::default()).is_ok(), valid);
            let mut output = Vec::new();
            assert_eq!(
                encode_writer(&png, &Default::default(), &mut output).is_ok(),
                valid
            );
            if !valid {
                assert!(output.is_empty());
            }
        }
    }
    let core = hdr::core(3, 2, false);
    let extra: Vec<_> = payloads.iter().map(|(k, d)| chunks::chunk(*k, d)).collect();
    let unordered = [&extra[0], &extra[1], &extra[2], &core[1], &core[2]];
    let (mut total, mut accepted) = (0, 0);
    chunks::permutations(&mut [0, 1, 2, 3, 4], 0, &mut |order| {
        let mut c = vec![core[0].clone()];
        c.extend(order.iter().map(|&i| unordered[i].clone()));
        c.push(core[3].clone());
        let bytes = chunks::file(&c);
        let valid = order[3..] == [3, 4];
        assert_eq!(decode(&bytes).is_ok(), valid, "{order:?}");
        assert_eq!(
            decode_reader(Fragments(&bytes, 1), &Default::default()).is_ok(),
            valid
        );
        total += 1;
        accepted += usize::from(valid);
    });
    assert_eq!((total, accepted), (120, 6));
    for target in 0..3 {
        let mut extra = extra.clone();
        extra.push(extra[target].clone());
        let bytes = hdr::fixture(2, 8, false, &extra);
        assert!(decode(&bytes).is_err());
        assert!(decode_reader(Fragments(&bytes, 1), &Default::default()).is_err());
        let mut png = expected(&h, 2, 8, false);
        png.ancillary.push(png.ancillary[target].clone());
        assert!(encode(&png, &Default::default()).is_err());
        let mut output = Vec::new();
        assert!(encode_writer(&png, &Default::default(), &mut output).is_err());
        assert!(output.is_empty());
    }
    for selected in [&extra[1..2], &extra[2..3]] {
        let bytes = hdr::fixture(2, 8, false, selected);
        let valid = selected[0][4..8] == *b"cLLI";
        assert_eq!(decode(&bytes).is_ok(), valid);
        assert_eq!(
            decode_reader(Fragments(&bytes, 1), &Default::default()).is_ok(),
            valid
        );
    }
    for budget in [35, 36] {
        let options = DecodeOptions {
            limits: Limits {
                max_metadata_bytes: budget,
                ..Default::default()
            },
            ..Default::default()
        };
        let bytes = h.fixture(2, 8, false);
        assert_eq!(decode_with_options(&bytes, &options).is_ok(), budget == 36);
        assert_eq!(
            decode_reader(Fragments(&bytes, 1), &options).is_ok(),
            budget == 36
        );
    }
}

fn c_expected(h: &Hdr) -> Vec<u8> {
    let display = h.luminance.iter().all(|&v| v <= 0x7fff_ffff);
    let light = h.light.iter().all(|&v| v <= 0x7fff_ffff);
    let mut values = vec![
        1,
        1 | if display { 2 } else { 0 } | if light { 4 } else { 0 },
        9,
        16,
        0,
        1,
    ];
    if display {
        values.extend(h.chromaticities.map(|v| u32::from(v) * 2));
        values.extend(h.luminance);
    }
    if light {
        values.extend(h.light);
    }
    values.into_iter().flat_map(u32::to_be_bytes).collect()
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_exact_hdr_fixed_values_and_invalid_integer_policy() {
    use std::{fs, path::PathBuf, process::Command};
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required"),
    );
    let scratch = std::env::temp_dir().join(format!("deadlib-png-hdr-{}.png", std::process::id()));
    let (mut exact, mut invalid_ignored) = (0, 0);
    for (label, h) in hdr::byte_cases().chain(hdr::boundary_cases()) {
        fs::write(&scratch, h.fixture(2, 8, false)).unwrap();
        let output = Command::new(&oracle)
            .arg("--hdr-metadata")
            .arg(&scratch)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, c_expected(&h), "{label}");
        if h.valid_wire_integers() {
            exact += 1;
        } else {
            invalid_ignored += 1;
        }
    }
    let h = Hdr::default();
    let mut writer_exact = 0;
    for (color, depth) in hdr::FORMATS {
        for interlaced in [false, true] {
            let png = expected(&h, color, depth, interlaced);
            let options = EncodeOptions {
                interlaced,
                ..Default::default()
            };
            let mut streamed = Vec::new();
            encode_writer(&png, &options, &mut streamed).unwrap();
            for bytes in [encode(&png, &options).unwrap(), streamed] {
                fs::write(&scratch, bytes).unwrap();
                let output = Command::new(&oracle)
                    .arg("--hdr-metadata")
                    .arg(&scratch)
                    .output()
                    .unwrap();
                assert!(output.status.success());
                assert_eq!(output.stdout, c_expected(&h));
                writer_exact += 1;
            }
        }
    }
    fs::remove_file(scratch).unwrap();
    println!(
        "libpng HDR: exact valid fixed values={exact}, invalid integer chunks ignored={invalid_ignored}, native writer exact={writer_exact}"
    );
    assert_eq!((exact, invalid_ignored, writer_exact), (7728, 524, 60));
}
