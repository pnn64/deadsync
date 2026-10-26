use deadlib_image_png::{EncodeOptions, Samples, decode, encode};
use std::{fs, io::Cursor, path::Path};

#[test]
fn pngsuite_native_samples_and_reencoding() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pngsuite");
    let mut paths: Vec<_> = fs::read_dir(path)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect();
    paths.sort();
    let (mut valid, mut invalid) = (0, 0);
    for path in paths {
        let name = path.file_name().unwrap().to_str().unwrap();
        let bytes = fs::read(&path).unwrap();
        if name.starts_with('x') {
            assert!(decode(&bytes).is_err(), "malformed fixture {name}");
            invalid += 1;
            continue;
        }
        let actual = decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut reader = png::Decoder::new(Cursor::new(&bytes)).read_info().unwrap();
        let mut expected = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut expected).unwrap();
        assert_eq!(info.width, actual.image.width, "{name}");
        assert_eq!(info.height, actual.image.height, "{name}");
        assert_eq!(
            info.color_type as u8, actual.image.color_type as u8,
            "{name}"
        );
        assert_eq!(info.bit_depth as u8, actual.image.bit_depth, "{name}");
        match &actual.image.samples {
            Samples::U16(values) => {
                let native: Vec<_> = expected[..info.buffer_size()]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&v| u16::from_be_bytes(v))
                    .collect();
                assert_eq!(values, &native, "{name}");
            }
            Samples::U8(values) if actual.image.bit_depth == 8 => {
                assert_eq!(values, &expected[..info.buffer_size()], "{name}")
            }
            Samples::U8(values) => {
                let depth = actual.image.bit_depth as usize;
                let width = info.width as usize;
                let stride = (width * depth).div_ceil(8);
                for (n, &sample) in values.iter().enumerate() {
                    let (y, x) = (n / width, n % width);
                    assert_eq!(
                        sample,
                        (expected[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                            & ((1 << depth) - 1),
                        "{name} sample {n}"
                    );
                }
            }
        }
        reader.finish().unwrap();
        for interlaced in [false, true] {
            let encoded = encode(
                &actual,
                &EncodeOptions {
                    interlaced,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|e| panic!("encode {name}: {e}"));
            assert_eq!(
                decode(&encoded).unwrap().image,
                actual.image,
                "roundtrip {name}"
            );
        }
        valid += 1;
    }
    assert_eq!((valid, invalid), (161, 14));
}
