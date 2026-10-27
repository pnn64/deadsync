//! PNG Third Edition named ancillary fields and suggested palette boundaries.
use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, DecodeEvent, DecodeOptions, DecodeStatus, Error, Limits,
    Metadata, Png, Samples, StreamDecoder, decode, decode_reader, encode, encode_writer,
    zlib::Compression,
};
use std::io::{self, Cursor, Read};
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/named.rs"]
mod named;
use chunks as wire;
use named::{FORMS, Form};

struct Fragmented<'a>(&'a [u8], usize);
impl Read for Fragmented<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.0.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}
fn parsers(bytes: &[u8]) -> [Result<Png, Error>; 3] {
    [
        decode(bytes),
        decode_reader(Fragmented(bytes, 1), &DecodeOptions::default()),
        decode_reader(Fragmented(bytes, 17), &DecodeOptions::default()),
    ]
}
fn base() -> Png {
    let mut png = decode(&Form::Text.fixture(b"Base", ChunkPosition::BeforePalette)).unwrap();
    png.ancillary.clear();
    png
}
fn latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap())
        .collect()
}
fn rust_reference(bytes: &[u8], form: Form, name: &[u8]) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut out = vec![0; reader.output_buffer_size().unwrap()];
    let size = reader.next_frame(&mut out).unwrap().buffer_size();
    assert_eq!(&out[..size], wire::PIXEL);
    reader.finish().unwrap();
    let info = reader.info();
    match form {
        Form::Text => {
            let text = &info.uncompressed_latin1_text[0];
            assert_eq!(latin1(&text.keyword), name);
            assert_eq!(latin1(&text.text), named::LATIN_TEXT);
        }
        Form::ZText => {
            let text = &info.compressed_latin1_text[0];
            assert_eq!(latin1(&text.keyword), name);
            assert_eq!(latin1(&text.get_text().unwrap()), named::LATIN_TEXT);
        }
        Form::IText | Form::IZText => {
            let text = &info.utf8_text[0];
            assert_eq!(latin1(&text.keyword), name);
            assert_eq!(text.get_text().unwrap(), named::UNICODE_TEXT);
        }
        Form::Icc => assert_eq!(info.icc_profile.as_deref().unwrap(), named::PROFILE),
        // Rust png exposes neither suggested palettes nor the ICC profile name.
        // The C oracle checks those exact fields independently below.
        Form::Palette8 | Form::Palette16 => (),
    }
}

#[test]
fn all_keyword_bytes_lengths_and_spacing_in_every_named_wire_form() {
    let cases = named::names();
    let mut count = 0;
    for case in &cases {
        for form in FORMS {
            let position = ChunkPosition::BeforePalette;
            let raw = form.raw(&case.bytes, position);
            let expected = form.value(&case.bytes);
            let bytes = form.fixture(&case.bytes, position);
            for result in parsers(&bytes) {
                assert_eq!(
                    result.is_ok(),
                    case.valid,
                    "{} {form:?}: {result:?}",
                    case.label
                );
                if let Ok(png) = result {
                    assert_eq!(png.image.samples, Samples::U8(wire::PIXEL.to_vec()));
                    assert_eq!(png.ancillary.as_slice(), std::slice::from_ref(&raw));
                    assert_eq!(
                        png.metadata(Limits::default()).unwrap().as_slice(),
                        std::slice::from_ref(&expected)
                    );
                }
            }
            let mut png = base();
            png.ancillary.push(raw);
            let buffered = encode(&png, &Default::default());
            let mut output = Vec::new();
            let writer = encode_writer(&png, &Default::default(), &mut output);
            assert_eq!(buffered.is_ok(), case.valid, "{} {form:?}", case.label);
            assert_eq!(writer.is_ok(), case.valid, "{} {form:?}", case.label);
            if case.valid {
                for bytes in [buffered.unwrap(), output] {
                    let actual = decode(&bytes).unwrap();
                    assert_eq!(
                        actual.metadata(Limits::default()).unwrap().as_slice(),
                        std::slice::from_ref(&expected)
                    );
                    rust_reference(&bytes, form, &case.bytes);
                }
            } else {
                assert!(output.is_empty());
            }
            for compression in [Compression::Stored, Compression::Fast] {
                let typed = expected.to_chunk(position, compression);
                assert_eq!(typed.is_ok(), case.valid, "typed {} {form:?}", case.label);
                if let Ok(chunk) = typed {
                    png.ancillary = vec![chunk];
                    assert_eq!(
                        png.metadata(Limits::default()).unwrap().as_slice(),
                        std::slice::from_ref(&expected)
                    );
                }
            }
            count += 1;
        }
    }
    assert_eq!(cases.len(), 2718);
    assert_eq!(count, 19026);
    assert_eq!(cases.iter().filter(|c| c.valid).count(), 1861);
}

#[test]
fn named_stream_events_and_legal_text_positions() {
    for byte in 0u8..=255 {
        let valid = matches!(byte,0x21..=0x7e | 0xa1..=0xff);
        for form in FORMS {
            let bytes = form.fixture(&[byte], ChunkPosition::BeforePalette);
            let expected = form.value(&[byte]);
            for fragment in [1, bytes.len()] {
                let mut decoder = StreamDecoder::new(DecodeOptions::default());
                let (mut offset, mut seen) = (0, 0);
                loop {
                    let end = bytes.len().min(offset + fragment);
                    let progress = match decoder.process(&bytes[offset..end]) {
                        Ok(progress) => progress,
                        Err(error) => {
                            assert!(!valid, "{form:?} {byte}: {error}");
                            assert_eq!(decoder.finish(), Err(error.clone()));
                            assert_eq!(decoder.process(&[]), Err(error));
                            break;
                        }
                    };
                    offset += progress.consumed;
                    match progress.status {
                        DecodeStatus::Event => {
                            if let DecodeEvent::Metadata { value, .. } = decoder.event().unwrap() {
                                assert_eq!(value, &expected);
                                seen += 1;
                            }
                        }
                        DecodeStatus::Done => {
                            assert!(valid);
                            assert_eq!(seen, 1);
                            decoder.finish().unwrap();
                            break;
                        }
                        DecodeStatus::NeedInput => assert!(offset < bytes.len()),
                    }
                }
            }
        }
    }
    for form in [Form::Text, Form::ZText, Form::IText, Form::IZText] {
        for position in wire::POSITIONS {
            let expected = form.value(b"A \xa1\xff B");
            for result in parsers(&form.fixture(b"A \xa1\xff B", position)) {
                let png = result.unwrap();
                assert_eq!(
                    png.metadata(Limits::default()).unwrap().as_slice(),
                    std::slice::from_ref(&expected)
                );
                assert_eq!(png.ancillary[0].position, position);
            }
        }
    }
}

fn file_with(chunks: &[AncillaryChunk]) -> Vec<u8> {
    let mut all = vec![wire::header(2, 8)];
    all.extend(chunks.iter().map(|c| wire::chunk(c.kind, &c.data)));
    all.push(wire::chunk(*b"IDAT", &wire::stored(&[0, 17, 34, 51])));
    all.push(wire::chunk(*b"IEND", &[]));
    wire::file(&all)
}
fn check_chunks(chunks: Vec<AncillaryChunk>, valid: bool) {
    for result in parsers(&file_with(&chunks)) {
        assert_eq!(result.is_ok(), valid, "{chunks:?}: {result:?}");
    }
    let mut png = base();
    png.ancillary = chunks;
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
fn check_palette(depth: u8, entries: Vec<[u16; 5]>, valid: bool) {
    let position = ChunkPosition::BeforePalette;
    let chunk = named::palette(b"Boundary", depth, &entries, position);
    let expected = Metadata::SuggestedPalette {
        name: b"Boundary".to_vec(),
        depth,
        entries,
    };
    // Compare both native directions with independently assembled big-endian
    // fields, rather than using the native serializer to construct input.
    assert_eq!(
        expected.to_chunk(position, Compression::Stored).unwrap(),
        chunk
    );
    check_chunks(vec![chunk.clone()], valid);
    if valid {
        assert_eq!(
            decode(&file_with(&[chunk]))
                .unwrap()
                .metadata(Limits::default())
                .unwrap(),
            [expected]
        );
    }
}

#[test]
fn names_are_case_sensitive_and_only_suggested_palette_names_must_be_distinct() {
    let position = ChunkPosition::BeforePalette;
    for a in [b"Palette".as_slice(), &[0xe9], b"A B"] {
        for form in [Form::Palette8, Form::Palette16] {
            check_chunks(
                vec![form.raw(a, position), Form::Palette16.raw(a, position)],
                false,
            );
        }
    }
    for (a, b) in [
        (b"Palette".as_slice(), b"palette".as_slice()),
        (&[0xe9], &[0xc9]),
        (b"A B", b"A  B"),
    ] {
        check_chunks(
            vec![
                Form::Palette8.raw(a, position),
                Form::Palette16.raw(b, position),
            ],
            b != b"A  B",
        );
    }
    for form in [Form::Text, Form::ZText, Form::IText, Form::IZText] {
        check_chunks(
            vec![
                form.raw(b"Repeated", position),
                form.raw(b"Repeated", position),
            ],
            true,
        );
    }
    let same_name: Vec<_> = [
        Form::Text,
        Form::ZText,
        Form::IText,
        Form::IZText,
        Form::Icc,
        Form::Palette8,
    ]
    .into_iter()
    .map(|form| form.raw(b"Shared Name", position))
    .collect();
    check_chunks(same_name, true);
    // ICC is a singleton regardless of whether its profile names differ.
    check_chunks(
        vec![Form::Icc.raw(b"A", position), Form::Icc.raw(b"a", position)],
        false,
    );
    for form in FORMS {
        // No separator anywhere; constructors deliberately do not add one.
        check_chunks(
            vec![AncillaryChunk {
                kind: form.kind(),
                data: vec![b'A'; 79],
                position,
            }],
            false,
        );
    }
}

#[test]
fn suggested_palette_depth_entry_shape_sample_and_frequency_boundaries() {
    let position = ChunkPosition::BeforePalette;
    for depth in 0u8..=255 {
        check_chunks(
            vec![AncillaryChunk {
                kind: *b"sPLT",
                data: vec![b'A', 0, depth],
                position,
            }],
            matches!(depth, 8 | 16),
        );
    }
    for depth in [8, 16] {
        let stride = if depth == 8 { 6 } else { 10 };
        for remainder in 1..stride {
            let mut data = vec![b'A', 0, depth];
            data.extend(vec![0; stride + remainder]);
            check_chunks(
                vec![AncillaryChunk {
                    kind: *b"sPLT",
                    data,
                    position,
                }],
                false,
            );
        }
        for count in [0, 1, 2, 3, 255, 256, 257] {
            check_palette(depth, vec![[1, 2, 3, 0, 65535]; count], true);
        }
        for first in [0u16, 1, 255, 256, 32767, 32768, 65535] {
            for second in [0u16, 1, 255, 256, 32767, 32768, 65535] {
                check_palette(
                    depth,
                    vec![[0, 0, 0, 0, first], [0, 0, 0, 0, second]],
                    first >= second,
                );
            }
        }
    }
    for field in 0..4 {
        for sample in [0u16, 1, 127, 254, 255, 256, 65535] {
            for depth in [8, 16] {
                let mut entry = [0; 5];
                entry[field] = sample;
                let value = Metadata::SuggestedPalette {
                    name: b"Samples".to_vec(),
                    depth,
                    entries: vec![entry],
                };
                let chunk = value.to_chunk(position, Compression::Stored);
                if depth == 8 && sample > 255 {
                    assert!(chunk.is_err());
                } else {
                    check_palette(depth, vec![entry], true);
                }
            }
        }
    }
    for (color, channels, depths) in [
        (0, 1, &[1, 2, 4, 8, 16][..]),
        (2, 3, &[8, 16][..]),
        (3, 1, &[1, 2, 4, 8][..]),
        (4, 2, &[8, 16][..]),
        (6, 4, &[8, 16][..]),
    ] {
        for &depth in depths {
            for form in [Form::Palette8, Form::Palette16] {
                let raw = form.raw(b"Any color", position);
                let mut chunks = vec![wire::header(color, depth), wire::chunk(raw.kind, &raw.data)];
                if color == 3 {
                    chunks.push(wire::chunk(*b"PLTE", &[0, 0, 0, 255, 255, 255]));
                }
                chunks.push(wire::chunk(
                    *b"IDAT",
                    &wire::stored(&vec![0; 1 + (channels * usize::from(depth)).div_ceil(8)]),
                ));
                chunks.push(wire::chunk(*b"IEND", &[]));
                for result in parsers(&wire::file(&chunks)) {
                    let png = result.unwrap();
                    assert_eq!(
                        png.metadata(Limits::default()).unwrap(),
                        [form.value(b"Any color")]
                    );
                }
            }
        }
    }
}

fn take<'a>(input: &mut &'a [u8], len: usize) -> &'a [u8] {
    let (value, rest) = input.split_at(len);
    *input = rest;
    value
}
fn word(input: &mut &[u8]) -> usize {
    u32::from_be_bytes(take(input, 4).try_into().unwrap()) as usize
}
type Record = ([u8; 4], Vec<u8>, Vec<u8>);
fn records(mut bytes: &[u8]) -> Vec<Record> {
    assert_eq!(word(&mut bytes), 1);
    let count = word(&mut bytes);
    let mut records = Vec::new();
    for _ in 0..count {
        let kind = take(&mut bytes, 4).try_into().unwrap();
        let size = word(&mut bytes);
        let name = take(&mut bytes, size).to_vec();
        let size = word(&mut bytes);
        let data = take(&mut bytes, size).to_vec();
        records.push((kind, name, data));
    }
    assert!(bytes.is_empty());
    records
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_exact_named_metadata_and_invalid_name_policies() {
    use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required"),
    );
    let scratch =
        std::env::temp_dir().join(format!("deadlib-png-keywords-{}.png", std::process::id()));
    let mut policy: BTreeMap<String, [usize; 5]> = BTreeMap::new();
    let mut checks = 0;
    for case in named::names().into_iter().filter(|case| case.c_reference) {
        for form in FORMS {
            let bytes = form.fixture(&case.bytes, ChunkPosition::BeforePalette);
            fs::write(&scratch, bytes).unwrap();
            let output = Command::new(&oracle)
                .arg("--named-metadata")
                .arg(&scratch)
                .output()
                .unwrap();
            assert!(
                matches!(output.status.code(), Some(0 | 1)),
                "unexpected oracle failure {:?}",
                output.status
            );
            let stats = policy.entry(format!("{form:?}")).or_default();
            if case.valid {
                assert!(
                    output.status.success(),
                    "{} {form:?}: {}",
                    case.label,
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    records(&output.stdout),
                    [(form.kind(), case.bytes.clone(), form.reference_value())],
                    "{} {form:?}",
                    case.label
                );
                stats[0] += 1;
            } else if !output.status.success() {
                stats[1] += 1;
            } else {
                let actual = records(&output.stdout);
                if actual.is_empty() {
                    stats[2] += 1;
                } else if actual[0].1 == case.bytes {
                    stats[3] += 1;
                } else {
                    stats[4] += 1;
                }
            }
            checks += 1;
        }
    }
    fs::remove_file(scratch).unwrap();
    println!(
        "libpng named metadata: {checks} files; [valid exact, invalid fatal, invalid ignored, invalid retained, invalid name changed]: {policy:?}"
    );
    assert_eq!(checks, 3710);
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_suggested_palette_field_boundaries() {
    use std::{fs, path::PathBuf, process::Command};
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required"),
    );
    let scratch = std::env::temp_dir().join(format!("deadlib-png-splt-{}.png", std::process::id()));
    let mut cases = Vec::new();
    for depth in [8, 16] {
        for count in [0, 1, 2, 255, 256, 257] {
            cases.push((depth, vec![[1, 2, 3, 0, 65535]; count], true));
        }
        for a in [0u16, 1, 255, 256, 32767, 32768, 65535] {
            for b in [0u16, 1, 255, 256, 32767, 32768, 65535] {
                cases.push((depth, vec![[0, 0, 0, 0, a], [0, 0, 0, 0, b]], a >= b));
            }
        }
        for field in 0..4 {
            for sample in [0u16, 1, 127, 254, 255, 256, 65535] {
                if depth == 8 && sample > 255 {
                    continue;
                }
                let mut entry = [0; 5];
                entry[field] = sample;
                cases.push((depth, vec![entry], true));
            }
        }
    }
    let (mut exact_valid, mut ignored_empty, mut invalid_retained, mut invalid_ignored) =
        (0, 0, 0, 0);
    for (depth, entries, valid) in &cases {
        let raw = named::palette(b"Boundary", *depth, entries, ChunkPosition::BeforePalette);
        fs::write(&scratch, file_with(&[raw])).unwrap();
        let output = Command::new(&oracle)
            .arg("--named-metadata")
            .arg(&scratch)
            .output()
            .unwrap();
        assert!(matches!(output.status.code(), Some(0 | 1)));
        if !output.status.success() {
            assert!(!valid);
            invalid_ignored += 1;
            continue;
        }
        let actual = records(&output.stdout);
        if actual.is_empty() {
            if *valid {
                assert!(entries.is_empty());
                ignored_empty += 1;
            } else {
                invalid_ignored += 1;
            }
        } else {
            let mut expected = vec![*depth];
            for entry in entries {
                for value in entry {
                    expected.extend(value.to_be_bytes());
                }
            }
            assert_eq!(actual, [(*b"sPLT", b"Boundary".to_vec(), expected)]);
            if *valid {
                exact_valid += 1;
            } else {
                invalid_retained += 1;
            }
        }
    }
    fs::remove_file(scratch).unwrap();
    println!(
        "libpng suggested palettes: {} files; valid exact={exact_valid}, valid empty ignored={ignored_empty}, invalid ascending retained={invalid_retained}, invalid ignored/rejected={invalid_ignored}",
        cases.len()
    );
    assert_eq!(cases.len(), 158);
}
