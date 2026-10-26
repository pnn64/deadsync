use deadlib_image_png::{ColorType, EncodeOptions, Filter, Image, Png, Samples, decode, encode};
use std::{fs, path::Path, process::Command};

fn verify(oracle: &Path, path: &Path) {
    let actual = decode(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        deadlib_image_png::decode_reader(fs::File::open(path).unwrap(), &Default::default())
            .unwrap(),
        actual
    );
    let output = Command::new(oracle).arg(path).output().unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let mut bytes = output.stdout.as_slice();
    for value in [
        actual.image.width,
        actual.image.height,
        u32::from(actual.image.bit_depth),
        actual.image.color_type as u32,
    ] {
        assert!(bytes.len() >= 4);
        assert_eq!(u32::from_be_bytes(bytes[..4].try_into().unwrap()), value);
        bytes = &bytes[4..];
    }
    match &actual.image.samples {
        Samples::U16(v) => assert_eq!(
            bytes,
            v.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>(),
            "{}",
            path.display()
        ),
        Samples::U8(v) if actual.image.bit_depth == 8 => assert_eq!(bytes, v, "{}", path.display()),
        Samples::U8(v) => {
            let depth = actual.image.bit_depth as usize;
            let w = actual.image.width as usize;
            let stride = (w * depth).div_ceil(8);
            assert_eq!(bytes.len(), stride * actual.image.height as usize);
            for (n, &sample) in v.iter().enumerate() {
                let (y, x) = (n / w, n % w);
                assert_eq!(
                    sample,
                    (bytes[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                        & ((1 << depth) - 1),
                    "{} sample={n}",
                    path.display()
                );
            }
        }
    }
}

#[test]
#[ignore = "build tests/oracle and set DEADLIB_LIBPNG_ORACLE; see tests/oracle/README.md"]
fn libpng_native_sample_conformance() {
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let oracle = Path::new(&oracle);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pngsuite");
    let mut count = 0;
    for file in fs::read_dir(fixtures).unwrap() {
        let path = file.unwrap().path();
        if path.extension().is_none_or(|v| v != "png")
            || path.file_name().unwrap().to_str().unwrap().starts_with('x')
        {
            continue;
        }
        verify(oracle, &path);
        count += 1;
    }
    assert_eq!(count, 161);
    let scratch =
        std::env::temp_dir().join(format!("deadlib-png-oracle-{}.png", std::process::id()));
    for color in [
        ColorType::Grayscale,
        ColorType::Rgb,
        ColorType::Indexed,
        ColorType::GrayscaleAlpha,
        ColorType::Rgba,
    ] {
        for depth in [1, 2, 4, 8, 16] {
            if !color.allows_depth(depth) {
                continue;
            }
            for (w, h) in [(1, 1), (3, 2), (17, 13)] {
                let n = w as usize * h as usize * color.channels();
                let samples = if depth == 16 {
                    Samples::U16((0..n).map(|n| (n * 37139) as u16).collect())
                } else {
                    Samples::U8(
                        (0..n)
                            .map(|n| ((n * 37 + n * n) % (1usize << depth)) as u8)
                            .collect(),
                    )
                };
                let mut png = Png::new(Image {
                    width: w,
                    height: h,
                    color_type: color,
                    bit_depth: depth,
                    samples,
                });
                if color == ColorType::Indexed {
                    png.palette = (0..1usize << depth)
                        .map(|n| [n as u8, n as u8, n as u8])
                        .collect();
                }
                for filter in [
                    Filter::None,
                    Filter::Sub,
                    Filter::Up,
                    Filter::Average,
                    Filter::Paeth,
                ] {
                    for interlaced in [false, true] {
                        let bytes = encode(
                            &png,
                            &EncodeOptions {
                                filter,
                                interlaced,
                                idat_chunk_size: 7,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        fs::write(&scratch, bytes).unwrap();
                        verify(oracle, &scratch);
                        count += 1;
                        let mut streamed = Vec::new();
                        deadlib_image_png::encode_writer(
                            &png,
                            &EncodeOptions {
                                filter,
                                interlaced,
                                idat_chunk_size: 7,
                                ..Default::default()
                            },
                            &mut streamed,
                        )
                        .unwrap();
                        fs::write(&scratch, streamed).unwrap();
                        verify(oracle, &scratch);
                        count += 1;
                    }
                }
            }
        }
    }
    fs::remove_file(scratch).unwrap();
    assert_eq!(count, 1061);
}

#[test]
#[ignore = "set DEADLIB_LIBPNG_ORACLE and DEADLIB_PNG_CORPUS; fetch inputs with scripts/fetch_corpus.py"]
fn libpng_external_corpus() {
    let oracle =
        std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE is required");
    let root = std::env::var_os("DEADLIB_PNG_CORPUS").expect("DEADLIB_PNG_CORPUS is required");
    let scratch = std::env::temp_dir().join(format!(
        "deadlib-png-corpus-oracle-{}.png",
        std::process::id()
    ));
    let mut files = 0;
    let mut checks = 0;
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "png") {
            continue;
        }
        verify(Path::new(&oracle), &path);
        let original = decode(&fs::read(path).unwrap()).unwrap();
        let png = Png::new(original.image);
        checks += 1;
        for interlaced in [false, true] {
            for compression in [
                deadlib_image_png::zlib::Compression::Stored,
                deadlib_image_png::zlib::Compression::Fast,
            ] {
                let options = EncodeOptions {
                    interlaced,
                    compression,
                    ..Default::default()
                };
                let mut streamed = Vec::new();
                deadlib_image_png::encode_writer(&png, &options, &mut streamed).unwrap();
                for encoded in [encode(&png, &options).unwrap(), streamed] {
                    assert_eq!(decode(&encoded).unwrap().image, png.image);
                    fs::write(&scratch, encoded).unwrap();
                    verify(Path::new(&oracle), &scratch);
                    checks += 1;
                }
            }
        }
        files += 1;
    }
    assert!(files != 0, "corpus contains no PNG files");
    fs::remove_file(scratch).unwrap();
    assert_eq!(checks, files * 9);
    println!("Verified {files} corpus files, {checks} native-sample libpng comparisons");
}
