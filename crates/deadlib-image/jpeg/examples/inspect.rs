//! Stable binary output for the independent C coefficient oracle.
use deadlib_image_jpeg::{Limits, Upsampling, decode_coefficients};
use std::io::Write;
mod support;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = std::fs::read(args.next().ok_or("input JPEG required")?)?;
    let prefix = args.next().ok_or("output prefix required")?;
    let prefix = std::path::PathBuf::from(prefix);
    let image = decode_coefficients(&input, Limits::default())?;
    support::coefficients(&image, &prefix.with_extension("coef"))?;
    let planes = image.reconstruct()?;
    let mut out = std::fs::File::create(prefix.with_extension("planes"))?;
    for c in &planes.components {
        for &v in &c.samples {
            out.write_all(&v.to_le_bytes())?;
        }
    }
    if let Ok(pixels) = planes.to_image(Upsampling::Nearest) {
        let mut out = std::fs::File::create(prefix.with_extension("pixels"))?;
        match pixels.samples {
            deadlib_image::Samples::U8(v) => out.write_all(&v)?,
            deadlib_image::Samples::U16(v) => {
                for n in v {
                    out.write_all(&n.to_le_bytes())?;
                }
            }
        }
    }
    Ok(())
}
