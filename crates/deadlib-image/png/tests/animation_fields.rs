//! PNG Third Edition acTL/fcTL/fdAT scalar fields and fragmented recovery.
#[allow(dead_code)]
#[path = "support/apng_recovery.rs"]
mod apng_recovery;
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/animation_fields.rs"]
mod fields;
use apng_recovery::Fixture;
use deadlib_image_png::{
    AnimationPolicy, ColorType, DecodeEvent, DecodeOptions, DecodeStatus, Error, Image, ImageKind,
    Limits, Png, RowSamples, Samples, StreamDecoder, decode_reader, decode_with_options,
};
use fields::{Case, Outcome};
use std::{
    collections::BTreeMap,
    io::{self, Read},
};

fn options(policy: AnimationPolicy) -> DecodeOptions {
    DecodeOptions {
        animation_policy: policy,
        limits: Limits {
            max_input_bytes: 8192,
            max_output_bytes: 128,
            max_metadata_bytes: 4096,
            max_chunks: 64,
            max_frames: 4,
        },
        ..Default::default()
    }
}
struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.1);
        self.0.read(&mut out[..n])
    }
}
fn known(png: &Png, f: &Fixture, c: &Case) {
    assert_eq!(png.image, f.image, "{}", c.name);
    assert_eq!(png.palette, f.palette, "{}", c.name);
    assert_eq!(png.interlaced, f.interlaced);
    assert_eq!(png.ancillary.len(), 2);
    for (chunk, data) in png
        .ancillary
        .iter()
        .zip([b"Stage\0before".as_slice(), b"Stage\0after"])
    {
        assert_eq!(chunk.kind, *b"tEXt");
        assert_eq!(chunk.data, data);
    }
    if c.outcome == Outcome::Valid {
        assert!(png.animation_error.is_none(), "{}", c.name);
        let animation = png.animation.as_ref().unwrap();
        assert_eq!(animation.plays, c.plays, "{}", c.name);
        assert_eq!(animation.default_is_frame, f.default_is_frame);
        assert_eq!(animation.frames.len(), 2);
        for (index, frame) in animation.frames.iter().enumerate() {
            assert_eq!(frame.control, c.controls[index], "{}", c.name);
            assert_eq!(
                frame.image.as_ref().unwrap_or(&png.image),
                c.image(f, index),
                "{}",
                c.name
            );
            assert_eq!(frame.image.is_none(), index == 0 && f.default_is_frame);
        }
    } else {
        assert_eq!(c.outcome, Outcome::AnimationFault);
        assert!(png.animation.is_none(), "{}", c.name);
        assert!(
            matches!(png.animation_error, Some(Error::Invalid(_))),
            "{}",
            c.name
        );
    }
}
fn expect(result: Result<Png, Error>, f: &Fixture, c: &Case, policy: AnimationPolicy) {
    match (c.outcome, policy) {
        (Outcome::Resource, _) => assert_eq!(result, Err(Error::LimitExceeded), "{}", c.name),
        (Outcome::AnimationFault, AnimationPolicy::Strict) => {
            assert!(
                matches!(result, Err(Error::Invalid(_))),
                "{}: {result:?}",
                c.name
            );
        }
        _ => known(&result.unwrap_or_else(|e| panic!("{}: {e}", c.name)), f, c),
    }
}
fn row(row: deadlib_image_png::Row<'_>, expected: &Image) {
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
fn push(bytes: &[u8], fragment: usize, f: &Fixture, c: &Case, policy: AnimationPolicy) {
    let mut decoder = StreamDecoder::new(options(policy));
    let (mut offset, mut controls, mut errors, mut default_ends, mut frame_ends) = (0, 0, 0, 0, 0);
    for _ in 0..bytes.len() * 4 + 64 {
        let end = bytes.len().min(offset + fragment);
        let p = match decoder.process(&bytes[offset..end]) {
            Ok(p) => p,
            Err(error) => {
                match c.outcome {
                    Outcome::Resource => assert_eq!(error, Error::LimitExceeded, "{}", c.name),
                    Outcome::AnimationFault => {
                        assert_eq!(policy, AnimationPolicy::Strict, "{}: {error}", c.name);
                        assert!(matches!(error, Error::Invalid(_)), "{}: {error}", c.name);
                    }
                    Outcome::Valid => panic!("{}: {error}", c.name),
                }
                assert_eq!(decoder.finish(), Err(error.clone()));
                assert_eq!(decoder.process(&[]), Err(error));
                assert!(decoder.event().is_none() && !decoder.is_done());
                return;
            }
        };
        offset += p.consumed;
        assert_eq!(decoder.total_in(), offset);
        match decoder.event() {
            Some(DecodeEvent::Animation { frames, plays }) => {
                assert_eq!(errors, 0);
                assert_eq!(plays, c.plays);
                assert!((1..=4).contains(&frames));
            }
            Some(DecodeEvent::AnimationError(error)) => {
                assert_eq!(
                    (c.outcome, policy),
                    (Outcome::AnimationFault, AnimationPolicy::RecoverDefault)
                );
                assert_eq!(errors, 0);
                assert!(matches!(error, Error::Invalid(_)));
                errors += 1;
            }
            Some(DecodeEvent::FrameControl {
                index,
                control,
                uses_default_image,
            }) => {
                assert_eq!(errors, 0);
                assert_eq!(index as usize, controls);
                assert_eq!(control, c.controls[index as usize], "{}", c.name);
                assert_eq!(uses_default_image, index == 0 && f.default_is_frame);
                controls += 1;
            }
            Some(DecodeEvent::Row(r)) => {
                let image = match r.image {
                    ImageKind::Default => &f.image,
                    ImageKind::AnimationFrame(index) => {
                        assert_eq!(errors, 0);
                        c.image(f, index as usize)
                    }
                };
                row(r, image);
            }
            Some(DecodeEvent::ImageEnd(ImageKind::Default)) => default_ends += 1,
            Some(DecodeEvent::ImageEnd(ImageKind::AnimationFrame(_))) => {
                assert_eq!(errors, 0);
                frame_ends += 1;
            }
            _ => (),
        }
        match p.status {
            DecodeStatus::Done => {
                assert_eq!(offset, bytes.len());
                decoder.finish().unwrap();
                assert_eq!(default_ends, 1);
                match c.outcome {
                    Outcome::Valid => {
                        assert_eq!(
                            (errors, controls, frame_ends),
                            (0, 2, 2 - usize::from(f.default_is_frame))
                        );
                    }
                    Outcome::AnimationFault => assert_eq!(errors, 1),
                    Outcome::Resource => panic!("{}: resource accepted", c.name),
                }
                return;
            }
            DecodeStatus::NeedInput if offset == bytes.len() => {
                panic!("{}: unexpected EOF: {:?}", c.name, decoder.finish())
            }
            _ => (),
        }
    }
    panic!("{}: push stalled", c.name);
}
#[test]
fn control_integer_flag_length_and_rectangle_equivalence_classes() {
    let mut counts = BTreeMap::<&str, [usize; 3]>::new();
    for (color, depth) in apng_recovery::formats() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let f = Fixture::new(color, depth, interlaced, default_is_frame);
                fields::fields(&f, true, |c| {
                    let bytes = c.bytes();
                    counts.entry(c.category).or_default()[match c.outcome {
                        Outcome::Valid => 0,
                        Outcome::AnimationFault => 1,
                        Outcome::Resource => 2,
                    }] += 1;
                    for policy in [AnimationPolicy::Strict, AnimationPolicy::RecoverDefault] {
                        expect(
                            decode_with_options(&bytes, &options(policy)),
                            &f,
                            &c,
                            policy,
                        );
                        for fragment in [1, 7] {
                            expect(
                                decode_reader(Fragments(&bytes, fragment), &options(policy)),
                                &f,
                                &c,
                                policy,
                            );
                        }
                        for fragment in [1, 31] {
                            push(&bytes, fragment, &f, &c, policy);
                        }
                    }
                });
            }
        }
    }
    let total: usize = counts.values().flatten().sum();
    assert_eq!(total, 80460);
    println!(
        "Animation scalar audit: {total} files; {} API/policy checks",
        total * 10
    );
    for (category, [valid, invalid, resource]) in counts {
        println!(
            "Animation field {category}: {valid} valid, {invalid} animation faults, {resource} fatal resource cases"
        );
    }
}
#[test]
fn every_delay_word_retains_exact_unsigned_values_and_zero_denominator() {
    let f = Fixture::new(ColorType::Rgb, 8, false, false);
    let mut count = 0;
    for offset in [20, 22] {
        for value in 0..=u16::MAX {
            let c = fields::timing(&f, 0, offset, value);
            let bytes = c.bytes();
            let png = decode_with_options(&bytes, &options(AnimationPolicy::Strict)).unwrap();
            known(&png, &f, &c);
            let control = png.animation.as_ref().unwrap().frames[0].control;
            let denominator = if control.delay_den == 0 {
                100.0
            } else {
                f64::from(control.delay_den)
            };
            let recovered = control.delay_seconds() * denominator;
            assert!(
                (recovered - f64::from(control.delay_num)).abs()
                    <= f64::EPSILON * f64::from(control.delay_num).max(1.0),
                "{}",
                c.name
            );
            push(&bytes, 31, &f, &c, AnimationPolicy::Strict);
            count += 1;
        }
    }
    assert_eq!(count, 131072);
    println!(
        "Animation delay audit: {count} files; {} API checks; all 65536 values of each word",
        count * 2
    );
}

fn reference_frame(
    reader: &mut png::Reader<std::io::Cursor<&[u8]>>,
    image: &Image,
    control: Option<(deadlib_image_png::FrameControl, u32)>,
) {
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!((info.width, info.height), (image.width, image.height));
    assert_eq!(
        (info.color_type as u8, info.bit_depth as u8),
        (image.color_type as u8, image.bit_depth)
    );
    if let Some((expected, sequence)) = control {
        let actual = reader.info().frame_control().unwrap();
        assert_eq!(
            (
                actual.sequence_number,
                actual.width,
                actual.height,
                actual.x_offset,
                actual.y_offset
            ),
            (
                sequence,
                expected.width,
                expected.height,
                expected.x_offset,
                expected.y_offset
            )
        );
        assert_eq!(
            (
                actual.delay_num,
                actual.delay_den,
                actual.dispose_op as u8,
                actual.blend_op as u8
            ),
            (
                expected.delay_num,
                expected.delay_den,
                expected.dispose as u8,
                expected.blend as u8
            )
        );
    }
    if reader.info().interlaced {
        let stride = reader.output_line_size(reader.info().width).unwrap();
        bytes = (0..info.height as usize)
            .flat_map(|y| {
                bytes[y * stride..y * stride + info.line_size]
                    .iter()
                    .copied()
            })
            .collect();
    }
    let bytes = &bytes[..info.buffer_size()];
    match &image.samples {
        Samples::U16(samples) => assert_eq!(
            bytes,
            samples
                .iter()
                .flat_map(|v| v.to_be_bytes())
                .collect::<Vec<_>>()
        ),
        Samples::U8(samples) if image.bit_depth == 8 => assert_eq!(bytes, samples),
        Samples::U8(samples) => {
            let depth = usize::from(image.bit_depth);
            let width = image.width as usize;
            let stride = (width * depth).div_ceil(8);
            assert_eq!(bytes.len(), stride * image.height as usize);
            for (n, &sample) in samples.iter().enumerate() {
                assert_eq!(
                    (bytes[(n / width) * stride + (n % width) * depth / 8]
                        >> (8 - depth - (n % width) * depth % 8))
                        & ((1 << depth) - 1),
                    sample
                );
            }
        }
    }
}
fn reference(f: &Fixture, c: &Case) {
    let bytes = c.bytes();
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes.as_slice()))
        .read_info()
        .unwrap_or_else(|e| panic!("{}: {e}", c.name));
    let animation = reader.info().animation_control.unwrap();
    assert_eq!(
        (animation.num_frames, animation.num_plays),
        (2, c.plays),
        "{}",
        c.name
    );
    if !f.default_is_frame {
        reference_frame(&mut reader, &f.image, None);
    }
    let sequences: Vec<_> = c
        .chunks
        .iter()
        .filter(|chunk| matches!(&chunk.kind, b"fcTL" | b"fdAT"))
        .enumerate()
        .filter_map(|(index, chunk)| (chunk.kind == *b"fcTL").then_some(index as u32))
        .collect();
    for (index, sequence) in sequences.into_iter().enumerate() {
        reference_frame(
            &mut reader,
            c.image(f, index),
            Some((c.controls[index], sequence)),
        );
    }
    reader.finish().unwrap();
}
#[test]
#[ignore = "explicit Rust png APNG field differential audit; see tests/oracle/README.md"]
fn rust_png_control_fields_and_all_delay_words() {
    let mut count = 0;
    for (color, depth) in apng_recovery::formats() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let f = Fixture::new(color, depth, interlaced, default_is_frame);
                fields::fields(&f, true, |c| {
                    if c.outcome == Outcome::Valid {
                        reference(&f, &c);
                        count += 1;
                    }
                });
            }
        }
    }
    let f = Fixture::new(ColorType::Rgb, 8, false, false);
    for offset in [20, 22] {
        for value in 0..=u16::MAX {
            reference(&f, &fields::timing(&f, 0, offset, value));
            count += 1;
        }
    }
    assert_eq!(count, 136352);
    println!(
        "Rust png APNG scalar audit: {count} exact valid files, control values and native samples; invalid-input recovery is independently asserted"
    );
}
