use deadlib_image_jpeg::CoefficientImage;
use std::io::Write;

pub fn coefficients(image: &CoefficientImage<'_>, path: &std::path::Path) -> std::io::Result<()> {
    let mut out = std::fs::File::create(path)?;
    let word = |out: &mut std::fs::File, v: usize| out.write_all(&(v as u32).to_le_bytes());
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
    Ok(())
}
