use deadlib_image_png::{DecodeOptions, Error, Limits, decode, decode_with_options};

fn crc(bytes: &[u8]) -> u32 {
    let mut state = !0u32;
    for &b in bytes {
        state ^= u32::from(b);
        for _ in 0..8 {
            state = (state >> 1) ^ if state & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !state
}
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc(&out[4..]).to_be_bytes());
    out
}
fn ihdr(color: u8, depth: u8) -> Vec<u8> {
    let mut data = vec![0, 0, 0, 1, 0, 0, 0, 1, depth, color, 0, 0, 0];
    let out = chunk(b"IHDR", &data);
    data.clear();
    out
}
fn file(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
    out
}
fn image_data(raw: &[u8]) -> Vec<u8> {
    // Independently constructed zlib stored block, including Adler-32.
    let len = raw.len() as u16;
    let mut data = vec![0x78, 0x01, 0x01];
    data.extend_from_slice(&len.to_le_bytes());
    data.extend_from_slice(&(!len).to_le_bytes());
    data.extend_from_slice(raw);
    let (mut a, mut b) = (1u32, 0u32);
    for &v in raw {
        a = (a + u32::from(v)) % 65521;
        b = (b + a) % 65521;
    }
    data.extend_from_slice(&((b << 16) | a).to_be_bytes());
    chunk(b"IDAT", &data)
}

#[test]
fn independent_known_answer_and_normative_failures() {
    let h = ihdr(0, 8);
    let d = image_data(&[0, 197]);
    let e = chunk(b"IEND", &[]);
    let known = file(&[h.clone(), d.clone(), e.clone()]);
    assert_eq!(
        decode(&known).unwrap().image.samples,
        deadlib_image_png::Samples::U8(vec![197])
    );
    let bad = vec![
        file(&[d.clone(), h.clone(), e.clone()]), // header not first
        file(&[h.clone(), h.clone(), d.clone(), e.clone()]),
        file(&[h.clone(), e.clone()]),
        file(&[h.clone(), d.clone(), chunk(b"IEND", &[0])]),
        file(&[h.clone(), d.clone(), e.clone(), e.clone()]),
        file(&[h.clone(), image_data(&[5, 197]), e.clone()]),
        file(&[h.clone(), image_data(&[0]), e.clone()]),
        file(&[h.clone(), image_data(&[0, 1, 2]), e.clone()]),
        file(&[h.clone(), chunk(b"PLTE", &[1, 2, 3]), d.clone(), e.clone()]),
        file(&[ihdr(3, 1), d.clone(), e.clone()]),
        file(&[
            ihdr(3, 1),
            chunk(b"PLTE", &[1, 2, 3]),
            image_data(&[0, 128]),
            e.clone(),
        ]),
        file(&[ihdr(3, 1), chunk(b"PLTE", &[0; 9]), d.clone(), e.clone()]),
        file(&[h.clone(), chunk(b"tRNS", &[0]), d.clone(), e.clone()]),
        file(&[
            ihdr(6, 8),
            chunk(b"tRNS", &[0, 0]),
            image_data(&[0, 0, 0, 0, 0]),
            e.clone(),
        ]),
        file(&[h.clone(), chunk(b"ABCD", &[]), d.clone(), e.clone()]),
        file(&[h.clone(), chunk(b"ab1D", &[]), d.clone(), e.clone()]),
        file(&[
            h.clone(),
            d.clone(),
            chunk(b"teXt", &[]),
            chunk(b"IDAT", &[]),
            e.clone(),
        ]),
    ];
    for (n, bytes) in bad.iter().enumerate() {
        assert!(decode(bytes).is_err(), "malformed {n}");
    }
    for color in 0..=6 {
        for depth in [0, 1, 2, 3, 4, 8, 16, 32] {
            let valid =
                deadlib_image_png::ColorType::try_from(color).is_ok_and(|c| c.allows_depth(depth));
            if !valid {
                assert!(decode(&file(&[ihdr(color, depth), d.clone(), e.clone()])).is_err());
            }
        }
    }
}

#[test]
fn configured_resource_limits() {
    let bytes = file(&[
        ihdr(0, 8),
        chunk(b"teXt", &[1, 2, 3]),
        image_data(&[0, 1]),
        chunk(b"IEND", &[]),
    ]);
    for limits in [
        Limits {
            max_input_bytes: 1,
            ..Default::default()
        },
        Limits {
            max_output_bytes: 1,
            ..Default::default()
        },
        Limits {
            max_metadata_bytes: 1,
            ..Default::default()
        },
        Limits {
            max_chunks: 1,
            ..Default::default()
        },
    ] {
        assert_eq!(
            decode_with_options(&bytes, &DecodeOptions { limits }),
            Err(Error::LimitExceeded)
        );
    }
    let mut enormous = vec![0; 13];
    enormous[..4].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
    enormous[4..8].copy_from_slice(&0x7fff_ffffu32.to_be_bytes());
    enormous[8] = 16;
    enormous[9] = 6;
    assert_eq!(
        decode(&file(&[chunk(b"IHDR", &enormous)])),
        Err(Error::LimitExceeded)
    );
}
