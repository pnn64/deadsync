use deadlib_image_gif::*;

// Independently packed 1x1 white GIF89a; the native encoder is not the oracle.
fn tiny() -> Vec<u8> {
    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x4c\x01\x00\x3b".to_vec()
}

fn compatible() -> DecodeOptions {
    DecodeOptions {
        strict: false,
        ..Default::default()
    }
}

fn image_data(compressed: &[u8]) -> Vec<u8> {
    monochrome_image(compressed, 2, 1)
}

fn monochrome_image(compressed: &[u8], minimum: u8, width: u16) -> Vec<u8> {
    let mut bytes = tiny()[..30].to_vec();
    bytes[6..8].copy_from_slice(&width.to_le_bytes());
    bytes[24..26].copy_from_slice(&width.to_le_bytes());
    bytes[29] = minimum;
    for block in compressed.chunks(255) {
        bytes.push(u8::try_from(block.len()).unwrap());
        bytes.extend_from_slice(block);
    }
    bytes.extend_from_slice(&[0, 0x3b]);
    bytes
}

fn reference_indices(bytes: &[u8]) -> Vec<u8> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    let mut reader = options.read_info(bytes).unwrap();
    let indices = reader.read_next_frame().unwrap().unwrap().buffer.to_vec();
    assert!(reader.read_next_frame().unwrap().is_none());
    indices
}

fn insert_control(bytes: &mut Vec<u8>, packed: u8, transparent: u8) {
    bytes.splice(19..19, [0x21, 0xf9, 4, packed, 0, 0, transparent, 0]);
}

fn recovered() -> Vec<u8> {
    let mut bytes = image_data(&[0x04]); // CLEAR, black pixel; no EOI.
    bytes[11] = 2; // Unused background outside the two-entry palette.
    insert_control(&mut bytes, 0x80, 0); // Reserved control bit.
    bytes.splice(19..19, [0x21, 0xfe, 1, b'x', 0]);
    bytes.extend_from_slice(b"suffix");
    bytes
}

fn streamed(parts: &[&[u8]], options: &DecodeOptions) -> Result<Gif, Error> {
    let mut decoder = Decoder::new(options.clone());
    let mut gif = None;
    for part in parts {
        let mut at = 0;
        while at < part.len() {
            let progress = decoder.update(&part[at..])?;
            assert!(progress.consumed > 0, "nonempty input must advance");
            at += progress.consumed;
            match progress.event {
                Some(Event::Header) => gif = Some(Gif::new(decoder.header().unwrap().clone())),
                Some(Event::Block(block)) => gif.as_mut().unwrap().blocks.push(block),
                Some(Event::End) | None => {}
            }
        }
        assert_eq!(decoder.update(&[])?.consumed, 0);
    }
    decoder.finish()?;
    gif.ok_or(Error::Truncated)
}

#[test]
fn unused_background_is_preserved_and_explicit_canvas_controls_disposal() {
    let mut bytes = tiny();
    bytes[6] = 2; // Two-pixel logical canvas, one-pixel frame.
    bytes[11] = 2;
    insert_control(&mut bytes, 8, 0); // Restore background after the frame.
    assert!(decode(&bytes).is_err());
    let gif = decode_with_options(&bytes, &compatible()).unwrap();
    assert_eq!(gif.header.background_index, 2);
    assert_eq!(gif.header.global_palette, Some(vec![[0; 3], [255; 3]]));
    let first = gif.frames().next().unwrap();
    assert_eq!(first.indices, [1]);
    assert!(Compositor::new(&gif.header).is_err());
    assert!(encode(&gif, &EncodeOptions::default()).is_err());

    let mut second = first.clone();
    second.left = 1;
    second.indices = vec![0];
    second.control = None;
    for (background, rgba) in [
        (Background::Transparent, [0; 4]),
        (Background::Color([7, 11, 13, 17]), [7, 11, 13, 17]),
    ] {
        let mut compositor = Compositor::with_options(
            &gif.header,
            CompositeOptions {
                background,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(compositor.pixels(), [rgba, rgba].as_flattened());
        assert_eq!(
            compositor.render(first).unwrap().rgba,
            [[255; 4], rgba].as_flattened()
        );
        assert_eq!(
            compositor.render(&second).unwrap().rgba,
            [rgba, [0, 0, 0, 255]].as_flattened()
        );
        compositor.reset();
        assert_eq!(compositor.pixels(), [rgba, rgba].as_flattened());
    }
}

#[test]
fn compatibility_keeps_structural_and_pixel_validation() {
    let valid = decode(&tiny()).unwrap();
    for field in 0..4 {
        let mut header = valid.header.clone();
        match field {
            0 => header.width = 0,
            1 => header.height = 0,
            2 => header.color_resolution = 0,
            _ => header.global_palette = Some(vec![[0; 3]; 3]),
        }
        for background in [Background::Transparent, Background::Color([1; 4])] {
            assert!(
                Compositor::with_options(
                    &header,
                    CompositeOptions {
                        background,
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
    }
    let mut empty = tiny();
    empty[6] = 0;
    assert!(decode_with_options(&empty, &compatible()).is_err());
    let invalid_pixel = image_data(&[0x54, 0x01]); // CLEAR, index 2, EOI.
    assert!(decode_with_options(&invalid_pixel, &compatible()).is_err());
    let mut invalid_transparency = tiny();
    insert_control(&mut invalid_transparency, 1, 2);
    assert!(decode_with_options(&invalid_transparency, &compatible()).is_err());
    let mut invalid_control = tiny();
    invalid_control.splice(19..19, [0x21, 0xf9, 3, 0, 0, 0, 0, 0]);
    assert!(decode_with_options(&invalid_control, &compatible()).is_err());
}

#[test]
fn missing_eoi_requires_complete_pixels_and_the_data_terminator() {
    let bytes = image_data(&[0x04]);
    assert!(decode(&bytes).is_err());
    assert!(lzw::decode(&[0x04], 2, 1).is_err());
    let gif = decode_with_options(&bytes, &compatible()).unwrap();
    assert_eq!(gif.frames().next().unwrap().indices, [0]);
    for end in 0..bytes.len() {
        assert!(
            decode_with_options(&bytes[..end], &compatible()).is_err(),
            "truncation at {end} must not replace a terminator or trailer"
        );
        assert!(streamed(&[&bytes[..end]], &compatible()).is_err());
    }
    let mut decoder = Decoder::new(compatible());
    let mut at = 0;
    while at < bytes.len() - 2 {
        let progress = decoder.update(&bytes[at..bytes.len() - 2]).unwrap();
        at += progress.consumed;
        assert!(!matches!(
            progress.event,
            Some(Event::Block(Block::Frame(_)))
        ));
    }
    assert_eq!(decoder.finish(), Err(Error::Truncated));
    let progress = decoder.update(&[0]).unwrap();
    assert!(matches!(
        progress.event,
        Some(Event::Block(Block::Frame(_)))
    ));
    assert_eq!(decoder.finish(), Err(Error::Truncated));
    assert_eq!(decoder.update(&[0x3b]).unwrap().event, Some(Event::End));
    decoder.finish().unwrap();
}

#[test]
fn compatibility_rejects_underfilled_overfilled_and_undefined_lzw() {
    for compressed in [
        &[0x2c][..],       // CLEAR, EOI with no pixel.
        &[0x04, 0x0a][..], // A second literal exceeds the frame.
        &[0x84, 0x0b][..], // A dictionary phrase exceeds the frame.
        &[0x3c][..],       // CLEAR followed by undefined code 7.
    ] {
        let bytes = image_data(compressed);
        assert!(decode_with_options(&bytes, &compatible()).is_err());
        for split in 0..=bytes.len() {
            assert!(streamed(&[&bytes[..split], &bytes[split..]], &compatible()).is_err());
        }
    }
    let mut underfilled = image_data(&[0x04]);
    underfilled[6] = 2;
    underfilled[24] = 2;
    assert!(decode_with_options(&underfilled, &compatible()).is_err());
}

#[test]
fn one_bit_minimum_preserves_codes_and_width_transitions() {
    // Independent explicit code widths, also checked by the reference decoder:
    // CLEAR=2, EOI=3. The second literal grows the width from two to three.
    let cases: &[(&[u8], &[u8])] = &[
        (&[0x32], &[0]),                         // CLEAR:2, 0:2, EOI:2.
        (&[0xd2, 0x00], &[0, 1]),                // CLEAR:2, 0:2, 1:2, EOI:3.
        (&[0x12, 0x07], &[0, 1, 0, 1]),          // Dictionary code 4 at width three.
        (&[0x52, 0x07], &[0, 1, 1, 1]),          // KwKwK code 5 at width three.
        (&[0x12, 0x82, 0x01], &[0, 1, 0, 1, 0]), // EOI grows to width four.
        (&[0x92, 0x62], &[0, 1, 1, 0]),          // CLEAR restores width two.
        (&[0x12], &[0, 1]),                      // No EOI; only two residual bits remain.
    ];
    assert!(lzw::Decoder::new().reset(1).is_err());
    assert!(lzw::encode(&[0, 1], 1).is_err());
    for &(compressed, expected) in cases {
        let bytes = monochrome_image(compressed, 1, expected.len() as u16);
        assert_eq!(reference_indices(&bytes), expected);
        assert!(decode(&bytes).is_err());
        assert!(lzw::decode(compressed, 1, expected.len()).is_err());
        let actual = decode_with_options(&bytes, &compatible()).unwrap();
        assert_eq!(actual.frames().next().unwrap().indices, expected);
        for split in 0..=bytes.len() {
            assert_eq!(
                streamed(&[&bytes[..split], &bytes[split..]], &compatible()).unwrap(),
                actual
            );
        }
        // Minimum code size is a wire detail, so conforming min-two encoding
        // preserves the complete decoded model, including original indices.
        let normalized = encode(&actual, &EncodeOptions::default()).unwrap();
        assert_eq!(decode(&normalized).unwrap(), actual);
        assert_eq!(reference_indices(&normalized), expected);
    }
    // Padding that the LZW parser sees as additional pixels is not recovered.
    assert!(decode_with_options(&monochrome_image(&[0x02], 1, 1), &compatible()).is_err());
    for minimum in [0, 9] {
        let bytes = monochrome_image(&[0x32], minimum, 1);
        for options in [DecodeOptions::default(), compatible()] {
            assert_eq!(
                decode_with_options(&bytes, &options),
                Err(Error::Invalid("LZW minimum code size"))
            );
        }
    }
}

#[test]
fn one_bit_minimum_reaches_a_frozen_dictionary_and_resets() {
    // Literal-only stream with independently specified width runs. This does
    // not reproduce the decoder's dictionary or width-update implementation.
    let mut codes = vec![(2u16, 2u8)];
    let mut expected = Vec::new();
    for (width, count) in [
        (2, 2),
        (3, 3),
        (4, 8),
        (5, 16),
        (6, 32),
        (7, 64),
        (8, 128),
        (9, 256),
        (10, 512),
        (11, 1024),
        (12, 4096),
    ] {
        for _ in 0..count {
            let pixel = (expected.len() & 1) as u8;
            expected.push(pixel);
            codes.push((u16::from(pixel), width));
        }
    }
    // Entry 4095 is the last inserted pair [1, 0]. Use it after the table
    // freezes, then CLEAR and two literals to exercise width growth again.
    codes.extend_from_slice(&[(4095, 12), (2, 12), (1, 2), (0, 2), (3, 3)]);
    expected.extend_from_slice(&[1, 0, 1, 0]);
    let mut compressed = Vec::new();
    let mut bits = 0u64;
    let mut remaining = 0u8;
    for (code, width) in codes {
        bits |= u64::from(code) << remaining;
        remaining += width;
        while remaining >= 8 {
            compressed.push(bits as u8);
            bits >>= 8;
            remaining -= 8;
        }
    }
    if remaining != 0 {
        compressed.push(bits as u8);
    }
    let bytes = monochrome_image(&compressed, 1, expected.len() as u16);
    assert_eq!(reference_indices(&bytes), expected);
    let actual = decode_with_options(&bytes, &compatible()).unwrap();
    assert_eq!(actual.frames().next().unwrap().indices, expected);
    for size in [1, 2, 7, 255, 1024, bytes.len()] {
        let parts: Vec<_> = bytes.chunks(size).collect();
        assert_eq!(streamed(&parts, &compatible()).unwrap(), actual);
    }
}

#[test]
fn recovered_frame_recycling_cannot_supply_missing_pixels() {
    let first = image_data(&[0x04]);
    let second = image_data(&[0x2c]);
    let mut bytes = first[..first.len() - 1].to_vec();
    bytes.extend_from_slice(&second[19..]);
    for chunk in [1, 7, bytes.len()] {
        let mut decoder = Decoder::new(compatible());
        let mut at = 0;
        let mut frames = 0;
        loop {
            assert!(at < bytes.len(), "accepted an underfilled second frame");
            let end = (at + chunk).min(bytes.len());
            match decoder.update(&bytes[at..end]) {
                Ok(progress) => {
                    assert!(progress.consumed > 0);
                    at += progress.consumed;
                    if let Some(Event::Block(Block::Frame(frame))) = progress.event {
                        frames += 1;
                        assert_eq!(frame.indices, [0]);
                        decoder.recycle_frame(frame);
                    }
                }
                Err(error) => {
                    assert_eq!(error, Error::Invalid("incomplete LZW image"));
                    break;
                }
            }
        }
        assert_eq!(frames, 1);
        assert!(decoder.finish().is_err());
        assert!(decoder.update(&tiny()).is_err());
    }
}

#[test]
fn recovered_inputs_match_at_every_stream_split() {
    let mut background = tiny();
    background[11] = 255;
    let mut control_and_suffix = tiny();
    insert_control(&mut control_and_suffix, 0x80, 0);
    control_and_suffix.extend_from_slice(b"GIF89a\0\xff");
    for bytes in [
        background,
        image_data(&[0x04]),
        control_and_suffix,
        recovered(),
    ] {
        let expected = decode_with_options(&bytes, &compatible()).unwrap();
        for split in 0..=bytes.len() {
            assert_eq!(
                streamed(&[&bytes[..split], &bytes[split..]], &compatible()).unwrap(),
                expected,
                "split {split}"
            );
        }
        let parts: Vec<_> = bytes.chunks(1).collect();
        assert_eq!(streamed(&parts, &compatible()).unwrap(), expected);
    }
}

#[test]
fn trailing_bytes_are_charged_and_stream_errors_stay_failed() {
    let base = tiny();
    let mut bytes = base.clone();
    bytes.extend_from_slice(b"tail");
    assert!(decode(&bytes).is_err());
    assert!(streamed(&[&base, b"tail"], &DecodeOptions::default()).is_err());
    let mut options = compatible();
    options.limits.max_input_bytes = bytes.len();
    assert_eq!(
        decode_with_options(&bytes, &options).unwrap(),
        decode(&base).unwrap()
    );
    assert!(streamed(&[&base, b"tail"], &options).is_ok());
    options.limits.max_input_bytes -= 1;
    assert_eq!(
        decode_with_options(&bytes, &options),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        streamed(&[&base, b"tail"], &options),
        Err(Error::LimitExceeded)
    );

    options.limits.max_input_bytes = base.len() + 1;
    let mut decoder = Decoder::new(options);
    let mut at = 0;
    while at < base.len() {
        let progress = decoder.update(&base[at..]).unwrap();
        at += progress.consumed;
    }
    decoder.finish().unwrap();
    assert_eq!(decoder.update(b"x").unwrap().consumed, 1);
    assert_eq!(decoder.update(b"y"), Err(Error::LimitExceeded));
    assert_eq!(decoder.finish(), Err(Error::Invalid("decoder is failed")));
    assert_eq!(
        decoder.update(&[]),
        Err(Error::Invalid("decoder is failed"))
    );
}

#[test]
fn recovered_inputs_still_obey_every_resource_limit() {
    let bytes = recovered();
    assert!(decode_with_options(&bytes, &compatible()).is_ok());
    for field in 0..6 {
        let mut options = compatible();
        match field {
            0 => options.limits.max_input_bytes = bytes.len() - 1,
            1 => options.limits.max_frame_pixels = 0,
            2 => options.limits.max_total_pixels = 0,
            3 => options.limits.max_metadata_bytes = 0,
            4 => options.limits.max_blocks = 0,
            _ => options.limits.max_frames = 0,
        }
        assert_eq!(
            decode_with_options(&bytes, &options),
            Err(Error::LimitExceeded)
        );
        let parts: Vec<_> = bytes.chunks(1).collect();
        assert_eq!(streamed(&parts, &options), Err(Error::LimitExceeded));
    }
}
