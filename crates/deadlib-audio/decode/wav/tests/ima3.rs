use deadlib_audio_decode_wav::{
    Adpcm, AdpcmData, AdpcmForwardReader, AdpcmKind, AdpcmReader, AdpcmSpec, AdpcmWriter,
    Container, Endian, Format, RawChunk, Wave, WriteOptions,
};
use std::io::{Cursor, Read};

// Published RIFFNEW step values; the oracle uses no production codec helpers.
const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
fn descriptor(channels: u16, groups: u16, endian: Endian) -> Vec<u8> {
    let mut b = Vec::new();
    let block = channels * (4 + 12 * groups);
    let frames = 1 + 32 * groups;
    let word = |n: u16| match endian {
        Endian::Little => n.to_le_bytes(),
        Endian::Big => n.to_be_bytes(),
    };
    let dword = |n: u32| match endian {
        Endian::Little => n.to_le_bytes(),
        Endian::Big => n.to_be_bytes(),
    };
    b.extend(word(0x11));
    b.extend(word(channels));
    b.extend(dword(8000));
    b.extend(dword(8000 * u32::from(block) / u32::from(frames)));
    for n in [block, 3, 2, frames] {
        b.extend(word(n));
    }
    b
}
// Pack one bit at a time, including codes spanning two interleaved channel words.
fn pack(headers: &[(i16, u8)], codes: &[Vec<u8>], endian: Endian) -> Vec<u8> {
    let channels = headers.len();
    let mut b = vec![0; 4 * channels + codes[0].len() / 32 * 12 * channels];
    for (c, &(sample, index)) in headers.iter().enumerate() {
        b[c * 4..c * 4 + 2].copy_from_slice(&match endian {
            Endian::Little => sample.to_le_bytes(),
            Endian::Big => sample.to_be_bytes(),
        });
        b[c * 4 + 2] = index;
        for (i, &code) in codes[c].iter().enumerate() {
            for k in 0..3 {
                let bit = i * 3 + k;
                let offset = 4 * channels + (bit / 32 * channels + c) * 4 + bit % 32 / 8;
                b[offset] |= ((code >> k) & 1) << (bit % 8);
            }
        }
    }
    b
}
fn reference(b: &[u8], channels: usize, endian: Endian) -> Vec<i16> {
    let frames = 1 + (b.len() - channels * 4) * 8 / (3 * channels);
    let mut result = vec![0; frames * channels];
    for c in 0..channels {
        let a = [b[c * 4], b[c * 4 + 1]];
        let mut value = i32::from(match endian {
            Endian::Little => i16::from_le_bytes(a),
            Endian::Big => i16::from_be_bytes(a),
        });
        let mut index = usize::from(b[c * 4 + 2]);
        result[c] = value as i16;
        for i in 0..frames - 1 {
            let mut code = 0;
            for k in 0..3 {
                let bit = i * 3 + k;
                let offset = 4 * channels + (bit / 32 * channels + c) * 4 + bit % 32 / 8;
                code |= ((b[offset] >> (bit % 8)) & 1) << k;
            }
            let step = STEPS[index];
            let levels = [
                step / 4,
                step / 4 + step / 2,
                step / 4 + step,
                step / 4 + step + step / 2,
            ];
            let magnitude = levels[usize::from(code % 4)];
            value = (value + if code < 4 { magnitude } else { -magnitude }).clamp(-32768, 32767);
            index = (index as i32 + [-1, -1, 1, 2][usize::from(code % 4)]).clamp(0, 88) as usize;
            result[(i + 1) * channels + c] = value as i16;
        }
    }
    result
}

#[test]
fn independent_three_bit_gold_vector_and_every_initial_state() {
    let fmt = descriptor(1, 1, Endian::Little);
    let codec = Adpcm::from_format(Format::parse(&fmt, Endian::Little).unwrap()).unwrap();
    assert_eq!(codec.bits_per_sample(), 3);
    let codes = vec![(0..32).map(|n| (n % 8) as u8).collect()];
    let block = pack(&[(0, 0)], &codes, Endian::Little);
    assert_eq!(
        &block[4..],
        &[
            0x88, 0xc6, 0xfa, 0x88, 0xc6, 0xfa, 0x88, 0xc6, 0xfa, 0x88, 0xc6, 0xfa
        ]
    );
    let expected = [
        0, 1, 5, 13, 27, 25, 19, 9, -6, -4, 3, 14, 31, 28, 21, 9, -9, -6, 3, 16, 37, 34, 25, 10,
        -12, -8, 2, 18, 42, 38, 26, 9, -19,
    ];
    let mut out = [0; 33];
    assert_eq!(codec.decode_block(&block, &mut out).unwrap(), 33);
    assert_eq!(out, expected);
    // All table entries, codewords and signed initial extrema, with adaptation.
    for index in 0..89 {
        for code in 0..8 {
            for initial in [i16::MIN, 0, i16::MAX] {
                let block = pack(&[(initial, index)], &[vec![code; 32]], Endian::Little);
                codec.decode_block(&block, &mut out).unwrap();
                assert_eq!(out.as_slice(), reference(&block, 1, Endian::Little));
            }
        }
    }
}

#[test]
fn word_boundary_crossings_do_not_read_the_next_channel() {
    for channels in [1u16, 2, 6, 33] {
        for endian in [Endian::Little, Endian::Big] {
            let fmt = descriptor(channels, 3, endian);
            let codec = Adpcm::from_format(Format::parse(&fmt, endian).unwrap()).unwrap();
            let headers = (0..channels)
                .map(|c| ((i32::from(c) * 1421 - 30000) as i16, (c * 11 % 89) as u8))
                .collect::<Vec<_>>();
            let codes = (0..channels)
                .map(|c| {
                    (0..96)
                        .map(|i| ((i * 5 + usize::from(c) * 3) % 8) as u8)
                        .collect()
                })
                .collect::<Vec<_>>();
            let block = pack(&headers, &codes, endian);
            let mut out = vec![123; codec.scratch_samples() + 11];
            assert_eq!(codec.decode_block(&block, &mut out).unwrap(), 97);
            assert_eq!(
                &out[..codec.scratch_samples()],
                reference(&block, usize::from(channels), endian)
            );
            assert_eq!(&out[codec.scratch_samples()..], &[123; 11]);
        }
    }
}

#[test]
fn three_bit_geometry_short_blocks_and_bad_headers_are_checked() {
    for channels in [1u16, 2, 6, 33] {
        let fmt = descriptor(channels, 3, Endian::Little);
        let codec = Adpcm::from_format(Format::parse(&fmt, Endian::Little).unwrap()).unwrap();
        let header = 4 * usize::from(channels);
        let group = 12 * usize::from(channels);
        for bytes in 0..=codec.block_bytes() + 1 {
            let valid = bytes >= header
                && bytes <= codec.block_bytes()
                && (bytes - header).is_multiple_of(group);
            assert_eq!(
                codec.block_frames(bytes).is_ok(),
                valid,
                "{channels} / {bytes}"
            );
        }
        let mut out = vec![0; codec.scratch_samples()];
        let mut block = vec![0; header + group];
        block[3] = 0xff; // Reserved byte does not enter the step index.
        assert_eq!(codec.decode_block(&block, &mut out).unwrap(), 33);
        assert!(
            codec
                .decode_block(&block, &mut out[..33 * usize::from(channels) - 1])
                .is_err()
        );
        block[2] = 89;
        assert!(codec.decode_block(&block, &mut out).is_err());
        let spec = AdpcmSpec {
            kind: AdpcmKind::Ima3,
            channels,
            sample_rate: 8000,
            block_bytes: 4 * channels,
        };
        let mut format = [0; 50];
        let len = spec.format(Endian::Little, &mut format).unwrap();
        let single =
            Adpcm::from_format(Format::parse(&format[..len], Endian::Little).unwrap()).unwrap();
        let samples = vec![-1243; usize::from(channels)];
        let mut encoded = vec![0; single.block_bytes()];
        single.encode_block(&samples, &mut encoded).unwrap();
        single.decode_block(&encoded, &mut out).unwrap();
        assert_eq!(&out[..samples.len()], &samples);
    }
    for bits in [2u16, 5] {
        let mut fmt = descriptor(1, 1, Endian::Little);
        fmt[14..16].copy_from_slice(&bits.to_le_bytes());
        assert!(Adpcm::from_format(Format::parse(&fmt, Endian::Little).unwrap()).is_err());
    }
    for block in [8u16, 12, 20, 24, 32] {
        let mut fmt = [0; 50];
        assert!(
            AdpcmSpec {
                kind: AdpcmKind::Ima3,
                channels: 1,
                sample_rate: 8000,
                block_bytes: block
            }
            .format(Endian::Little, &mut fmt)
            .is_err()
        );
    }
}

struct Short<'a>(&'a [u8]);
impl Read for Short<'_> {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = b.len().min(3);
        self.0.read(&mut b[..n])
    }
}

#[test]
fn three_bit_writers_and_all_frame_readers_preserve_block_and_fact_contracts() {
    for channels in [1u16, 2, 6, 33] {
        for container in [
            Container::Riff,
            Container::Rifx,
            Container::Rf64,
            Container::Bw64,
        ] {
            let spec = AdpcmSpec {
                kind: AdpcmKind::Ima3,
                channels,
                sample_rate: 8000,
                block_bytes: 28 * channels,
            };
            let options = WriteOptions {
                container,
                auto_rf64: false,
                force_extensible: false,
            };
            let input = (0..149 * usize::from(channels))
                .map(|i| (i as u32).wrapping_mul(104729) as i16)
                .collect::<Vec<_>>();
            let metadata = [RawChunk {
                id: *b"JUNK",
                data: b"abcde",
            }];
            let mut writer =
                AdpcmWriter::new(Cursor::new(Vec::new()), spec, options, &metadata).unwrap();
            let mut known =
                AdpcmWriter::with_frames(Vec::new(), spec, options, &metadata, 149).unwrap();
            for batch in input.chunks(7 * usize::from(channels)) {
                writer.write_i16(batch).unwrap();
                known.write_i16(batch).unwrap();
            }
            let bytes = writer.finish().unwrap().into_inner();
            assert_eq!(known.finish_stream().unwrap(), bytes);
            let wave = Wave::parse(&bytes).unwrap();
            assert_eq!(wave.fact, Some(149));
            let mut expected = Vec::new();
            for block in wave.data().chunks(usize::from(spec.block_bytes)) {
                expected.extend(reference(block, usize::from(channels), container.endian()));
            }
            expected.truncate(input.len());
            let borrowed = AdpcmData::new(&wave).unwrap();
            let mut scratch = vec![0; borrowed.codec().scratch_samples()];
            let mut reader = AdpcmReader::new(Cursor::new(&bytes)).unwrap();
            let mut forward = AdpcmForwardReader::new(Short(&bytes)).unwrap();
            let mut output = vec![0; 11 * usize::from(channels)];
            let mut position = 0;
            loop {
                let n = forward.read_i16(&mut output).unwrap();
                if n == 0 {
                    break;
                }
                assert_eq!(&output[..n], &expected[position..position + n]);
                position += n;
            }
            assert_eq!(position, expected.len());
            forward.finish().unwrap();
            for start in [0u64, 1, 31, 32, 33, 64, 65, 66, 148, 149] {
                let n = borrowed
                    .read_frames(start, &mut output, &mut scratch)
                    .unwrap();
                let offset = start as usize * usize::from(channels);
                assert_eq!(&output[..n], &expected[offset..offset + n]);
                reader.seek_frame(start).unwrap();
                let m = reader.read_i16(&mut output).unwrap();
                assert_eq!(m, n);
                assert_eq!(&output[..m], &expected[offset..offset + m]);
            }
        }
    }
}
