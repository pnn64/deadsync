use deadlib_audio_decode_mp3::{
    bands::Bands, bits::BitReader, scalefactors::ScaleFactors, spectral, *,
};

#[test]
fn zero_regions_consume_no_bits_between_coded_pairs() {
    for rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000] {
        let bands = Bands::for_rate(rate).unwrap();
        let first = bands.long()[8] as usize;
        let second = bands.long()[16] as usize;
        for books in [[0, 1, 0], [1, 0, 1], [0, 0, 0]] {
            for pairs in [0, 1, first / 2, first / 2 + 1, second / 2, 288] {
                let mut writer = bits::BitWriter::new();
                let mut expected = [0; 576];
                for line in (0..2 * pairs).step_by(2) {
                    let book = if line < first {
                        books[0]
                    } else if line < second {
                        books[1]
                    } else {
                        books[2]
                    };
                    if book == 1 {
                        // Normative book-1 pair (1,0), positive sign: 01|0.
                        writer.write(0b010, 3).unwrap();
                        expected[line] = 1;
                    }
                }
                let mut cursor =
                    BitReader::bounded(writer.as_bytes(), 0, writer.len_bits()).unwrap();
                let info = GranuleChannel {
                    big_values: pairs as u16,
                    region0_count: 7,
                    region1_count: 7,
                    table_select: books,
                    ..Default::default()
                };
                let mut actual = [9; 576];
                assert_eq!(
                    spectral::quantized(&mut cursor, &info, bands, &mut actual),
                    Ok(2 * pairs)
                );
                assert_eq!(
                    actual, expected,
                    "rate={rate} books={books:?} pairs={pairs}"
                );
                assert_eq!(cursor.position(), writer.len_bits());
            }
        }
    }
}

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
        48000 => (3, 1),
        32000 => (3, 2),
        22050 => (2, 0),
        24000 => (2, 1),
        16000 => (2, 2),
        11025 => (0, 0),
        12000 => (0, 1),
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

#[test]
fn combined_stereo_matches_known_independent_window_and_mixed_boundaries() {
    for rate in [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000] {
        let bands = Bands::for_rate(rate).unwrap();
        for (kind, mixed, short_data) in [
            (0, false, true),
            (1, false, true),
            (3, false, true),
            (2, false, true),
            (2, true, true),
            (2, true, false),
        ] {
            let mut source = [[0.0; 576], [-0.0; 576]];
            source[0] = std::array::from_fn(|n| {
                [0.0, -0.0, 1.0, -2.0, f64::from_bits(1), f64::MIN_POSITIVE][n % 6]
            });
            let mut intensity_lines = [false; 576];
            if kind != 2 {
                source[1][bands.long()[4] as usize] = 1.0;
                intensity_lines[bands.long()[5] as usize..].fill(true);
            } else if short_data {
                for (window, last_band) in [4, 7, 10].into_iter().enumerate() {
                    source[1][3 * bands.short()[last_band] as usize + window] = 1.0;
                    for frequency in bands.short()[last_band + 1] as usize..192 {
                        intensity_lines[3 * frequency + window] = true;
                    }
                }
            } else {
                source[1][bands.long()[2] as usize] = 1.0;
                intensity_lines[bands.long()[3] as usize..bands.mixed_lines()].fill(true);
                intensity_lines[bands.mixed_lines()..].fill(true);
            }
            for mode in [1, 3] {
                let header = joint_header(rate, mode);
                let info = [GranuleChannel {
                    block_type: kind,
                    mixed_block: mixed,
                    ..Default::default()
                }; 2];
                for pan in [0, 7] {
                    let factors = std::array::from_fn(|_| ScaleFactors {
                        long: [pan; 22],
                        short: [[pan; 3]; 13],
                        long_width: [3; 22],
                        short_width: [[3; 3]; 13],
                        ..Default::default()
                    });
                    let mut output = source;
                    spectral::stereo(header, &info, &factors, bands, &mut output).unwrap();
                    for line in 0..576 {
                        let (left, right) = (source[0][line], source[1][line]);
                        let expected = if pan == 0 && intensity_lines[line] {
                            (
                                left * if header.version() == MpegVersion::Mpeg1 {
                                    0.0
                                } else {
                                    1.0
                                },
                                left * 1.0,
                            )
                        } else if mode == 3 {
                            (
                                (left + right) * std::f64::consts::FRAC_1_SQRT_2,
                                (left - right) * std::f64::consts::FRAC_1_SQRT_2,
                            )
                        } else {
                            (left, right)
                        };
                        assert_eq!(
                            (output[0][line].to_bits(), output[1][line].to_bits()),
                            (expected.0.to_bits(), expected.1.to_bits()),
                            "rate={rate} kind={kind} mixed={mixed} short_data={short_data} mode={mode} pan={pan} line={line}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cached_mpeg1_intensity_gains_match_runtime_tangent_bits() {
    let header = joint_header(44100, 1);
    let bands = Bands::for_rate(44100).unwrap();
    let info = [GranuleChannel::default(); 2];
    let values = [
        0.0,
        -0.0,
        1.0,
        -2.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::INFINITY,
    ];
    let source = std::array::from_fn(|n| values[n % values.len()]);
    for pan in 1..=5 {
        let ratio = (std::f64::consts::PI * std::hint::black_box(pan as f64) / 12.0).tan();
        let expected = [ratio / (1.0 + ratio), 1.0 / (1.0 + ratio)];
        let factors = std::array::from_fn(|_| ScaleFactors {
            long: [pan; 22],
            long_width: [3; 22],
            ..Default::default()
        });
        for _ in 0..3 {
            let mut output = [source, [0.0; 576]];
            spectral::stereo(header, &info, &factors, bands, &mut output).unwrap();
            for channel in 0..2 {
                for (line, &sample) in source.iter().enumerate() {
                    assert_eq!(
                        output[channel][line].to_bits(),
                        (sample * expected[channel]).to_bits(),
                        "pan={pan} channel={channel} line={line}"
                    );
                }
            }
        }
    }
}
