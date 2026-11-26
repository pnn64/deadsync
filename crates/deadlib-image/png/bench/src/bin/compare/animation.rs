use super::{Case, measure};
use deadlib_image_png::{Png, Samples};
use std::{
    io::{Cursor, Write},
    time::Duration,
};
use zune_core::{bytestream::ZCursor, options::DecoderOptions};

#[derive(Debug, PartialEq, Eq)]
struct Frame {
    // width, height, x/y offset, delay numerator/denominator, disposal, blend
    control: Option<[u32; 8]>,
    pixels: Vec<u8>,
}

fn pixels(image: &deadlib_image_png::Image) -> &[u8] {
    let Samples::U8(samples) = &image.samples else {
        unreachable!()
    };
    samples
}

fn rust_frames(bytes: &[u8]) -> Result<Vec<Frame>, png::DecodingError> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    let count = reader.info().animation_control.unwrap().num_frames;
    let separate_default = u32::from(reader.info().frame_control.is_none());
    let canvas_size = reader.output_buffer_size().unwrap();
    let mut frames = Vec::with_capacity((count + separate_default) as usize);
    let mut canvas = Vec::new();
    for _ in 0..count + separate_default {
        canvas.resize(canvas_size, 0);
        let info = reader.next_frame(&mut canvas)?;
        let pixels = if info.buffer_size() == canvas_size {
            std::mem::take(&mut canvas)
        } else {
            canvas[..info.buffer_size()].to_vec()
        };
        let control = reader.info().frame_control.map(|c| {
            [
                c.width,
                c.height,
                c.x_offset,
                c.y_offset,
                c.delay_num.into(),
                c.delay_den.into(),
                c.dispose_op as u32,
                c.blend_op as u32,
            ]
        });
        frames.push(Frame { control, pixels });
    }
    reader.finish()?;
    Ok(frames)
}

fn zune_frames(bytes: &[u8]) -> Result<Vec<Frame>, String> {
    let options = DecoderOptions::new_fast().png_set_decode_animated(true);
    let mut decoder = zune_png::PngDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(|err| format!("{err:?}"))?;
    let mut frames = Vec::new();
    while decoder.more_frames() {
        let c = decoder.frame_info().ok_or("Zune missing frame header")?;
        let pixels = decoder.decode_raw().map_err(|err| format!("{err:?}"))?;
        let control = c.is_part_of_seq.then_some([
            c.width as u32,
            c.height as u32,
            c.x_offset as u32,
            c.y_offset as u32,
            c.delay_num.into(),
            c.delay_denom.into(),
            c.dispose_op as u32,
            c.blend_op as u32,
        ]);
        frames.push(Frame { control, pixels });
    }
    Ok(frames)
}

fn native_matches(decoded: &Png, expected: &[Frame]) -> bool {
    let animation = decoded.animation.as_ref().unwrap();
    let mut expected = expected.iter();
    if !animation.default_is_frame
        && expected
            .next()
            .is_none_or(|first| first.control.is_some() || first.pixels != pixels(&decoded.image))
    {
        return false;
    }
    animation.frames.len() == expected.len()
        && animation.frames.iter().zip(expected).all(|(frame, other)| {
            let c = frame.control;
            let control = [
                c.width,
                c.height,
                c.x_offset,
                c.y_offset,
                c.delay_num.into(),
                c.delay_den.into(),
                c.dispose as u32,
                c.blend as u32,
            ];
            other.control == Some(control)
                && other.pixels == pixels(frame.image.as_ref().unwrap_or(&decoded.image))
        })
}

fn rust_encode(decoded: &Png) -> Result<Vec<u8>, png::EncodingError> {
    let mut output = Vec::new();
    let animation = decoded.animation.as_ref().unwrap();
    let mut encoder = png::Encoder::new(&mut output, decoded.image.width, decoded.image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_filter(png::Filter::Adaptive);
    encoder.set_animated(animation.frames.len() as u32, animation.plays)?;
    encoder.set_sep_def_img(!animation.default_is_frame)?;
    let mut writer = encoder.write_header()?;
    if !animation.default_is_frame {
        writer.write_image_data(pixels(&decoded.image))?;
    }
    for frame in &animation.frames {
        let c = frame.control;
        writer.reset_frame_position()?;
        writer.set_frame_dimension(c.width, c.height)?;
        writer.set_frame_position(c.x_offset, c.y_offset)?;
        writer.set_frame_delay(c.delay_num, c.delay_den)?;
        writer.set_dispose_op(png::DisposeOp::from_u8(c.dispose as u8).unwrap())?;
        writer.set_blend_op(png::BlendOp::from_u8(c.blend as u8).unwrap())?;
        writer.write_image_data(pixels(frame.image.as_ref().unwrap_or(&decoded.image)))?;
    }
    writer.finish()?;
    Ok(output)
}

pub fn bench(case: &Case<'_>, index: usize, sample: Duration, out: &mut impl Write) {
    assert_eq!((case.color, case.depth, case.adam7), (6, 8, 0));
    let expected = rust_frames(case.bytes).unwrap();
    let original = deadlib_image_png::decode(case.bytes).unwrap();
    assert!(native_matches(&original, &expected), "{}", case.name);
    let size = expected.iter().map(|frame| frame.pixels.len()).sum();
    let sample_time = |measure: &mut dyn FnMut() -> (u64, u64)| {
        if sample.is_zero() { (0, 0) } else { measure() }
    };
    for pos in 0..3 {
        let id = (index + pos) % 3;
        let timing = match id {
            0 => Ok(sample_time(&mut || {
                measure(sample, || deadlib_image_png::decode(case.bytes).unwrap())
            })),
            1 => Ok(sample_time(&mut || {
                measure(sample, || rust_frames(case.bytes).unwrap())
            })),
            _ => zune_frames(case.bytes).and_then(|frames| {
                if frames == expected {
                    Ok(sample_time(&mut || {
                        measure(sample, || zune_frames(case.bytes).unwrap())
                    }))
                } else {
                    Err("frame controls or pixels differ".to_owned())
                }
            }),
        };
        case.row(
            out,
            "decode-apng",
            ["deadlib", "rust-png", "zune-png"][id],
            timing.map(|(ns, cycles)| (size, ns, cycles)),
        );
    }

    for pos in 0..3 {
        let id = (index + pos) % 3;
        let operation = || {
            if id == 2 {
                rust_encode(&original).map_err(|err| err.to_string())
            } else {
                let compression = if id == 0 {
                    deadlib_image_png::zlib::Compression::Fast
                } else {
                    deadlib_image_png::zlib::Compression::Rle
                };
                let mut output = Vec::new();
                deadlib_image_png::encode_writer(
                    &original,
                    &deadlib_image_png::EncodeOptions {
                        compression,
                        ..Default::default()
                    },
                    &mut output,
                )
                .map_err(|err| err.to_string())?;
                Ok(output)
            }
        };
        let timing = operation().and_then(|bytes| {
            if rust_frames(&bytes).map_err(|err| err.to_string())? != expected {
                return Err("frame controls or pixels differ after encoding".to_owned());
            }
            let size = bytes.len();
            drop(bytes);
            let (ns, cycles) = sample_time(&mut || measure(sample, || operation().unwrap()));
            Ok((size, ns, cycles))
        });
        case.row(
            out,
            "encode-apng",
            ["deadlib-fast", "deadlib-rle", "rust-png-fast"][id],
            timing,
        );
    }
}
