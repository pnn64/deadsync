//! Known answers derived from PNG Third Edition 7.1 and scalar chunk tables.
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/scalar_fields.rs"]
mod fields;
use deadlib_image_png::{
    AnimationPolicy, ColorType, DecodeEvent, DecodeOptions, DecodeStatus, Error, ImageKind, Limits,
    PalettePolicy, Png, RowSamples, Samples, StreamDecoder, Transparency, decode_reader,
    decode_with_options,
};
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, Read},
};

struct Reader<'a> {
    bytes: &'a [u8],
    fragment: usize,
}
impl Read for Reader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.fragment);
        self.bytes.read(&mut out[..n])
    }
}
fn known(png: &Png, case: &fields::Case, limits: Limits) {
    assert_eq!(png.image, case.format.image(), "{}", case.name);
    assert_eq!(png.palette, case.palette(), "{}", case.name);
    assert_eq!(png.interlaced, case.format.interlaced);
    assert!(png.animation.is_none() && png.animation_error.is_none());
    assert_eq!(png.transparency, case.transparency);
    let expected: Vec<_> = case.metadata.iter().cloned().collect();
    assert_eq!(png.metadata(limits).unwrap(), expected, "{}", case.name);
    if case.metadata.is_some() {
        assert_eq!(png.ancillary.len(), 1);
        assert_eq!(png.ancillary[0].kind, case.kind);
        assert_eq!(png.ancillary[0].data, case.data);
    } else {
        assert!(png.ancillary.is_empty());
    }
    let mut rgba = [0u16; 4];
    if case.format.color == ColorType::Indexed {
        let first = case.palette()[0];
        for n in 0..3 {
            rgba[n] = u16::from(first[n]) * 257;
        }
    }
    let mask = ((1u32 << case.format.depth) - 1) as u16;
    rgba[3] = if matches!(
        case.format.color,
        ColorType::GrayscaleAlpha | ColorType::Rgba
    ) {
        0
    } else {
        match &case.transparency {
            Some(Transparency::Grayscale(v)) if v & mask == 0 => 0,
            Some(Transparency::Rgb(v)) if v.iter().all(|v| v & mask == 0) => 0,
            Some(Transparency::Palette(v)) => u16::from(v.first().copied().unwrap_or(255)) * 257,
            _ => 65535,
        }
    };
    assert_eq!(
        png.to_rgba16(limits).unwrap().samples,
        Samples::U16(rgba.to_vec()),
        "{}",
        case.name
    );
}
fn push(
    bytes: &[u8],
    case: &fields::Case,
    options: DecodeOptions,
    fragment: usize,
) -> Result<(), Error> {
    let mut decoder = StreamDecoder::new(options);
    let mut offset = 0;
    let (mut rows, mut ends, mut palettes, mut transparency, mut metadata) = (0, 0, 0, 0, 0);
    for _ in 0..bytes.len() * 4 + 64 {
        let end = bytes.len().min(offset + fragment);
        let p = match decoder.process(&bytes[offset..end]) {
            Ok(p) => p,
            Err(e) => {
                assert_eq!(rows, 0, "{}", case.name);
                assert!(matches!(e, Error::Invalid(_)), "{}: {e}", case.name);
                assert_eq!(decoder.finish(), Err(e.clone()));
                assert_eq!(decoder.process(&[]), Err(e.clone()));
                assert!(!decoder.is_done());
                assert!(decoder.event().is_none());
                return Err(e);
            }
        };
        offset += p.consumed;
        assert_eq!(offset, decoder.total_in());
        match decoder.event() {
            Some(DecodeEvent::Row(row)) => {
                rows += 1;
                assert_eq!(row.image, ImageKind::Default);
                assert_eq!((row.x, row.y, row.width), (0, 0, 1));
                match row.samples {
                    RowSamples::U8(v) => assert_eq!(v, vec![0; case.format.channels()]),
                    RowSamples::U16(v) => assert_eq!(v, vec![0; case.format.channels()]),
                }
            }
            Some(DecodeEvent::ImageEnd(kind)) => {
                assert_eq!(kind, ImageKind::Default);
                ends += 1;
            }
            Some(DecodeEvent::Palette(v)) => {
                palettes += 1;
                assert_eq!(v, case.palette());
            }
            Some(DecodeEvent::Transparency(v)) => {
                transparency += 1;
                assert_eq!(Some(v), case.transparency.as_ref());
            }
            Some(DecodeEvent::Metadata { chunk, value }) => {
                metadata += 1;
                assert_eq!((chunk.kind, &chunk.data), (case.kind, &case.data));
                assert_eq!(Some(value), case.metadata.as_ref());
            }
            Some(DecodeEvent::AnimationError(e)) => panic!("{}: {e}", case.name),
            _ => (),
        }
        match p.status {
            DecodeStatus::Done => {
                assert_eq!(offset, bytes.len());
                decoder.finish()?;
                assert_eq!((rows, ends), (1, 1));
                assert_eq!(palettes, usize::from(!case.palette().is_empty()));
                assert_eq!(transparency, usize::from(case.transparency.is_some()));
                assert_eq!(metadata, usize::from(case.metadata.is_some()));
                return Ok(());
            }
            DecodeStatus::NeedInput if offset == bytes.len() => return decoder.finish(),
            _ => (),
        }
    }
    panic!("{}: decoder stalled", case.name);
}
fn check(case: &fields::Case) {
    let bytes = case.wire();
    for policy in [AnimationPolicy::Strict, AnimationPolicy::RecoverDefault] {
        let options = DecodeOptions {
            limits: Limits {
                max_input_bytes: 4096,
                max_output_bytes: 64,
                max_metadata_bytes: 4096,
                max_chunks: 16,
                max_frames: 1,
            },
            animation_policy: policy,
            palette_policy: PalettePolicy::Strict,
        };
        let result = decode_with_options(&bytes, &options);
        if case.valid {
            known(
                result
                    .as_ref()
                    .unwrap_or_else(|e| panic!("{}: {e}", case.name)),
                case,
                options.limits,
            );
        } else {
            assert!(
                matches!(result, Err(Error::Invalid(_))),
                "{}: {result:?}",
                case.name
            );
        }
        for fragment in [1, 7] {
            let actual = decode_reader(
                Reader {
                    bytes: &bytes,
                    fragment,
                },
                &options,
            );
            if case.valid {
                assert_eq!(actual, result, "{} fragment={fragment}", case.name);
            } else {
                assert!(
                    matches!(actual, Err(Error::Invalid(_))),
                    "{}: {actual:?}",
                    case.name
                );
            }
        }
        for fragment in [1, 31] {
            assert_eq!(
                push(&bytes, case, options, fragment).is_ok(),
                case.valid,
                "{} fragment={fragment}",
                case.name
            );
        }
    }
}
#[test]
fn independently_framed_scalar_fields_across_all_formats_and_parser_boundaries() {
    let (mut count, mut accepted, mut rejected) = (0, 0, 0);
    let mut kinds = BTreeMap::<[u8; 4], (usize, usize)>::new();
    let mut seeds = HashSet::new();
    let mut oracle = 0;
    fields::visit(|case| {
        check(&case);
        count += 1;
        if case.valid {
            accepted += 1;
            kinds.entry(case.kind).or_default().0 += 1;
        } else {
            rejected += 1;
            kinds.entry(case.kind).or_default().1 += 1;
        }
        if case.seed {
            assert!(seeds.insert(case.name.clone()), "{}", case.name);
        }
        if case.oracle {
            assert!(case.valid);
            oracle += 1;
        }
    });
    println!(
        "Scalar field audit: {count} files, {accepted} accepted, {rejected} invalid, {} parser/policy checks, {} unique seeds, {oracle} C cases",
        count * 10,
        seeds.len()
    );
    for (kind, counts) in kinds {
        println!(
            "Scalar field {}: {} accepted, {} invalid",
            std::str::from_utf8(&kind).unwrap(),
            counts.0,
            counts.1
        );
    }
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn libpng_scalar_integer_getters_match_independent_known_answers() {
    use std::{fs, process::Command};
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "deadlib-scalar-fields-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let mut count = 0;
    let mut histogram_drops = 0;
    fields::visit(|case| {
        if !case.oracle {
            return;
        }
        assert!(case.valid);
        let path = root.join(format!("{}.png", case.name));
        fs::write(&path, case.wire()).unwrap();
        let output = Command::new(&oracle)
            .arg("--scalar-metadata")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: {}",
            case.name,
            String::from_utf8_lossy(&output.stderr)
        );
        let mut input = output.stdout.as_slice();
        fn word(input: &mut &[u8]) -> u32 {
            assert!(input.len() >= 4);
            let value = u32::from_be_bytes(input[..4].try_into().unwrap());
            *input = &input[4..];
            value
        }
        assert_eq!(word(&mut input), 1);
        let mut actual = Vec::new();
        while !input.is_empty() {
            assert!(input.len() >= 4);
            let kind: [u8; 4] = input[..4].try_into().unwrap();
            input = &input[4..];
            let size = word(&mut input) as usize;
            assert!(size <= 768);
            let values = (0..size).map(|_| word(&mut input)).collect::<Vec<_>>();
            actual.push((kind, values));
        }
        actual.sort_by_key(|r| r.0);
        let expected = case.c_records();
        if case.kind == *b"hIST" && actual != expected {
            // libpng 1.6.50's CDhIST entry places hIST before PLTE. PNG Table 7
            // requires PLTE first. Preserve native known answers and record
            // this pinned reference drop, rather than copying its behavior.
            assert!(String::from_utf8_lossy(&output.stderr).contains("hIST: out of place"));
            assert_eq!(
                actual,
                expected
                    .into_iter()
                    .filter(|r| r.0 != *b"hIST")
                    .collect::<Vec<_>>(),
                "{}",
                case.name
            );
            histogram_drops += 1;
        } else {
            assert_eq!(
                actual,
                expected,
                "{}: {}",
                case.name,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        count += 1;
    });
    println!(
        "C scalar metadata audit: {count} cases, {} exact integer/byte matches, {histogram_drops} pinned hIST drops; fixtures retained in {}",
        count - histogram_drops,
        root.display()
    );
}
