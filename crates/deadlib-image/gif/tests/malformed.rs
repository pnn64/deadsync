use deadlib_image_gif::*;

// Independently packed, conforming 1x1 white GIF89a; no encoder constructs it.
fn tiny() -> Vec<u8> {
    b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x4c\x01\x00\x3b".to_vec()
}
fn insert(bytes: &[u8], data: &[u8]) -> Vec<u8> {
    let mut b = bytes.to_vec();
    b.splice(19..19, data.iter().copied());
    b
}

#[test]
fn every_truncation_and_every_stream_split() {
    let base = tiny();
    let bytes = insert(
        &base,
        &[
            0x21, 0xf9, 4, 0x0f, 0xff, 0xff, 0, 0, 0x21, 0xfe, 0, 0x21, 0xff, 11, b'N', b'E', b'T',
            b'S', b'C', b'A', b'P', b'E', b'2', b'.', b'0', 3, 1, 0, 0, 0,
        ],
    );
    let expected = decode(&bytes).unwrap();
    assert_eq!(expected.frames().next().unwrap().indices, [1]);
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err(), "truncation {end}");
    }
    for split in 0..=bytes.len() {
        let mut d = Decoder::default();
        let mut blocks = Vec::new();
        for part in [&bytes[..split], &bytes[split..]] {
            let mut at = 0;
            while at < part.len() {
                let p = d.update(&part[at..]).unwrap();
                assert!(p.consumed > 0);
                at += p.consumed;
                if let Some(Event::Block(b)) = p.event {
                    blocks.push(b);
                }
            }
            assert_eq!(d.update(&[]).unwrap().consumed, 0);
        }
        d.finish().unwrap();
        assert_eq!(blocks, expected.blocks);
    }
}
#[test]
fn reject_invalid_syntax_dimensions_indices_and_controls() {
    let base = tiny();
    for (offset, value) in [
        (0, b'X'),
        (3, b'9'),
        (6, 0),
        (8, 0),
        (11, 2),
        (19, 0),
        (24, 0),
        (26, 0),
        (28, 0x18),
        (29, 1),
        (29, 9),
        (30, 255),
        (31, 0xff),
        (33, 1),
    ] {
        let mut bytes = base.clone();
        bytes[offset] = value;
        assert!(decode(&bytes).is_err(), "offset {offset} value {value}");
    }
    let mut bytes = base.clone();
    bytes.extend_from_slice(&[0]);
    assert!(decode(&bytes).is_err());
    let gce = [0x21, 0xf9, 4, 1, 0, 0, 2, 0]; // Invalid transparent index.
    assert!(decode(&insert(&base, &gce)).is_err());
    for bad in [
        vec![0x21, 0xf9, 3, 0, 0, 0, 0, 0],
        vec![0x21, 0xf9, 4, 0, 0, 0, 0, 1],
        vec![0x21, 0xff, 10],
        vec![0x21, 1, 11],
    ] {
        assert!(decode(&insert(&base, &bad)).is_err());
    }
    // No EOI, too few pixels, and too many pixels are errors even in compatibility mode.
    for compressed in [vec![4], vec![0x2c], vec![0x04, 0x0a]] {
        let mut bytes = base[..30].to_vec();
        bytes.push(compressed.len() as u8);
        bytes.extend(compressed);
        bytes.extend_from_slice(&[0, 0x3b]);
        assert!(
            decode_with_options(
                &bytes,
                &DecodeOptions {
                    strict: false,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    let mut d = Decoder::default();
    assert!(d.update(b"BADBAD1234567").is_err());
    assert!(d.update(&base).is_err());
    assert!(d.finish().is_err());
}
#[test]
fn strict_and_compatibility_are_explicit() {
    let base = tiny();
    for data in [
        vec![0x21, 0xf9, 4, 0x80, 0, 0, 0, 0],
        vec![0x21, 0xf9, 4, 0, 0, 0, 0, 0, 0x21, 0xf9, 4, 4, 0, 0, 0, 0],
    ] {
        let bytes = insert(&base, &data);
        assert!(decode(&bytes).is_err());
        assert!(
            decode_with_options(
                &bytes,
                &DecodeOptions {
                    strict: false,
                    ..Default::default()
                }
            )
            .is_ok()
        );
    }
    let mut out = base.clone();
    out[20] = 1;
    assert!(decode(&out).is_err());
    assert!(
        decode_with_options(
            &out,
            &DecodeOptions {
                check_bounds: false,
                ..Default::default()
            }
        )
        .is_ok()
    );
    let mut old = insert(&base, &[0x21, 0xfe, 0]);
    old[4] = b'7';
    assert!(decode(&old).is_err());
    assert!(
        decode_with_options(
            &old,
            &DecodeOptions {
                strict: false,
                ..Default::default()
            }
        )
        .is_ok()
    );
    let mut dangling = base[..base.len() - 1].to_vec();
    dangling.extend_from_slice(&[0x21, 0xf9, 4, 0, 0, 0, 0, 0, 0x3b]);
    assert!(decode(&dangling).is_err());
    assert!(
        decode_with_options(
            &dangling,
            &DecodeOptions {
                strict: false,
                ..Default::default()
            }
        )
        .is_ok()
    );
}
#[test]
fn cumulative_limits_and_encoder_validation() {
    let bytes = insert(&tiny(), &[0x21, 0xfe, 1, b'a', 1, b'b', 0]);
    for field in 0..6 {
        let mut options = DecodeOptions::default();
        match field {
            0 => options.limits.max_input_bytes = bytes.len() - 1,
            1 => options.limits.max_frame_pixels = 0,
            2 => options.limits.max_total_pixels = 0,
            3 => options.limits.max_metadata_bytes = 1,
            4 => options.limits.max_blocks = 2,
            _ => options.limits.max_frames = 0,
        }
        assert_eq!(
            decode_with_options(&bytes, &options),
            Err(Error::LimitExceeded)
        );
    }
    let mut options = DecodeOptions::default();
    options.limits.max_input_bytes = bytes.len();
    assert!(decode_with_options(&bytes, &options).is_ok());
    let g = decode(&tiny()).unwrap();
    for p in [vec![], vec![[0; 3]], vec![[0; 3]; 3], vec![[0; 3]; 512]] {
        let mut invalid = g.clone();
        invalid.header.global_palette = Some(p);
        assert!(encode(&invalid, &EncodeOptions::default()).is_err());
    }
    let mut invalid = g.clone();
    let Block::Frame(f) = &mut invalid.blocks[0] else {
        panic!()
    };
    f.indices = vec![2];
    assert!(encode(&invalid, &EncodeOptions::default()).is_err());
    assert!(
        encode(
            &g,
            &EncodeOptions {
                image_sub_block_size: 0
            }
        )
        .is_err()
    );
    let mut writer = Encoder::new(Vec::new(), g.header.clone(), EncodeOptions::default()).unwrap();
    assert!(
        writer
            .write_block(&Block::Extension(Extension::Comment(vec![vec![]])))
            .is_err()
    );
    assert!(writer.write_block(&g.blocks[0]).is_err());
    assert!(writer.finish().is_err());
}

// A separate explicit code packer, driven by a list of codes, not by native LZW.
fn pack(codes: &[u16], min: u8) -> Vec<u8> {
    let clear = 1 << min;
    let mut next = clear + 2;
    let mut width = min + 1;
    let mut previous = false;
    let mut bytes = Vec::new();
    let mut acc = 0u64;
    let mut count = 0;
    for &code in codes {
        acc |= u64::from(code) << count;
        count += width;
        while count >= 8 {
            bytes.push(acc as u8);
            acc >>= 8;
            count -= 8;
        }
        if code == clear {
            next = clear + 2;
            width = min + 1;
            previous = false;
        } else if code != clear + 1 {
            if previous && next < 4096 {
                next += 1;
                if next == 1 << width && width < 12 {
                    width += 1;
                }
            }
            previous = true;
        }
    }
    if count != 0 {
        bytes.push(acc as u8);
    }
    bytes
}
#[test]
fn deferred_clear_frozen_dictionary_and_end_width_boundaries() {
    for min in 2..=8 {
        let clear = 1u16 << min;
        let mut codes = vec![clear];
        let expected: Vec<u8> = (0..7000).map(|i| (i % clear) as u8).collect();
        codes.extend(expected.iter().map(|&i| i as u16));
        codes.extend_from_slice(&[clear, clear, 1, clear + 1]);
        let packed = pack(&codes, min);
        let mut expected = expected;
        expected.push(1);
        assert_eq!(lzw::decode(&packed, min, expected.len()).unwrap(), expected);
        for chunk in [1, 7, 255] {
            let mut d = lzw::Decoder::new();
            d.reset(min).unwrap();
            let mut out = Vec::new();
            for p in packed.chunks(chunk) {
                d.update(p, &mut out, expected.len()).unwrap();
            }
            assert!(d.ended());
            assert_eq!(out, expected);
        }
        // The initial clear is a SHOULD, not a MUST in the specification.
        assert_eq!(
            lzw::decode(&pack(&[1, clear + 1], min), min, 1).unwrap(),
            [1]
        );
        for count in [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 14, 15, 16, 254, 255, 256, 4095, 4096,
        ] {
            let input: Vec<_> = (0..count).map(|i| (i % clear as usize) as u8).collect();
            let encoded = lzw::encode(&input, min).unwrap();
            assert_eq!(lzw::decode(&encoded, min, count).unwrap(), input);
        }
    }
}
#[test]
fn longest_dictionary_chain_and_exact_output_limits() {
    // Repeated KwKwK increases phrase length to the maximum legal chain.
    let mut codes = vec![4, 0];
    codes.extend(6..4096);
    codes.extend_from_slice(&[4095, 5]);
    let size = 1 + (2..=4091).sum::<usize>() + 4091;
    let packed = pack(&codes, 2);
    let out = lzw::decode(&packed, 2, size).unwrap();
    assert_eq!(out.len(), size);
    assert!(out.iter().all(|&v| v == 0));
    assert!(lzw::decode(&packed, 2, size - 1).is_err());
}
