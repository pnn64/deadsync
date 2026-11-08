use deadlib_audio_decode_flac::*;
use std::io::{Cursor, Read, Seek, SeekFrom};

fn encode(variable: bool, slots: u16) -> (Vec<u8>, Vec<i32>) {
    let input: Vec<_> = (0i32..2017).map(|i| i.wrapping_mul(123456789)).collect();
    let options = EncodeOptions {
        bits_per_sample: 32,
        channels: 1,
        block_size: 64,
        compression_level: 0,
        variable_block_size: variable,
        ..Default::default()
    };
    let mut encoder = Encoder::with_metadata(Cursor::new(Vec::new()), options, &[], slots).unwrap();
    encoder.write_interleaved(&input).unwrap();
    (
        encoder.finish_seekable().unwrap().output.into_inner(),
        input,
    )
}
struct ShortSource {
    source: Cursor<Vec<u8>>,
    max: usize,
    bytes: u64,
}
impl Read for ShortSource {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out.len().min(self.max);
        let n = self.source.read(&mut out[..n])?;
        self.bytes += n as u64;
        Ok(n)
    }
}
impl Seek for ShortSource {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.source.seek(from)
    }
}
#[test]
fn every_sample_short_reads_seektable_and_nonzero_origin() {
    for variable in [false, true] {
        for slots in [0, 17] {
            let (encoded, input) = encode(variable, slots);
            let mut bytes = vec![123; 59];
            bytes.extend(encoded);
            let mut source = Cursor::new(bytes);
            source.set_position(59);
            let source = ShortSource {
                source,
                max: 17,
                bytes: 0,
            };
            let mut reader = SeekReader::new(source).unwrap();
            assert_eq!(reader.index_len(), 0);
            // Start near EOF so the first hint demonstrably skips earlier frames.
            reader.seek(2016).unwrap();
            if slots != 0 {
                assert_eq!(reader.seek_stats().table_hits, 1);
                assert!(reader.index_len() < 8);
            }
            assert_eq!(
                reader.read_next().unwrap().unwrap().pcm.plane(0),
                &input[2016..]
            );
            assert!(reader.read_next().unwrap().is_none());
            for sample in (0..=input.len()).rev().chain(0..=input.len()) {
                reader.seek(sample as u64).unwrap();
                assert_eq!(reader.position(), sample as u64);
                let mut output = Vec::new();
                while let Some(frame) = reader.read_next().unwrap() {
                    assert_eq!(frame.start_sample, (sample + output.len()) as u64);
                    output.extend_from_slice(frame.pcm.plane(0));
                }
                assert_eq!(output, input[sample..]);
                assert_eq!(
                    reader.integrity(),
                    if sample == 0 {
                        Integrity::Verified
                    } else {
                        Integrity::Partial
                    }
                );
            }
            assert!(reader.seek(2018).is_err());
            reader.seek(0).unwrap();
            assert_eq!(
                reader.read_next().unwrap().unwrap().pcm.plane(0),
                &input[..64]
            );
        }
    }
}
#[test]
fn invalid_seektable_targets_fall_back_without_wrong_pcm() {
    for variable in [false, true] {
        let (encoded, input) = encode(variable, 17);
        let table = Metadata::parse(&encoded, Limits::default())
            .unwrap()
            .blocks
            .into_iter()
            .find(|b| b.kind == 3)
            .unwrap();
        let offset = table.data.as_ptr() as usize - encoded.as_ptr() as usize;
        for corrupt in 0..5 {
            let mut bytes = encoded.clone();
            let last = table
                .data
                .as_chunks::<18>()
                .0
                .iter()
                .rposition(|p| p[..8] != [255; 8])
                .unwrap();
            let point = &mut bytes[offset + last * 18..offset + last * 18 + 18];
            match corrupt {
                0 => point[8..16].fill(255), // overflow/outside source
                1 => point[8..16].fill(0),   // first frame, wrong coded number
                2 => point[15] ^= 1,         // not a frame boundary
                3 => point[16..18].fill(0),  // illegal target block size
                _ => point[17] ^= 1,         // declared block size differs
            }
            let mut reader = SeekReader::new(Cursor::new(bytes)).unwrap();
            reader.seek(2016).unwrap();
            assert_eq!(reader.seek_stats().table_rejections, 1);
            assert_eq!(
                reader.read_next().unwrap().unwrap().pcm.plane(0),
                &input[2016..]
            );
            assert!(reader.read_next().unwrap().is_none());
        }
    }
}
#[test]
fn unknown_totals_empty_stream_limits_and_error_recovery() {
    let (mut data, input) = encode(false, 0);
    // Replace STREAMINFO with the same parameters but unknown total and digest.
    let mut info = Metadata::parse(&data, Limits::default())
        .unwrap()
        .stream_info;
    info.total_samples = 0;
    info.md5 = [0; 16];
    data[8..42].copy_from_slice(&info.encode().unwrap());
    let mut reader = SeekReader::new(Cursor::new(&data)).unwrap();
    assert!(reader.seek(2018).is_err());
    assert!(reader.read_next().is_err());
    reader.seek(2016).unwrap();
    assert_eq!(
        reader.read_next().unwrap().unwrap().pcm.plane(0),
        &input[2016..]
    );
    let options = SeekOptions {
        max_index_frames: 1,
        ..Default::default()
    };
    let mut reader = SeekReader::with_options(Cursor::new(&data), options).unwrap();
    assert!(matches!(reader.seek(2000), Err(Error::LimitExceeded(_))));
    reader.seek(0).unwrap();
    assert!(reader.read_next().unwrap().is_some());
    let data = Encoder::new(Cursor::new(Vec::new()), EncodeOptions::default())
        .unwrap()
        .finish_seekable()
        .unwrap()
        .output
        .into_inner();
    let mut reader = SeekReader::new(Cursor::new(&data)).unwrap();
    reader.seek(0).unwrap();
    assert!(reader.read_next().unwrap().is_none());
    assert_eq!(reader.integrity(), Integrity::Verified);
    assert!(reader.seek(1).is_err());
    reader.seek(0).unwrap();
    assert!(reader.read_next().unwrap().is_none());
}

#[test]
fn validated_table_skips_source_reads_and_can_be_disabled() {
    let input: Vec<_> = (0i32..50_000).map(|i| i.wrapping_mul(123456789)).collect();
    let options = EncodeOptions {
        bits_per_sample: 32,
        channels: 1,
        block_size: 256,
        compression_level: 0,
        ..Default::default()
    };
    let mut encoder = Encoder::with_metadata(Cursor::new(Vec::new()), options, &[], 64).unwrap();
    encoder.write_interleaved(&input).unwrap();
    let data = encoder.finish_seekable().unwrap().output.into_inner();
    for enabled in [false, true] {
        let source = ShortSource {
            source: Cursor::new(data.clone()),
            max: 8192,
            bytes: 0,
        };
        let mut reader = SeekReader::with_options(
            source,
            SeekOptions {
                use_seektable: enabled,
                ..Default::default()
            },
        )
        .unwrap();
        reader.seek(49_999).unwrap();
        assert_eq!(
            reader.read_next().unwrap().unwrap().pcm.plane(0),
            &input[49_999..]
        );
        assert_eq!(reader.seek_stats().table_hits != 0, enabled);
        let source = reader.into_inner();
        if enabled {
            assert!(source.bytes < data.len() as u64 / 4);
        } else {
            assert!(source.bytes >= data.len() as u64);
        }
    }
}
