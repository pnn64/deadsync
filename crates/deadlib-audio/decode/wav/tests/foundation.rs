use deadlib_audio_decode_wav::{Container, Encoding, Endian, Error, Format, Limits, Wave};

fn fmt(tag: u16, channels: u16, bits: u16, e: Endian) -> Vec<u8> {
    let mut b = Vec::new();
    let align = channels * (bits / 8);
    for v in [tag, channels] {
        b.extend(if e == Endian::Little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        });
    }
    for v in [48_000u32, 48_000 * u32::from(align)] {
        b.extend(if e == Endian::Little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        });
    }
    for v in [align, bits] {
        b.extend(if e == Endian::Little {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        });
    }
    b
}
fn form(e: Endian, chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut b = if e == Endian::Little {
        b"RIFF\0\0\0\0WAVE".to_vec()
    } else {
        b"RIFX\0\0\0\0WAVE".to_vec()
    };
    for (id, payload) in chunks {
        b.extend(*id);
        let n = payload.len() as u32;
        b.extend(if e == Endian::Little {
            n.to_le_bytes()
        } else {
            n.to_be_bytes()
        });
        b.extend(*payload);
        if payload.len() & 1 != 0 {
            b.push(0xcd);
        }
    }
    let n = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&if e == Endian::Little {
        n.to_le_bytes()
    } else {
        n.to_be_bytes()
    });
    b
}
fn ext(channels: u16, bits: u16, valid: u16, mask: u32, float: bool) -> Vec<u8> {
    let mut f = fmt(0xfffe, channels, bits, Endian::Little);
    f.extend(22u16.to_le_bytes());
    f.extend(valid.to_le_bytes());
    f.extend(mask.to_le_bytes());
    f.extend(if float { 3u32 } else { 1u32 }.to_le_bytes());
    f.extend([0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
    f
}

#[test]
fn integer_edges_in_both_byte_orders() {
    for e in [Endian::Little, Endian::Big] {
        for bits in [8, 16, 24, 32] {
            let values: &[i32] = match bits {
                8 => &[-128, -1, 0, 1, 127, 42],
                16 => &[-32768, -1, 0, 1, 32767, 42],
                24 => &[-8388608, -1, 0, 1, 8388607, 42],
                _ => &[i32::MIN, -1, 0, 1, i32::MAX, 42],
            };
            let mut data = Vec::new();
            for &n in values {
                if bits == 8 {
                    data.push((n + 128) as u8);
                } else if e == Endian::Little {
                    data.extend(&n.to_le_bytes()[..bits as usize / 8]);
                } else {
                    data.extend(&n.to_be_bytes()[4 - bits as usize / 8..]);
                }
            }
            let b = form(e, &[(b"fmt ", &fmt(1, 2, bits, e)), (b"data", &data)]);
            let w = Wave::parse(&b).unwrap();
            let pcm = w.pcm().unwrap();
            assert_eq!(pcm.frames(), 3);
            let mut out = [1234; 9];
            assert_eq!(pcm.read_i32(0, &mut out).unwrap(), 6);
            assert_eq!(&out[..6], values);
            assert_eq!(&out[6..], &[1234; 3]);
            let mut short = [1234; 3];
            assert_eq!(pcm.read_i32(1, &mut short).unwrap(), 2);
            assert_eq!(&short[..2], &values[2..4]);
            assert_eq!(short[2], 1234);
            assert_eq!(
                pcm.frame_range(1, 2).unwrap().as_bytes().unwrap(),
                &data[2 * bits as usize / 8..]
            );
            assert!(pcm.frame_range(u64::MAX, 2).is_err());
            assert!(pcm.read_i32(4, &mut out).is_err());
            assert_eq!(pcm.read_i32(3, &mut out).unwrap(), 0);
        }
    }
}

#[test]
fn float_bits_are_never_normalized() {
    for e in [Endian::Little, Endian::Big] {
        let v32 = [
            0u32, 0x80000000, 1, 0x7f800000, 0xff800000, 0x7fc01234, 0x7f801234,
        ];
        let data: Vec<u8> = v32
            .iter()
            .flat_map(|n| {
                if e == Endian::Little {
                    n.to_le_bytes()
                } else {
                    n.to_be_bytes()
                }
            })
            .collect();
        let b = form(e, &[(b"fmt ", &fmt(3, 1, 32, e)), (b"data", &data)]);
        let w = Wave::parse(&b).unwrap();
        let p = w.pcm().unwrap();
        let mut out = [0.0; 7];
        assert_eq!(p.read_f32(0, &mut out).unwrap(), 7);
        assert_eq!(out.map(f32::to_bits), v32);
        assert!(p.read_i32(0, &mut [0; 7]).is_err());
        let v64 = [
            0u64,
            0x8000000000000000,
            1,
            0x7ff0000000000000,
            0x7ff8000012345678,
            0x7ff0000000000001,
        ];
        let data: Vec<u8> = v64
            .iter()
            .flat_map(|n| {
                if e == Endian::Little {
                    n.to_le_bytes()
                } else {
                    n.to_be_bytes()
                }
            })
            .collect();
        let b = form(e, &[(b"fmt ", &fmt(3, 1, 64, e)), (b"data", &data)]);
        let w = Wave::parse(&b).unwrap();
        let mut out = [0.0; 6];
        assert_eq!(w.pcm().unwrap().read_f64(0, &mut out).unwrap(), 6);
        assert_eq!(out.map(f64::to_bits), v64);
    }
}

#[test]
fn extensible_valid_bits_masks_and_many_channels() {
    for channels in [1, 2, 6, 32, 33, 1000] {
        let mask = if channels == 6 { 0x3f } else { 0 };
        let b = form(
            Endian::Little,
            &[
                (b"fmt ", &ext(channels, 24, 20, mask, false)),
                (b"data", &vec![0; channels as usize * 3]),
            ],
        );
        let w = Wave::parse(&b).unwrap();
        assert_eq!(w.format.valid_bits, 20);
        assert_eq!(w.format.channel_mask, mask);
        assert_eq!(w.format.encoding, Encoding::Pcm);
        assert_eq!(w.pcm().unwrap().frames(), 1);
    }
    for (valid, mask) in [(0, 3), (25, 3), (20, 1)] {
        assert!(Format::parse(&ext(2, 24, valid, mask, false), Endian::Little).is_err());
    }
    assert!(Format::parse(&ext(2, 32, 24, 3, true), Endian::Little).is_err());
    let mut vendor = ext(2, 16, 16, 3, false);
    vendor[39] ^= 1;
    assert!(matches!(
        Format::parse(&vendor, Endian::Little).unwrap().encoding,
        Encoding::Guid(_)
    ));
}

#[test]
fn unusual_order_unknown_payloads_exact_padding_and_riff_boundary() {
    let mut b = form(
        Endian::Little,
        &[
            (b"JUNK", &[7]),
            (b"data", &[1, 2]),
            (b"LIST", b"INFOINAM\x01\0\0\0A\0"),
            (b"fmt ", &fmt(1, 1, 16, Endian::Little)),
            (b"zzzz", &[5, 4, 3]),
        ],
    );
    let end = b.len();
    b.extend(b"ignored");
    let w = Wave::parse(&b).unwrap();
    let chunks: Vec<_> = w.chunks().unwrap().map(Result::unwrap).collect();
    assert_eq!(chunks.len(), 5);
    assert_eq!(chunks[0].padding, &[0xcd]);
    assert_eq!(chunks[4].data, &[5, 4, 3]);
    assert_eq!(w.as_bytes(), &b[..end]);
    assert_eq!(
        w.data().unwrap().as_ptr(),
        b[w.data_offset().unwrap() as usize..].as_ptr()
    );
    for n in 0..end {
        assert!(Wave::parse(&b[..n]).is_err(), "prefix {n}");
    }
}

fn big_form(kind: &[u8; 4]) -> Vec<u8> {
    let f = fmt(1, 1, 8, Endian::Little);
    // Two same-ID sentinel chunks exercise ds64 entries consumed in order.
    let mut ds = Vec::new();
    ds.extend(0u64.to_le_bytes());
    ds.extend(3u64.to_le_bytes());
    ds.extend(3u64.to_le_bytes());
    ds.extend(2u32.to_le_bytes());
    for size in [1u64, 3] {
        ds.extend(b"big1");
        ds.extend(size.to_le_bytes());
    }
    let mut b = form(
        Endian::Little,
        &[
            (b"ds64", &ds),
            (b"big1", &[9]),
            (b"fmt ", &f),
            (b"big1", &[1, 2, 3]),
            (b"data", &[0, 128, 255]),
        ],
    );
    let size = (b.len() - 8) as u64;
    b[..4].copy_from_slice(kind);
    b[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    b[20..28].copy_from_slice(&size.to_le_bytes());
    // Walk normal sizes first, then replace only selected size fields.
    let mut pos = 12;
    while pos < b.len() {
        let n = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
        if &b[pos..pos + 4] == b"big1" || &b[pos..pos + 4] == b"data" {
            b[pos + 4..pos + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        }
        pos += 8 + n + (n & 1);
    }
    b
}

#[test]
fn rf64_bw64_tables_and_64bit_checks() {
    for kind in [b"RF64", b"BW64"] {
        let b = big_form(kind);
        let w = Wave::parse(&b).unwrap();
        assert_eq!(
            w.container,
            if kind == b"RF64" {
                Container::Rf64
            } else {
                Container::Bw64
            }
        );
        assert_eq!(w.ds64.unwrap().sample_count, 3);
        assert_eq!(w.pcm().unwrap().frames(), 3);
        assert_eq!(w.chunks().unwrap().count(), 5);
        for n in 0..b.len() {
            assert!(Wave::parse(&b[..n]).is_err());
        }
        let mut bad = b.clone();
        bad[20..28].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(Wave::parse(&bad).is_err());
        let mut bad = b.clone();
        bad[48..52].copy_from_slice(b"nope");
        assert!(Wave::parse(&bad).is_err());
        assert!(matches!(
            Wave::with_limits(
                &b,
                Limits {
                    size_table_entries: 1,
                    ..Limits::default()
                }
            ),
            Err(Error::Limit(_))
        ));
    }
}

#[test]
fn malformed_structural_fields_fail_without_panicking() {
    let f = fmt(1, 1, 16, Endian::Little);
    for chunks in [
        vec![(b"fmt ", f.as_slice())],
        vec![(b"data", &[0, 0][..])],
        vec![
            (b"fmt ", f.as_slice()),
            (b"fmt ", f.as_slice()),
            (b"data", &[0, 0][..]),
        ],
        vec![(b"fmt ", f.as_slice()), (b"data", &[0][..])],
    ] {
        assert!(Wave::parse(&form(Endian::Little, &chunks)).is_err());
    }
    for (offset, value) in [(2, 0u8), (4, 0), (8, 1), (12, 3), (14, 15)] {
        let mut bad = f.clone();
        if offset == 2 {
            bad[2..4].fill(0);
        } else if offset == 4 {
            bad[4..8].fill(0);
        } else {
            bad[offset] = value;
        }
        assert!(Format::parse(&bad, Endian::Little).is_err());
    }
    let b = form(Endian::Little, &[(b"fmt ", &f), (b"data", &[0, 0])]);
    assert!(matches!(
        Wave::with_limits(
            &b,
            Limits {
                chunks: 1,
                ..Limits::default()
            }
        ),
        Err(Error::Limit(_))
    ));
    // Mutate each header byte through extrema, including all size fields.
    for i in 0..b.len() {
        for v in [0, 0x7f, 0xff] {
            let mut bad = b.clone();
            bad[i] = v;
            let _ = Wave::parse(&bad);
        }
    }
}

#[test]
fn aligned_direct_access_and_unaligned_fallback() {
    let f = fmt(1, 1, 32, Endian::Little);
    let data = [i32::MIN, -1, 0, i32::MAX].map(i32::to_le_bytes).concat();
    let b = form(Endian::Little, &[(b"fmt ", &f), (b"data", &data)]);
    for pad in 0..4 {
        let mut storage = vec![0; pad];
        storage.extend(&b);
        let w = Wave::parse(&storage[pad..]).unwrap();
        let p = w.pcm().unwrap();
        if let Some(s) = p.as_i32() {
            assert_eq!(s, [i32::MIN, -1, 0, i32::MAX]);
            assert_eq!(s.as_ptr().cast::<u8>(), p.as_bytes().unwrap().as_ptr());
        }
        assert_eq!(
            p.as_i32().is_some(),
            cfg!(target_endian = "little") && p.as_bytes().unwrap().as_ptr().align_offset(4) == 0
        );
        let mut out = [0; 4];
        p.read_i32(0, &mut out).unwrap();
        assert_eq!(out, [i32::MIN, -1, 0, i32::MAX]);
        assert!(p.as_f32().is_none());
    }
}
