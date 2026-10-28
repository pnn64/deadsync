//! Stable binary output for the independent C coefficient oracle.
use deadlib_image_jpeg::{Limits, Upsampling, decode_coefficients};
use std::io::Write;

fn word(out: &mut impl Write, v: usize) -> std::io::Result<()> {
    out.write_all(&(v as u32).to_le_bytes())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = std::fs::read(args.next().ok_or("input JPEG required")?)?;
    let prefix = args.next().ok_or("output prefix required")?;
    let prefix = std::path::PathBuf::from(prefix);
    let image = decode_coefficients(&input, Limits::default())?;
    let mut out = std::fs::File::create(prefix.with_extension("coef"))?;
    out.write_all(b"JCO1")?;
    for v in [
        image.frame.precision as usize,
        image.frame.width as usize,
        image.frame.height as usize,
        image.components.len(),
    ] {
        word(&mut out, v)?;
    }
    for c in &image.components {
        let bw = c.width.div_ceil(8);
        let bh = c.height.div_ceil(8);
        for v in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
            bw,
            bh,
        ] {
            word(&mut out, v)?;
        }
        for q in c.quantization {
            out.write_all(&q.to_le_bytes())?;
        }
        for y in 0..bh {
            for x in 0..bw {
                for v in c.blocks[y * c.blocks_wide + x] {
                    out.write_all(&v.to_le_bytes())?;
                }
            }
        }
    }
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
