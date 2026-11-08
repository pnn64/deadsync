use deadlib_image_png::{
    AnimationPolicy, BlendOp, DecodeOptions, DisposeOp, EncodeOptions, FrameControl, Limits, Png,
    Samples, decode, decode_reader, decode_with_options, encode, encode_writer, zlib::Compression,
};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wpt-apng")
}
fn unpack(bytes: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .take((16 << 20) + 1)
        .read_to_end(&mut data)
        .unwrap();
    assert!(data.len() <= 16 << 20);
    data
}
fn word(data: &[u8]) -> u32 {
    u32::from_be_bytes(data[..4].try_into().unwrap())
}
struct Golden {
    width: u32,
    height: u32,
    animation: bool,
    default_is_frame: bool,
    plays: u32,
    count: usize,
    records: Vec<u8>,
}
impl Golden {
    fn read(path: &Path) -> Self {
        let data = unpack(&fs::read(path).unwrap());
        assert_eq!(&data[..8], b"DLAPNGR1");
        let width = word(&data[8..]);
        let height = word(&data[12..]);
        let count = word(&data[28..]) as usize;
        assert_eq!(
            data.len(),
            32 + count * (22 + width as usize * height as usize * 8)
        );
        Self {
            width,
            height,
            animation: word(&data[16..]) != 0,
            default_is_frame: word(&data[20..]) != 0,
            plays: word(&data[24..]),
            count,
            records: data[32..].to_vec(),
        }
    }
    fn record(&self, index: usize) -> &[u8] {
        let size = 22 + self.width as usize * self.height as usize * 8;
        &self.records[index * size..(index + 1) * size]
    }
}
fn control(data: &[u8]) -> FrameControl {
    FrameControl {
        width: word(data),
        height: word(&data[4..]),
        x_offset: word(&data[8..]),
        y_offset: word(&data[12..]),
        delay_num: u16::from_be_bytes(data[16..18].try_into().unwrap()),
        delay_den: u16::from_be_bytes(data[18..20].try_into().unwrap()),
        dispose: match data[20] {
            0 => DisposeOp::None,
            1 => DisposeOp::Background,
            2 => DisposeOp::Previous,
            _ => panic!("golden disposal"),
        },
        blend: match data[21] {
            0 => BlendOp::Source,
            1 => BlendOp::Over,
            _ => panic!("golden blend"),
        },
    }
}
fn samples_equal(actual: &[u16], expected: &[u8], label: &str, frame: usize) {
    assert_eq!(actual.len() * 2, expected.len());
    for (n, (&actual, bytes)) in actual.iter().zip(expected.as_chunks::<2>().0).enumerate() {
        let expected = u16::from_be_bytes(*bytes);
        assert_eq!(actual, expected, "{label}: frame {frame}, sample {n}");
    }
}
fn rendered(png: &Png, expected: &Golden, label: &str) {
    assert_eq!(
        (png.image.width, png.image.height),
        (expected.width, expected.height)
    );
    if expected.animation {
        let animation = png.animation.as_ref().unwrap();
        assert_eq!(animation.default_is_frame, expected.default_is_frame);
        assert_eq!(animation.plays, expected.plays);
        assert_eq!(animation.frames.len(), expected.count);
        let mut compositor = png.compositor(Limits::default()).unwrap();
        for _ in 0..2 {
            for n in 0..expected.count {
                let record = expected.record(n);
                let actual = compositor.next_frame().unwrap().unwrap();
                assert_eq!(actual.control, control(record), "{label}/{n}");
                assert_eq!(
                    (actual.width, actual.height),
                    (expected.width, expected.height)
                );
                samples_equal(actual.samples, &record[22..], label, n);
            }
            assert!(compositor.next_frame().unwrap().is_none());
            assert!(compositor.next_frame().unwrap().is_none());
            compositor.reset();
        }
    } else {
        assert!(png.animation.is_none());
        let image = png.to_rgba16(Limits::default()).unwrap();
        let Samples::U16(values) = image.samples else {
            unreachable!()
        };
        samples_equal(&values, &expected.record(0)[22..], label, 0);
    }
}
fn cases() -> Vec<PathBuf> {
    let mut paths: Vec<_> = fs::read_dir(root().join("goldens"))
        .unwrap()
        .map(|v| v.unwrap().path())
        .filter(|v| v.extension().is_some_and(|s| s == "z"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 35);
    paths
}
struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = self.0.len().min(out.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}

#[test]
fn external_wpt_all_rendered_rgba16_frames_and_both_writers() {
    let options = DecodeOptions {
        animation_policy: AnimationPolicy::RecoverDefault,
        ..Default::default()
    };
    let (mut animations, mut recovery, mut frames, mut encodings) = (0, 0, 0, 0);
    for path in cases() {
        let label = path.file_name().unwrap().to_str().unwrap();
        let name = label.split('.').next().unwrap();
        let expected = Golden::read(&path);
        let wire = fs::read(root().join("support").join(format!("{name}.png"))).unwrap();
        let png = decode_with_options(&wire, &options).unwrap();
        if expected.animation {
            assert_eq!(decode(&wire).unwrap(), png);
            assert!(png.animation_error.is_none());
            animations += 1;
        } else {
            assert!(decode(&wire).is_err());
            assert!(png.animation_error.is_some());
            recovery += 1;
        }
        rendered(&png, &expected, label);
        for fragment in [1, 7] {
            let read = decode_reader(Fragments(&wire, fragment), &options).unwrap();
            assert_eq!(read, png);
            rendered(&read, &expected, label);
        }
        for interlaced in [false, true] {
            for compression in [Compression::Stored, Compression::Fast] {
                let options = EncodeOptions {
                    interlaced,
                    compression,
                    idat_chunk_size: 257,
                    ..Default::default()
                };
                let mut streamed = Vec::new();
                encode_writer(&png, &options, &mut streamed).unwrap();
                for bytes in [encode(&png, &options).unwrap(), streamed] {
                    let decoded = decode(&bytes).unwrap();
                    assert!(decoded.animation_error.is_none());
                    rendered(&decoded, &expected, label);
                    encodings += 1;
                }
            }
        }
        frames += expected.count;
    }
    assert_eq!((animations, recovery, frames, encodings), (35, 0, 201, 280));
}

/// WPT's static references are 8-bit display results, whereas our canvas keeps
/// 16-bit arithmetic. Compare at their precision, independently of exact goldens.
#[test]
fn wpt_published_final_states_at_reference_precision() {
    let options = DecodeOptions {
        animation_policy: AnimationPolicy::RecoverDefault,
        ..Default::default()
    };
    let (mut compared, mut managed) = (0, 0);
    let (mut largest_error, mut rounded_channels) = (0, 0);
    for entry in fs::read_dir(root()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "html") {
            continue;
        }
        let html = fs::read_to_string(&path).unwrap();
        let Some((_, tail)) = html.split_once("<link rel=\"match\" href=\"") else {
            continue;
        };
        let reference = tail.split('"').next().unwrap();
        let input = html
            .split_once("<img src=\"support/")
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap();
        if input == "062.png" {
            // This reftest additionally requires Display-P3 -> sRGB color
            // management. Raw stored-space frames still have exact goldens.
            assert!(html.contains("color(display-p3"));
            managed += 1;
            continue;
        }
        let png = decode_with_options(
            &fs::read(root().join("support").join(input)).unwrap(),
            &options,
        )
        .unwrap();
        let mut actual = Vec::new();
        if png.animation.is_some() {
            let mut c = png.compositor(Limits::default()).unwrap();
            while let Some(frame) = c.next_frame().unwrap() {
                actual = frame.samples.to_vec();
            }
        } else {
            let Samples::U16(values) = png.to_rgba16(Limits::default()).unwrap().samples else {
                unreachable!()
            };
            actual = values;
        }
        let html = fs::read_to_string(root().join(reference)).unwrap();
        if let Some((_, tail)) = html.split_once("<img src=\"support/") {
            let name = tail.split('"').next().unwrap();
            let image = decode(&fs::read(root().join("support").join(name)).unwrap())
                .unwrap()
                .to_rgba16(Limits::default())
                .unwrap();
            let Samples::U16(values) = image.samples else {
                unreachable!()
            };
            assert_eq!(values.len(), actual.len());
            for (n, (a, b)) in actual
                .as_chunks::<4>()
                .0
                .iter()
                .zip(values.as_chunks::<4>().0)
                .enumerate()
            {
                assert_eq!(a[3], b[3], "{input}: alpha {n}");
                if a[3] != 0 {
                    for k in 0..3 {
                        let a = (u32::from(a[k]) + 128) / 257;
                        let b = u32::from(b[k]) / 257;
                        let error = a.abs_diff(b);
                        largest_error = largest_error.max(error);
                        rounded_channels += usize::from(error != 0);
                        assert!(
                            a.abs_diff(b) <= 1,
                            "{input}: pixel {n} channel {k}, native {a}, reference {b}"
                        );
                    }
                }
            }
        } else {
            assert!(reference.contains("checkerboard"));
            assert!(
                actual.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0),
                "{input}"
            );
        }
        compared += 1;
    }
    assert_eq!((compared, managed), (28, 1));
    println!(
        "WPT display references: {compared} checked, {managed} requires color management; maximum RGB difference={largest_error}/255, differing channels={rounded_channels}"
    );
}

#[test]
#[ignore = "build tests/oracle, set DEADLIB_LIBPNG_ORACLE, and provide Python 3"]
fn regenerate_exact_render_goldens_with_independent_libpng_and_fractions() {
    use std::{
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let oracle = std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE required");
    let python = std::env::var_os("DEADLIB_PYTHON").unwrap_or_else(|| "python".into());
    let output = std::env::temp_dir().join(format!(
        "deadlib-apng-goldens-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    assert!(!output.exists());
    assert_eq!(output.parent().unwrap(), std::env::temp_dir());
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/generate_apng_render_goldens.py");
    let result = Command::new(python)
        .arg(script)
        .arg("--oracle")
        .arg(oracle)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for path in cases() {
        assert_eq!(
            unpack(&fs::read(&path).unwrap()),
            unpack(&fs::read(output.join(path.file_name().unwrap())).unwrap())
        );
    }
    // Retain regenerated answers so the command and its output are reviewable.
    println!(
        "Regenerated 35 libpng/fraction APNG answers at {}",
        output.display()
    );
}
