use deadlib_image_png::{
    ColorType, DecodeEvent, DecodeOptions, DecodeStatus, EncodeOptions, Error, Image, ImageKind,
    Limits, Png, Samples, StreamDecoder, decode, decode_reader, encode,
};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

struct Fragmented<'a> {
    input: &'a [u8],
    size: usize,
    interrupt: bool,
    calls: usize,
}
impl Read for Fragmented<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.interrupt && self.calls.is_multiple_of(3) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let n = self.input.len().min(out.len()).min(self.size);
        out[..n].copy_from_slice(&self.input[..n]);
        self.input = &self.input[n..];
        Ok(n)
    }
}
fn reader(input: &[u8], size: usize) -> Fragmented<'_> {
    Fragmented {
        input,
        size,
        interrupt: true,
        calls: 0,
    }
}
fn fixture(height: u32) -> Vec<u8> {
    let image = Image {
        width: 17,
        height,
        color_type: ColorType::Rgba,
        bit_depth: 16,
        samples: Samples::U16(
            (0..17 * height * 4)
                .map(|n| n.wrapping_mul(37139) as u16)
                .collect(),
        ),
    };
    encode(
        &Png::new(image),
        &EncodeOptions {
            idat_chunk_size: 7,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn pngsuite_and_apng_arbitrary_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<_> = fs::read_dir(root.join("pngsuite"))
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|v| v == "png"))
        .collect();
    paths.push(root.join("apng/basic_f20.png"));
    paths.sort();
    for path in paths {
        let bytes = fs::read(&path).unwrap();
        let expected = decode(&bytes);
        for fragment in [1, 2, 7, 4096] {
            let actual = decode_reader(reader(&bytes, fragment), &DecodeOptions::default());
            match &expected {
                Ok(png) => assert_eq!(
                    actual
                        .as_ref()
                        .unwrap_or_else(|e| panic!("{} chunk={fragment}: {e}", path.display())),
                    png,
                    "{} chunk={fragment}",
                    path.display()
                ),
                Err(_) => assert!(actual.is_err(), "{} chunk={fragment}", path.display()),
            }
        }
    }
}

#[test]
fn every_truncation_limits_and_reader_errors() {
    let bytes = fixture(2);
    for end in 0..bytes.len() {
        assert!(
            decode_reader(reader(&bytes[..end], 1), &DecodeOptions::default()).is_err(),
            "end={end}"
        );
    }
    for limits in [
        Limits {
            max_input_bytes: 1,
            ..Default::default()
        },
        Limits {
            max_output_bytes: 1,
            ..Default::default()
        },
        Limits {
            max_chunks: 1,
            ..Default::default()
        },
    ] {
        assert_eq!(
            decode_reader(
                reader(&bytes, 1),
                &DecodeOptions {
                    limits,
                    ..Default::default()
                }
            ),
            Err(Error::LimitExceeded)
        );
    }
    struct Failed;
    impl Read for Failed {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::PermissionDenied.into())
        }
    }
    assert!(matches!(
        decode_reader(Failed, &DecodeOptions::default()),
        Err(Error::Io(_))
    ));
}

fn rows(bytes: &[u8], fragment: usize) -> Result<(usize, usize, usize), Error> {
    let mut decoder = StreamDecoder::new(DecodeOptions::default());
    let mut offset = 0;
    let mut rows = 0;
    let mut ends = 0;
    loop {
        let end = bytes.len().min(offset + fragment);
        let progress = decoder.process(&bytes[offset..end])?;
        offset += progress.consumed;
        assert_eq!(decoder.total_in(), offset);
        match progress.status {
            DecodeStatus::Event => match decoder.event().unwrap() {
                DecodeEvent::Row(_) => rows += 1,
                DecodeEvent::ImageEnd(_) => ends += 1,
                _ => (),
            },
            DecodeStatus::NeedInput if offset == bytes.len() => {
                decoder.finish()?;
                panic!("incomplete PNG accepted");
            }
            DecodeStatus::NeedInput => assert_eq!(offset, end),
            DecodeStatus::Done => {
                decoder.finish()?;
                return Ok((offset, rows, decoder.peak_image_buffer_bytes()));
            }
        }
        assert!(ends <= 21);
    }
}

#[test]
fn prefix_consumption_provisional_rows_and_verified_image_end() {
    let mut bytes = fixture(3);
    let len = bytes.len();
    bytes.extend_from_slice(b"trailing bytes");
    let (used, count, _) = rows(&bytes, 65536).unwrap();
    assert_eq!(used, len);
    assert_eq!(count, 3);
    assert!(decode_reader(bytes.as_slice(), &DecodeOptions::default()).is_err());
    let bytes = fixture(3);
    let mut bad = bytes.clone();
    let mut pos = 8;
    let mut last = 0;
    while pos < bad.len() {
        let n = u32::from_be_bytes(bad[pos..pos + 4].try_into().unwrap()) as usize;
        if &bad[pos + 4..pos + 8] == b"IDAT" {
            last = pos + 8 + n;
        }
        pos += n + 12;
    }
    bad[last] ^= 1;
    let mut decoder = StreamDecoder::new(DecodeOptions::default());
    let mut offset = 0;
    let mut provisional = 0;
    loop {
        match decoder.process(&bad[offset..]) {
            Ok(progress) => {
                offset += progress.consumed;
                match decoder.event() {
                    Some(DecodeEvent::Row(_)) => provisional += 1,
                    Some(DecodeEvent::ImageEnd(_)) => panic!("unverified ImageEnd"),
                    _ => (),
                };
                assert_ne!(progress.status, DecodeStatus::Done);
            }
            Err(error) => {
                assert_eq!(error, Error::Checksum);
                assert_eq!(decoder.process(&[]), Err(error));
                break;
            }
        }
    }
    assert_eq!(provisional, 3);
}

#[test]
fn row_pipeline_memory_is_independent_of_height() {
    let mut peaks = Vec::new();
    for height in [2, 4096] {
        let bytes = fixture(height);
        let (used, count, peak) = rows(&bytes, 31).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(count, height as usize);
        assert!(peak < 45 * 1024);
        peaks.push(peak);
    }
    assert_eq!(peaks[0], peaks[1]);
}

#[test]
fn apng_image_roles_and_completion() {
    let bytes =
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/apng/basic_f20.png"))
            .unwrap();
    let expected = decode(&bytes).unwrap();
    let a = expected.animation.as_ref().unwrap();
    let mut decoder = StreamDecoder::new(DecodeOptions::default());
    let mut offset = 0;
    let mut controls = 0;
    let mut ended = Vec::new();
    loop {
        let end = bytes.len().min(offset + 1);
        let progress = decoder.process(&bytes[offset..end]).unwrap();
        offset += progress.consumed;
        match decoder.event() {
            Some(DecodeEvent::FrameControl { index, .. }) => {
                assert_eq!(index, controls);
                controls += 1;
            }
            Some(DecodeEvent::ImageEnd(kind)) => ended.push(kind),
            _ => (),
        }
        if progress.status == DecodeStatus::Done {
            break;
        }
    }
    assert_eq!(controls, a.frames.len() as u32);
    assert_eq!(
        ended.len(),
        a.frames.len() + usize::from(!a.default_is_frame)
    );
    assert_eq!(ended[0], ImageKind::Default);
}
