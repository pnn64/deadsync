use deadlib_audio_decode_flac::{
    bitstream::BitWriter,
    checksum::{Md5, crc8, crc16},
    *,
};
use std::io::{Cursor, Read};

fn stream(variable: bool) -> (Vec<u8>, Vec<i32>) {
    let sizes = if variable { [16, 32, 3] } else { [32, 32, 3] };
    let mut frames = Vec::new();
    let mut expected = Vec::new();
    let mut md5 = Md5::new();
    for (number, n) in sizes.into_iter().enumerate() {
        let mut head = vec![
            255,
            248 | u8::from(variable),
            0x60,
            0x0e,
            if variable {
                expected.len() as u8
            } else {
                number as u8
            },
            (n - 1) as u8,
        ];
        head.push(crc8(&head));
        let mut w = BitWriter::new();
        w.write(2, 8).unwrap();
        for i in 0..n {
            let sample = (expected.len() as i32)
                .wrapping_mul(123456789)
                .wrapping_add(i);
            expected.push(sample);
            md5.update(&sample.to_le_bytes());
            w.signed(sample.into(), 32).unwrap();
        }
        head.extend_from_slice(w.as_bytes());
        head.extend_from_slice(&crc16(&head).to_be_bytes());
        frames.extend(head);
    }
    let info = StreamInfo {
        min_block_size: if variable { 16 } else { 32 },
        max_block_size: 32,
        min_frame_size: 0,
        max_frame_size: 0,
        sample_rate: 1,
        channels: 1,
        bits_per_sample: 32,
        total_samples: expected.len() as u64,
        md5: md5.finalize(),
    };
    let mut stream = b"fLaC".to_vec();
    MetadataBlock {
        kind: 0,
        data: &info.encode().unwrap(),
    }
    .write(true, &mut stream)
    .unwrap();
    stream.extend(frames);
    (stream, expected)
}
struct ShortReader<'a> {
    data: &'a [u8],
    max: usize,
}
impl Read for ShortReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = out.len().min(self.max).min(self.data.len());
        out[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}
#[test]
fn all_short_read_sizes_and_seeks_are_sample_exact() {
    for variable in [false, true] {
        let (bytes, expected) = stream(variable);
        for max in 1..=bytes.len() {
            let mut reader = Reader::new(ShortReader { data: &bytes, max }).unwrap();
            let mut actual = Vec::new();
            while let Some(frame) = reader.read_next().unwrap() {
                assert_eq!(frame.start_sample, actual.len() as u64);
                actual.extend_from_slice(frame.pcm.plane(0));
            }
            assert_eq!(actual, expected);
            assert_eq!(reader.integrity(), Integrity::Verified);
        }
        let mut decoder = Decoder::new(&bytes).unwrap();
        for sample in (0..=expected.len()).rev().chain(0..=expected.len()) {
            decoder.seek(sample as u64).unwrap();
            assert_eq!(decoder.position(), sample as u64);
            let mut actual = Vec::new();
            while let Some(frame) = decoder.read_next().unwrap() {
                assert_eq!(frame.start_sample, sample as u64 + actual.len() as u64);
                actual.extend_from_slice(frame.pcm.plane(0));
            }
            assert_eq!(actual, expected[sample..]);
            assert_eq!(
                decoder.integrity(),
                if sample == 0 {
                    Integrity::Verified
                } else {
                    Integrity::Partial
                }
            );
        }
        assert!(decoder.seek(expected.len() as u64 + 1).is_err());
    }
}
#[test]
fn truncations_count_digest_and_terminal_errors() {
    let (bytes, _) = stream(false);
    for n in 0..bytes.len() {
        let result = (|| -> Result<(), Error> {
            let mut r = Reader::new(Cursor::new(&bytes[..n]))?;
            while r.read_next()?.is_some() {}
            Ok(())
        })();
        assert!(result.is_err(), "prefix {n}");
    }
    let mut wrong = bytes.clone();
    wrong[26] ^= 1;
    let mut reader = Reader::new(Cursor::new(wrong)).unwrap();
    for _ in 0..3 {
        assert!(reader.read_next().unwrap().is_some());
    }
    assert_eq!(reader.read_next().unwrap_err(), Error::Checksum("PCM MD5"));
    assert!(reader.read_next().is_err());
    let mut options = DecodeOptions::default();
    options.limits.frame_bytes = 16;
    let mut reader = Reader::with_options(Cursor::new(&bytes), options).unwrap();
    assert!(matches!(reader.read_next(), Err(Error::LimitExceeded(_))));
}
#[test]
fn headerless_resynchronization_checks_full_frame_crc() {
    let (bytes, expected) = stream(false);
    let meta = Metadata::parse(&bytes, Limits::default()).unwrap();
    let mut data = b"garbage\xff\xf8\0\0 false sync".to_vec();
    data.extend_from_slice(&bytes[meta.audio_offset..]);
    // This generated frame's rate is inherited, so raw callers provide STREAMINFO.
    let mut decoder = FrameDecoder::new(Some(meta.stream_info)).unwrap();
    let (skip, frame) = decoder.find_frame(&data).unwrap();
    assert!(skip > 0);
    assert_eq!(frame.pcm.plane(0), &expected[..32]);
}
