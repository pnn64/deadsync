use deadlib_audio_decode_wav::{
    Adpcm, AdpcmData, AdpcmKind, AdpcmSpec, AdpcmWriter, Container, Endian, Format, Wave,
    WriteOptions,
};
use std::io::Cursor;

fn descriptor(
    kind: AdpcmKind,
    channels: u16,
    block_bytes: u16,
    endian: Endian,
) -> (Vec<u8>, usize) {
    let spec = AdpcmSpec {
        kind,
        channels,
        block_bytes,
        sample_rate: 44100,
    };
    let mut header = [0; 50];
    let n = spec.format(endian, &mut header).unwrap();
    let f = Format::parse(&header[..n], endian).unwrap();
    let codec = Adpcm::from_format(f).unwrap();
    (header[..n].to_vec(), codec.frames_per_block())
}

#[test]
fn constant_vectors_and_header_only_blocks_are_exact() {
    for endian in [Endian::Little, Endian::Big] {
        for (kind, block) in [(AdpcmKind::Ima, 8), (AdpcmKind::Microsoft, 9)] {
            let (f, frames) = descriptor(kind, 1, block, endian);
            let codec = Adpcm::from_format(Format::parse(&f, endian).unwrap()).unwrap();
            let mut b = vec![0xaa; block as usize + 1];
            codec.encode_block(&vec![100; frames], &mut b).unwrap();
            assert_eq!(b[block as usize], 0xaa);
            let expected = if kind == AdpcmKind::Ima {
                if endian == Endian::Little {
                    vec![100, 0, 0, 0, 0, 0, 0, 0]
                } else {
                    vec![0, 100, 0, 0, 0, 0, 0, 0]
                }
            } else if endian == Endian::Little {
                vec![0, 16, 0, 100, 0, 100, 0, 0, 0]
            } else {
                vec![0, 0, 16, 0, 100, 0, 100, 0, 0]
            };
            assert_eq!(&b[..block as usize], expected);
            let mut out = vec![0; frames];
            codec.decode_block(&b[..block as usize], &mut out).unwrap();
            assert_eq!(out, vec![100; frames]);
        }
        for channels in [1u16, 2] {
            for kind in [AdpcmKind::Ima, AdpcmKind::Microsoft] {
                let block = channels * if kind == AdpcmKind::Ima { 4 } else { 7 };
                let (f, frames) = descriptor(kind, channels, block, endian);
                let codec = Adpcm::from_format(Format::parse(&f, endian).unwrap()).unwrap();
                let values: Vec<_> = (0..frames * channels as usize)
                    .map(|i| if i & 1 == 0 { i16::MIN } else { i16::MAX })
                    .collect();
                let mut b = vec![0; block as usize];
                codec.encode_block(&values, &mut b).unwrap();
                let mut out = vec![0; values.len()];
                codec.decode_block(&b, &mut out).unwrap();
                assert_eq!(values, out);
            }
        }
    }
}

#[test]
fn streaming_block_splits_are_deterministic_and_fact_trims_last_block() {
    for kind in [AdpcmKind::Ima, AdpcmKind::Microsoft] {
        for container in [
            Container::Riff,
            Container::Rifx,
            Container::Rf64,
            Container::Bw64,
        ] {
            for channels in [1u16, 2] {
                let block = if kind == AdpcmKind::Ima {
                    32 * channels
                } else {
                    23 * channels
                };
                let spec = AdpcmSpec {
                    kind,
                    channels,
                    block_bytes: block,
                    sample_rate: 44100,
                };
                let (f, frames) = descriptor(kind, channels, block, container.endian());
                let codec =
                    Adpcm::from_format(Format::parse(&f, container.endian()).unwrap()).unwrap();
                for length in [0, 1, 2, frames - 1, frames, frames + 1, frames * 3 + 7] {
                    let input: Vec<_> = (0..length * channels as usize)
                        .map(|i| ((i * 7177 + 9123) & 65535) as i16)
                        .collect();
                    let options = WriteOptions {
                        container,
                        ..WriteOptions::default()
                    };
                    let mut w =
                        AdpcmWriter::new(Cursor::new(Vec::new()), spec, options, &[]).unwrap();
                    for batch in input.chunks(channels as usize * 7) {
                        w.write_i16(batch).unwrap();
                    }
                    assert_eq!(w.frames_written(), length as u64);
                    let b = w.finish().unwrap().into_inner();
                    let wave = Wave::parse(&b).unwrap();
                    let view = AdpcmData::new(&wave).unwrap();
                    assert_eq!(view.frames(), length as u64);
                    assert_eq!(wave.data().len(), length.div_ceil(frames) * block as usize);
                    let mut expected = Vec::new();
                    let mut encoded = vec![0; block as usize];
                    for batch in input.chunks(frames * channels as usize) {
                        codec.encode_block(batch, &mut encoded).unwrap();
                        expected.extend(&encoded);
                    }
                    assert_eq!(wave.data(), expected);
                    let mut w =
                        AdpcmWriter::new(Cursor::new(Vec::new()), spec, options, &[]).unwrap();
                    w.write_i16(&input).unwrap();
                    assert_eq!(b, w.finish().unwrap().into_inner());
                    let mut actual = vec![0; input.len()];
                    view.read_frames(0, &mut actual, &mut vec![0; codec.scratch_samples()])
                        .unwrap();
                    if length > 0 {
                        assert_eq!(&actual[..channels as usize], &input[..channels as usize]);
                    }
                }
            }
        }
    }
}

#[test]
fn arbitrary_ima_channel_counts_and_microsoft_custom_predictors() {
    for channels in [1u16, 2, 6, 32, 4000] {
        let (f, frames) = descriptor(AdpcmKind::Ima, channels, channels * 8, Endian::Little);
        let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
        let values: Vec<_> = (0..frames * channels as usize).map(|i| i as i16).collect();
        let mut block = vec![0; codec.block_bytes()];
        codec.encode_block(&values, &mut block).unwrap();
        let mut out = vec![0; values.len()];
        assert_eq!(codec.decode_block(&block, &mut out).unwrap(), frames);
        assert_eq!(&out[..channels as usize], &values[..channels as usize]);
    }
    let (mut f, frames) = descriptor(AdpcmKind::Microsoft, 1, 23, Endian::Little);
    f[16..18].copy_from_slice(&36u16.to_le_bytes());
    f[20..22].copy_from_slice(&8u16.to_le_bytes());
    f.extend([0, 0, 0, 1]);
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    let values: Vec<_> = (0..frames)
        .map(|i| if i & 1 == 0 { 1000 } else { -1000 })
        .collect();
    let mut block = vec![0; 23];
    codec.encode_block(&values, &mut block).unwrap();
    assert_eq!(block[0], 7);
    let mut out = vec![0; frames];
    codec.decode_block(&block, &mut out).unwrap();
    assert_eq!(values, out);
}

#[test]
fn invalid_encoder_arguments_do_not_modify_output_or_pending_samples() {
    for spec in [
        AdpcmSpec {
            kind: AdpcmKind::Ima,
            channels: 0,
            sample_rate: 8000,
            block_bytes: 8,
        },
        AdpcmSpec {
            kind: AdpcmKind::Ima,
            channels: 2,
            sample_rate: 8000,
            block_bytes: 12,
        },
        AdpcmSpec {
            kind: AdpcmKind::Microsoft,
            channels: 3,
            sample_rate: 8000,
            block_bytes: 24,
        },
        AdpcmSpec {
            kind: AdpcmKind::Microsoft,
            channels: 1,
            sample_rate: 8000,
            block_bytes: u16::MAX,
        },
    ] {
        assert!(spec.format(Endian::Little, &mut [0; 50]).is_err());
    }
    let (f, frames) = descriptor(AdpcmKind::Ima, 2, 32, Endian::Little);
    let codec = Adpcm::from_format(Format::parse(&f, Endian::Little).unwrap()).unwrap();
    for values in [vec![], vec![1], vec![1; frames * 2 + 2]] {
        let mut out = [0xab; 32];
        assert!(codec.encode_block(&values, &mut out).is_err());
        assert_eq!(out, [0xab; 32]);
    }
    let mut short = [0xab; 31];
    assert!(codec.encode_block(&[1, 2], &mut short).is_err());
    assert_eq!(short, [0xab; 31]);
    let spec = AdpcmSpec {
        kind: AdpcmKind::Ima,
        channels: 2,
        sample_rate: 8000,
        block_bytes: 32,
    };
    let mut w =
        AdpcmWriter::new(Cursor::new(Vec::new()), spec, WriteOptions::default(), &[]).unwrap();
    w.write_i16(&[1, 2]).unwrap();
    assert!(w.write_i16(&[3]).is_err());
    assert_eq!(w.frames_written(), 1);
    w.write_i16(&[4, 5]).unwrap();
    let bytes = w.finish().unwrap().into_inner();
    assert_eq!(
        AdpcmData::new(&Wave::parse(&bytes).unwrap())
            .unwrap()
            .frames(),
        2
    );
}
