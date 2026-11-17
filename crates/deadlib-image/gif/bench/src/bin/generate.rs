//! Deterministic, independently encoded performance inputs. No external files.
use deadlib_image_gif as native;
use std::{borrow::Cow, fs, path::Path};

fn palette(colors: usize, shift: usize) -> native::Palette {
    (0..colors)
        .map(|i| {
            let i = i + shift;
            [i as u8, (i * 73) as u8, (i * 139) as u8]
        })
        .collect()
}

fn pixels(w: u16, h: u16, colors: usize, pattern: &str, mut seed: u32) -> Vec<u8> {
    (0..usize::from(w) * usize::from(h))
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            match pattern {
                "flat" => 0,
                "bands" => ((i / usize::from(w) / 7 + i % usize::from(w) / 11) % colors) as u8,
                "noise" => (seed as usize % colors) as u8,
                _ => unreachable!(),
            }
        })
        .collect()
}

fn reference(g: &native::Gif) -> Vec<u8> {
    let global: Vec<_> = g
        .header
        .global_palette
        .iter()
        .flatten()
        .flatten()
        .copied()
        .collect();
    let mut bytes = Vec::new();
    {
        let mut encoder =
            gif::Encoder::new(&mut bytes, g.header.width, g.header.height, &global).unwrap();
        if g.loop_count().is_some() {
            encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        }
        for frame in g.frames() {
            let control = frame.control.unwrap();
            // gif's encoder accepts wire-order pixels when the interlace flag
            // is set. Build them here independently of the native encoder.
            let mut wire = Vec::new();
            if frame.interlaced {
                for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                    for y in (start..usize::from(frame.height)).step_by(step) {
                        let w = usize::from(frame.width);
                        wire.extend_from_slice(&frame.indices[y * w..(y + 1) * w]);
                    }
                }
            }
            encoder
                .write_frame(&gif::Frame {
                    left: frame.left,
                    top: frame.top,
                    width: frame.width,
                    height: frame.height,
                    interlaced: frame.interlaced,
                    palette: frame
                        .local_palette
                        .as_ref()
                        .map(|p| p.iter().flatten().copied().collect()),
                    buffer: Cow::Borrowed(if frame.interlaced {
                        &wire
                    } else {
                        &frame.indices
                    }),
                    delay: control.delay,
                    transparent: control.transparent_index,
                    dispose: gif::DisposalMethod::from_u8(control.disposal as u8).unwrap(),
                    needs_user_input: control.user_input,
                })
                .unwrap();
        }
    }
    bytes
}

fn write(root: &Path, name: &str, g: &native::Gif, sub_blocks: u8) {
    let outputs = [
        (
            "native",
            native::encode(
                g,
                &native::EncodeOptions {
                    image_sub_block_size: sub_blocks,
                },
            )
            .unwrap(),
        ),
        ("reference", reference(g)),
    ];
    for (encoder, bytes) in outputs {
        let actual = native::decode(&bytes).unwrap();
        assert_eq!(actual.frames().count(), g.frames().count());
        for (a, b) in actual.frames().zip(g.frames()) {
            assert_eq!(
                (a.left, a.top, a.width, a.height),
                (b.left, b.top, b.width, b.height)
            );
            assert_eq!(a.indices, b.indices);
            assert_eq!(a.interlaced, b.interlaced);
            assert_eq!(a.local_palette, b.local_palette);
            assert_eq!(a.control, b.control);
        }
        let path = root.join(format!("{name}-{encoder}.gif"));
        fs::write(path, bytes).unwrap();
    }
}

fn main() {
    let root = std::env::args_os().nth(1).expect("output directory");
    let root = Path::new(&root);
    fs::create_dir_all(root).unwrap();
    for colors in [2, 4, 8, 16, 32, 64, 128, 256] {
        for (w, h, pattern, interlaced) in [
            (1, 1, "flat", false),
            (3, 2, "noise", false),
            (257, 193, "bands", false),
            (257, 193, "noise", false),
            (768, 512, "flat", false),
            (768, 512, "bands", false),
            (129, 73, "noise", true),
        ] {
            let mut g = native::Gif::new(native::Header::new(w, h));
            g.header.global_palette = Some(palette(colors, 0));
            let mut frame = native::Frame::new(w, h, pixels(w, h, colors, pattern, 0x5138));
            frame.interlaced = interlaced;
            frame.control = Some(native::GraphicControl {
                disposal: native::Disposal::Keep,
                delay: 7,
                ..Default::default()
            });
            g.blocks.push(native::Block::Frame(frame));
            let sub_blocks = if w == 3 {
                1
            } else if interlaced {
                7
            } else {
                255
            };
            write(
                root,
                &format!("p{colors}-{w}x{h}-{pattern}-i{}", u8::from(interlaced)),
                &g,
                sub_blocks,
            );
        }
        let mut g = native::Gif::new(native::Header::new(257, 193));
        g.header.global_palette = Some(palette(colors, 0));
        g.blocks
            .push(native::Block::Extension(native::Extension::looping(
                native::LoopCount::Infinite,
            )));
        for n in 0..12 {
            let (w, h) = if n % 4 == 0 { (257, 193) } else { (129, 73) };
            let mut frame = native::Frame::new(
                w,
                h,
                pixels(
                    w,
                    h,
                    colors,
                    if n % 2 == 0 { "bands" } else { "noise" },
                    371 + n,
                ),
            );
            frame.left = if w == 257 { 0 } else { (n * 7) as u16 };
            frame.top = if h == 193 { 0 } else { (n * 5) as u16 };
            frame.interlaced = n % 3 == 1;
            if n % 2 != 0 {
                frame.local_palette = Some(palette(colors, n as usize * 11));
            }
            frame.control = Some(native::GraphicControl {
                disposal: [
                    native::Disposal::Keep,
                    native::Disposal::Background,
                    native::Disposal::Previous,
                ][n as usize % 3],
                delay: (n * 3) as u16,
                transparent_index: Some(0),
                user_input: n % 4 == 0,
            });
            g.blocks.push(native::Block::Frame(frame));
        }
        write(root, &format!("p{colors}-animation"), &g, 255);
    }
}
