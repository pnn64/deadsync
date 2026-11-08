//! Decoder language-tag preservation; no locale-extension interpretation.
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/language_extensions.rs"]
mod fields;
use deadlib_image_png::{
    AnimationPolicy, DecodeEvent, DecodeOptions, DecodeStatus, Error, ImageKind, Metadata, Png,
    RowSamples, Samples, StreamDecoder, decode_reader, decode_with_options,
};
use fields::{Case, Outcome};
use std::io::{self, Read};

struct Fragment<'a>(&'a [u8], usize);
impl Read for Fragment<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let n = bytes.len().min(self.1);
        self.0.read(&mut bytes[..n])
    }
}
fn value(c: &Case, compressed: bool) -> Metadata {
    match c.outcome {
        Outcome::Typed => Metadata::InternationalText {
            keyword: b"Comment".to_vec(),
            language: String::from_utf8(c.tag.clone()).unwrap(),
            translated_keyword: fields::TRANSLATED.into(),
            text: fields::TEXT.into(),
            compressed,
        },
        Outcome::Opaque => Metadata::Unknown {
            kind: *b"iTXt",
            data: c.data(compressed),
        },
        Outcome::Malformed => panic!("no typed value for malformed tag"),
    }
}
fn known(
    result: Result<Png, Error>,
    c: &Case,
    compressed: bool,
    position: deadlib_image_png::ChunkPosition,
) {
    if c.outcome == Outcome::Malformed {
        assert!(
            matches!(result, Err(Error::Invalid(_))),
            "{}: {result:?}",
            c.name
        );
        return;
    }
    let png = result.unwrap_or_else(|e| panic!("{}: {e}", c.name));
    assert_eq!(png.image.samples, Samples::U8(chunks::PIXEL.to_vec()));
    assert_eq!(
        (png.image.width, png.image.height, png.image.bit_depth),
        (1, 1, 8)
    );
    assert!(png.animation.is_none() && png.animation_error.is_none());
    assert_eq!(png.ancillary.len(), 1);
    assert_eq!(
        (
            png.ancillary[0].kind,
            &png.ancillary[0].data,
            png.ancillary[0].position
        ),
        (*b"iTXt", &c.data(compressed), position)
    );
    assert_eq!(
        png.metadata(Default::default()).unwrap(),
        vec![value(c, compressed)],
        "{}",
        c.name
    );
}
fn push(
    bytes: &[u8],
    c: &Case,
    compressed: bool,
    position: deadlib_image_png::ChunkPosition,
    fragment: usize,
    options: DecodeOptions,
) {
    let mut decoder = StreamDecoder::new(options);
    let (mut offset, mut metadata, mut rows, mut ends) = (0, 0, 0, 0);
    for _ in 0..bytes.len() * 4 + 64 {
        let end = bytes.len().min(offset + fragment);
        let p = match decoder.process(&bytes[offset..end]) {
            Ok(p) => p,
            Err(e) => {
                assert_eq!(c.outcome, Outcome::Malformed, "{}: {e}", c.name);
                assert!(matches!(e, Error::Invalid(_)));
                assert_eq!(decoder.finish(), Err(e.clone()));
                assert_eq!(decoder.process(&[]), Err(e));
                assert!(decoder.event().is_none() && !decoder.is_done());
                return;
            }
        };
        offset += p.consumed;
        assert_eq!(offset, decoder.total_in());
        match decoder.event() {
            Some(DecodeEvent::Metadata {
                chunk,
                value: actual,
            }) => {
                metadata += 1;
                assert_eq!(
                    (chunk.kind, &chunk.data, chunk.position),
                    (*b"iTXt", &c.data(compressed), position)
                );
                assert_eq!(actual, &value(c, compressed), "{}", c.name);
            }
            Some(DecodeEvent::Row(row)) => {
                rows += 1;
                assert_eq!(
                    (row.image, row.x, row.y, row.width),
                    (ImageKind::Default, 0, 0, 1)
                );
                assert_eq!(row.samples, RowSamples::U8(&chunks::PIXEL));
            }
            Some(DecodeEvent::ImageEnd(image)) => {
                assert_eq!(image, ImageKind::Default);
                ends += 1;
            }
            Some(DecodeEvent::AnimationError(e)) => panic!("unexpected animation recovery: {e}"),
            _ => (),
        }
        match p.status {
            DecodeStatus::Done => {
                assert_ne!(c.outcome, Outcome::Malformed, "{}", c.name);
                assert_eq!((metadata, rows, ends), (1, 1, 1));
                assert_eq!(offset, bytes.len());
                decoder.finish().unwrap();
                return;
            }
            DecodeStatus::NeedInput if offset == bytes.len() => {
                panic!("{}: incomplete PNG", c.name)
            }
            _ => (),
        }
    }
    panic!("{}: stalled parser", c.name);
}
fn reference(bytes: &[u8], c: &Case) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut pixels = [0; 3];
    reader.next_frame(&mut pixels).unwrap();
    reader.finish().unwrap();
    assert_eq!(pixels, chunks::PIXEL);
    let texts = &reader.info().utf8_text;
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].keyword, "Comment");
    assert_eq!(texts[0].language_tag.as_bytes(), c.tag);
    assert_eq!(texts[0].translated_keyword, fields::TRANSLATED);
    assert_eq!(texts[0].get_text().unwrap(), fields::TEXT);
}
#[test]
fn extension_singletons_payload_bounds_uniqueness_and_raw_preservation() {
    let registered: Vec<_> = include_str!("fixtures/language/language-tag-extensions-registry")
        .lines()
        .filter_map(|line| line.strip_prefix("Identifier: "))
        .collect();
    assert_eq!(registered, ["t", "u"]);
    let mut counts = [0usize; 3];
    let (mut files, mut reference_files) = (0, 0);
    for c in fields::cases() {
        counts[match c.outcome {
            Outcome::Typed => 0,
            Outcome::Opaque => 1,
            Outcome::Malformed => 2,
        }] += 1;
        for compressed in [false, true] {
            for position in chunks::POSITIONS {
                let bytes = c.bytes(compressed, position);
                for policy in [AnimationPolicy::Strict, AnimationPolicy::RecoverDefault] {
                    let options = DecodeOptions {
                        animation_policy: policy,
                        ..Default::default()
                    };
                    known(
                        decode_with_options(&bytes, &options),
                        &c,
                        compressed,
                        position,
                    );
                    for fragment in [1, 7] {
                        known(
                            decode_reader(Fragment(&bytes, fragment), &options),
                            &c,
                            compressed,
                            position,
                        );
                    }
                    for fragment in [1, 31] {
                        push(&bytes, &c, compressed, position, fragment, options);
                    }
                }
                if c.outcome != Outcome::Malformed {
                    reference(&bytes, &c);
                    reference_files += 1;
                }
                files += 1;
            }
        }
    }
    assert_eq!(counts, [48, 2434, 276]);
    assert_eq!((files, reference_files), (16548, 14892));
    println!(
        "Language extension audit: {files} files, {} API/policy checks; {} typed / {} opaque / {} malformed tags; {reference_files} exact Rust png metadata/sample comparisons",
        files * 10,
        counts[0],
        counts[1],
        counts[2]
    );
}
