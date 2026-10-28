//! Synthetic coefficient extrema for independent entropy validation.
use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, EntropyCoding, SampleData, Subsampling,
    ZIGZAG, encode_coefficients, quantize,
};
mod support;

#[allow(clippy::too_many_arguments)]
fn case(
    root: &std::path::Path,
    precision: u8,
    space: ColorSpace,
    name: &str,
    width: u16,
    height: u16,
    wide: bool,
    dense: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let channels = match space {
        ColorSpace::Grayscale => 1,
        ColorSpace::Rgb => 3,
        _ => 4,
    };
    let pixels = vec![0u16; width as usize * height as usize * channels];
    let input = EncodeInput {
        width,
        height,
        precision,
        color_space: space,
        samples: SampleData::U16(&pixels),
    };
    let mut image = quantize(
        &input,
        EncodeOptions {
            precision,
            rgb_to_ycbcr: false,
            subsampling: if space == ColorSpace::Grayscale {
                Subsampling::S444
            } else {
                Subsampling::S420
            },
            ..EncodeOptions::default()
        },
    )?;
    let dc = i16::MAX;
    let ac = i16::MAX;
    for (component, c) in image.components.iter_mut().enumerate() {
        c.quantization.fill(if wide { 65535 } else { 1 });
        for (i, block) in c.blocks.iter_mut().enumerate() {
            block.fill(0);
            // Each DC difference stays within the signed 16-bit arithmetic model.
            block[0] = if dense {
                [-1, -dc - 1, -1, dc - 1][i % 4]
            } else {
                [0, dc, 0, -dc][i % 4]
            };
            for k in 1..64 {
                if dense || k == 1 || k == 63 {
                    let value = [ac, i16::MIN, 1, -1, 2, -2, 3, -3][(k + i + component) % 8];
                    block[ZIGZAG[k]] = value;
                }
            }
        }
    }
    let name = format!(
        "p{precision}-{name}-{width}x{height}-w{}-d{}",
        u8::from(wide),
        u8::from(dense)
    );
    support::coefficients(&image, &root.join(format!("{name}-expected.coef")))?;
    std::fs::write(
        root.join(format!("{name}.jpg")),
        encode_coefficients(
            &image,
            CodingOptions {
                entropy_coding: EntropyCoding::Arithmetic,
                ..CodingOptions::default()
            },
        )?,
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: arithmetic_fixtures output-directory")?,
    );
    std::fs::create_dir_all(&root)?;
    for precision in [8, 12] {
        for (space, name) in [
            (ColorSpace::Grayscale, "gray"),
            (ColorSpace::Rgb, "rgb"),
            (ColorSpace::Cmyk, "cmyk"),
            (ColorSpace::Ycck, "ycck"),
        ] {
            for wide in [false, true] {
                for dense in [false, true] {
                    case(&root, precision, space, name, 17, 17, wide, dense)?;
                }
            }
        }
        for (width, height) in [(1, 1), (1, 17), (17, 1)] {
            case(
                &root,
                precision,
                ColorSpace::Grayscale,
                "edge",
                width,
                height,
                false,
                true,
            )?;
        }
    }
    Ok(())
}
