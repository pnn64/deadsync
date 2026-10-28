use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, Error, Frame, FrameControl, Image, Limits, Png,
    Samples, decode, decode_reader,
};
use std::{
    fs,
    io::{self, Cursor, Read},
    path::Path,
};
#[allow(dead_code)]
#[path = "support/apng_recovery.rs"]
mod apng_recovery;
#[allow(dead_code)]
#[path = "support/chunks.rs"]
mod chunks;
#[path = "support/compositor.rs"]
mod oracle;

fn control(width: u32, height: u32) -> FrameControl {
    FrameControl {
        width,
        height,
        x_offset: 0,
        y_offset: 0,
        delay_num: 0,
        delay_den: 0,
        dispose: DisposeOp::None,
        blend: BlendOp::Source,
    }
}
fn image(values: Vec<u16>) -> Image {
    Image {
        width: (values.len() / 4) as u32,
        height: 1,
        color_type: ColorType::Rgba,
        bit_depth: 16,
        samples: Samples::U16(values),
    }
}

#[test]
fn every_palette_size_transparency_length_and_expansion_boundary_matches_model() {
    for depth in [1, 2, 4, 8] {
        for palette_len in 1..=1u32 << depth {
            for width in [1, 17, 255, 256, 257] {
                let mut png = Png::new(Image {
                    width,
                    height: 1,
                    color_type: ColorType::Indexed,
                    bit_depth: depth,
                    samples: Samples::U8(
                        (0..width)
                            .map(|n| ((n * 73 + 19) & ((1 << depth) - 1)) as u8)
                            .collect(),
                    ),
                });
                png.palette = (0..palette_len)
                    .map(|n| [n as u8, (n * 71 + 19) as u8, (n * 113 + 27) as u8])
                    .collect();
                for length in [0, 1, palette_len / 2, palette_len] {
                    png.transparency = Some(deadlib_image_png::Transparency::Palette(
                        (0..length).map(|n| (n * 173) as u8).collect(),
                    ));
                    let expected: Vec<_> = oracle::expand(&png, &png.image)
                        .into_iter()
                        .flatten()
                        .collect();
                    let limits = Limits {
                        max_output_bytes: width as usize * 8,
                        ..Limits::default()
                    };
                    assert_eq!(
                        png.to_rgba16(limits).unwrap().samples,
                        Samples::U16(expected)
                    );
                    assert_eq!(
                        png.to_rgba16(Limits {
                            max_output_bytes: limits.max_output_bytes - 1,
                            ..limits
                        }),
                        Err(Error::LimitExceeded)
                    );
                }
            }
        }
    }
}

#[test]
fn opaque_white_blends_stay_white_at_every_source_alpha() {
    let mut png = Png::new(image(vec![65535; 65536 * 4]));
    png.animation = Some(Animation {
        plays: 0,
        default_is_frame: true,
        frames: vec![
            Frame {
                control: control(65536, 1),
                image: None,
            },
            Frame {
                control: FrameControl {
                    blend: BlendOp::Over,
                    ..control(65536, 1)
                },
                image: Some(image(
                    (0..=u16::MAX)
                        .flat_map(|a| [65535, 65535, 65535, a])
                        .collect(),
                )),
            },
        ],
    });
    let mut compositor = png.compositor(Limits::default()).unwrap();
    for _ in 0..2 {
        assert!(
            compositor
                .next_frame()
                .unwrap()
                .unwrap()
                .samples
                .iter()
                .all(|&v| v == 65535)
        );
    }
}

#[test]
fn native_rgba_source_rows_preserve_offsets_transparent_rgb_and_disposal() {
    for depth in [8, 16] {
        for width in [1, 3, 17, 257] {
            let make = |w: u32, h: u32, salt: u16| {
                let max = if depth == 16 { 65535 } else { 255 };
                let values: Vec<u16> = (0..w as usize * h as usize * 4)
                    .map(|n| {
                        if n % 4 == 3 {
                            [0, 1, max / 2, max][n / 4 % 4]
                        } else {
                            (n as u16).wrapping_mul(913) ^ salt
                        }
                    })
                    .collect();
                Image {
                    width: w,
                    height: h,
                    color_type: ColorType::Rgba,
                    bit_depth: depth,
                    samples: if depth == 16 {
                        Samples::U16(values)
                    } else {
                        Samples::U8(values.into_iter().map(|v| v as u8).collect())
                    },
                }
            };
            for dispose in [DisposeOp::None, DisposeOp::Background, DisposeOp::Previous] {
                let mut png = Png::new(make(width + 4, 7, 19));
                png.animation = Some(Animation {
                    plays: 0,
                    default_is_frame: true,
                    frames: vec![
                        Frame {
                            control: control(width + 4, 7),
                            image: None,
                        },
                        Frame {
                            control: FrameControl {
                                x_offset: 2,
                                y_offset: 1,
                                dispose,
                                ..control(width, 3)
                            },
                            image: Some(make(width, 3, 71)),
                        },
                        Frame {
                            control: FrameControl {
                                x_offset: 1,
                                y_offset: 4,
                                dispose: DisposeOp::Previous,
                                ..control(width, 2)
                            },
                            image: Some(make(width, 2, 137)),
                        },
                        Frame {
                            control: FrameControl {
                                x_offset: width + 3,
                                y_offset: 6,
                                ..control(1, 1)
                            },
                            image: Some(make(1, 1, 227)),
                        },
                    ],
                });
                let expected = oracle::render(&png);
                let mut compositor = png.compositor(Limits::default()).unwrap();
                for frame in &expected {
                    assert_eq!(compositor.next_frame().unwrap().unwrap().samples, frame);
                }
                assert!(compositor.next_frame().unwrap().is_none());
                compositor.reset();
                assert_eq!(
                    compositor.next_frame().unwrap().unwrap().samples,
                    expected[0]
                );
            }
        }
    }
}

#[test]
fn every_source_alpha_at_fifteen_destination_boundaries_matches_exact_fractions() {
    let mut checked = 0;
    for da in [
        0, 1, 2, 127, 128, 255, 256, 257, 32767, 32768, 32769, 65279, 65533, 65534, 65535,
    ] {
        let mut background = Vec::new();
        let mut foreground = Vec::new();
        for sa in 0..=u16::MAX {
            background.extend([sa.rotate_left(3), !sa, sa.wrapping_mul(379), da]);
            foreground.extend([sa.wrapping_mul(913), sa.rotate_left(7), 65535 - sa, sa]);
        }
        let mut png = Png::new(image(background.clone()));
        png.animation = Some(Animation {
            plays: 0,
            default_is_frame: true,
            frames: vec![
                Frame {
                    control: control(65536, 1),
                    image: None,
                },
                Frame {
                    control: FrameControl {
                        blend: BlendOp::Over,
                        ..control(65536, 1)
                    },
                    image: Some(image(foreground.clone())),
                },
            ],
        });
        let mut renderer = png.compositor(Limits::default()).unwrap();
        assert_eq!(renderer.next_frame().unwrap().unwrap().samples, background);
        let result = renderer.next_frame().unwrap().unwrap();
        for ((actual, source), dest) in result
            .samples
            .as_chunks::<4>()
            .0
            .iter()
            .zip(foreground.as_chunks::<4>().0)
            .zip(background.as_chunks::<4>().0)
        {
            assert_eq!(
                *actual,
                oracle::over(*source, *dest),
                "source alpha={} destination alpha={da}",
                source[3]
            );
            checked += 1;
        }
        assert!(renderer.next_frame().unwrap().is_none());
    }
    assert_eq!(checked, 983040);
}

struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let n = out.len().min(self.0.len()).min(self.1);
        out[..n].copy_from_slice(&self.0[..n]);
        self.0 = &self.0[n..];
        Ok(n)
    }
}
fn render(png: &Png, answers: &[Vec<u16>]) {
    let mut renderer = png.compositor(Limits::default()).unwrap();
    for _ in 0..2 {
        for (frame, answer) in png.animation.as_ref().unwrap().frames.iter().zip(answers) {
            let result = renderer.next_frame().unwrap().unwrap();
            assert_eq!(
                (result.width, result.height),
                (png.image.width, png.image.height)
            );
            assert_eq!(result.control, frame.control);
            assert_eq!(result.samples, answer);
        }
        assert!(renderer.next_frame().unwrap().is_none());
        assert!(renderer.next_frame().unwrap().is_none());
        renderer.reset();
    }
}
fn rust_frame(reader: &mut png::Reader<Cursor<&[u8]>>, image: &Image) {
    let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(
        (
            info.width,
            info.height,
            info.bit_depth as u8,
            info.color_type as u8
        ),
        (
            image.width,
            image.height,
            image.bit_depth,
            image.color_type as u8
        )
    );
    let stride = if reader.info().interlaced {
        reader.output_line_size(reader.info().width).unwrap()
    } else {
        info.line_size
    };
    let samples: Vec<u16> = match &image.samples {
        Samples::U8(v) => v.iter().map(|&v| u16::from(v)).collect(),
        Samples::U16(v) => v.clone(),
    };
    let per_row = image.width as usize * image.color_type.channels();
    for y in 0..image.height as usize {
        for n in 0..per_row {
            let actual = match image.bit_depth {
                16 => {
                    u16::from_be_bytes([bytes[y * stride + 2 * n], bytes[y * stride + 2 * n + 1]])
                }
                8 => u16::from(bytes[y * stride + n]),
                d => u16::from(
                    (bytes[y * stride + n * d as usize / 8]
                        >> (8 - d - (n * d as usize % 8) as u8))
                        & ((1 << d) - 1),
                ),
            };
            assert_eq!(actual, samples[y * per_row + n]);
        }
    }
}

#[test]
fn all_native_formats_disposal_geometry_default_roles_and_adam7_match_rational_model() {
    let mut count = 0;
    let mut states = std::collections::BTreeSet::new();
    let mut transitions = std::collections::BTreeSet::new();
    for (name, case) in oracle::fixtures() {
        let expected = oracle::render(&case.png);
        let bytes = case.wire();
        let actual = decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(actual, case.png, "{name}");
        assert_eq!(
            decode_reader(Fragments(&bytes, 1), &Default::default()).unwrap(),
            case.png
        );
        assert_eq!(
            decode_reader(Fragments(&bytes, 7), &Default::default()).unwrap(),
            case.png
        );
        render(&case.png, &expected);
        render(&actual, &expected);
        let expanded = case.png.to_rgba16(Limits::default()).unwrap();
        assert_eq!(
            expanded.samples,
            Samples::U16(
                oracle::expand(&case.png, &case.png.image)
                    .into_iter()
                    .flatten()
                    .collect()
            )
        );
        let frames = &case.png.animation.as_ref().unwrap().frames;
        for f in frames {
            states.insert((f.control.dispose as u8, f.control.blend as u8));
        }
        for pair in frames.windows(2) {
            transitions.insert((pair[0].control.dispose as u8, pair[1].control.dispose as u8));
        }
        let mut reference = png::Decoder::new(Cursor::new(bytes.as_slice()))
            .read_info()
            .unwrap();
        rust_frame(&mut reference, &case.png.image);
        for f in &frames[usize::from(case.png.animation.as_ref().unwrap().default_is_frame)..] {
            rust_frame(&mut reference, f.image.as_ref().unwrap());
        }
        reference.finish().unwrap();
        count += 1;
    }
    assert_eq!((count, states.len(), transitions.len()), (240, 6, 9));
    println!(
        "High-precision APNG: {count} independent wire files, 1920 displayed canvases, all six disposal/blend states and nine disposal transitions"
    );
}

#[test]
fn exact_canvas_and_previous_snapshot_limits_fail_before_advancing_and_reset_cleanly() {
    for (_, case) in oracle::fixtures() {
        let canvas = case.png.image.width as usize * case.png.image.height as usize * 8;
        let frames = &case.png.animation.as_ref().unwrap().frames;
        let snapshot = frames
            .iter()
            .enumerate()
            .filter(|(i, f)| *i != 0 && f.control.dispose == DisposeOp::Previous)
            .map(|(_, f)| f.control.width as usize * f.control.height as usize * 8)
            .max()
            .unwrap_or(0);
        let expected = oracle::render(&case.png);
        for max_output_bytes in [canvas - 1, canvas, canvas + snapshot - 1, canvas + snapshot] {
            let limits = Limits {
                max_output_bytes,
                ..Default::default()
            };
            if max_output_bytes < canvas {
                assert!(matches!(
                    case.png.compositor(limits),
                    Err(Error::LimitExceeded)
                ));
                assert_eq!(case.png.to_rgba16(limits), Err(Error::LimitExceeded));
                continue;
            }
            let mut renderer = case.png.compositor(limits).unwrap();
            for _ in 0..2 {
                let mut stopped = false;
                for (i, f) in frames.iter().enumerate() {
                    if i != 0
                        && f.control.dispose == DisposeOp::Previous
                        && canvas + f.control.width as usize * f.control.height as usize * 8
                            > max_output_bytes
                    {
                        assert!(matches!(renderer.next_frame(), Err(Error::LimitExceeded)));
                        assert!(matches!(renderer.next_frame(), Err(Error::LimitExceeded)));
                        stopped = true;
                        break;
                    }
                    assert_eq!(renderer.next_frame().unwrap().unwrap().samples, expected[i]);
                }
                if !stopped {
                    assert!(renderer.next_frame().unwrap().is_none());
                }
                renderer.reset();
            }
        }
    }
}

fn golden_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compositor-precision")
}
fn inflate(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    flate2::read::ZlibDecoder::new(fs::read(path).unwrap().as_slice())
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}
fn word(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes[..4].try_into().unwrap())
}

#[test]
fn independent_libpng_and_python_fraction_canvases_retain_all_sixteen_bits() {
    let root = golden_root();
    let mut count = 0;
    for (name, case) in oracle::fixtures() {
        let bytes = fs::read(root.join(format!("{name}.png"))).unwrap();
        assert_eq!(bytes, case.wire());
        let answer = inflate(&root.join(format!("{name}.rgba16.z")));
        assert_eq!(&answer[..8], b"DLAPNGR1");
        let animation = case.png.animation.as_ref().unwrap();
        assert_eq!(
            [
                word(&answer[8..]),
                word(&answer[12..]),
                word(&answer[16..]),
                word(&answer[20..]),
                word(&answer[24..]),
                word(&answer[28..])
            ],
            [
                case.png.image.width,
                case.png.image.height,
                1,
                u32::from(animation.default_is_frame),
                animation.plays,
                animation.frames.len() as u32
            ]
        );
        let samples = case.png.image.width as usize * case.png.image.height as usize * 4;
        assert_eq!(
            answer.len(),
            32 + (22 + samples * 2) * animation.frames.len()
        );
        let expected: Vec<Vec<u16>> = answer[32..]
            .chunks_exact(22 + samples * 2)
            .map(|record| {
                record[22..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| u16::from_be_bytes([v[0], v[1]]))
                    .collect()
            })
            .collect();
        assert_eq!(oracle::render(&case.png), expected, "{name}");
        render(&decode(&bytes).unwrap(), &expected);
        render(
            &decode_reader(Fragments(&bytes, 1), &Default::default()).unwrap(),
            &expected,
        );
        count += animation.frames.len();
    }
    assert_eq!(count, 1920);
}

#[test]
#[ignore = "requires DEADLIB_LIBPNG_ORACLE and Python 3"]
fn regenerate_precision_goldens_from_independent_c_frames_and_python_fractions() {
    let output = std::env::temp_dir().join(format!(
        "deadlib-compositor-goldens-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = std::process::Command::new(
        std::env::var_os("DEADLIB_PYTHON").unwrap_or_else(|| "python".into()),
    )
    .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/generate_compositor_goldens.py"))
    .arg("--input")
    .arg(golden_root())
    .arg("--oracle")
    .arg(std::env::var_os("DEADLIB_LIBPNG_ORACLE").expect("DEADLIB_LIBPNG_ORACLE"))
    .arg("--output")
    .arg(&output)
    .output()
    .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for (name, _) in oracle::fixtures() {
        let file = format!("{name}.rgba16.z");
        assert_eq!(
            inflate(&output.join(&file)),
            inflate(&golden_root().join(file))
        );
    }
    println!(
        "Regenerated 240 independent libpng/Fraction precision animations at {}",
        output.display()
    );
}
