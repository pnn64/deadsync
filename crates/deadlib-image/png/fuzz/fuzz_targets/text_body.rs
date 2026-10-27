#![no_main]
use deadlib_image_png::{
    ColorType, DecodeOptions, EncodeOptions, Image, Limits, Metadata, Png, Samples, decode,
    decode_reader, encode, encode_writer, zlib::Compression,
};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;
#[path = "../../tests/support/text_body.rs"]
mod body;
#[allow(dead_code)]
#[path = "../../tests/support/chunks.rs"]
mod chunks;

fn reference(wire: &[u8], text: &[u8], compressed: bool) {
    let mut reader = png::Decoder::new(Cursor::new(wire)).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(&pixels[..info.buffer_size()], chunks::PIXEL);
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

fuzz_target!(|input: &[u8]| {
    if input.len() > 4097 {
        return;
    }
    let flags = input.first().copied().unwrap_or(0);
    let text = input.get(1..).unwrap_or_default();
    let compressed = flags & 1 != 0;
    let position = chunks::POSITIONS[usize::from(flags >> 1) % 3];
    let compression = if flags & 8 == 0 {
        Compression::Stored
    } else {
        Compression::Fast
    };
    let decodable = !text.contains(&0);
    let encodable = text.iter().all(|&b| matches!(b,10|32..=126|160..=255));
    let raw = body::raw(text, compressed, position);
    let wire = body::fixture(text, compressed, position);
    let options = DecodeOptions {
        limits: Limits {
            max_input_bytes: 65536,
            max_output_bytes: 65536,
            max_metadata_bytes: 32768,
            max_chunks: 32,
            max_frames: 1,
        },
        ..Default::default()
    };
    let expected = Metadata::Text {
        keyword: b"Body".to_vec(),
        text: text.to_vec(),
        compressed,
    };
    for result in [decode(&wire), decode_reader(wire.as_slice(), &options)] {
        assert_eq!(result.is_ok(), decodable);
        if let Ok(png) = result {
            assert_eq!(png.ancillary.as_slice(), std::slice::from_ref(&raw));
            assert_eq!(
                png.metadata(Limits::default()).unwrap().as_slice(),
                std::slice::from_ref(&expected)
            );
        }
    }
    let typed = expected.to_chunk(position, compression);
    assert_eq!(typed.is_ok(), encodable);
    let mut png = Png::new(Image {
        width: 1,
        height: 1,
        color_type: ColorType::Rgb,
        bit_depth: 8,
        samples: Samples::U8(chunks::PIXEL.to_vec()),
    });
    png.palette = vec![[0, 0, 0], [255, 255, 255]];
    let mut candidates = vec![raw];
    if let Ok(chunk) = typed {
        candidates.push(chunk);
    }
    for chunk in candidates {
        png.ancillary = vec![chunk];
        let options = EncodeOptions {
            compression,
            ..Default::default()
        };
        let buffered = encode(&png, &options);
        let mut streamed = Vec::new();
        let writer = encode_writer(&png, &options, &mut streamed);
        assert_eq!(buffered.is_ok(), encodable);
        assert_eq!(writer.is_ok(), encodable);
        if encodable {
            for wire in [buffered.unwrap(), streamed] {
                assert_eq!(
                    decode(&wire)
                        .unwrap()
                        .metadata(Limits::default())
                        .unwrap()
                        .as_slice(),
                    std::slice::from_ref(&expected)
                );
                reference(&wire, text, compressed);
            }
        } else {
            assert!(streamed.is_empty());
        }
    }
});
