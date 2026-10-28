use deadlib_audio_decode_wav::{Adpcm, AdpcmData, Endian, Format, Wave};

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
