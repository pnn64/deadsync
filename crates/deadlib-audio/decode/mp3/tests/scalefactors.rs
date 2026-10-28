use deadlib_audio_decode_mp3::{
    bits::{BitReader, BitWriter},
    scalefactors::*,
    *,
};

fn header(version: u32, intensity: bool) -> FrameHeader {
    FrameHeader::parse(
        &(0xffe00000u32
            | version << 19
            | 1 << 17
            | 1 << 16
            | 9 << 12
            | 1 << 6
            | u32::from(intensity) << 4)
            .to_be_bytes(),
    )
    .unwrap()
}
fn side(compress: u16, shape: usize) -> SideInfo {
    let mut side = SideInfo::default();
    for row in &mut side.granules {
        for g in row {
            g.scalefac_compress = compress;
            g.part2_3_bits = 4095;
            g.block_type = if shape == 0 { 0 } else { 2 };
            g.mixed_block = shape == 2;
        }
    }
    side
}
#[test]
fn every_lsf_compression_and_shape() {
    for compress in 0..512 {
        for intensity in [false, true] {
            for shape in 0..3 {
                let plan = lsf_parameters(compress, shape != 0, shape == 2, intensity).unwrap();
                assert_eq!(plan.counts.into_iter().sum::<u8>(), [21, 36, 33][shape]);
                assert_eq!(plan.preflag, !intensity && compress >= 500);
                assert_eq!(plan.intensity_scale, intensity && compress & 1 != 0);
                for varied in [false, true] {
                    let mut data = BitWriter::new();
                    let mut flat = Vec::new();
                    for (count, width) in plan.counts.into_iter().zip(plan.widths) {
                        for _ in 0..count {
                            let mask = (1u32 << width) - 1;
                            let value = if varied {
                                (flat.len() as u32 * 7 + u32::from(compress)) & mask
                            } else {
                                mask
                            };
                            data.write(value, width as u32).unwrap();
                            flat.push((value as u8, width));
                        }
                    }
                    for version in [2, 0] {
                        let h = header(version, intensity);
                        let s = side(compress, shape);
                        let mut bits = BitReader::new(data.as_bytes());
                        let factors = read(&mut bits, h, &s, 0, 1, None).unwrap();
                        assert_eq!(bits.position(), plan.bits());
                        assert_eq!(factors.encoded_bits, plan.bits());
                        let mut actual = Vec::new();
                        if shape == 0 {
                            actual
                                .extend(factors.long[..21].iter().copied().zip(factors.long_width));
                        } else {
                            if shape == 2 {
                                actual.extend(
                                    factors.long[..6].iter().copied().zip(factors.long_width),
                                );
                            }
                            for band in if shape == 2 { 3 } else { 0 }..12 {
                                for window in 0..3 {
                                    actual.push((
                                        factors.short[band][window],
                                        factors.short_width[band][window],
                                    ));
                                }
                            }
                        }
                        assert_eq!(actual, flat);
                        assert_eq!(factors.long[21], 0);
                        assert_eq!(factors.short[12], [0; 3]);
                    }
                }
            }
        }
    }
    assert_eq!(
        lsf_parameters(399, false, false, false).unwrap().widths,
        [4, 4, 3, 3]
    );
    assert_eq!(
        lsf_parameters(500, false, false, false).unwrap().counts,
        [11, 10, 0, 0]
    );
    assert_eq!(
        lsf_parameters(359, false, false, true).unwrap().widths,
        [4, 5, 5, 0]
    );
    assert_eq!(
        lsf_parameters(360, false, false, true).unwrap().counts,
        [6, 6, 6, 3]
    );
    assert!(lsf_parameters(512, false, false, false).is_err());
    assert!(lsf_parameters(0, false, true, false).is_err());
}

#[test]
fn mpeg1_widths_and_all_scfsi_masks() {
    let widths = [
        [0, 0],
        [0, 1],
        [0, 2],
        [0, 3],
        [3, 0],
        [1, 1],
        [1, 2],
        [1, 3],
        [2, 1],
        [2, 2],
        [2, 3],
        [3, 1],
        [3, 2],
        [3, 3],
        [4, 2],
        [4, 3],
    ];
    for (compress, [a, b]) in widths.into_iter().enumerate() {
        for shape in 0..3 {
            let bits = match shape {
                0 => 11 * a + 10 * b,
                1 => 18 * a + 18 * b,
                _ => 17 * a + 18 * b,
            };
            let data = [0xff; 32];
            let mut reader = BitReader::new(&data);
            let factors = read(
                &mut reader,
                header(3, false),
                &side(compress as u16, shape),
                0,
                0,
                None,
            )
            .unwrap();
            assert_eq!(factors.encoded_bits, bits);
        }
    }
    let mut prev = ScaleFactors::default();
    for (i, f) in prev.long.iter_mut().enumerate() {
        *f = (i % 8) as u8;
    }
    for mask in 0..16 {
        let mut side = side(15, 0);
        for i in 0..4 {
            side.scfsi[0][i] = mask & (1 << i) != 0;
        }
        let data = [0xff; 32];
        let mut bits = BitReader::new(&data);
        let result = read(&mut bits, header(3, false), &side, 1, 0, Some(&prev)).unwrap();
        let mut consumed = 0;
        for (i, (first, last, width)) in [(0, 6, 4), (6, 11, 4), (11, 16, 3), (16, 21, 3)]
            .into_iter()
            .enumerate()
        {
            for band in first..last {
                assert_eq!(
                    result.long[band],
                    if mask & (1 << i) != 0 {
                        prev.long[band]
                    } else {
                        (1 << width) - 1
                    }
                );
            }
            if mask & (1 << i) == 0 {
                consumed += (last - first) * width;
            }
        }
        assert_eq!(result.encoded_bits, consumed);
    }
}

#[test]
fn scalefactor_boundaries_are_atomic() {
    for compress in [0, 399, 400, 499, 500, 511] {
        for shape in 0..3 {
            for intensity in [false, true] {
                let plan = lsf_parameters(compress, shape != 0, shape == 2, intensity).unwrap();
                let data = [0xff; 32];
                let h = header(2, intensity);
                let mut s = side(compress, shape);
                for length in 0..plan.bits() {
                    let mut bits = BitReader::bounded(&data, 0, length).unwrap();
                    assert!(matches!(
                        read(&mut bits, h, &s, 0, 1, None),
                        Err(Error::Truncated)
                    ));
                    assert_eq!(bits.position(), 0);
                }
                if plan.bits() != 0 {
                    s.granules[0][1].part2_3_bits = (plan.bits() - 1) as u16;
                    let mut bits = BitReader::new(&data);
                    assert!(read(&mut bits, h, &s, 0, 1, None).is_err());
                    assert_eq!(bits.position(), 0);
                }
            }
        }
    }
    let mut s = side(15, 0);
    s.scfsi[0] = [true; 4];
    let data = [0u8; 32];
    let mut bits = BitReader::new(&data);
    assert!(read(&mut bits, header(3, false), &s, 1, 0, None).is_err());
    assert_eq!(bits.position(), 0);
    assert!(read(&mut bits, header(2, false), &s, 1, 0, None).is_err());
    assert!(read(&mut bits, header(2, false), &s, 0, 2, None).is_err());
}
