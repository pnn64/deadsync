use deadlib_audio_decode_wav::{Adpcm, AdpcmData, Endian, Error, Format, Wave};

fn format(ima: bool, channels: u16, block: u16, frames: u16) -> Vec<u8> {
    let mut b = Vec::new();
    for v in [if ima { 0x11u16 } else { 2 }, channels] {
        b.extend(v.to_le_bytes());
    }
    b.extend(8000u32.to_le_bytes());
    b.extend((8000 * u32::from(block) / u32::from(frames)).to_le_bytes());
    b.extend(block.to_le_bytes());
    b.extend(4u16.to_le_bytes());
    b.extend(if ima { 2u16 } else { 32 }.to_le_bytes());
    b.extend(frames.to_le_bytes());
    if !ima {
        b.extend(7u16.to_le_bytes());
        for (a, d) in [
            (256i16, 0i16),
            (512, -256),
            (0, 0),
            (192, 64),
            (240, 0),
            (460, -208),
            (392, -232),
        ] {
            b.extend(a.to_le_bytes());
            b.extend(d.to_le_bytes());
        }
    }
    b
}
fn wav(fmt: &[u8], data: &[u8], frames: u32) -> Vec<u8> {
    let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
    for (id, payload) in [
        (b"fmt ", fmt),
        (b"fact", frames.to_le_bytes().as_slice()),
        (b"data", data),
    ] {
        b.extend(id);
        b.extend((payload.len() as u32).to_le_bytes());
        b.extend(payload);
        if payload.len() & 1 != 0 {
            b.push(0);
        }
    }
    let len = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&len.to_le_bytes());
    b
}

#[test]
fn independent_ima_vector_rounding_and_nibble_order() {
    let f = format(true, 1, 8, 9);
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    let mut out = [0; 9];
    codec
        .decode_block(&[0, 0, 0, 0, 0x10, 0x32, 0x54, 0x76], &mut out)
        .unwrap();
    assert_eq!(out, [0, 0, 1, 4, 8, 15, 27, 47, 88]);
    // Repeat all codes/steps and malformed initial indices. Output never leaves i16.
    for step in 0..=u8::MAX {
        for code in 0..16u8 {
            let result = codec.decode_block(
                &[0, 0, step, 255, code | code << 4, 0x88, 0x77, 0xff],
                &mut out,
            );
            assert_eq!(result.is_ok(), step <= 88);
        }
    }
}

#[test]
fn independent_microsoft_vector_initial_sample_order_and_padding() {
    let f = format(false, 1, 9, 6);
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    let mut out = [0; 6];
    codec
        .decode_block(&[0, 16, 0, 100, 0, 90, 0, 0x12, 0x34], &mut out)
        .unwrap();
    assert_eq!(out, [90, 100, 116, 148, 196, 260]);
    let f = format(false, 1, 9, 5);
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    assert_eq!(
        codec
            .decode_block(&[0, 16, 0, 100, 0, 90, 0, 0x12, 0x3f], &mut out)
            .unwrap(),
        5
    );
    assert_eq!(&out[..5], &[90, 100, 116, 148, 196]);
}

#[test]
fn custom_microsoft_coefficients_and_negative_fraction_rounding() {
    let mut f = format(false, 1, 8, 4);
    f[16..18].copy_from_slice(&36u16.to_le_bytes());
    f[20..22].copy_from_slice(&8u16.to_le_bytes());
    f.extend(1i16.to_le_bytes());
    f.extend(0i16.to_le_bytes());
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    let mut out = [0; 4];
    codec
        .decode_block(&[7, 16, 0, 0xff, 0xff, 0, 0, 0], &mut out)
        .unwrap();
    assert_eq!(out, [0, -1, 0, 0]); // -1/256 truncates toward zero, rather than -1.
    assert!(
        codec
            .decode_block(&[8, 16, 0, 0xff, 0xff, 0, 0, 0], &mut out)
            .is_err()
    );
    assert!(
        codec
            .decode_block(&[0, 0, 0x80, 0, 0, 0, 0, 0], &mut out)
            .is_err()
    );
}

// An intentionally wide scalar reference: no narrow path or threshold switch.
// Adaptation is bounded to the existing public decoder's checked i64 domain;
// all sample calculations themselves use i128.
fn microsoft_wide_oracle(
    parameters: &[[i16; 5]],
    codes: &[u8],
    frames: usize,
    out: &mut [i16],
) -> Result<(), &'static str> {
    const ADAPT: [i128; 16] = [
        230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
    ];
    let channels = parameters.len();
    for (c, &[delta, one, two, a, d]) in parameters.iter().enumerate() {
        let (mut delta, mut one, mut two, a, d) = (
            i128::from(delta),
            i128::from(one),
            i128::from(two),
            i128::from(a),
            i128::from(d),
        );
        out[c] = two as i16;
        out[channels + c] = one as i16;
        for frame in 2..frames {
            let code = codes[(frame - 2) * channels + c];
            let signed = if code < 8 {
                i128::from(code)
            } else {
                i128::from(code) - 16
            };
            let prediction = (one * a + two * d) / 256;
            let correction = delta * signed;
            assert!(i64::try_from(correction).is_ok());
            assert!(i64::try_from(prediction + correction).is_ok());
            let value = (prediction + correction).clamp(-32768, 32767);
            let adaptation = delta * ADAPT[usize::from(code)];
            if adaptation > i128::from(i64::MAX) {
                return Err("Microsoft adaptation overflow");
            }
            delta = (adaptation / 256).max(16);
            two = one;
            one = value;
            out[frame * channels + c] = value as i16;
        }
    }
    Ok(())
}

#[test]
fn microsoft_arithmetic_matches_i128_at_extremes_growth_and_overflow() {
    let mut successes = 0;
    let mut overflows = 0;
    for custom in [
        (1i16, 0i16),
        (192, 64),
        (512, -256),
        (-7, 3),
        (511, -257),
        (i16::MIN, i16::MIN),
        (i16::MIN, i16::MIN + 1),
        (i16::MAX, i16::MAX),
        (i16::MAX, i16::MIN),
    ] {
        for channels in [1usize, 2] {
            let frames = 500;
            for endian in [Endian::Little, Endian::Big] {
                let mut f = format(
                    false,
                    channels as u16,
                    (channels * 256) as u16,
                    frames as u16,
                );
                f[16..18].copy_from_slice(&36u16.to_le_bytes());
                f[20..22].copy_from_slice(&8u16.to_le_bytes());
                f.extend(custom.0.to_le_bytes());
                f.extend(custom.1.to_le_bytes());
                if endian == Endian::Big {
                    for range in [0..4, 12..f.len()] {
                        for word in f[range].as_chunks_mut::<2>().0 {
                            word.reverse();
                        }
                    }
                    f[4..8].reverse();
                    f[8..12].reverse();
                }
                let codec = Adpcm::from_format(Format::parse(&f, endian).unwrap()).unwrap();
                let word = |v: i16| {
                    if endian == Endian::Little {
                        v.to_le_bytes()
                    } else {
                        v.to_be_bytes()
                    }
                };
                for delta in [0i16, 1, 16, i16::MAX] {
                    for (one, two) in [
                        (i16::MIN, i16::MIN),
                        (i16::MAX, i16::MAX),
                        (i16::MIN, i16::MAX),
                        (-1, 0),
                    ] {
                        for pattern in 0..4 {
                            let mut parameters = vec![[delta, one, two, custom.0, custom.1]];
                            if channels == 2 {
                                parameters.push([delta, two, one, 192, 64]);
                            }
                            let mut state = 0x4d53414450434d_u64;
                            let codes: Vec<_> = (0..(frames - 2) * channels)
                                .map(|i| {
                                    state ^= state << 13;
                                    state ^= state >> 7;
                                    state ^= state << 17;
                                    match pattern {
                                        0 => [0, 1, 15, 3][i % 4],
                                        1 => 8,
                                        2 => {
                                            if i / channels < 8 {
                                                8
                                            } else {
                                                0
                                            }
                                        }
                                        _ => state as u8 & 15,
                                    }
                                })
                                .collect();
                            let mut data = vec![7];
                            if channels == 2 {
                                data.push(3);
                            }
                            for field in 0..3 {
                                for values in &parameters {
                                    data.extend(word(values[field]));
                                }
                            }
                            for &[a, b] in codes.as_chunks::<2>().0 {
                                data.push(a << 4 | b);
                            }
                            assert_eq!(data.len(), channels * 256);
                            let mut expected = vec![12345; frames * channels + 3];
                            let result =
                                microsoft_wide_oracle(&parameters, &codes, frames, &mut expected);
                            let mut actual = vec![12345; expected.len()];
                            match (codec.decode_block(&data, &mut actual), result) {
                                (Ok(n), Ok(())) => {
                                    assert_eq!(n, frames);
                                    successes += 1;
                                }
                                (Err(Error::Invalid(a)), Err(b)) => {
                                    assert_eq!(a, b);
                                    overflows += 1;
                                }
                                (a, b) => panic!("different result: {a:?} versus {b:?}"),
                            }
                            assert_eq!(
                                actual, expected,
                                "{custom:?} {endian:?} channels={channels} delta={delta} histories={one},{two} pattern={pattern}"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(successes > 0 && overflows > 0);
}

#[test]
fn ima_multichannel_word_groups_and_arbitrary_seek_ranges() {
    for channels in [1usize, 2, 6, 32] {
        let f = format(true, channels as u16, (channels * 12) as u16, 17);
        let mut block = Vec::new();
        for c in 0..channels {
            block.extend((c as i16 * 100).to_le_bytes());
            block.extend([0, 0]);
        }
        for _ in 0..2 {
            for c in 0..channels {
                block.extend(if c & 1 == 0 { [0x11; 4] } else { [0x99; 4] });
            }
        }
        let data = block.repeat(3);
        let bytes = wav(&f, &data, 49);
        let w = Wave::parse(&bytes).unwrap();
        let view = AdpcmData::new(&w).unwrap();
        assert_eq!(view.frames(), 49);
        let mut scratch = vec![0; view.codec().scratch_samples()];
        let mut all = vec![0; channels * 49];
        assert_eq!(
            view.read_frames(0, &mut all, &mut scratch).unwrap(),
            channels * 49
        );
        for c in 0..channels {
            assert_eq!(all[c], c as i16 * 100);
            assert_eq!(
                all[channels + c],
                c as i16 * 100 + if c & 1 == 0 { 1 } else { -1 }
            );
        }
        for start in [0, 1, 8, 16, 17, 18, 33, 34, 48, 49] {
            let mut out = vec![12345; channels * 20 + 1];
            let n = view.read_frames(start, &mut out, &mut scratch).unwrap();
            assert_eq!(
                &out[..n],
                &all[start as usize * channels..start as usize * channels + n]
            );
            assert!(out[n..].iter().all(|&n| n == 12345));
        }
        assert!(view.read_frames(50, &mut all, &mut scratch).is_err());
        assert!(view.read_frames(0, &mut all, &mut []).is_err());
    }
}

#[test]
fn microsoft_specified_division_is_not_an_arithmetic_shift() {
    let f = format(false, 1, 8, 4);
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    let mut out = [0; 4];
    codec
        .decode_block(&[3, 16, 0, 0xff, 0xff, 0, 0, 0], &mut out)
        .unwrap();
    assert_eq!(out, [0, -1, 0, 0]); // (-1*192 + 0*64)/256 == 0.
}

#[test]
fn microsoft_stereo_interleaving_short_blocks_and_fact_bounds() {
    let f = format(false, 2, 16, 4);
    let block = [
        0, 0, 16, 0, 16, 0, 100, 0, 0x9c, 0xff, 90, 0, 0xa6, 0xff, 0x19, 0x2a,
    ];
    let bytes = wav(&f, &block, 4);
    let w = Wave::parse(&bytes).unwrap();
    let view = AdpcmData::new(&w).unwrap();
    let mut out = [0; 8];
    view.read_frames(0, &mut out, &mut [0; 8]).unwrap();
    assert_eq!(out, [90, -90, 100, -100, 116, -212, 148, -440]);
    for frames in [0, 1, 3, 4] {
        let bytes = wav(&f, &block, frames);
        let w = Wave::parse(&bytes).unwrap();
        assert_eq!(AdpcmData::new(&w).unwrap().frames(), u64::from(frames));
    }
    let bytes = wav(&f, &block, 5);
    assert!(AdpcmData::new(&Wave::parse(&bytes).unwrap()).is_err());
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    assert_eq!(codec.decode_block(&block[..15], &mut out).unwrap(), 3);
    for n in 0..14 {
        assert!(codec.decode_block(&block[..n], &mut out).is_err());
    }
}
