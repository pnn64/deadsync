use crate::{Format, ImageResult};
use crate::{Use, decode_error, key_rgba, output_buffer, rgba};
use deadlib_image::RgbaImage;
use deadlib_image::{ColorType, Image, Samples};
use deadlib_image_png::{
    AnimationPolicy, DecodeOptions, Limits, Transparency, decode_default_reader,
};
use std::io::Read;

pub(crate) fn decode(
    input: impl Read,
    usage: Use,
    max_output_bytes: usize,
) -> ImageResult<RgbaImage> {
    let png = decode_default_reader(
        input,
        &DecodeOptions {
            animation_policy: AnimationPolicy::RecoverDefault,
            limits: Limits {
                max_output_bytes,
                ..Limits::default()
            },
            ..DecodeOptions::default()
        },
        max_output_bytes / 4,
    )
    .map_err(|error| decode_error(Format::Png, error))?;
    let Image {
        width,
        height,
        color_type,
        bit_depth,
        samples,
    } = png.image;
    let color_key = matches!(usage, Use::Texture { color_key: true });
    let mut image = match samples {
        Samples::U8(bytes) if color_type == ColorType::Rgba => rgba(width, height, bytes)?,
        Samples::U8(indices) if color_type == ColorType::Indexed => {
            let mut output = output_buffer(width, height)?;
            let mut colors = [[0, 0, 0, 255]; 256];
            let alpha = match &png.transparency {
                Some(Transparency::Palette(alpha)) => alpha.as_slice(),
                _ => &[],
            };
            for (index, (output, &[r, g, b])) in colors.iter_mut().zip(&png.palette).enumerate() {
                *output = [r, g, b, alpha.get(index).copied().unwrap_or(255)];
            }
            if color_key {
                // The native palette key changes the first matching entry.
                // Duplicate RGB entries remain distinct after the key.
                for pink in [[248, 0, 248, 255], [255, 0, 255, 255]] {
                    if let Some(color) = colors.iter_mut().find(|color| **color == pink) {
                        color[3] = 0;
                    }
                }
            }
            output.extend(
                indices
                    .into_iter()
                    .flat_map(|index| colors[usize::from(index)]),
            );
            return rgba(width, height, output);
        }
        Samples::U8(samples) => {
            let mut output = output_buffer(width, height)?;
            let max = (1u16 << bit_depth) - 1;
            let scale = 255 / max;
            expand_samples(&samples, &mut output, color_type, png.transparency, |v| {
                (u16::from(v) * scale) as u8
            });
            rgba(width, height, output)?
        }
        Samples::U16(samples) => {
            let mut output = output_buffer(width, height)?;
            // Gameplay texture conversion uses stb's high byte. Artwork
            // formerly passed through image's rounded full-range conversion.
            if usage == Use::Artwork {
                expand_samples(&samples, &mut output, color_type, png.transparency, |v| {
                    ((u32::from(v) + 128) / 257) as u8
                });
            } else {
                expand_samples(&samples, &mut output, color_type, png.transparency, |v| {
                    (v >> 8) as u8
                });
            }
            rgba(width, height, output)?
        }
    };
    if color_key {
        key_rgba(&mut image);
    }
    Ok(image)
}

fn expand_samples<T: Copy + Into<u16>>(
    samples: &[T],
    output: &mut Vec<u8>,
    color: ColorType,
    transparency: Option<Transparency>,
    scale: impl Fn(T) -> u8,
) {
    match (color, transparency) {
        (ColorType::Grayscale, Some(Transparency::Grayscale(key))) => {
            output.extend(samples.iter().copied().flat_map(|value| {
                let alpha = if value.into() == key { 0 } else { 255 };
                let gray = scale(value);
                [gray, gray, gray, alpha]
            }));
        }
        (ColorType::Grayscale, _) => {
            output.extend(samples.iter().copied().flat_map(|value| {
                let gray = scale(value);
                [gray, gray, gray, 255]
            }));
        }
        (ColorType::Rgb, Some(Transparency::Rgb(key))) => {
            output.extend(samples.as_chunks::<3>().0.iter().flat_map(|&rgb| {
                let alpha = if rgb.map(Into::into) == key { 0 } else { 255 };
                let [red, green, blue] = rgb.map(&scale);
                [red, green, blue, alpha]
            }));
        }
        (ColorType::Rgb, _) => {
            output.extend(samples.as_chunks::<3>().0.iter().flat_map(|&rgb| {
                let [red, green, blue] = rgb.map(&scale);
                [red, green, blue, 255]
            }));
        }
        (ColorType::GrayscaleAlpha, _) => {
            output.extend(
                samples
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .flat_map(|&[gray, alpha]| {
                        let gray = scale(gray);
                        [gray, gray, gray, scale(alpha)]
                    }),
            );
        }
        (ColorType::Rgba, _) => output.extend(samples.iter().copied().map(scale)),
        (ColorType::Indexed, _) => unreachable!("indexed samples are expanded separately"),
    }
}
