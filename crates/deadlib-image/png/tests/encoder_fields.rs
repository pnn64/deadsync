//! Encoder input constraints and literal wire answers, independent of decoding.
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "support/ancillary_order.rs"]
mod order;
#[allow(dead_code)]
#[path = "support/scalar_fields.rs"]
mod scalar;

use deadlib_image_png::{
    AncillaryChunk, ChunkPosition, ColorType, EncodeOptions, Error, Filter, Png, Transparency,
    encode, encode_writer, zlib::Compression,
};

fn options(interlaced: bool) -> EncodeOptions {
    EncodeOptions {
        interlaced,
        compression: Compression::Stored,
        filter: Filter::None,
        idat_chunk_size: 65536,
    }
}

fn check(png: &Png, opts: &EncodeOptions, expected: Option<&[u8]>, label: &str) {
    let buffered = encode(png, opts);
    let mut streamed = Vec::new();
    let result = encode_writer(png, opts, &mut streamed);
    if let Some(bytes) = expected {
        assert_eq!(buffered.unwrap(), bytes, "buffered {label}");
        assert_eq!(result.unwrap().bytes_written, bytes.len(), "{label}");
        assert_eq!(streamed, bytes, "streamed {label}");
    } else {
        assert!(
            matches!(buffered, Err(Error::Invalid(_))),
            "{label}: {buffered:?}"
        );
        assert!(
            matches!(result, Err(Error::Invalid(_))),
            "{label}: {result:?}"
        );
        assert!(streamed.is_empty(), "invalid input wrote output: {label}");
    }
}

#[test]
fn scalar_input_fields_and_exact_wire_values_for_both_encoders() {
    let (mut seen, mut accepted, mut rejected, mut unrepresentable) = (0, 0, 0, 0);
    scalar::visit(|mut case| {
        seen += 1;
        // The native API stores whole palette entries and fixed-width gray/RGB
        // transparency words. It cannot express malformed wire lengths there.
        if (case.kind == *b"PLTE" && (case.data.is_empty() || !case.data.len().is_multiple_of(3)))
            || (case.kind == *b"tRNS"
                && matches!(case.format.color, ColorType::Grayscale | ColorType::Rgb)
                && case.data.len()
                    != if case.format.color == ColorType::Grayscale {
                        2
                    } else {
                        6
                    })
        {
            unrepresentable += 1;
            return;
        }
        let mut png = Png::new(case.format.image());
        png.palette = case.palette();
        if case.kind == *b"tRNS" {
            png.transparency = case.transparency.clone();
        } else if case.kind != *b"PLTE" {
            png.ancillary.push(AncillaryChunk {
                kind: case.kind,
                data: case.data.clone(),
                position: if matches!(&case.kind, b"gAMA" | b"cHRM" | b"sBIT" | b"sRGB") {
                    ChunkPosition::BeforePalette
                } else {
                    ChunkPosition::BeforeData
                },
            });
        }
        // PNG requires unused high bits of these encoded sample words to be
        // zero. The input API deliberately accepts them and normalizes them.
        if case.valid
            && matches!(&case.kind, b"tRNS" | b"bKGD")
            && case.format.color != ColorType::Indexed
        {
            let mask = ((1u32 << case.format.depth) - 1) as u16;
            for word in case.data.as_chunks_mut::<2>().0 {
                *word = (u16::from_be_bytes(*word) & mask).to_be_bytes();
            }
        }
        let expected = case.valid.then(|| case.wire());
        check(
            &png,
            &options(case.format.interlaced),
            expected.as_deref(),
            &case.name,
        );
        if case.valid {
            accepted += 1
        } else {
            rejected += 1
        }
    });
    assert_eq!(seen, 207348);
    assert!(accepted > 90000 && rejected > 80000);
    println!(
        "Encoder scalar audit: {accepted} exact literal files, {rejected} rejected inputs, {unrepresentable} malformed wire lengths unrepresentable by the native API; each checked through both encoders"
    );
}

// The independent ordering factory includes a second empty IDAT. Both encoders
// emit a single chunk for these tiny stored streams; remove only that empty
// chunk, preserving every other literal byte and independently computed CRC.
fn without_empty_idat(bytes: &[u8]) -> Vec<u8> {
    let mut result = bytes[..8].to_vec();
    let mut offset = 8;
    while offset < bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let end = offset + length + 12;
        if length != 0 || &bytes[offset + 4..offset + 8] != b"IDAT" {
            result.extend_from_slice(&bytes[offset..end]);
        }
        offset = end;
    }
    result
}

#[test]
fn metadata_placement_multiplicity_and_pairs_follow_table_seven_when_writing() {
    let mut count = 0;
    for context in order::contexts() {
        // tRNS placement and multiplicity are fixed by Png::transparency, not
        // caller-controlled AncillaryChunk positions. Scalar tests cover it.
        for first in order::NAMES.into_iter().filter(|n| n != b"tRNS") {
            for second in order::NAMES.into_iter().filter(|n| n != b"tRNS") {
                for slot_a in order::PAIR_SLOTS {
                    for slot_b in order::PAIR_SLOTS {
                        let mut entries = vec![
                            order::entry(context, first, slot_a),
                            order::entry(context, second, slot_b),
                        ];
                        order::companion(context, &mut entries);
                        let valid = order::valid(context, &entries);
                        let png = order::expected(context, &entries);
                        let expected = valid.then(|| {
                            without_empty_idat(&order::fixture(context, &entries, usize::MAX))
                        });
                        check(
                            &png,
                            &options(false),
                            expected.as_deref(),
                            &format!("{context:?}/{entries:?}"),
                        );
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 19 * 17 * 17 * 9);
    println!(
        "Encoder ordering audit: {count} independently modeled pairs, both writers, exact literal output or rejection before output"
    );
}

#[test]
fn native_palette_transparency_and_option_boundaries_reject_before_writing() {
    let mut count = 0;
    for (color, depth) in scalar::FORMATS {
        let format = scalar::Format {
            color,
            depth,
            interlaced: false,
            palette: None,
        };
        for palette in 0..=257 {
            let mut png = Png::new(format.image());
            png.palette = vec![[0; 3]; palette];
            let valid = match color {
                ColorType::Indexed => (1..=(1usize << depth)).contains(&palette),
                ColorType::Grayscale | ColorType::GrayscaleAlpha => palette == 0,
                _ => palette <= 256,
            };
            if valid {
                for length in 0..=257 {
                    let mut png = png.clone();
                    png.transparency = Some(Transparency::Palette(vec![255; length]));
                    if color != ColorType::Indexed || length > palette {
                        check(
                            &png,
                            &options(false),
                            None,
                            "palette transparency length/type",
                        );
                        count += 1;
                    }
                }
            } else {
                check(&png, &options(false), None, "palette count/color/depth");
                count += 1;
            }
        }
        let mut png = Png::new(format.image());
        if color == ColorType::Indexed {
            png.palette = vec![[0; 3]];
        }
        for size in [0, 0x8000_0000usize, usize::MAX] {
            check(
                &png,
                &EncodeOptions {
                    idat_chunk_size: size,
                    ..options(false)
                },
                None,
                "chunk size",
            );
            count += 1;
        }
    }
    println!(
        "Encoder native boundary audit: {count} invalid inputs rejected by both writers before any output"
    );
}
