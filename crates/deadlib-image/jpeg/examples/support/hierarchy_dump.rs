//! Compact test-only pre-entropy frame and native sample serialization.
use deadlib_image_jpeg::{ComponentPlane, FrameHeader, HierarchicalData, HierarchicalFrame};
use std::{io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn word(out: &mut impl Write, n: usize) -> std::io::Result<()> {
    out.write_all(&(n as u32).to_le_bytes())
}
pub fn sample_dump(path: &Path, frame: &FrameHeader, planes: &[ComponentPlane]) -> Result<()> {
    let mut out = std::fs::File::create(path)?;
    out.write_all(b"JLP1")?;
    for n in [
        frame.precision as usize,
        frame.width as usize,
        frame.height as usize,
        planes.len(),
    ] {
        word(&mut out, n)?;
    }
    for c in planes {
        for n in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
        ] {
            word(&mut out, n)?;
        }
        for v in &c.samples {
            out.write_all(&v.to_le_bytes())?;
        }
    }
    Ok(())
}
pub fn frame_dump(path: &Path, frame: &HierarchicalFrame) -> Result<()> {
    match &frame.data {
        HierarchicalData::Lossless { components, .. } => {
            sample_dump(path, &frame.frame, components)
        }
        HierarchicalData::Dct(components) => {
            let mut out = std::fs::File::create(path)?;
            out.write_all(b"JCO1")?;
            for n in [
                frame.frame.precision as usize,
                frame.frame.width as usize,
                frame.frame.height as usize,
                components.len(),
            ] {
                word(&mut out, n)?;
            }
            for c in components {
                let (bw, bh) = (c.width.div_ceil(8), c.height.div_ceil(8));
                for n in [
                    c.spec.id as usize,
                    c.spec.horizontal_sampling as usize,
                    c.spec.vertical_sampling as usize,
                    c.width,
                    c.height,
                    bw,
                    bh,
                ] {
                    word(&mut out, n)?;
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
    }
}
