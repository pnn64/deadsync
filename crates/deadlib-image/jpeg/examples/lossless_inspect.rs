//! Exact native planar output for the independent lossless sample oracle.
use deadlib_image_jpeg::{Limits, decode_lossless};
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = std::fs::read(args.next().ok_or("input JPEG required")?)?;
    let mut out = std::fs::File::create(args.next().ok_or("output samples required")?)?;
    let image = decode_lossless(&input, Limits::default())?;
    out.write_all(b"JLP1")?;
    for n in [
        image.frame.precision as u32,
        image.frame.width as u32,
        image.frame.height as u32,
        image.components.len() as u32,
    ] {
        out.write_all(&n.to_le_bytes())?;
    }
    for c in image.components {
        for n in [
            c.spec.id as u32,
            c.spec.horizontal_sampling as u32,
            c.spec.vertical_sampling as u32,
            c.width as u32,
            c.height as u32,
        ] {
            out.write_all(&n.to_le_bytes())?;
        }
        for v in c.samples {
            out.write_all(&v.to_le_bytes())?;
        }
    }
    Ok(())
}
