use deadlib_image_gif::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn files(group: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/image-gif")
        .join(group);
    let mut files: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|p| p.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "gif"))
        .collect();
    files.sort();
    files
}
fn verify(bytes: &[u8], actual: &Gif, label: &Path) {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut reader = opts.read_info(bytes).unwrap();
    assert_eq!(
        (reader.width(), reader.height()),
        (actual.header.width, actual.header.height),
        "{}",
        label.display()
    );
    let global = actual
        .header
        .global_palette
        .as_ref()
        .map(|p| p.iter().flatten().copied().collect::<Vec<_>>());
    assert_eq!(reader.global_palette(), global.as_deref());
    let mut screen = gif_dispose::Screen::new_decoder(&reader);
    let mut native = Compositor::with_options(
        &actual.header,
        CompositeOptions {
            background: Background::Transparent,
            ..Default::default()
        },
    )
    .unwrap();
    for (n, expected) in actual.frames().enumerate() {
        let frame = reader
            .read_next_frame()
            .unwrap_or_else(|e| panic!("{} frame {n}: {e}", label.display()))
            .unwrap();
        assert_eq!(
            (frame.left, frame.top, frame.width, frame.height),
            (expected.left, expected.top, expected.width, expected.height),
            "{} frame {n}",
            label.display()
        );
        assert_eq!(
            frame.buffer.as_ref(),
            expected.indices,
            "{} frame {n}",
            label.display()
        );
        let local = expected
            .local_palette
            .as_ref()
            .map(|p| p.iter().flatten().copied().collect::<Vec<_>>());
        assert_eq!(frame.palette.as_deref(), local.as_deref());
        let c = expected.control.unwrap_or_default();
        assert_eq!(
            (frame.delay, frame.transparent, frame.needs_user_input),
            (c.delay, c.transparent_index, c.user_input)
        );
        if expected.control.is_some() {
            assert_eq!(frame.dispose as u8, c.disposal as u8);
        }
        screen.blit_frame(frame).unwrap();
        let rendered = native.render(expected).unwrap();
        let p = screen.pixels_rgba().to_contiguous_buf();
        let expected_rgba: Vec<u8> = p.0.iter().flat_map(|c| [c.r, c.g, c.b, c.a]).collect();
        assert_eq!(
            rendered.rgba,
            expected_rgba,
            "{} composited frame {n}",
            label.display()
        );
    }
    assert!(reader.read_next_frame().unwrap().is_none());
}

#[test]
fn established_image_gif_samples() {
    let paths = files("samples");
    assert_eq!(paths.len(), 13);
    let mut valid = 0;
    for path in paths {
        let bytes = fs::read(&path).unwrap();
        let compatibility = match path.file_name().unwrap().to_str().unwrap() {
            "set_hsts.gif" => Some(("background index outside global palette", 1)),
            // Frames 60 and 89 (zero-based) end after all pixels without EOI.
            "gifplayer-muybridge.gif" => Some(("incomplete LZW image", 380)),
            "moon_impact.gif" => Some(("graphic control block", 14)),
            _ => None,
        };
        let actual = if let Some((reason, frames)) = compatibility {
            assert_eq!(decode(&bytes).unwrap_err(), Error::Invalid(reason));
            let actual = decode_with_options(
                &bytes,
                &DecodeOptions {
                    strict: false,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(actual.frames().count(), frames);
            actual
        } else {
            decode(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        };
        verify(&bytes, &actual, &path);
        if path.file_name().unwrap() == "set_hsts.gif" {
            // Recovery preserves the invalid, unused background byte. Encoding
            // remains strict instead of silently changing that metadata.
            assert_eq!(actual.header.background_index, 255);
            assert_eq!(
                encode(&actual, &EncodeOptions::default()),
                Err(Error::Invalid("background index outside global palette"))
            );
        } else {
            let roundtrip = encode(&actual, &EncodeOptions::default()).unwrap();
            assert_eq!(decode(&roundtrip).unwrap(), actual);
            verify(&roundtrip, &actual, &path);
        }
        valid += 1;
    }
    assert_eq!(valid, 13);
}

#[test]
fn upstream_malformed_and_regression_cases() {
    let mut count = 0;
    for group in ["crashtest", "regression", "stall"] {
        for path in files(group) {
            let bytes = fs::read(&path).unwrap();
            let options = DecodeOptions {
                limits: Limits {
                    max_frame_pixels: 1 << 20,
                    max_total_pixels: 2 << 20,
                    max_blocks: 4096,
                    ..Default::default()
                },
                ..Default::default()
            };
            let result = decode_with_options(&bytes, &options);
            println!(
                "{}: {:?}",
                path.file_name().unwrap().to_string_lossy(),
                result.as_ref().map(|g| g.frames().count())
            );
            // Crash/stall cases are malformed; regression cases exercise valid
            // clear-heavy LZW and an unknown extension that must be retained.
            if group == "regression" {
                let actual = result.unwrap();
                if path.file_name().unwrap() == "issue-209-extension-block-type.gif" {
                    assert!(
                        actual
                            .blocks
                            .iter()
                            .any(|b| matches!(b, Block::Extension(Extension::Unknown { .. })))
                    );
                } else {
                    assert_eq!(actual.frames().count(), 1);
                }
                assert_eq!(
                    decode(&encode(&actual, &EncodeOptions::default()).unwrap()).unwrap(),
                    actual
                );
            } else {
                assert!(result.is_err(), "{} unexpectedly accepted", path.display());
            }
            count += 1;
        }
    }
    assert_eq!(count, 7);
}
