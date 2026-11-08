//! Independently framed files and known resource thresholds for all native formats.
#[allow(dead_code)]
#[path = "support/apng_recovery.rs"]
mod apng_recovery;
#[path = "support/budgets.rs"]
mod budgets;
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/resource.rs"]
mod resource;
use deadlib_image_png::{
    AnimationPolicy, DecodeEvent, DecodeOptions, DecodeStatus, Error, Image, ImageKind, Limits,
    PalettePolicy, Png, Row, RowSamples, Samples, StreamDecoder, decode_reader,
    decode_with_options,
};
use std::io::{self, Read};

struct Reader<'a> {
    bytes: &'a [u8],
    fragment: usize,
    position: usize,
    interrupt: bool,
    calls: usize,
    fault: Option<(usize, bool)>,
    injected: bool,
}
impl Read for Reader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        assert!(self.calls < self.bytes.len() * 4 + 1024);
        if self.interrupt && self.calls % 2 == 1 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if let Some((at, oversized)) = self.fault
            && self.position == at
        {
            self.injected = true;
            return if oversized {
                Ok(output.len() + 1)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected boundary fault",
                ))
            };
        }
        let before_fault = self.fault.map_or(usize::MAX, |(at, _)| at - self.position);
        let count = output
            .len()
            .min(self.fragment)
            .min(self.bytes.len() - self.position)
            .min(before_fault);
        output[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

fn reader(bytes: &[u8], fragment: usize) -> Reader<'_> {
    Reader {
        bytes,
        fragment,
        position: 0,
        interrupt: false,
        calls: 0,
        fault: None,
        injected: false,
    }
}

fn assert_png(png: &Png, case: &budgets::Case, limits: Limits) {
    assert_eq!(png.image, case.image);
    assert_eq!(png.interlaced, case.interlaced);
    assert_eq!(png.palette, case.palette);
    assert!(png.animation_error.is_none());
    if case.frames.is_empty() {
        assert!(png.animation.is_none());
    } else {
        let animation = png.animation.as_ref().unwrap();
        assert_eq!(animation.default_is_frame, case.default_is_frame);
        assert_eq!(animation.frames.len(), case.frames.len());
        for (frame, expected) in animation.frames.iter().zip(&case.frames) {
            assert_eq!(frame.image.as_ref().unwrap_or(&png.image), expected);
        }
    }
    let expected: Vec<_> = case
        .chunks
        .iter()
        .filter(|chunk| matches!(&chunk.kind, b"tEXt" | b"zTXt" | b"qsAf"))
        .map(|chunk| (chunk.kind, chunk.data.as_slice()))
        .collect();
    let actual: Vec<_> = png
        .ancillary
        .iter()
        .map(|chunk| (chunk.kind, chunk.data.as_slice()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(png.metadata(limits).unwrap().len(), 4);
}

fn assert_row(image: &Image, row: Row<'_>) {
    let channels = image.color_type.channels();
    for pixel in 0..row.width as usize {
        let at =
            (row.y as usize * image.width as usize + row.x as usize + pixel * row.x_step as usize)
                * channels;
        let start = pixel * channels;
        match (&image.samples, row.samples) {
            (Samples::U8(expected), RowSamples::U8(actual)) => assert_eq!(
                &expected[at..at + channels],
                &actual[start..start + channels]
            ),
            (Samples::U16(expected), RowSamples::U16(actual)) => assert_eq!(
                &expected[at..at + channels],
                &actual[start..start + channels]
            ),
            _ => panic!("native row representation"),
        }
    }
}

fn push(
    bytes: &[u8],
    options: DecodeOptions,
    fragment: usize,
    case: Option<&budgets::Case>,
) -> Result<(), Error> {
    let mut decoder = StreamDecoder::new(options);
    let mut offset = 0;
    let mut ended = 0;
    for _ in 0..bytes.len() * 4 + options.limits.max_output_bytes + 1024 {
        let end = bytes.len().min(offset + fragment);
        let progress = match decoder.process(&bytes[offset..end]) {
            Ok(progress) => progress,
            Err(error) => {
                let consumed = decoder.total_in();
                assert!(consumed <= options.limits.max_input_bytes);
                assert!(!decoder.is_done());
                assert!(decoder.event().is_none());
                assert_eq!(decoder.finish(), Err(error.clone()));
                assert_eq!(decoder.process(bytes), Err(error.clone()));
                assert_eq!(decoder.total_in(), consumed);
                return Err(error);
            }
        };
        offset += progress.consumed;
        assert_eq!(decoder.total_in(), offset);
        if let Some(case) = case {
            match decoder.event() {
                Some(DecodeEvent::Row(row)) => {
                    let image = match row.image {
                        ImageKind::Default => &case.image,
                        ImageKind::AnimationFrame(index) => &case.frames[index as usize],
                    };
                    assert_row(image, row);
                }
                Some(DecodeEvent::ImageEnd(_)) => ended += 1,
                Some(DecodeEvent::AnimationError(_)) => {
                    panic!("valid animation recovered as faulty")
                }
                _ => (),
            }
        }
        assert!(decoder.animation_error().is_none());
        match progress.status {
            DecodeStatus::Done => {
                assert_eq!(offset, bytes.len());
                assert!(decoder.is_done());
                decoder.finish()?;
                if let Some(case) = case {
                    assert_eq!(
                        ended,
                        case.frames.len() + usize::from(!case.default_is_frame)
                    );
                }
                return Ok(());
            }
            DecodeStatus::NeedInput if offset == bytes.len() => return decoder.finish(),
            DecodeStatus::NeedInput => assert_eq!(offset, end),
            DecodeStatus::Event => (),
        }
    }
    panic!("stream decoder stalled");
}

#[test]
fn exact_input_sample_inflation_metadata_chunk_and_frame_budgets() {
    let (mut probes, mut success, mut rejected) = (0, 0, 0);
    for case in budgets::cases() {
        let bytes = case.wire();
        for policy in [AnimationPolicy::Strict, AnimationPolicy::RecoverDefault] {
            for axis in 0..5 {
                let minimum = budgets::value(case.minimum, axis);
                for value in budgets::boundaries(minimum) {
                    let mut limits = case.minimum;
                    budgets::set(&mut limits, axis, value);
                    let options = DecodeOptions {
                        limits,
                        animation_policy: policy,
                        palette_policy: PalettePolicy::Strict,
                    };
                    let pass = value >= minimum;
                    let result = decode_with_options(&bytes, &options);
                    if pass {
                        assert_png(
                            result.as_ref().unwrap_or_else(|error| {
                                panic!("{} axis={axis} value={value}: {error}", case.name)
                            }),
                            &case,
                            limits,
                        );
                        success += 1;
                    } else {
                        assert_eq!(
                            result,
                            Err(Error::LimitExceeded),
                            "{} axis={axis} value={value}",
                            case.name
                        );
                        rejected += 1;
                    }
                    for fragment in [1, 7, bytes.len()] {
                        let collected = decode_reader(reader(&bytes, fragment), &options);
                        if pass {
                            assert_png(collected.as_ref().unwrap(), &case, limits);
                        } else {
                            assert_eq!(collected, Err(Error::LimitExceeded));
                        }
                        assert_eq!(
                            push(&bytes, options, fragment, Some(&case)),
                            if pass {
                                Ok(())
                            } else {
                                Err(Error::LimitExceeded)
                            },
                            "{} axis={axis} value={value} fragment={fragment}",
                            case.name
                        );
                    }
                    probes += 1;
                }
            }
        }
    }
    assert_eq!((probes, success, rejected), (4680, 2400, 2280));
    println!(
        "Independent resource audit: 120 files, {probes} budget/policy probes, {success} accepted, {rejected} rejected, 32760 buffered/reader/push API checks"
    );
}

fn header_result(bytes: &[u8], valid: bool) {
    for animation_policy in [AnimationPolicy::Strict, AnimationPolicy::RecoverDefault] {
        let options = DecodeOptions {
            limits: Limits {
                max_output_bytes: 1,
                ..Default::default()
            },
            animation_policy,
            ..Default::default()
        };
        for result in [
            decode_with_options(bytes, &options).map(|_| ()),
            decode_reader(reader(bytes, 1), &options).map(|_| ()),
            push(bytes, options, 7, None),
        ] {
            if valid {
                assert_eq!(result, Err(Error::LimitExceeded));
            } else {
                assert!(matches!(result, Err(Error::Invalid(_))), "{result:?}");
            }
        }
    }
}

fn header_wire(chunks: &[apng_recovery::Chunk]) -> Vec<u8> {
    let mut chunks = chunks.to_vec();
    if chunks[0].data[9] == 3 {
        chunks.insert(1, apng_recovery::Chunk::new(*b"PLTE", vec![0; 3]));
    }
    apng_recovery::file(&chunks)
}

#[test]
fn ihdr_integer_color_depth_and_method_fields_fail_before_large_allocation() {
    let fixture = budgets::cases().remove(0);
    let mut chunks = fixture.chunks;
    let (mut dimensions, mut formats, mut methods) = (0, 0, 0);
    let edges = [
        0,
        1,
        2,
        0x7fff_fffe,
        0x7fff_ffff,
        0x8000_0000,
        0xffff_fffe,
        0xffff_ffff,
    ];
    for (color, depth) in budgets::FORMATS {
        chunks[0].data[8..13].copy_from_slice(&[depth, color as u8, 0, 0, 0]);
        for width in edges {
            for height in edges {
                chunks[0].data[..4].copy_from_slice(&u32::to_be_bytes(width));
                chunks[0].data[4..8].copy_from_slice(&u32::to_be_bytes(height));
                header_result(
                    &header_wire(&chunks),
                    (1..=0x7fff_ffff).contains(&width) && (1..=0x7fff_ffff).contains(&height),
                );
                dimensions += 1;
            }
        }
    }
    chunks[0].data[..8].copy_from_slice(&[0, 0, 0, 3, 0, 0, 0, 2]);
    for color in 0..=255u8 {
        for depth in 0..=255u8 {
            chunks[0].data[8..13].copy_from_slice(&[depth, color, 0, 0, 0]);
            header_result(
                &header_wire(&chunks),
                budgets::FORMATS
                    .iter()
                    .any(|&(c, d)| c as u8 == color && d == depth),
            );
            formats += 1;
        }
    }
    for (color, depth) in budgets::FORMATS {
        for offset in 10..13 {
            for value in 0..=255u8 {
                chunks[0].data[8..13].copy_from_slice(&[depth, color as u8, 0, 0, 0]);
                chunks[0].data[offset] = value;
                header_result(
                    &header_wire(&chunks),
                    value == 0 || offset == 12 && value == 1,
                );
                methods += 1;
            }
        }
    }
    assert_eq!((dimensions, formats, methods), (960, 65536, 11520));
    println!(
        "IHDR audit: {dimensions} dimension pairs, {formats} color/depth pairs, {methods} method bytes, 468096 parser/policy API checks; no giant allocation"
    );
}

#[test]
fn interruptions_and_reader_faults_at_chunk_boundaries_and_final_eof_probe() {
    let mut checks = 0;
    for case in budgets::cases() {
        let bytes = case.wire();
        let mut points = vec![0, 1, 7, 8, bytes.len()];
        let mut offset = 8;
        for chunk in &case.chunks {
            points.extend([
                offset,
                offset + 4,
                offset + 8,
                offset + 8 + chunk.data.len(),
                offset + 11 + chunk.data.len(),
            ]);
            offset += 12 + chunk.data.len();
        }
        points.sort_unstable();
        points.dedup();
        for policy in [AnimationPolicy::Strict, AnimationPolicy::RecoverDefault] {
            let options = DecodeOptions {
                limits: case.minimum,
                animation_policy: policy,
                ..Default::default()
            };
            for fragment in [1, 7, 8192] {
                let mut interrupted = reader(&bytes, fragment);
                interrupted.interrupt = true;
                assert_png(
                    &decode_reader(&mut interrupted, &options).unwrap(),
                    &case,
                    case.minimum,
                );
                assert!(interrupted.calls > 3);
                checks += 1;
                for &at in &points {
                    for oversized in [false, true] {
                        let mut source = reader(&bytes, fragment);
                        source.interrupt = true;
                        source.fault = Some((at, oversized));
                        assert!(
                            matches!(decode_reader(&mut source, &options), Err(Error::Io(_))),
                            "{} fault={at} oversized={oversized}",
                            case.name
                        );
                        assert!(source.injected);
                        checks += 1;
                    }
                }
            }
        }
    }
    println!(
        "Reader fault audit: {checks} interruption/short-read/IO/oversized-count checks across all formats, interlace modes, animation roles and EOF probing"
    );
}

#[test]
fn native_resource_fuzzer_contract_accepts_independent_boundary_and_io_seeds() {
    for case in budgets::high_ratio() {
        let bytes = case.wire();
        let options = DecodeOptions {
            limits: case.minimum,
            ..Default::default()
        };
        assert_eq!(
            decode_with_options(&bytes, &options).unwrap().image,
            case.image
        );
        assert_eq!(
            decode_reader(reader(&bytes, 1), &options).unwrap().image,
            case.image
        );
        push(&bytes, options, bytes.len(), Some(&case)).unwrap();
    }
    let seeds = budgets::seeds();
    for (_, bytes) in &seeds {
        resource::check(bytes, chunks::crc);
    }
    assert_eq!(seeds.len(), 6376);
    println!(
        "Native resource fuzzer contract: {} independent format/policy/IO/budget seeds",
        seeds.len()
    );
}
