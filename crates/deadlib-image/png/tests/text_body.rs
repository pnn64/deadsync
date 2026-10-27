use deadlib_image_png::{
    ChunkPosition, DecodeEvent, DecodeOptions, DecodeStatus, EncodeOptions, Limits, Metadata, Png,
    Samples, StreamDecoder, decode, decode_reader, encode, encode_writer, zlib::Compression,
};
use std::io::{self, Cursor, Read};
#[path = "support/text_body.rs"]
mod body;
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;

struct Fragmented<'a>(&'a [u8], usize);
impl Read for Fragmented<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.0.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}
fn value(text: &[u8], compressed: bool) -> Metadata {
    Metadata::Text {
        keyword: b"Body".to_vec(),
        text: text.to_vec(),
        compressed,
    }
}
fn base() -> Png {
    let mut png = decode(&body::fixture(b"", false, ChunkPosition::BeforePalette)).unwrap();
    png.ancillary.clear();
    png
}
fn reference(bytes: &[u8], text: &[u8], compressed: bool) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let count = reader.next_frame(&mut pixels).unwrap().buffer_size();
    assert_eq!(&pixels[..count], chunks::PIXEL);
    reader.finish().unwrap();
    let actual = if compressed {
        reader.info().compressed_latin1_text[0].get_text().unwrap()
    } else {
        reader.info().uncompressed_latin1_text[0].text.clone()
    };
    assert_eq!(
        actual
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap())
            .collect::<Vec<_>>(),
        text
    );
}

#[test]
fn encoders_reject_text_controls_before_output() {
    for compressed in [false, true] {
        let text = b"before\x1bafter";
        let png = decode(&body::fixture(text, compressed, ChunkPosition::AfterData)).unwrap();
        assert!(encode(&png, &Default::default()).is_err());
        let mut output = Vec::new();
        assert!(encode_writer(&png, &Default::default(), &mut output).is_err());
        assert!(output.is_empty());
        assert!(
            value(text, compressed)
                .to_chunk(ChunkPosition::AfterData, Compression::Stored)
                .is_err()
        );
    }
}

#[test]
fn every_body_byte_context_and_placement_preserves_decode_recovery_but_enforces_encoding() {
    let mut count = 0;
    for byte in 0u8..=255 {
        let encodable = matches!(byte,10|32..=126|160..=255);
        for compressed in [false, true] {
            for text in [
                vec![byte],
                vec![byte, b'A', b'B'],
                vec![b'A', byte, b'B'],
                vec![b'A', b'B', byte],
            ] {
                for position in chunks::POSITIONS {
                    let raw = body::raw(&text, compressed, position);
                    let expected = value(&text, compressed);
                    let bytes = body::fixture(&text, compressed, position);
                    for result in [
                        decode(&bytes),
                        decode_reader(Fragmented(&bytes, 1), &DecodeOptions::default()),
                        decode_reader(Fragmented(&bytes, 7), &DecodeOptions::default()),
                    ] {
                        if byte == 0 {
                            assert!(result.is_err());
                        } else {
                            let png = result.unwrap();
                            assert_eq!(png.image.samples, Samples::U8(chunks::PIXEL.to_vec()));
                            assert_eq!(png.ancillary.as_slice(), std::slice::from_ref(&raw));
                            assert_eq!(
                                png.metadata(Limits::default()).unwrap().as_slice(),
                                std::slice::from_ref(&expected)
                            );
                        }
                    }
                    let mut png = base();
                    png.ancillary = vec![raw];
                    let buffered = encode(&png, &Default::default());
                    let mut output = Vec::new();
                    let written = encode_writer(&png, &Default::default(), &mut output);
                    assert_eq!(
                        buffered.is_ok(),
                        encodable,
                        "byte={byte} compressed={compressed}"
                    );
                    assert_eq!(written.is_ok(), encodable);
                    if encodable {
                        for wire in [buffered.unwrap(), output] {
                            assert_eq!(
                                decode(&wire)
                                    .unwrap()
                                    .metadata(Limits::default())
                                    .unwrap()
                                    .as_slice(),
                                std::slice::from_ref(&expected)
                            );
                            reference(&wire, &text, compressed);
                        }
                    } else {
                        assert!(output.is_empty());
                    }
                    for compression in [Compression::Stored, Compression::Fast] {
                        let chunk = expected.to_chunk(position, compression);
                        assert_eq!(chunk.is_ok(), encodable);
                        if let Ok(chunk) = chunk {
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
        }
    }
    assert_eq!(count, 6144);
}

#[test]
fn recovery_events_preserve_non_null_controls_and_itxt_unicode_rules() {
    for byte in 0u8..=255 {
        for compressed in [false, true] {
            let wire = body::fixture(&[byte], compressed, ChunkPosition::AfterData);
            let expected = value(&[byte], compressed);
            let mut decoder = StreamDecoder::new(DecodeOptions::default());
            let (mut offset, mut metadata) = (0, 0);
            loop {
                let end = wire.len().min(offset + 1);
                let progress = match decoder.process(&wire[offset..end]) {
                    Ok(progress) => progress,
                    Err(error) => {
                        assert_eq!(byte, 0);
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
                            metadata += 1;
                        }
                    }
                    DecodeStatus::Done => {
                        assert_ne!(byte, 0);
                        assert_eq!(metadata, 1);
                        decoder.finish().unwrap();
                        break;
                    }
                    DecodeStatus::NeedInput => assert!(offset < wire.len()),
                }
            }
            // iTXt discourages these Unicode controls but does not prohibit
            // them. UTF-8 encoding/decoding must not inherit Latin-1 rules.
            let text = char::from(byte).to_string();
            let international = Metadata::InternationalText {
                keyword: b"Body".to_vec(),
                language: String::new(),
                translated_keyword: text.clone(),
                text,
                compressed,
            };
            let chunk = international.to_chunk(ChunkPosition::BeforePalette, Compression::Stored);
            if byte == 0 {
                assert!(chunk.is_err());
            } else {
                let mut png = base();
                png.ancillary = vec![chunk.unwrap()];
                let mut output = Vec::new();
                encode_writer(&png, &Default::default(), &mut output).unwrap();
                for wire in [encode(&png, &Default::default()).unwrap(), output] {
                    assert_eq!(
                        decode(&wire)
                            .unwrap()
                            .metadata(Limits::default())
                            .unwrap()
                            .as_slice(),
                        std::slice::from_ref(&international)
                    );
                }
            }
        }
    }
}

#[test]
fn empty_and_long_latin1_text_has_no_keyword_length_limit() {
    for size in [0, 1, 79, 80, 81, 65535, 65536, 65537] {
        let text = vec![0xa0; size]; // Non-breaking space is allowed in the body.
        for compressed in [false, true] {
            let expected = value(&text, compressed);
            for compression in [Compression::Stored, Compression::Fast] {
                let mut png = base();
                png.ancillary = vec![
                    expected
                        .to_chunk(ChunkPosition::BeforePalette, compression)
                        .unwrap(),
                ];
                let options = EncodeOptions {
                    compression,
                    ..Default::default()
                };
                let mut output = Vec::new();
                encode_writer(&png, &options, &mut output).unwrap();
                for wire in [encode(&png, &options).unwrap(), output] {
                    assert_eq!(
                        decode_reader(Fragmented(&wire, 7), &DecodeOptions::default())
                            .unwrap()
                            .metadata(Limits::default())
                            .unwrap()
                            .as_slice(),
                        std::slice::from_ref(&expected)
                    );
                    reference(&wire, &text, compressed);
                }
            }
        }
    }
}

fn c_word(bytes: &mut &[u8]) -> usize {
    let value = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    *bytes = &bytes[4..];
    value
}
type Record = ([u8; 4], Vec<u8>, Vec<u8>);
fn c_records(mut bytes: &[u8]) -> Vec<Record> {
    assert_eq!(c_word(&mut bytes), 1);
    let count = c_word(&mut bytes);
    let mut result = Vec::new();
    for _ in 0..count {
        let kind = bytes[..4].try_into().unwrap();
        bytes = &bytes[4..];
        let len = c_word(&mut bytes);
        let name = bytes[..len].to_vec();
        bytes = &bytes[len..];
        let len = c_word(&mut bytes);
        let value = bytes[..len].to_vec();
        bytes = &bytes[len..];
        result.push((kind, name, value));
    }
    assert!(bytes.is_empty());
    result
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE"]
fn libpng_all_body_bytes_and_native_writer_values() {
    use std::{fs, path::PathBuf, process::Command};
    let oracle = PathBuf::from(
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required"),
    );
    let scratch =
        std::env::temp_dir().join(format!("deadlib-png-text-body-{}.png", std::process::id()));
    let (
        mut raw_exact,
        mut writer_exact,
        mut malformed_retained,
        mut malformed_changed,
        mut ignored,
    ) = (0, 0, 0, 0, 0);
    for byte in 0u8..=255 {
        let valid = matches!(byte,10|32..=126|160..=255);
        for compressed in [false, true] {
            let raw = body::raw(&[byte], compressed, ChunkPosition::BeforePalette);
            let expected = vec![(raw.kind, b"Body".to_vec(), vec![byte])];
            let wire = body::fixture(&[byte], compressed, ChunkPosition::BeforePalette);
            fs::write(&scratch, &wire).unwrap();
            let output = Command::new(&oracle)
                .arg("--named-metadata")
                .arg(&scratch)
                .output()
                .unwrap();
            assert!(matches!(output.status.code(), Some(0 | 1)));
            if valid {
                assert!(
                    output.status.success(),
                    "byte={byte}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(c_records(&output.stdout), expected);
                raw_exact += 1;
                let png = decode(&wire).unwrap();
                let mut streamed = Vec::new();
                encode_writer(&png, &Default::default(), &mut streamed).unwrap();
                for wire in [encode(&png, &Default::default()).unwrap(), streamed] {
                    fs::write(&scratch, &wire).unwrap();
                    let output = Command::new(&oracle)
                        .arg("--named-metadata")
                        .arg(&scratch)
                        .output()
                        .unwrap();
                    assert!(output.status.success());
                    assert_eq!(c_records(&output.stdout), expected);
                    writer_exact += 1;
                }
            } else if !output.status.success() {
                ignored += 1;
            } else {
                let actual = c_records(&output.stdout);
                if actual.is_empty() {
                    ignored += 1;
                } else if actual == expected {
                    malformed_retained += 1;
                } else {
                    malformed_changed += 1;
                }
            }
        }
    }
    fs::remove_file(scratch).unwrap();
    println!(
        "libpng text bodies: raw valid exact={raw_exact}, native writer exact={writer_exact}, malformed retained={malformed_retained}, malformed changed={malformed_changed}, ignored/rejected={ignored}"
    );
    assert_eq!(raw_exact, 384);
    assert_eq!(writer_exact, 768);
    assert_eq!(malformed_retained + malformed_changed + ignored, 128);
}
