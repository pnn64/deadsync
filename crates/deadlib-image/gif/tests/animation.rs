use deadlib_image_gif::*;

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
fn consecutive_previous_first_previous_local_palette_clipping_and_timing() {
    let mut c = Compositor::new(&header()).unwrap();
    let first = frame(0, 0, 4, 3, 1, Disposal::Previous);
    c.render(&first).unwrap();
    let second = frame(2, 1, 4, 3, 2, Disposal::Previous);
    assert_eq!(
        c.render(&second).unwrap().rgba,
        rgba(&[0, 0, 0, 0, 0, 0, 2, 2, 0, 0, 2, 2], [0; 4])
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
