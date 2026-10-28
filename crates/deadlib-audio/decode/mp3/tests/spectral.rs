use deadlib_audio_decode_mp3::{
    bands::Bands, bits::BitReader, scalefactors::ScaleFactors, spectral, *,
};

#[test]
fn scalefactor_band_boundaries_and_mixed_alignment() {
    for rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000] {
        let b = Bands::for_rate(rate).unwrap();
        assert_eq!(
            (b.long()[0], b.long()[22], b.short()[0], b.short()[13]),
            (0, 576, 0, 192)
        );
        assert!(b.long().windows(2).all(|p| p[0] < p[1]));
        assert!(b.short().windows(2).all(|p| p[0] < p[1]));
        assert_eq!(b.long()[b.mixed_long_bands()] as usize, b.mixed_lines());
        assert_eq!(b.short()[3] as usize * 3, b.mixed_lines());
    }
    assert_eq!(Bands::for_rate(24000).unwrap().long()[18], 332);
    assert_eq!(Bands::for_rate(8000).unwrap().mixed_subbands(), 4);
    assert!(Bands::for_rate(12345).is_err());
}

#[test]
fn switched_huffman_regions_count_long_and_short_window_bands() {
    for rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000] {
        for mixed in [false, true] {
            let bands = Bands::for_rate(rate).unwrap();
            // Eight band entries: MPEG-1 mixed has eight long bands;
            // LSF mixed has six long bands plus short band 3, windows 0 and 1.
            let boundary = if !mixed {
                3 * bands.short()[3] as usize
            } else if rate >= 32000 {
                36
            } else if rate == 8000 {
                96
            } else {
                48
            };
            let mut encoded = Vec::new();
            for pair in 0..288 {
                // Normative (1,0), positive sign: book 1 = 01|0;
                // book 2 = 011|0. Distinct words expose a wrong boundary.
                encoded.extend_from_slice(if pair * 2 < boundary {
                    &[false, true, false]
                } else {
                    &[false, true, true, false]
                });
            }
            let mut bytes = vec![0; encoded.len().div_ceil(8)];
            for (n, &bit) in encoded.iter().enumerate() {
                bytes[n / 8] |= u8::from(bit) << (7 - n % 8);
            }
            let mut bits = BitReader::bounded(&bytes, 0, encoded.len()).unwrap();
            let info = GranuleChannel {
                block_type: 2,
                mixed_block: mixed,
                big_values: 288,
                table_select: [1, 2, 0],
                region0_count: if mixed { 7 } else { 8 },
                ..Default::default()
            };
            let mut output = [0; 576];
            spectral::quantized(&mut bits, &info, bands, &mut output).unwrap();
            assert_eq!(bits.position(), encoded.len(), "{rate} mixed={mixed}");
            for (line, &value) in output.iter().enumerate() {
                assert_eq!(
                    value,
                    i16::from(line % 2 == 0),
                    "{rate} mixed={mixed} line={line}"
                );
            }
        }
    }
}

#[test]
fn requantization_half_steps_preemphasis_and_short_gain() {
    let b = Bands::for_rate(44100).unwrap();
    let mut input = [0; 576];
    input[0] = 1;
    input[1] = -1;
    let mut info = GranuleChannel {
        global_gain: 210,
        ..Default::default()
    };
    let mut sf = ScaleFactors::default();
    let mut output = [0.0; 576];
    for scale in [false, true] {
        info.scalefac_scale = scale;
        for value in 0..4 {
            sf.long[0] = value;
            spectral::requantize(&input, &info, &sf, b, &mut output).unwrap();
            let expected = 2.0_f64.powf(-(value as f64) * if scale { 1.0 } else { 0.5 });
            assert!((output[0] - expected).abs() < 1e-14);
            assert!((output[1] + expected).abs() < 1e-14);
        }
    }
    input.fill(0);
    input[62] = 1;
    info.scalefac_scale = false;
    sf = ScaleFactors {
        preflag: true,
        ..Default::default()
    };
    spectral::requantize(&input, &info, &sf, b, &mut output).unwrap();
    assert!((output[62] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-14);
    info.block_type = 2;
    info.subblock_gain = [1, 2, 3];
    sf = ScaleFactors::default();
    sf.short[0] = [1, 2, 3];
    input.fill(0);
    input[0] = 1;
    input[4] = 1;
    input[8] = 1;
    spectral::requantize(&input, &info, &sf, b, &mut output).unwrap();
    for (window, &sample) in output[..3].iter().enumerate() {
        let expected = 2.0_f64.powf(-(window as f64 + 1.0) * 2.5);
        assert!((sample - expected).abs() < 1e-14);
    }
}

#[test]
fn every_i16_coefficient_matches_runtime_power_at_scale_extremes() {
    let bands = Bands::for_rate(44100).unwrap();
    for gain in [0, 210, 255] {
        for factor in [0, 1, 31, 255] {
            for scale in [false, true] {
                let info = GranuleChannel {
                    global_gain: gain,
                    scalefac_scale: scale,
                    ..Default::default()
                };
                let factors = ScaleFactors {
                    long: [factor; 22],
                    ..Default::default()
                };
                let amplitude = ((f64::from(gain) - 210.0) * 0.25).exp2()
                    * (-0.5 * f64::from((1 + u16::from(scale)) * u16::from(factor))).exp2();
                for start in (0..=u16::MAX as usize).step_by(576) {
                    let input = std::array::from_fn(|n| (start + n) as u16 as i16);
                    let mut output = [0.0; 576];
                    spectral::requantize(&input, &info, &factors, bands, &mut output).unwrap();
                    for (&value, actual) in input.iter().zip(output) {
                        let expected = if value == 0 {
                            0.0
                        } else {
                            std::hint::black_box(f64::from(value.unsigned_abs()))
                                .powf(4.0 / 3.0)
                                .copysign(f64::from(value))
                                * amplitude
                        };
                        assert_eq!(
                            actual.to_bits(),
                            expected.to_bits(),
                            "coefficient {value}, gain {gain}, factor {factor}, scale {scale}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn big_values_errors_and_partial_count1_stuffing() {
    let bands = Bands::for_rate(44100).unwrap();
    let mut output = [0; 576];
    let mut info = GranuleChannel {
        big_values: 1,
        table_select: [1, 0, 0],
        ..Default::default()
    };
    let mut empty = BitReader::new(&[]);
    assert_eq!(
        spectral::quantized(&mut empty, &info, bands, &mut output),
        Err(Error::Truncated)
    );
    assert_eq!(empty.position(), 0);
    info.big_values = 0;
    let data = [0u8];
    let mut partial = BitReader::bounded(&data, 0, 1).unwrap();
    assert_eq!(
        spectral::quantized(&mut partial, &info, bands, &mut output).unwrap(),
        0
    );
    assert_eq!(output, [0; 576]);
    assert_eq!(partial.position(), 0);
    info.table_select[0] = 4;
    info.big_values = 1;
    assert!(spectral::quantized(&mut empty, &info, bands, &mut output).is_err());
}

fn joint_header(rate: u32, mode: u8) -> FrameHeader {
    let (version, index) = match rate {
        44100 => (3, 0),
        22050 => (2, 0),
        8000 => (0, 2),
        _ => panic!("test rate"),
    };
    let word: u32 = (0x7ff << 21)
        | (version << 19)
        | (1 << 17)
        | (1 << 16)
        | (9 << 12)
        | (index << 10)
        | (1 << 6)
        | ((mode as u32) << 4);
    FrameHeader::parse(&word.to_be_bytes()).unwrap()
}

#[test]
fn mid_side_only_preserves_exact_equations_and_checks_shapes_first() {
    let pairs = [
        (0.0, -0.0),
        (-0.0, 0.0),
        (1.0, 1.0),
        (1.0, -1.0),
        (f64::MAX, f64::MAX),
        (f64::MIN_POSITIVE, f64::from_bits(1)),
        (f64::from_bits(1), -f64::from_bits(1)),
    ];
    for rate in [44100, 22050, 8000] {
        let bands = Bands::for_rate(rate).unwrap();
        let header = joint_header(rate, 2);
        let factors = [ScaleFactors::default(), ScaleFactors::default()];
        let source: [[f64; 576]; 2] = std::array::from_fn(|channel| {
            std::array::from_fn(|n| {
                if channel == 0 {
                    pairs[n % pairs.len()].0
                } else {
                    pairs[n % pairs.len()].1
                }
            })
        });
        for kind in 0..=255 {
            for mixed in [false, true] {
                let mut info = [GranuleChannel {
                    block_type: kind,
                    mixed_block: mixed,
                    ..Default::default()
                }; 2];
                let mut output = source;
                spectral::stereo(header, &info, &factors, bands, &mut output).unwrap();
                for n in 0..576 {
                    let (mid, side) = (source[0][n], source[1][n]);
                    assert_eq!(
                        output[0][n].to_bits(),
                        ((mid + side) * std::f64::consts::FRAC_1_SQRT_2).to_bits()
                    );
                    assert_eq!(
                        output[1][n].to_bits(),
                        ((mid - side) * std::f64::consts::FRAC_1_SQRT_2).to_bits()
                    );
                }
                info[1].mixed_block = !mixed;
                let mut output = source;
                assert_eq!(
                    spectral::stereo(header, &info, &factors, bands, &mut output),
                    Err(Error::Invalid("joint-stereo block mismatch"))
                );
                assert_eq!(output, source);
                info[1].mixed_block = mixed;
                info[1].block_type = kind.wrapping_add(1);
                assert_eq!(
                    spectral::stereo(header, &info, &factors, bands, &mut output),
                    Err(Error::Invalid("joint-stereo block mismatch"))
                );
                assert_eq!(output, source);
            }
        }
    }
}

#[test]
fn intensity_positions_and_zero_width_partitions() {
    for rate in [44100, 22050, 8000] {
        let header = joint_header(rate, 3);
        let bands = Bands::for_rate(rate).unwrap();
        let info = [GranuleChannel::default(); 2];
        for scale in [false, true] {
            for pan in 0..8 {
                let mut sf = [ScaleFactors::default(), ScaleFactors::default()];
                sf[1].long[0] = pan;
                sf[1].long_width[0] = 3;
                sf[1].intensity_scale = scale;
                let mut spectra = [[0.0; 576]; 2];
                spectra[0][0] = 1.0;
                spectra[0][575] = 1.0;
                spectral::stereo(header, &info, &sf, bands, &mut spectra).unwrap();
                let expected = if pan == 7 {
                    (
                        std::f64::consts::FRAC_1_SQRT_2,
                        std::f64::consts::FRAC_1_SQRT_2,
                    )
                } else if rate == 44100 {
                    let angle = pan as f64 * std::f64::consts::PI / 12.0;
                    let total = angle.sin() + angle.cos();
                    (angle.sin() / total, angle.cos() / total)
                } else {
                    let base = if scale {
                        0.5_f64.sqrt()
                    } else {
                        0.5_f64.sqrt().sqrt()
                    };
                    if pan % 2 == 1 {
                        (base.powi((pan as i32 + 1) / 2), 1.0)
                    } else {
                        (1.0, base.powi(pan as i32 / 2))
                    }
                };
                assert!((spectra[0][0] - expected.0).abs() < 1e-14);
                assert!((spectra[1][0] - expected.1).abs() < 1e-14);
                // Zero-width LSF partitions mean position zero, not an illegal pan.
                assert_eq!(spectra[0][575], if rate == 44100 { 0.0 } else { 1.0 });
                assert_eq!(spectra[1][575], 1.0);
            }
        }
    }
}

#[test]
fn top_band_inherits_pan_only_when_preceding_band_is_intensity_coded() {
    for rate in [44100, 22050] {
        for short in [false, true] {
            let bands = Bands::for_rate(rate).unwrap();
            let mut info = [GranuleChannel::default(); 2];
            if short {
                info[0].block_type = 2;
                info[1].block_type = 2;
            }
            let mut sf = [ScaleFactors::default(), ScaleFactors::default()];
            sf[1].long[20] = 6;
            sf[1].long_width[20] = 3;
            sf[1].short[11] = [6; 3];
            sf[1].short_width[11] = [3; 3];
            let top = if short {
                3 * bands.short()[12] as usize
            } else {
                bands.long()[21] as usize
            };
            for preceding_nonzero in [false, true] {
                let mut spectra = [[0.0; 576]; 2];
                spectra[0][top] = 1.0;
                if preceding_nonzero {
                    spectra[1][top - if short { 3 } else { 1 }] = 1.0;
                }
                spectral::stereo(joint_header(rate, 1), &info, &sf, bands, &mut spectra).unwrap();
                let expected = match (rate, preceding_nonzero) {
                    (44100, false) => (1.0, 0.0),
                    (44100, true) => (0.5, 0.5),
                    (_, false) => (1.0, 2.0_f64.powf(-0.75)),
                    (_, true) => (1.0, 1.0),
                };
                assert!((spectra[0][top] - expected.0).abs() < 1e-14);
                assert!((spectra[1][top] - expected.1).abs() < 1e-14);
            }
        }
    }
}

#[test]
fn short_intensity_bounds_are_independent_per_window() {
    let bands = Bands::for_rate(44100).unwrap();
    let info = [GranuleChannel {
        block_type: 2,
        ..Default::default()
    }; 2];
    let mut sf = [ScaleFactors::default(), ScaleFactors::default()];
    sf[1].short.fill([3; 3]);
    let mut spectra = [[0.0; 576]; 2];
    spectra[0].fill(1.0);
    spectra[1][3 * 5] = 2.0; // Window zero, band one.
    spectral::stereo(joint_header(44100, 1), &info, &sf, bands, &mut spectra).unwrap();
    assert_eq!(spectra[0][0], 1.0);
    assert!((spectra[0][1] - 0.5).abs() < 1e-14);
    assert_eq!(spectra[1][15], 2.0);
    assert!((spectra[0][24] - 0.5).abs() < 1e-14);
}
