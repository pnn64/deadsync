use deadlib_image_gif::*;
use std::borrow::Cow;

fn palette(size: usize) -> Palette {
    (0..size)
        .map(|i| [i as u8, (i * 71) as u8, (i * 139) as u8])
        .collect()
}
fn pixels(size: usize, colors: usize, mut seed: u32) -> Vec<u8> {
    (0..size)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed as usize % colors) as u8
        })
        .collect()
}
fn verify_reference(bytes: &[u8], expected: &Gif) {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut reader = options.read_info(bytes).unwrap();
    assert_eq!(reader.width(), expected.header.width);
    assert_eq!(reader.height(), expected.header.height);
    let global: Vec<_> = expected
        .header
        .global_palette
        .iter()
        .flatten()
        .flatten()
        .copied()
        .collect();
    assert_eq!(
        reader.global_palette(),
        if global.is_empty() {
            None
        } else {
            Some(global.as_slice())
        }
    );
    for f in expected.frames() {
        let actual = reader.read_next_frame().unwrap().unwrap();
        assert_eq!(
            (actual.left, actual.top, actual.width, actual.height),
            (f.left, f.top, f.width, f.height)
        );
        assert_eq!(actual.buffer.as_ref(), f.indices);
        assert_eq!(actual.delay, f.control.unwrap_or_default().delay);
        assert_eq!(
            actual.transparent,
            f.control.unwrap_or_default().transparent_index
        );
        assert_eq!(
            actual.dispose as u8,
            f.control.unwrap_or_default().disposal as u8
        );
        assert_eq!(
            actual.needs_user_input,
            f.control.unwrap_or_default().user_input
        );
    }
    assert!(reader.read_next_frame().unwrap().is_none());
}

#[test]
fn native_encoder_reference_decoder_all_palette_sizes_and_interlace() {
    for colors in [2, 4, 8, 16, 32, 64, 128, 256] {
        for (width, height) in [(1, 1), (3, 2), (17, 13), (257, 73)] {
            for interlaced in [false, true] {
                let mut header = Header::new(width, height);
                header.global_palette = Some(palette(colors));
                let mut gif = Gif::new(header);
                let mut frame = Frame::new(
                    width,
                    height,
                    pixels(width as usize * height as usize, colors, 123),
                );
                frame.interlaced = interlaced;
                frame.control = Some(GraphicControl {
                    delay: 123,
                    transparent_index: Some((colors - 1) as u8),
                    disposal: Disposal::Previous,
                    user_input: true,
                });
                gif.blocks.push(Block::Frame(frame));
                for size in [1, 7, 255] {
                    let encoded = encode(
                        &gif,
                        &EncodeOptions {
                            image_sub_block_size: size,
                        },
                    )
                    .unwrap();
                    assert_eq!(decode(&encoded).unwrap(), gif);
                    verify_reference(&encoded, &gif);
                }
            }
        }
    }
}

#[test]
fn reference_encoder_native_decoder_and_lzw_saturation() {
    for colors in [2, 4, 16, 256] {
        let p = palette(colors);
        let flat: Vec<_> = p.iter().flatten().copied().collect();
        for data in [
            pixels(512 * 256, colors, 987),
            vec![0; 512 * 256],
            (0..512 * 256).map(|i| (i % colors) as u8).collect(),
        ] {
            let mut bytes = Vec::new();
            {
                let mut encoder = gif::Encoder::new(&mut bytes, 512, 256, &flat).unwrap();
                encoder
                    .write_frame(&gif::Frame {
                        width: 512,
                        height: 256,
                        buffer: Cow::Borrowed(&data),
                        ..Default::default()
                    })
                    .unwrap();
            }
            let decoded = decode(&bytes).unwrap();
            assert_eq!(decoded.frames().next().unwrap().indices, data);
            let native = encode(&decoded, &EncodeOptions::default()).unwrap();
            verify_reference(&native, &decoded);
        }
    }
}

#[test]
fn arbitrary_input_boundaries_and_recycling() {
    let mut g = Gif::new(Header::new(17, 13));
    g.header.global_palette = Some(palette(16));
    for i in 0..5 {
        let mut f = Frame::new(17, 13, pixels(221, 16, i + 1));
        f.interlaced = i % 2 == 0;
        g.blocks.push(Block::Frame(f));
    }
    let bytes = encode(&g, &EncodeOptions::default()).unwrap();
    for chunk in [1, 2, 3, 7, 255, 1024] {
        let mut decoder = Decoder::default();
        let mut blocks = Vec::new();
        for part in bytes.chunks(chunk) {
            let mut at = 0;
            while at < part.len() {
                let p = decoder.update(&part[at..]).unwrap();
                assert!(p.consumed > 0);
                at += p.consumed;
                if let Some(Event::Block(b)) = p.event {
                    blocks.push(b);
                }
            }
        }
        decoder.finish().unwrap();
        assert_eq!(blocks, g.blocks);
    }
    let mut decoder = Decoder::default();
    let mut at = 0;
    let mut n = 0;
    while at < bytes.len() {
        let p = decoder.update(&bytes[at..]).unwrap();
        at += p.consumed;
        if let Some(Event::Block(Block::Frame(f))) = p.event {
            assert_eq!(&f, g.frames().nth(n).unwrap());
            n += 1;
            decoder.recycle_frame(f);
        }
    }
    decoder.finish().unwrap();
    assert_eq!(n, 5);
}

#[test]
fn lzw_hand_packed_kwkwk_and_clear() {
    // clear, '0', code 6 ('00'), code 7 ('000'), end; widths 3,3,3,3,4.
    let bytes = [0x84, 0x5f];
    assert_eq!(lzw::decode(&bytes, 2, 6).unwrap(), vec![0; 6]);
    // Clear between two literals; a clear may appear anywhere.
    let bytes = [0x04, 0x53];
    assert_eq!(lzw::decode(&bytes, 2, 2).unwrap(), vec![0, 1]);
    assert!(lzw::decode(&[0xff], 2, 1).is_err());
    assert!(lzw::decode(&bytes, 1, 2).is_err());
}

#[test]
fn gif87a_and_shared_image() {
    let mut g = Gif::new(Header::new(2, 1));
    g.header.version = Version::Gif87a;
    g.header.global_palette = Some(palette(2));
    g.blocks.push(Block::Frame(Frame::new(2, 1, vec![1, 0])));
    let bytes = encode(&g, &EncodeOptions::default()).unwrap();
    assert!(bytes.starts_with(b"GIF87a"));
    assert_eq!(decode(&bytes).unwrap(), g);
    let image = g.frames().next().unwrap().clone().into_image();
    image.validate().unwrap();
    assert_eq!(Frame::from_image(image).unwrap().indices, [1, 0]);
}
