use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, ColorType, DecodeEvent, DecodeOptions, DecodeStatus, Image,
    Limits, Metadata, Png, Samples, StreamDecoder, decode, decode_reader, encode, encode_writer,
    zlib::Compression,
};
use std::io::Write;

fn base() -> Png {
    Png::new(Image {
        width: 1,
        height: 1,
        color_type: ColorType::Grayscale,
        bit_depth: 8,
        samples: Samples::U8(vec![17]),
    })
}
fn crc(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ if c & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !c
}
fn fixture(data: Vec<u8>) -> Vec<u8> {
    let bytes = encode(&base(), &Default::default()).unwrap();
    let mut out = bytes[..33].to_vec();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(b"iTXt");
    out.extend_from_slice(&data);
    let checksum = crc(&out[37..]);
    out.extend_from_slice(&checksum.to_be_bytes());
    out.extend_from_slice(&bytes[33..]);
    out
}
fn wire(language: &[u8], translated: &[u8], text: &[u8], compressed: bool, method: u8) -> Vec<u8> {
    let mut data = b"Comment\0".to_vec();
    data.extend_from_slice(&[u8::from(compressed), method]);
    data.extend_from_slice(language);
    data.push(0);
    data.extend_from_slice(translated);
    data.push(0);
    if compressed {
        let mut compressor =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        compressor.write_all(text).unwrap();
        data.extend_from_slice(&compressor.finish().unwrap());
    } else {
        data.extend_from_slice(text);
    }
    data
}
fn check_events(bytes: &[u8], expected: &Metadata) {
    let mut decoder = StreamDecoder::new(DecodeOptions::default());
    let mut offset = 0;
    let mut seen = 0;
    loop {
        let end = bytes.len().min(offset + 1);
        let progress = decoder.process(&bytes[offset..end]).unwrap();
        offset += progress.consumed;
        match progress.status {
            DecodeStatus::Event => {
                if let DecodeEvent::Metadata { value, .. } = decoder.event().unwrap() {
                    assert_eq!(value, expected);
                    seen += 1;
                }
            }
            DecodeStatus::Done => {
                assert_eq!(seen, 1);
                break;
            }
            DecodeStatus::NeedInput => assert!(offset < bytes.len()),
        }
    }
}

#[test]
fn whatwg_utf8_replacement_bom_and_raw_retention() {
    // Known answers from the Encoding Standard's UTF-8 decode algorithm:
    // one leading BOM is consumed; malformed maximal subparts become U+FFFD.
    let cases: &[(&[u8], &str)] = &[
        (b"", ""),
        (b"\xef\xbb\xbf", ""),
        (b"\xef\xbb\xbf\xef\xbb\xbftext", "\u{feff}text"),
        (b"a\xef\xbb\xbfb", "a\u{feff}b"),
        (b"\xc0\xaf", "\u{fffd}\u{fffd}"),
        (b"\xe0\x80\x80", "\u{fffd}\u{fffd}\u{fffd}"),
        (b"\xe2\x82", "\u{fffd}"),
        (b"\xe2(\xa1", "\u{fffd}(\u{fffd}"),
        (b"\xed\xa0\x80", "\u{fffd}\u{fffd}\u{fffd}"),
        (b"\xf4\x90\x80\x80", "\u{fffd}\u{fffd}\u{fffd}\u{fffd}"),
        (b"\xef\xbb", "\u{fffd}"),
        (b"\xe2\x82A", "\u{fffd}A"),
        (
            b"\xc2\x80\xed\x9f\xbf\xf4\x8f\xbf\xbf",
            "\u{80}\u{d7ff}\u{10ffff}",
        ),
    ];
    for &(input, output) in cases {
        for compressed in [false, true] {
            let data = wire(b"en", input, input, compressed, 0);
            let bytes = fixture(data.clone());
            let actual = decode(&bytes).unwrap();
            assert_eq!(actual.ancillary[0].data, data);
            let expected = Metadata::InternationalText {
                keyword: b"Comment".to_vec(),
                language: "en".into(),
                translated_keyword: output.into(),
                text: output.into(),
                compressed,
            };
            assert_eq!(
                actual.metadata(Limits::default()).unwrap(),
                vec![expected.clone()]
            );
            assert_eq!(
                decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
                actual
            );
            check_events(&bytes, &expected);
            if std::str::from_utf8(input).is_err() {
                assert!(encode(&actual, &Default::default()).is_err());
                let mut out = Vec::new();
                assert!(encode_writer(&actual, &Default::default(), &mut out).is_err());
                assert!(out.is_empty());
                // Explicit typed reserialization emits replacement characters as valid UTF-8.
                let mut repaired = base();
                repaired.ancillary = vec![
                    expected
                        .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                        .unwrap(),
                ];
                let encoded = encode(&repaired, &Default::default()).unwrap();
                assert_eq!(
                    decode(&encoded)
                        .unwrap()
                        .metadata(Limits::default())
                        .unwrap(),
                    vec![expected]
                );
            } else {
                let encoded = encode(&actual, &Default::default()).unwrap();
                assert_eq!(decode(&encoded).unwrap(), actual);
            }
        }
    }
}

#[test]
fn uncompressed_method_is_ignored_only_by_decoders() {
    for method in 0..=255 {
        let data = wire(b"", b"", b"text", false, method);
        let bytes = fixture(data.clone());
        let actual = decode(&bytes).unwrap();
        assert_eq!(actual.ancillary[0].data, data);
        assert_eq!(
            decode_reader(bytes.as_slice(), &Default::default()).unwrap(),
            actual
        );
        assert_eq!(encode(&actual, &Default::default()).is_ok(), method == 0);
        let mut out = Vec::new();
        assert_eq!(
            encode_writer(&actual, &Default::default(), &mut out).is_ok(),
            method == 0
        );
        if method != 0 {
            assert!(out.is_empty());
        }
        if method != 0 {
            assert!(decode(&fixture(wire(b"", b"", b"text", true, method))).is_err());
        }
    }
}

#[test]
fn bcp47_grammar_grandfathered_private_extensions_and_duplicates() {
    let valid = [
        "",
        "en",
        "en-GB",
        "es-419",
        "zh-Hans",
        "zh-Hans-CN",
        "tlh-Cyrl-AQ",
        "ar-AE-u-nu-latn",
        "x-private",
        "X-A-12345678",
        "de-CH-1901",
        "sl-rozaj-biske-1994",
        "zh-cmn-Hans-CN",
        "en-a-aaa-b-ccc-bbb-x-xyz",
        "en-US-x-twain",
        "sr-Latn-RS",
        "qaa-Qaaa-QM-x-southern",
        "hy-Latn-IT-arevela",
        "art-lojban",
        "cel-gaulish",
        "en-GB-oed",
        "i-ami",
        "i-bnn",
        "i-default",
        "i-enochian",
        "i-hak",
        "i-klingon",
        "i-lux",
        "i-mingo",
        "i-navajo",
        "i-pwn",
        "i-tao",
        "i-tay",
        "i-tsu",
        "no-bok",
        "no-nyn",
        "sgn-BE-FR",
        "sgn-BE-NL",
        "sgn-CH-DE",
        "zh-guoyu",
        "zh-hakka",
        "zh-min",
        "zh-min-nan",
        "zh-xiang",
    ];
    let invalid = [
        "e",
        "en--US",
        "-en",
        "en-",
        "en_US",
        "en-US-US",
        "en-Latn-Latn",
        "en-a",
        "en-a-foo-A-bar",
        "de-1901-1901",
        "sl-rozaj-ROZAJ",
        "x",
        "en-x",
        "en-x-",
        "en-x-123456789",
        "en-a-123456789",
        "i-default-x-private",
        "abcdefghi",
        "12-US",
        "en-abc-def-ghi-jkl",
        "en-abc1",
        "en-1234-a",
        "en-\u{e9}",
    ];
    for (tags, accepted) in [(&valid[..], true), (&invalid[..], false)] {
        for tag in tags {
            for tag in [tag.to_string(), tag.to_ascii_uppercase()] {
                let value = Metadata::InternationalText {
                    keyword: b"Comment".to_vec(),
                    language: tag.clone(),
                    translated_keyword: "".into(),
                    text: "text".into(),
                    compressed: false,
                };
                assert_eq!(
                    value
                        .to_chunk(ChunkPosition::BeforePalette, Compression::Fast)
                        .is_ok(),
                    accepted,
                    "{tag}"
                );
                let bytes = fixture(wire(tag.as_bytes(), b"", b"text", false, 0));
                assert_eq!(decode(&bytes).is_ok(), accepted, "{tag}");
                assert_eq!(
                    decode_reader(bytes.as_slice(), &Default::default()).is_ok(),
                    accepted,
                    "{tag}"
                );
                if !accepted {
                    let mut raw = base();
                    raw.ancillary = vec![AncillaryChunk {
                        kind: *b"iTXt",
                        data: wire(tag.as_bytes(), b"", b"text", false, 0),
                        position: ChunkPosition::BeforePalette,
                    }];
                    assert!(encode(&raw, &Default::default()).is_err());
                }
            }
        }
    }
    // RFC 5646 sets no fixed total tag length; metadata limits bound this.
    let language = format!("x{}", "-private".repeat(1000));
    assert!(decode(&fixture(wire(language.as_bytes(), b"", b"text", false, 0))).is_ok());
}
