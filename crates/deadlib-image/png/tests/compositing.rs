use deadlib_image_png::{
    Animation, BlendOp, ColorType, DisposeOp, Frame, FrameControl, Image, Limits, Png, Samples,
    Transparency,
};
fn img(w: u32, v: &[u16]) -> Image {
    Image {
        width: w,
        height: 1,
        color_type: ColorType::Rgba,
        bit_depth: 16,
        samples: Samples::U16(v.to_vec()),
    }
}
fn frame(v: &[u16], x: u32, dispose: DisposeOp, blend: BlendOp) -> Frame {
    Frame {
        control: FrameControl {
            width: (v.len() / 4) as u32,
            height: 1,
            x_offset: x,
            y_offset: 0,
            delay_num: 1,
            delay_den: 0,
            dispose,
            blend,
        },
        image: Some(img((v.len() / 4) as u32, v)),
    }
}
#[test]
fn source_over_and_disposal_known_answers() {
    let blue = [0, 0, 65535, 65535];
    let red = [65535, 0, 0, 32768];
    let green = [0, 65535, 0, 65535];
    let mut png = Png::new(img(2, &[blue, blue].concat()));
    png.animation = Some(Animation {
        plays: 1,
        default_is_frame: true,
        frames: vec![
            Frame {
                control: FrameControl {
                    width: 2,
                    height: 1,
                    x_offset: 0,
                    y_offset: 0,
                    delay_num: 1,
                    delay_den: 0,
                    dispose: DisposeOp::None,
                    blend: BlendOp::Source,
                },
                image: None,
            },
            frame(&red, 0, DisposeOp::Previous, BlendOp::Over),
            frame(&green, 1, DisposeOp::Background, BlendOp::Source),
            frame(&[123, 456, 789, 0], 0, DisposeOp::None, BlendOp::Over),
        ],
    });
    let mut render = png.compositor(Limits::default()).unwrap();
    assert_eq!(
        render.next_frame().unwrap().unwrap().samples,
        [blue, blue].concat()
    );
    assert_eq!(
        render.next_frame().unwrap().unwrap().samples,
        [[32768, 0, 32767, 65535], blue].concat()
    );
    assert_eq!(
        render.next_frame().unwrap().unwrap().samples,
        [blue, green].concat()
    );
    assert_eq!(
        render.next_frame().unwrap().unwrap().samples,
        [blue, [0; 4]].concat()
    );
    assert!(render.next_frame().unwrap().is_none());
    render.reset();
    assert_eq!(
        render.next_frame().unwrap().unwrap().samples,
        [blue, blue].concat()
    );
}
#[test]
fn separate_default_and_first_previous() {
    let mut png = Png::new(img(2, &[65535; 8]));
    png.animation = Some(Animation {
        plays: 0,
        default_is_frame: false,
        frames: vec![
            frame(
                &[100, 200, 300, 65535],
                0,
                DisposeOp::Previous,
                BlendOp::Source,
            ),
            frame(&[123, 456, 789, 0], 1, DisposeOp::None, BlendOp::Over),
        ],
    });
    let mut render = png.compositor(Limits::default()).unwrap();
    assert_eq!(
        render.next_frame().unwrap().unwrap().samples,
        [100, 200, 300, 65535, 0, 0, 0, 0]
    );
    assert_eq!(render.next_frame().unwrap().unwrap().samples, [0; 8]);
    assert_eq!(
        png.animation.as_ref().unwrap().frames[0]
            .control
            .delay_seconds(),
        0.01
    );
}
#[test]
fn explicit_precision_palette_and_transparency_expansion() {
    let mut png = Png::new(Image {
        width: 4,
        height: 1,
        color_type: ColorType::Grayscale,
        bit_depth: 2,
        samples: Samples::U8(vec![0, 1, 2, 3]),
    });
    png.transparency = Some(Transparency::Grayscale(2));
    let out = png.to_rgba16(Limits::default()).unwrap();
    assert_eq!(
        out.samples,
        Samples::U16(vec![
            0, 0, 0, 65535, 21845, 21845, 21845, 65535, 43690, 43690, 43690, 0, 65535, 65535,
            65535, 65535
        ])
    );
    png.image.color_type = ColorType::Indexed;
    png.palette = vec![[0, 1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11]];
    png.transparency = Some(Transparency::Palette(vec![0, 127]));
    let Samples::U16(out) = png.to_rgba16(Limits::default()).unwrap().samples else {
        unreachable!()
    };
    assert_eq!(&out[..8], &[0, 257, 514, 0, 771, 1028, 1285, 32639]);
    assert_eq!(out[11], 65535);
}
