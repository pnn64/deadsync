use deadlib_image_gif::*;

#[path = "../../../../tests/support/allocations.rs"]
mod alloc;

fn header() -> Header {
    let mut h = Header::new(4, 3);
    h.global_palette = Some(vec![[0, 0, 0], [255, 0, 0], [0, 255, 0], [0, 0, 255]]);
    h
}
fn frame(x: u16, y: u16, w: u16, h: u16, index: u8, disposal: Disposal) -> Frame {
    let mut f = Frame::new(w, h, vec![index; w as usize * h as usize]);
    f.left = x;
    f.top = y;
    f.control = Some(GraphicControl {
        disposal,
        ..Default::default()
    });
    f
}
fn rgba(indices: &[u8], background: [u8; 4]) -> Vec<u8> {
    let p = header().global_palette.unwrap();
    indices
        .iter()
        .flat_map(|&i| {
            if i == 4 {
                background
            } else {
                let c = p[i as usize];
                [c[0], c[1], c[2], 255]
            }
        })
        .collect()
}
#[test]
fn offsets_transparency_background_and_previous() {
    for background in [
        Background::LogicalScreen,
        Background::Transparent,
        Background::Color([5, 6, 7, 8]),
    ] {
        let color = match background {
            Background::LogicalScreen => [0, 0, 0, 255],
            Background::Transparent => [0; 4],
            Background::Color(c) => c,
        };
        let mut c = Compositor::with_options(
            &header(),
            CompositeOptions {
                background,
                ..Default::default()
            },
        )
        .unwrap();
        let a = frame(0, 0, 4, 3, 1, Disposal::Keep);
        assert_eq!(c.render(&a).unwrap().rgba, rgba(&[1; 12], color));
        let mut b = frame(1, 0, 2, 2, 2, Disposal::Previous);
        b.indices[1] = 0;
        b.control.as_mut().unwrap().transparent_index = Some(0);
        assert_eq!(
            c.render(&b).unwrap().rgba,
            rgba(&[1, 2, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1], color)
        );
        let d = frame(0, 2, 2, 1, 3, Disposal::Background);
        assert_eq!(
            c.render(&d).unwrap().rgba,
            rgba(&[1, 1, 1, 1, 1, 1, 1, 1, 3, 3, 1, 1], color)
        );
        let e = frame(3, 2, 1, 1, 2, Disposal::Previous);
        assert_eq!(
            c.render(&e).unwrap().rgba,
            rgba(&[1, 1, 1, 1, 1, 1, 1, 1, 4, 4, 1, 2], color)
        );
        let f = frame(0, 0, 1, 1, 3, Disposal::Keep);
        assert_eq!(
            c.render(&f).unwrap().rgba,
            rgba(&[3, 1, 1, 1, 1, 1, 1, 1, 4, 4, 1, 1], color)
        );
        c.reset();
        assert_eq!(c.pixels(), rgba(&[4; 12], color));
    }
}

#[test]
fn offscreen_render_validates_before_applying_pending_disposal() {
    let mut compositor = Compositor::new(&header()).unwrap();
    let visible = frame(0, 0, 4, 3, 1, Disposal::Previous);
    let drawn = rgba(&[1; 12], [0; 4]);
    assert_eq!(compositor.render(&visible).unwrap().rgba, drawn);

    let mut offscreen = frame(4, 0, 4, 3, 255, Disposal::Previous);
    assert!(matches!(
        compositor.render(&offscreen),
        Err(Error::Invalid("pixel index outside active palette"))
    ));
    assert_eq!(compositor.pixels(), drawn);
    offscreen.indices.fill(1);
    let rendered = compositor.render(&offscreen).unwrap();
    assert_eq!(rendered.rgba, rgba(&[0; 12], [0; 4]));
    assert_eq!(rendered.control, offscreen.control);
}

#[test]
fn consecutive_previous_first_previous_local_palette_clipping_and_timing() {
    let mut c = Compositor::new(&header()).unwrap();
    let first = frame(0, 0, 4, 3, 1, Disposal::Previous);
    c.render(&first).unwrap();
    let second = frame(2, 1, 4, 3, 2, Disposal::Previous);
    assert_eq!(
        c.render(&second).unwrap().rgba,
        rgba(&[0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 2, 2], [0; 4])
    );
    let same_clipped_rect = frame(2, 1, 2, 2, 3, Disposal::Previous);
    assert_eq!(
        c.render(&same_clipped_rect).unwrap().rgba,
        rgba(&[0, 0, 0, 0, 0, 0, 3, 3, 0, 0, 3, 3], [0; 4])
    );
    let mut third = frame(3, 2, 1, 1, 0, Disposal::Unspecified);
    third.local_palette = Some(vec![[23, 24, 25], [3, 4, 5]]);
    let c3 = third.control.as_mut().unwrap();
    c3.delay = 0;
    c3.user_input = true;
    let out = c.render(&third).unwrap();
    assert_eq!(out.control, third.control);
    assert_eq!(&out.rgba[..44], rgba(&[0; 11], [0; 4]));
    assert_eq!(&out.rgba[44..], [23, 24, 25, 255]);
    let off = frame(65535, 65535, 1, 1, 1, Disposal::Reserved7);
    assert_eq!(c.render(&off).unwrap().rgba[44..], [23, 24, 25, 255]);
}

struct Font;
impl TextRasterizer for Font {
    fn rasterize(&mut self, ch: u8, _w: u8, _h: u8, mask: &mut [u8]) -> Result<(), Error> {
        if ch != b' ' {
            mask[0] = 1;
        }
        Ok(())
    }
}
#[test]
fn repeated_text_uses_active_palette_without_allocation_churn() {
    for external in [false, true] {
        let palette = vec![[1, 2, 3], [4, 5, 6]];
        let mut h = Header::new(2, 1);
        let mut options = CompositeOptions::default();
        if external {
            options.external_palette = Some(palette);
        } else {
            h.global_palette = Some(palette);
        }
        let text = PlainText {
            left: 0,
            top: 0,
            width: 2,
            height: 1,
            cell_width: 1,
            cell_height: 1,
            foreground_index: 1,
            background_index: 0,
            control: Some(GraphicControl {
                disposal: Disposal::Previous,
                ..Default::default()
            }),
            data: vec![b"A ".to_vec()],
        };
        let mut c = Compositor::with_options(&h, options).unwrap();
        c.render_text(&text, &mut Font).unwrap();
        alloc::assert_no_churn(|| {
            for _ in 0..3 {
                assert_eq!(
                    c.render_text(&text, &mut Font).unwrap().rgba,
                    [4, 5, 6, 255, 1, 2, 3, 255]
                );
            }
        });
    }
}

#[test]
fn rasterizer_error_leaves_canvas_and_disposal_pending() {
    struct FailingFont;
    impl TextRasterizer for FailingFont {
        fn rasterize(&mut self, ch: u8, _w: u8, _h: u8, mask: &mut [u8]) -> Result<(), Error> {
            mask.fill(1);
            if ch == b'B' {
                return Err(Error::Invalid("font failed"));
            }
            Ok(())
        }
    }

    let mut compositor = Compositor::new(&header()).unwrap();
    compositor
        .render(&frame(0, 0, 4, 3, 1, Disposal::Previous))
        .unwrap();
    let text = PlainText {
        left: 3,
        top: 2,
        width: 4,
        height: 2,
        cell_width: 2,
        cell_height: 2,
        foreground_index: 2,
        background_index: 0,
        control: None,
        data: vec![b"A".to_vec(), b"B".to_vec()],
    };
    assert!(matches!(
        compositor.render_text(&text, &mut FailingFont),
        Err(Error::Invalid("font failed"))
    ));
    assert_eq!(compositor.pixels(), rgba(&[1; 12], [0; 4]));
    assert_eq!(
        compositor
            .render(&frame(0, 0, 1, 1, 3, Disposal::Keep))
            .unwrap()
            .rgba,
        rgba(&[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0; 4])
    );
}

#[test]
fn extensions_roundtrip_scope_and_text_rendering() {
    let mut g = Gif::new(header());
    g.blocks
        .push(Block::Extension(Extension::looping(LoopCount::Finite(7))));
    g.blocks.push(Block::Extension(Extension::Comment(vec![
        b"hello".to_vec(),
        vec![0, 255],
    ])));
    g.blocks.push(Block::Extension(Extension::Application {
        identifier: *b"MYAPP123",
        authentication: *b"xyz",
        data: vec![vec![3, 2, 1], vec![7]],
    }));
    g.blocks.push(Block::Extension(Extension::Unknown {
        label: 0x42,
        control: None,
        data: vec![vec![1], vec![2, 3]],
    }));
    let text = PlainText {
        left: 0,
        top: 0,
        width: 4,
        height: 2,
        cell_width: 2,
        cell_height: 1,
        foreground_index: 1,
        background_index: 2,
        control: Some(GraphicControl {
            disposal: Disposal::Previous,
            delay: 17,
            transparent_index: Some(2),
            user_input: true,
        }),
        data: vec![b"AB".to_vec(), b" Cextra".to_vec()],
    };
    g.blocks
        .push(Block::Extension(Extension::PlainText(text.clone())));
    g.blocks.push(Block::Frame(Frame::new(1, 1, vec![3])));
    let bytes = encode(&g, &EncodeOptions::default()).unwrap();
    assert_eq!(decode(&bytes).unwrap(), g);
    assert_eq!(g.loop_count(), Some(LoopCount::Finite(7)));
    // Insert a comment between the GCE and its text target: it cannot consume control.
    let pos = bytes.windows(3).position(|w| w == [0x21, 0xf9, 4]).unwrap();
    let mut reordered = bytes.clone();
    reordered.splice(pos + 8..pos + 8, [0x21, 0xfe, 1, b'x', 0]);
    let decoded = decode(&reordered).unwrap();
    let Block::Extension(Extension::PlainText(actual)) = &decoded.blocks[5] else {
        panic!()
    };
    assert_eq!(actual, &text);
    assert!(decoded.frames().next().unwrap().control.is_none());
    let mut c = Compositor::new(&g.header).unwrap();
    assert!(matches!(
        c.render_block(&g.blocks[4]),
        Err(Error::TextRendererRequired)
    ));
    assert_eq!(
        c.render_text(&text, &mut Font).unwrap().rgba,
        rgba(&[1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0], [0; 4])
    );
    let f = g.frames().next().unwrap();
    assert_eq!(
        c.render(f).unwrap().rgba,
        rgba(&[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0; 4])
    );
}

#[test]
fn missing_and_external_palette() {
    let h = Header::new(1, 1);
    let f = Frame::new(1, 1, vec![1]);
    let g = Gif {
        header: h.clone(),
        blocks: vec![Block::Frame(f.clone())],
    };
    let bytes = encode(&g, &EncodeOptions::default()).unwrap();
    assert_eq!(decode(&bytes).unwrap(), g);
    assert!(matches!(Compositor::new(&h), Err(Error::MissingPalette)));
    let mut c = Compositor::with_options(
        &h,
        CompositeOptions {
            external_palette: Some(vec![[1, 2, 3], [4, 5, 6]]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(c.render(&f).unwrap().rgba, [4, 5, 6, 255]);
    for (e, count) in [
        (Extension::looping(LoopCount::Infinite), LoopCount::Infinite),
        (
            Extension::Application {
                identifier: *b"ANIMEXTS",
                authentication: *b"1.0",
                data: vec![vec![1, 255, 255]],
            },
            LoopCount::Finite(65535),
        ),
    ] {
        assert_eq!(e.loop_count(), Some(count));
    }
}

#[test]
fn owned_canvas_reuses_pixels_and_keeps_the_current_display() {
    let mut compositor = Compositor::new(&header()).unwrap();
    compositor
        .render(&frame(1, 1, 2, 2, 2, Disposal::Background))
        .unwrap();
    let expected = compositor.pixels().to_vec();
    let pointer = compositor.pixels().as_ptr();
    let (image, churn) = alloc::measure(|| compositor.into_image());
    assert_eq!((churn.allocs, churn.reallocs), (0, 0));
    image.validate().unwrap();
    assert_eq!((image.width, image.height), (4, 3));
    assert_eq!((image.color_type, image.bit_depth), (ColorType::Rgba, 8));
    let Samples::U8(pixels) = image.samples else {
        panic!("RGBA8 canvas must have byte samples");
    };
    assert_eq!(pixels.as_ptr(), pointer);
    assert_eq!(pixels, expected);
}

#[test]
fn global_external_and_mutated_local_palettes_survive_reset() {
    for global in [false, true] {
        let mut header = Header::new(2, 1);
        if global {
            header.global_palette = Some(vec![[11, 12, 13], [21, 22, 23]]);
        }
        let mut compositor = Compositor::with_options(
            &header,
            CompositeOptions {
                background: Background::Transparent,
                external_palette: Some(vec![[31, 32, 33], [41, 42, 43]]),
                ..Default::default()
            },
        )
        .unwrap();
        let expected = if global {
            [11, 12, 13, 255, 21, 22, 23, 255]
        } else {
            [31, 32, 33, 255, 41, 42, 43, 255]
        };
        let mut frame = Frame::new(2, 1, vec![0, 1]);
        assert_eq!(compositor.render(&frame).unwrap().rgba, expected);

        frame.local_palette = Some(vec![[51, 52, 53], [61, 62, 63]]);
        assert_eq!(
            compositor.render(&frame).unwrap().rgba,
            [51, 52, 53, 255, 61, 62, 63, 255]
        );
        frame.local_palette.as_mut().unwrap()[1] = [71, 72, 73];
        assert_eq!(
            compositor.render(&frame).unwrap().rgba,
            [51, 52, 53, 255, 71, 72, 73, 255]
        );

        frame.local_palette = None;
        assert_eq!(compositor.render(&frame).unwrap().rgba, expected);
        compositor.reset();
        assert_eq!(compositor.pixels(), [0; 8]);
        assert_eq!(compositor.render(&frame).unwrap().rgba, expected);
    }
}

#[test]
fn uniform_and_mixed_rows_preserve_transparency_and_clipping() {
    for width in [1u16, 15, 16, 17, 31, 32, 33, 255, 256, 257] {
        for different in [None, Some(0), Some(width / 2), Some(width - 1)] {
            for (colors, transparent) in [
                (4, None),
                (4, Some(1)),
                (4, Some(2)),
                (2, None),
                (2, Some(0)),
                (2, Some(1)),
            ] {
                let mut h = header();
                h.global_palette.as_mut().unwrap().truncate(colors);
                h.width = width + 2;
                h.height = 4;
                let mut f = frame(2, 2, width + 3, 3, 1, Disposal::Keep);
                f.control.as_mut().unwrap().transparent_index = transparent;
                let stride = f.width as usize;
                f.indices[stride..2 * stride].fill(if colors == 2 { 0 } else { 2 });
                if let Some(column) = different {
                    f.indices[column as usize] = if colors == 2 { 0 } else { 3 };
                }
                let background = [5, 6, 7, 8];
                let mut expected = vec![background; h.width as usize * h.height as usize];
                for y in 2..h.height as usize {
                    for x in 2..h.width as usize {
                        let index = f.indices[(y - 2) * stride + x - 2];
                        if Some(index) != transparent {
                            let color = h.global_palette.as_ref().unwrap()[index as usize];
                            expected[y * h.width as usize + x] =
                                [color[0], color[1], color[2], 255];
                        }
                    }
                }
                let mut compositor = Compositor::with_options(
                    &h,
                    CompositeOptions {
                        background: Background::Color(background),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(compositor.render(&f).unwrap().rgba, expected.as_flattened());
            }
        }
    }
}

#[test]
fn deterministic_animation_against_independent_full_canvas_model() {
    let h = header();
    let mut c = Compositor::new(&h).unwrap();
    let p = h.global_palette.as_ref().unwrap();
    let mut expected = vec![[0, 0, 0, 255]; 12];
    let mut pending: Option<(Frame, Vec<[u8; 4]>)> = None;
    let mut seed = 1234567u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for n in 0..512 {
        if let Some((f, before)) = pending.take() {
            for y in 0..3 {
                for x in 0..4 {
                    if x >= f.left as usize
                        && x < f.left as usize + f.width as usize
                        && y >= f.top as usize
                        && y < f.top as usize + f.height as usize
                    {
                        match f.control.unwrap().disposal {
                            Disposal::Previous => expected[y * 4 + x] = before[y * 4 + x],
                            Disposal::Background => expected[y * 4 + x] = [0, 0, 0, 255],
                            _ => {}
                        }
                    }
                }
            }
        }
        let before = expected.clone();
        let disposal = [
            Disposal::Unspecified,
            Disposal::Keep,
            Disposal::Background,
            Disposal::Previous,
            Disposal::Reserved4,
            Disposal::Reserved5,
            Disposal::Reserved6,
            Disposal::Reserved7,
        ][n % 8];
        let mut f = frame(
            (next() % 6) as u16,
            (next() % 5) as u16,
            (next() % 5 + 1) as u16,
            (next() % 4 + 1) as u16,
            0,
            disposal,
        );
        for index in &mut f.indices {
            *index = (next() % 4) as u8;
        }
        if n % 3 == 0 {
            f.control.as_mut().unwrap().transparent_index = Some((next() % 4) as u8);
        }
        for y in 0..f.height as usize {
            for x in 0..f.width as usize {
                let sx = f.left as usize + x;
                let sy = f.top as usize + y;
                let index = f.indices[y * f.width as usize + x];
                if sx < 4 && sy < 3 && Some(index) != f.control.unwrap().transparent_index {
                    let color = p[index as usize];
                    expected[sy * 4 + sx] = [color[0], color[1], color[2], 255];
                }
            }
        }
        assert_eq!(
            c.render(&f).unwrap().rgba,
            expected.iter().flatten().copied().collect::<Vec<_>>(),
            "frame {n}"
        );
        pending = Some((f, before));
    }
}
