//! Native precision color output for independent pixel differential checks.
use deadlib_image_jpeg::{CmykPolarity, Limits, Upsampling, decode_coefficients};
use std::io::Write;
fn samples(path: &std::path::Path, data: impl IntoIterator<Item = u16>) -> std::io::Result<()> {
    let mut out = std::fs::File::create(path)?;
    for value in data {
        out.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: colors input prefix nearest|linear".into());
    }
    let bytes = std::fs::read(&args[0])?;
    let image = decode_coefficients(&bytes, Limits::default())?;
    let planes = image.reconstruct()?;
    let prefix = std::path::Path::new(&args[1]);
    let method = if args[2] == "nearest" {
        Upsampling::Nearest
    } else {
        Upsampling::Linear
    };
    let rgb = planes.to_image(method)?;
    let max = (1u32 << image.frame.precision) - 1;
    match rgb.samples {
        deadlib_image::Samples::U8(v) => {
            samples(&prefix.with_extension("rgb"), v.into_iter().map(u16::from))?
        }
        deadlib_image::Samples::U16(v) => samples(
            &prefix.with_extension("rgb"),
            v.into_iter()
                .map(|s| ((u32::from(s) * max + 32767) / 65535) as u16),
        )?,
    }
    if let Ok(mut cmyk) = planes.to_cmyk(method) {
        samples(&prefix.with_extension("cmyk"), cmyk.samples.iter().copied())?;
        cmyk.convert_polarity(CmykPolarity::Conventional)?;
        samples(&prefix.with_extension("ink"), cmyk.samples.iter().copied())?;
    }
    Ok(())
}
