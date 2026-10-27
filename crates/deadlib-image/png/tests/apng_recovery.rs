use deadlib_image_png::{
    AnimationPolicy, DecodeEvent, DecodeOptions, DecodeStatus, Error, Image, ImageKind, Limits,
    Png, RowSamples, Samples, StreamDecoder, decode, decode_reader, decode_with_options, encode,
    encode_writer,
};
use std::io::{self, Read};
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/apng_recovery.rs"]
mod fixtures;
use fixtures::{Chunk, Fixture};

fn recovery() -> DecodeOptions {
    DecodeOptions {
        animation_policy: AnimationPolicy::RecoverDefault,
        ..Default::default()
    }
}
struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.0.len().min(out.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}
fn assert_static(png: &Png, fixture: &Fixture) {
    assert_eq!(png.image, fixture.image);
    assert_eq!(png.palette, fixture.palette);
    assert_eq!(png.interlaced, fixture.interlaced);
    assert!(png.animation.is_none());
    assert!(png.animation_error.is_some());
    let mut without_diagnostic = png.clone();
    without_diagnostic.animation_error = None;
    assert_eq!(without_diagnostic, decode(&fixture.static_wire()).unwrap());
}
fn reference_frame(reader: &mut png::Reader<std::io::Cursor<&[u8]>>, expected: &Image) {
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(
        (
            info.width,
            info.height,
            info.bit_depth as u8,
            info.color_type as u8
        ),
        (
            expected.width,
            expected.height,
            expected.bit_depth,
            expected.color_type as u8
        )
    );
    if reader.info().interlaced {
        let stride = reader.output_line_size(reader.info().width).unwrap();
        output = (0..info.height as usize)
            .flat_map(|y| {
                output[y * stride..y * stride + info.line_size]
                    .iter()
                    .copied()
            })
            .collect();
    }
    assert_packed(&output[..info.buffer_size()], expected);
}
fn assert_packed(bytes: &[u8], expected: &Image) {
    match &expected.samples {
        Samples::U16(values) => assert_eq!(
            bytes,
            values
                .iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<_>>()
        ),
        Samples::U8(values) if expected.bit_depth == 8 => assert_eq!(bytes, values),
        Samples::U8(values) => {
            let depth = usize::from(expected.bit_depth);
            let width = expected.width as usize;
            let stride = (width * depth).div_ceil(8);
            assert_eq!(bytes.len(), stride * expected.height as usize);
            for (n, &sample) in values.iter().enumerate() {
                let (y, x) = (n / width, n % width);
                assert_eq!(
                    (bytes[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                        & ((1 << depth) - 1),
                    sample
                );
            }
        }
    }
}
fn stream(bytes: &[u8], fragment: usize, fixture: &Fixture) -> Result<(usize, usize), Error> {
    stream_options(bytes, fragment, fixture, recovery())
}
fn stream_options(
    bytes: &[u8],
    fragment: usize,
    fixture: &Fixture,
    options: DecodeOptions,
) -> Result<(usize, usize), Error> {
    let mut decoder = StreamDecoder::new(options);
    let (mut offset, mut errors, mut default_ends) = (0, 0, 0);
    let mut aborted = false;
    loop {
        let end = bytes.len().min(offset + fragment);
        let progress = decoder.process(&bytes[offset..end])?;
        offset += progress.consumed;
        assert_eq!(decoder.total_in(), offset);
        match progress.status {
            DecodeStatus::Event => match decoder.event().unwrap() {
                DecodeEvent::AnimationError(error) => {
                    assert!(!aborted);
                    assert_eq!(decoder.animation_error(), Some(error));
                    errors += 1;
                    aborted = true;
                }
                DecodeEvent::FrameControl { .. } | DecodeEvent::Animation { .. } => {
                    assert!(!aborted)
                }
                DecodeEvent::Row(row) => {
                    let expected = match row.image {
                        ImageKind::Default => &fixture.image,
                        ImageKind::AnimationFrame(index) => {
                            assert!(!aborted);
                            &fixture.frames[index as usize]
                        }
                    };
                    let channels = expected.color_type.channels();
                    for pixel in 0..row.width as usize {
                        let start = (row.y as usize * expected.width as usize
                            + row.x as usize
                            + pixel * row.x_step as usize)
                            * channels;
                        match (&expected.samples, row.samples) {
                            (Samples::U8(a), RowSamples::U8(b)) => assert_eq!(
                                &a[start..start + channels],
                                &b[pixel * channels..(pixel + 1) * channels]
                            ),
                            (Samples::U16(a), RowSamples::U16(b)) => assert_eq!(
                                &a[start..start + channels],
                                &b[pixel * channels..(pixel + 1) * channels]
                            ),
                            _ => panic!("native representation"),
                        }
                    }
                }
                DecodeEvent::ImageEnd(ImageKind::Default) => default_ends += 1,
                DecodeEvent::ImageEnd(_) => assert!(!aborted),
                _ => (),
            },
            DecodeStatus::NeedInput => {
                assert_eq!(offset, end);
                if offset == bytes.len() {
                    decoder.finish()?;
                    panic!("incomplete PNG accepted");
                }
            }
            DecodeStatus::Done => {
                decoder.finish()?;
                assert_eq!(default_ends, 1);
                return Ok((offset, errors));
            }
        }
    }
}

#[test]
fn independent_valid_animation_and_recovered_static_samples_all_formats() {
    let mut count = 0;
    for (color, depth) in fixtures::formats() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let fixture = Fixture::new(color, depth, interlaced, default_is_frame);
                let wire = fixture.wire();
                let mut reference = png::Decoder::new(std::io::Cursor::new(wire.as_slice()))
                    .read_info()
                    .unwrap();
                if !default_is_frame {
                    reference_frame(&mut reference, &fixture.image);
                }
                for frame in &fixture.frames {
                    reference_frame(&mut reference, frame);
                }
                reference.finish().unwrap();
                let strict = decode(&wire).unwrap();
                assert_eq!(strict.image, fixture.image);
                assert_eq!(strict.palette, fixture.palette);
                assert!(strict.animation_error.is_none());
                let animation = strict.animation.as_ref().unwrap();
                assert_eq!(animation.default_is_frame, default_is_frame);
                assert_eq!(animation.frames.len(), 2);
                for (index, frame) in animation.frames.iter().enumerate() {
                    assert_eq!(
                        frame.image.as_ref().unwrap_or(&strict.image),
                        &fixture.frames[index]
                    );
                }
                assert_eq!(decode_with_options(&wire, &recovery()).unwrap(), strict);
                assert_eq!(stream(&wire, 1, &fixture).unwrap(), (wire.len(), 0));
                for (name, wire) in fixture.cases() {
                    assert!(
                        decode(&wire).is_err(),
                        "{color:?}/{depth}/{interlaced}/{default_is_frame}/{name}"
                    );
                    for result in [
                        decode_with_options(&wire, &recovery()),
                        decode_reader(Fragments(&wire, 1), &recovery()),
                        decode_reader(Fragments(&wire, 7), &recovery()),
                    ] {
                        let png = result.unwrap_or_else(|error| {
                            panic!(
                                "{color:?}/{depth}/{interlaced}/{default_is_frame}/{name}: {error}"
                            )
                        });
                        if name.starts_with("crc-") || name == "frame-adler" {
                            assert_eq!(png.animation_error, Some(Error::Checksum));
                        }
                        if name == "multiple-animation-faults" || name == "frame-excess-output" {
                            assert!(matches!(png.animation_error, Some(Error::Invalid(_))));
                        }
                        assert_static(&png, &fixture);
                    }
                    for fragment in [1, 7, wire.len()] {
                        assert_eq!(
                            stream(&wire, fragment, &fixture).unwrap(),
                            (wire.len(), 1),
                            "{name}"
                        );
                    }
                    count += 1;
                }
            }
        }
    }
    println!(
        "Recovered {count} independent APNG faults across 60 format/interlace/default-role configurations"
    );
}

fn permutations(values: &mut [usize], at: usize, f: &mut impl FnMut(&[usize])) {
    if at == values.len() {
        f(values);
    } else {
        for i in at..values.len() {
            values.swap(at, i);
            permutations(values, at + 1, f);
            values.swap(at, i);
        }
    }
}

#[test]
fn default_control_before_actl_all_formats_faults_and_deferred_events() {
    let mut faults = 0;
    for (color, depth) in fixtures::formats() {
        for interlaced in [false, true] {
            let mut fixture = Fixture::new(color, depth, interlaced, true);
            let expected = decode(&fixture.wire()).unwrap();
            let ac = fixture
                .chunks
                .iter()
                .position(|c| &c.kind == b"acTL")
                .unwrap();
            let fc = fixture
                .chunks
                .iter()
                .position(|c| &c.kind == b"fcTL")
                .unwrap();
            fixture.chunks.swap(ac, fc);
            let wire = fixture.wire();
            assert_eq!(decode(&wire).unwrap(), expected);
            assert_eq!(decode_with_options(&wire, &recovery()).unwrap(), expected);
            assert_eq!(
                decode_reader(Fragments(&wire, 1), &Default::default()).unwrap(),
                expected
            );
            let mut reference = png::Decoder::new(std::io::Cursor::new(wire.as_slice()))
                .read_info()
                .unwrap();
            for frame in &fixture.frames {
                reference_frame(&mut reference, frame);
            }
            reference.finish().unwrap();
            for fragment in [1, 7, wire.len()] {
                assert_eq!(stream(&wire, fragment, &fixture).unwrap(), (wire.len(), 0));
            }
            // The public event order stays Animation -> default FrameControl,
            // even when the wire has fcTL first. The control owns no row data.
            let end = 8 + fixture
                .chunks
                .iter()
                .take(fc + 1)
                .map(|c| 12 + c.data.len())
                .sum::<usize>();
            let mut decoder = StreamDecoder::new(Default::default());
            let mut offset = 0;
            while offset < end {
                let p = decoder.process(&wire[offset..end]).unwrap();
                offset += p.consumed;
                match decoder.event() {
                    Some(DecodeEvent::FrameControl { .. }) => panic!("control before animation"),
                    Some(DecodeEvent::Animation {
                        frames: 2,
                        plays: 3,
                    }) => assert_eq!(offset, end),
                    _ => (),
                }
            }
            let p = decoder.process(&[]).unwrap();
            assert_eq!((p.consumed, p.status), (0, DecodeStatus::Event));
            assert!(matches!(
                decoder.event(),
                Some(DecodeEvent::FrameControl {
                    index: 0,
                    uses_default_image: true,
                    ..
                })
            ));
            assert_eq!(decoder.finish(), Err(Error::Truncated));
            let options = DecodeOptions {
                limits: Limits {
                    max_frames: 0,
                    ..Default::default()
                },
                ..recovery()
            };
            assert_eq!(
                decode_with_options(&wire, &options),
                Err(Error::LimitExceeded)
            );
            assert_eq!(
                decode_reader(Fragments(&wire, 1), &options),
                Err(Error::LimitExceeded)
            );
            for (name, bytes) in fixture.cases() {
                assert!(decode(&bytes).is_err(), "{name}");
                assert_static(&decode_with_options(&bytes, &recovery()).unwrap(), &fixture);
                assert_static(
                    &decode_reader(Fragments(&bytes, 1), &recovery()).unwrap(),
                    &fixture,
                );
                for fragment in [1, 7, bytes.len()] {
                    assert_eq!(
                        stream(&bytes, fragment, &fixture).unwrap(),
                        (bytes.len(), 1),
                        "{name}"
                    );
                }
                faults += 1;
            }
        }
    }
    assert_eq!(faults, 1530);
}

#[test]
fn animation_chunk_permutations_and_reencoding_clear_diagnostics() {
    let fixture = Fixture::new(deadlib_image_png::ColorType::Rgba, 16, true, true);
    let positions: Vec<_> = fixture
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| fixtures::is_animation(&c.kind) || &c.kind == b"IDAT")
        .map(|(i, _)| i)
        .collect();
    assert_eq!(positions.len(), 5);
    let mut count = 0;
    permutations(&mut [0, 1, 2, 3, 4], 0, &mut |order| {
        let mut changed = fixture.chunks.clone();
        for (slot, &position) in positions.iter().enumerate() {
            changed[position] = fixture.chunks[positions[order[slot]]].clone();
        }
        let wire = fixtures::file(&changed);
        let valid = order == [0, 1, 2, 3, 4] || order == [1, 0, 2, 3, 4];
        assert_eq!(decode(&wire).is_ok(), valid, "{order:?}");
        let png = decode_with_options(&wire, &recovery()).unwrap();
        if valid {
            assert!(png.animation.is_some());
            assert!(png.animation_error.is_none());
        } else {
            assert_eq!(png.image, fixture.image);
            assert!(png.animation.is_none());
            assert!(png.animation_error.is_some());
            let mut actual = png;
            actual.animation_error = None;
            let static_chunks: Vec<_> = changed
                .iter()
                .filter(|c| !fixtures::is_animation(&c.kind))
                .cloned()
                .collect();
            assert_eq!(actual, decode(&fixtures::file(&static_chunks)).unwrap());
        }
        count += 1;
    });
    assert_eq!(count, 120);
    let wire = &fixture.cases()[0].1;
    let png = decode_with_options(wire, &recovery()).unwrap();
    let mut streamed = Vec::new();
    encode_writer(&png, &Default::default(), &mut streamed).unwrap();
    for wire in [encode(&png, &Default::default()).unwrap(), streamed] {
        let actual = decode(&wire).unwrap();
        assert!(actual.animation.is_none());
        assert!(actual.animation_error.is_none());
        assert_eq!(actual.image, fixture.image);
    }
}

#[test]
fn recovery_never_accepts_bad_core_png_or_bypasses_limits() {
    let fixture = Fixture::new(deadlib_image_png::ColorType::Rgb, 8, false, false);
    let mut base = fixture.chunks.clone();
    base.iter_mut()
        .find(|c| &c.kind == b"acTL")
        .unwrap()
        .data
        .clear();
    let mut cases = Vec::new();
    for kind in [*b"IHDR", *b"IDAT", *b"IEND", *b"tEXt"] {
        let mut c = base.clone();
        c.iter_mut().find(|c| c.kind == kind).unwrap().bad_crc = true;
        cases.push(fixtures::file(&c));
    }
    let mut c = base.clone();
    c.iter_mut()
        .find(|c| &c.kind == b"IEND")
        .unwrap()
        .data
        .push(1);
    cases.push(fixtures::file(&c));
    let mut c = base.clone();
    c.iter_mut().find(|c| &c.kind == b"IDAT").unwrap().data[0] = 0;
    cases.push(fixtures::file(&c));
    let mut c = base.clone();
    c.insert(2, Chunk::new(*b"ABCD", Vec::new()));
    cases.push(fixtures::file(&c));
    let mut c = base.clone();
    c.insert(0, Chunk::new(*b"fcTL", vec![0; 26]));
    cases.push(fixtures::file(&c));
    let mut c = base.clone();
    c.insert(2, Chunk::new(*b"ac1L", Vec::new()));
    cases.push(fixtures::file(&c));
    let mut c = base.clone();
    c.insert(2, Chunk::new(*b"gAMA", vec![0; 4]));
    cases.push(fixtures::file(&c));
    let mut c = base.clone();
    let end = c.len() - 1;
    c.insert(end, Chunk::new(*b"IDAT", Vec::new()));
    cases.push(fixtures::file(&c));
    let mut truncated = fixtures::file(&base);
    truncated.pop();
    cases.push(truncated);
    let mut trailing = fixtures::file(&base);
    trailing.extend(b"tail");
    cases.push(trailing);
    for wire in cases {
        assert!(decode_with_options(&wire, &recovery()).is_err());
        assert!(decode_reader(Fragments(&wire, 1), &recovery()).is_err());
        let mut decoder = StreamDecoder::new(recovery());
        let mut offset = 0;
        loop {
            match decoder.process(&wire[offset..]) {
                Err(error) => {
                    assert_eq!(decoder.process(&wire), Err(error.clone()));
                    assert_eq!(decoder.finish(), Err(error));
                    assert!(decoder.event().is_none());
                    break;
                }
                Ok(progress) => {
                    offset += progress.consumed;
                    if progress.status == DecodeStatus::Done {
                        assert!(offset < wire.len());
                        break;
                    }
                    if progress.status == DecodeStatus::NeedInput && offset == wire.len() {
                        assert_eq!(decoder.finish(), Err(Error::Truncated));
                        break;
                    }
                }
            }
        }
    }
    let valid = fixture.wire();
    for limits in [
        Limits {
            max_frames: 0,
            ..Default::default()
        },
        Limits {
            max_input_bytes: valid.len() - 1,
            ..Default::default()
        },
        Limits {
            max_chunks: 2,
            ..Default::default()
        },
        Limits {
            max_metadata_bytes: 0,
            ..Default::default()
        },
        Limits {
            max_output_bytes: fixture.image.width as usize * fixture.image.height as usize * 3 - 1,
            ..Default::default()
        },
    ] {
        let options = DecodeOptions {
            limits,
            ..recovery()
        };
        assert_eq!(
            decode_with_options(&valid, &options),
            Err(Error::LimitExceeded)
        );
        assert_eq!(
            decode_reader(Fragments(&valid, 1), &options),
            Err(Error::LimitExceeded)
        );
    }
    // A frame allocation that exceeds the cumulative budget is fatal too.
    let options = DecodeOptions {
        limits: Limits {
            max_output_bytes: 21,
            ..Default::default()
        },
        ..recovery()
    };
    assert_eq!(
        decode_with_options(&valid, &options),
        Err(Error::LimitExceeded)
    );
    // Header events must not let the collecting reader allocate past its cap,
    // even when no IDAT or APNG data has arrived yet.
    let mut header = fixture.chunks[0].clone();
    header.data[..8].copy_from_slice(&[0x7f, 255, 255, 255, 0x7f, 255, 255, 255]);
    let wire = fixtures::file(&[header]);
    let options = DecodeOptions {
        limits: Limits {
            max_output_bytes: 1,
            ..Default::default()
        },
        ..recovery()
    };
    assert_eq!(
        decode_with_options(&wire, &options),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        decode_reader(Fragments(&wire, 1), &options),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn all_truncations_io_errors_and_multiple_fdat_boundaries() {
    let fixture = Fixture::new(deadlib_image_png::ColorType::Rgba, 16, true, false);
    let damaged = &fixture.cases()[0].1;
    for end in 0..damaged.len() {
        assert!(
            decode_with_options(&damaged[..end], &recovery()).is_err(),
            "end={end}"
        );
        assert!(decode_reader(Fragments(&damaged[..end], 1), &recovery()).is_err());
    }
    struct Fails<'a>(&'a [u8]);
    impl Read for Fails<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if self.0.is_empty() {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                self.0.read(out)
            }
        }
    }
    assert!(matches!(
        decode_reader(Fails(damaged), &recovery()),
        Err(Error::Io(_))
    ));
    // Split the final frame's zlib at every byte, including empty data parts.
    let last = fixture
        .chunks
        .iter()
        .rposition(|c| &c.kind == b"fdAT")
        .unwrap();
    let data = &fixture.chunks[last].data;
    let sequence = u32::from_be_bytes(data[..4].try_into().unwrap());
    for split in 0..=data.len() - 4 {
        for bad in [None, Some(0), Some(1)] {
            let mut chunks = fixture.chunks.clone();
            let mut first = sequence.to_be_bytes().to_vec();
            first.extend_from_slice(&data[4..4 + split]);
            let mut second = (sequence + 1).to_be_bytes().to_vec();
            second.extend_from_slice(&data[4 + split..]);
            chunks[last] = Chunk::new(*b"fdAT", first);
            chunks.insert(last + 1, Chunk::new(*b"fdAT", second));
            if let Some(part) = bad {
                chunks[last + part].bad_crc = true;
            }
            let wire = fixtures::file(&chunks);
            let png = decode_with_options(&wire, &recovery()).unwrap();
            if bad.is_some() {
                assert_static(&png, &fixture);
            } else {
                assert_eq!(png, decode(&wire).unwrap());
            }
            for fragment in [1, 2, 7, wire.len()] {
                assert_eq!(
                    stream(&wire, fragment, &fixture).unwrap(),
                    (wire.len(), usize::from(bad.is_some()))
                );
            }
        }
    }
}

#[test]
fn palette_strictness_keeps_static_and_animation_errors_distinct() {
    use deadlib_image_png::{ColorType, PalettePolicy};
    for default_is_frame in [false, true] {
        let mut fixture = Fixture::new(ColorType::Indexed, 2, true, default_is_frame);
        fixture.palette.truncate(2);
        fixture.image.samples = Samples::U8(vec![0; 6]);
        if default_is_frame {
            fixture.frames[0] = fixture.image.clone();
        }
        fixture
            .chunks
            .iter_mut()
            .find(|c| &c.kind == b"PLTE")
            .unwrap()
            .data = fixture.palette.iter().flatten().copied().collect();
        fixture
            .chunks
            .iter_mut()
            .find(|c| &c.kind == b"IDAT")
            .unwrap()
            .data = chunks::stored(&fixtures::filtered(&fixture.image, true));
        let wire = fixture.wire();
        let strict_pixels = DecodeOptions {
            palette_policy: PalettePolicy::Strict,
            ..recovery()
        };
        let png = decode_with_options(&wire, &strict_pixels).unwrap();
        assert_static(&png, &fixture);
        for fragment in [1, 2, 7, wire.len()] {
            assert_eq!(
                stream_options(&wire, fragment, &fixture, strict_pixels).unwrap(),
                (wire.len(), 1)
            );
        }
        // Default indexed recovery retains the invalid frame indices and uses
        // opaque black when expanded, so it need not abort this animation.
        let png = decode_with_options(&wire, &recovery()).unwrap();
        assert!(png.animation.is_some());
        assert!(png.animation_error.is_none());
        fixture.image.samples = Samples::U8(vec![3; 6]);
        fixture
            .chunks
            .iter_mut()
            .find(|c| &c.kind == b"IDAT")
            .unwrap()
            .data = chunks::stored(&fixtures::filtered(&fixture.image, true));
        fixture
            .chunks
            .iter_mut()
            .find(|c| &c.kind == b"acTL")
            .unwrap()
            .data
            .clear();
        let wire = fixture.wire();
        assert!(decode_with_options(&wire, &strict_pixels).is_err());
        let png = decode_with_options(&wire, &recovery()).unwrap();
        assert_eq!(png.image, fixture.image);
        assert!(png.animation_error.is_some());
        assert_eq!(
            png.to_rgba16(Limits::default()).unwrap().samples,
            Samples::U16([0, 0, 0, 65535].repeat(6))
        );
    }
}

#[test]
fn discarded_large_chunks_stay_bounded_and_report_only_the_first_fault() {
    let fixture = Fixture::new(deadlib_image_png::ColorType::Rgb, 8, false, false);
    let mut modified = fixture.chunks.clone();
    let first = modified.iter().position(|c| &c.kind == b"acTL").unwrap();
    modified[first].data.clear();
    let mut large = Chunk::new(*b"fcTL", vec![0; 65536]);
    large.bad_crc = true;
    modified.insert(first + 1, large);
    let wire = fixtures::file(&modified);
    let options = DecodeOptions {
        limits: Limits {
            max_metadata_bytes: 23,
            ..Default::default()
        },
        ..recovery()
    };
    let png = decode_reader(Fragments(&wire, 7), &options).unwrap();
    assert_static(&png, &fixture);
    assert!(matches!(png.animation_error, Some(Error::Invalid(_))));
    for fragment in [1, 7, 8192, wire.len()] {
        assert_eq!(
            stream_options(&wire, fragment, &fixture, options).unwrap(),
            (wire.len(), 1)
        );
    }
    let mut decoder = StreamDecoder::new(options);
    let mut offset = 0;
    loop {
        let progress = decoder.process(&wire[offset..]).unwrap();
        offset += progress.consumed;
        if progress.status == DecodeStatus::Done {
            break;
        }
    }
    assert!(decoder.peak_image_buffer_bytes() < 40 * 1024);
    assert_eq!(offset, wire.len());
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_static_fallback_known_answers_and_crc_policy() {
    use std::{fs, path::PathBuf, process::Command};
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required"),
    );
    let scratch =
        std::env::temp_dir().join(format!("deadlib-apng-recovery-{}.png", std::process::id()));
    let (mut exact, mut crc_rejected) = (0, 0);
    for (color, depth) in fixtures::formats() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let fixture = Fixture::new(color, depth, interlaced, default_is_frame);
                let mut inputs = vec![
                    ("static".into(), fixture.static_wire()),
                    ("valid".into(), fixture.wire()),
                ];
                inputs.extend(fixture.cases());
                for (name, bytes) in inputs {
                    fs::write(&scratch, bytes).unwrap();
                    let output = Command::new(&oracle).arg(&scratch).output().unwrap();
                    if name.starts_with("crc-") || name == "multiple-animation-faults" {
                        assert_eq!(output.status.code(), Some(1));
                        crc_rejected += 1;
                        continue;
                    }
                    assert!(
                        output.status.success(),
                        "{name}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let bytes = &output.stdout;
                    assert_eq!(
                        bytes[..16],
                        [
                            3u32.to_be_bytes(),
                            2u32.to_be_bytes(),
                            u32::from(depth).to_be_bytes(),
                            (color as u32).to_be_bytes()
                        ]
                        .concat()
                    );
                    assert_packed(&bytes[16..], &fixture.image);
                    exact += 1;
                }
            }
        }
    }
    fs::remove_file(scratch).unwrap();
    println!(
        "libpng APNG static images: exact native defaults={exact}, strict CRC rejections={crc_rejected}"
    );
    assert_eq!(exact, 2880);
    assert_eq!(crc_rejected, 240);
}
