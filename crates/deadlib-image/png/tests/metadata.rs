use deadlib_image_png::{
    AncillaryChunk, ChunkPosition as Pos, ColorType, DecodeOptions, EncodeOptions, Error, Image,
    Limits, Metadata as M, Png, Samples, decode, decode_with_options, encode, zlib::Compression,
};
use std::io::Cursor;

fn base() -> Png {
    Png::new(Image {
        width: 2,
        height: 1,
        color_type: ColorType::Rgba,
        bit_depth: 16,
        samples: Samples::U16(vec![0, 1, 2, 65535, 65535, 32768, 256, 257]),
    })
}
fn values() -> Vec<(M, Pos)> {
    vec![
        (M::Gamma(45455), Pos::BeforePalette),
        (
            M::Chromaticities([31270, 32900, 64000, 33000, 30000, 60000, 15000, 6000]),
            Pos::BeforePalette,
        ),
        (M::SignificantBits(vec![13, 14, 15, 16]), Pos::BeforePalette),
        (M::Srgb(0), Pos::BeforePalette),
        (M::Cicp([9, 16, 0, 1]), Pos::BeforePalette),
        (
            M::MasteringDisplay {
                chromaticities: [35400, 14600, 8500, 39850, 6550, 2300, 15635, 16450],
                luminance: [40_000_000, 5],
            },
            Pos::BeforePalette,
        ),
        (M::ContentLight([10_000_000, 2_500_000]), Pos::BeforePalette),
        (M::BackgroundRgb([65535, 32768, 255]), Pos::BeforeData),
        (M::Histogram(vec![1, 1]), Pos::BeforeData),
        (
            M::PhysicalDimensions {
                x: 3780,
                y: 3780,
                unit: 1,
            },
            Pos::BeforeData,
        ),
        (
            M::SuggestedPalette {
                name: b"Palette".to_vec(),
                depth: 16,
                entries: vec![[0, 1, 2, 65535, 100], [123, 456, 789, 32768, 1]],
            },
            Pos::BeforeData,
        ),
        (
            M::Exif(b"II\x2a\0\x08\0\0\0\0\0\0\0\0\0".to_vec()),
            Pos::BeforeData,
        ),
        (M::Time([2026, 10, 26, 14, 30, 60]), Pos::AfterData),
        (
            M::Text {
                keyword: b"Author".to_vec(),
                text: b"Latin-1 \xe4\xf6\xfc".to_vec(),
                compressed: false,
            },
            Pos::BeforeData,
        ),
        (
            M::Text {
                keyword: b"Comment".to_vec(),
                text: b"native zlib text".repeat(100),
                compressed: true,
            },
            Pos::AfterData,
        ),
        (
            M::InternationalText {
                keyword: b"Title".to_vec(),
                language: "ja".into(),
                translated_keyword: "題名".into(),
                text: "日本語の画像 🦀".into(),
                compressed: false,
            },
            Pos::BeforeData,
        ),
        (
            M::InternationalText {
                keyword: b"Comment".to_vec(),
                language: "fr-FR".into(),
                translated_keyword: "Commentaire".into(),
                text: "Précision préservée".repeat(100),
                compressed: true,
            },
            Pos::AfterData,
        ),
        (
            M::Unknown {
                kind: *b"vpAg",
                data: vec![1, 3, 5, 7],
            },
            Pos::AfterData,
        ),
    ]
}

#[test]
fn metadata_wire_and_typed_roundtrip_with_reference_text() {
    let mut png = base();
    png.palette = vec![[0, 0, 0], [255, 255, 255]];
    for (value, position) in values() {
        png.ancillary
            .push(value.to_chunk(position, Compression::Fast).unwrap());
    }
    let encoded = encode(&png, &EncodeOptions::default()).unwrap();
    let decoded = decode(&encoded).unwrap();
    let mut streamed = Vec::new();
    deadlib_image_png::encode_writer(&png, &EncodeOptions::default(), &mut streamed).unwrap();
    assert_eq!(decode(&streamed).unwrap(), decoded);
    assert_eq!(
        deadlib_image_png::decode_reader(encoded.as_slice(), &Default::default()).unwrap(),
        decoded
    );
    let expected = png.metadata(Limits::default()).unwrap();
    let actual = decoded.metadata(Limits::default()).unwrap();
    // Encoder groups by legal placement, preserving order within each group.
    for value in expected {
        assert!(actual.contains(&value), "{value:?}");
    }
    assert_eq!(decoded.color_description().unwrap().kind, *b"cICP");
    let mut reader = png::Decoder::new(Cursor::new(encoded)).read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut data).unwrap();
    reader.finish().unwrap();
    let info = reader.info();
    assert_eq!(info.uncompressed_latin1_text[0].keyword, "Author");
    assert_eq!(info.uncompressed_latin1_text[0].text, "Latin-1 äöü");
    assert_eq!(
        info.compressed_latin1_text[0].get_text().unwrap(),
        "native zlib text".repeat(100)
    );
    assert_eq!(info.utf8_text[0].get_text().unwrap(), "日本語の画像 🦀");
    assert_eq!(
        info.utf8_text[1].get_text().unwrap(),
        "Précision préservée".repeat(100)
    );
}

#[test]
fn icc_and_palette_metadata() {
    let mut png = base();
    let profile = M::IccProfile {
        name: b"Test Profile".to_vec(),
        profile: (0..=255).cycle().take(4096).collect(),
    };
    png.ancillary.push(
        profile
            .to_chunk(Pos::BeforePalette, Compression::Fast)
            .unwrap(),
    );
    let decoded = decode(&encode(&png, &EncodeOptions::default()).unwrap()).unwrap();
    assert_eq!(decoded.metadata(Limits::default()).unwrap(), vec![profile]);
    let mut indexed = Png::new(Image {
        width: 2,
        height: 1,
        color_type: ColorType::Indexed,
        bit_depth: 1,
        samples: Samples::U8(vec![0, 1]),
    });
    indexed.palette = vec![[1, 2, 3], [4, 5, 6]];
    for value in [
        M::BackgroundIndex(1),
        M::Histogram(vec![1, 1]),
        M::SuggestedPalette {
            name: b"8-bit".to_vec(),
            depth: 8,
            entries: vec![[1, 2, 3, 255, 10], [4, 5, 6, 0, 0]],
        },
    ] {
        indexed.ancillary.push(
            value
                .to_chunk(Pos::BeforeData, Compression::Stored)
                .unwrap(),
        );
    }
    let decoded = decode(&encode(&indexed, &EncodeOptions::default()).unwrap()).unwrap();
    assert_eq!(
        decoded.metadata(Limits::default()).unwrap(),
        indexed.metadata(Limits::default()).unwrap()
    );
}

#[test]
fn metadata_constraints_and_expansion_budget() {
    let gamma = M::Gamma(45455)
        .to_chunk(Pos::BeforePalette, Compression::Fast)
        .unwrap();
    let mut png = base();
    png.ancillary = vec![gamma.clone(), gamma.clone()];
    assert!(encode(&png, &EncodeOptions::default()).is_err());
    png.ancillary = vec![AncillaryChunk {
        position: Pos::AfterData,
        ..gamma
    }];
    assert!(encode(&png, &EncodeOptions::default()).is_err());
    for (kind, data) in [
        (*b"gAMA", vec![0; 4]),
        (*b"cICP", vec![9, 16, 1, 1]),
        (*b"pHYs", vec![0, 0, 0, 1, 0, 0, 0, 1, 2]),
        (*b"tIME", vec![7, 234, 0, 1, 0, 0, 0]),
        (*b"tEXt", b"bad  key\0text".to_vec()),
        (*b"sBIT", vec![17; 4]),
        (*b"mDCV", vec![0; 24]),
        (*b"hIST", vec![0, 1]),
        (*b"zTXt", b"Comment\0\x01junk".to_vec()),
    ] {
        png.ancillary = vec![AncillaryChunk {
            kind,
            data,
            position: Pos::BeforePalette,
        }];
        assert!(encode(&png, &EncodeOptions::default()).is_err(), "{kind:?}");
    }
    png.ancillary = vec![
        M::Text {
            keyword: b"Comment".to_vec(),
            text: vec![b'a'; 100_000],
            compressed: true,
        }
        .to_chunk(Pos::AfterData, Compression::Fast)
        .unwrap(),
    ];
    let bytes = encode(&png, &EncodeOptions::default()).unwrap();
    let options = DecodeOptions {
        limits: Limits {
            max_metadata_bytes: 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        decode_with_options(&bytes, &options),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn unknown_chunks_obey_png_editor_copy_rules() {
    let mut png = base();
    png.ancillary = vec![
        AncillaryChunk {
            kind: *b"vpAG",
            data: vec![1],
            position: Pos::BeforeData,
        },
        AncillaryChunk {
            kind: *b"vpAg",
            data: vec![2],
            position: Pos::BeforeData,
        },
    ];
    let decoded = decode(&encode(&png, &EncodeOptions::default()).unwrap()).unwrap();
    assert_eq!(decoded.ancillary.len(), 1);
    assert_eq!(decoded.ancillary[0].kind, *b"vpAg");
    png.ancillary[0].kind = *b"ABCD";
    assert!(encode(&png, &EncodeOptions::default()).is_err());
}

#[test]
fn metadata_slots_without_palette_have_the_same_wire_position() {
    let mut png = base();
    let value = M::Text {
        keyword: b"Comment".to_vec(),
        text: b"same position without PLTE".to_vec(),
        compressed: false,
    };
    png.ancillary.push(
        value
            .to_chunk(Pos::BeforeData, Compression::Stored)
            .unwrap(),
    );
    for streaming in [false, true] {
        let bytes = if streaming {
            let mut out = Vec::new();
            deadlib_image_png::encode_writer(&png, &EncodeOptions::default(), &mut out).unwrap();
            out
        } else {
            encode(&png, &EncodeOptions::default()).unwrap()
        };
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.ancillary[0].position, Pos::BeforePalette);
        assert_eq!(
            decoded.metadata(Limits::default()).unwrap(),
            vec![value.clone()]
        );
    }
}
