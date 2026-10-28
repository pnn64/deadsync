//! Native hierarchical reference planes and a compact frame/scan summary.
use deadlib_image_jpeg::{
    ComponentPlane, FrameHeader, HierarchicalData, Limits, decode_hierarchical,
};
use std::io::Write;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let bytes = std::fs::read(args.next().ok_or("input JPEG required")?)?;
    let destination = std::path::PathBuf::from(args.next().ok_or("output JLP1 required")?);
    let mut output = std::fs::File::create(&destination)?;
    let raw = args.next().is_some_and(|v| v == "raw");
    let dump = args.next().is_some_and(|v| v == "frames");
    let image = decode_hierarchical(&bytes, Limits::default())?;
    let components = if raw {
        image.reconstruct_reference()?
    } else {
        image.reconstruct()?.components
    };
    output.write_all(b"JLP1")?;
    for v in [
        image.header.precision as u32,
        image.header.width as u32,
        image.header.height as u32,
        components.len() as u32,
    ] {
        output.write_all(&v.to_le_bytes())?;
    }
    for c in components {
        for v in [
            c.spec.id as u32,
            c.spec.horizontal_sampling as u32,
            c.spec.vertical_sampling as u32,
            c.width as u32,
            c.height as u32,
        ] {
            output.write_all(&v.to_le_bytes())?;
        }
        for v in c.samples {
            output.write_all(&v.to_le_bytes())?;
        }
    }
    for (i, f) in image.frames.iter().enumerate() {
        println!(
            "frame {i} SOF{:02x} {}x{} {} components EXP{}{} {} scans",
            f.frame.process as u8,
            f.frame.width,
            f.frame.height,
            f.frame.components.len(),
            u8::from(f.expansion.horizontal),
            u8::from(f.expansion.vertical),
            f.scans
        );
        if dump {
            match &f.data {
                HierarchicalData::Lossless { components, .. } => {
                    let mut out = std::fs::File::create(
                        destination.with_extension(format!("frame-{i}.samples")),
                    )?;
                    sample_planes(&mut out, &f.frame, components)?;
                }
                HierarchicalData::Dct(components) => {
                    let mut out = std::fs::File::create(
                        destination.with_extension(format!("frame-{i}.coef")),
                    )?;
                    out.write_all(b"JCO1")?;
                    for v in [
                        f.frame.precision as usize,
                        f.frame.width as usize,
                        f.frame.height as usize,
                        components.len(),
                    ] {
                        word(&mut out, v)?;
                    }
                    for c in components {
                        let (bw, bh) = (c.width.div_ceil(8), c.height.div_ceil(8));
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
                }
            }
        }
    }
    Ok(())
}
fn word(out: &mut std::fs::File, v: usize) -> std::io::Result<()> {
    out.write_all(&(v as u32).to_le_bytes())
}
fn sample_planes(
    out: &mut std::fs::File,
    frame: &FrameHeader,
    components: &[ComponentPlane],
) -> std::io::Result<()> {
    out.write_all(b"JLP1")?;
    for v in [
        frame.precision as usize,
        frame.width as usize,
        frame.height as usize,
        components.len(),
    ] {
        word(out, v)?;
    }
    for c in components {
        for v in [
            c.spec.id as usize,
            c.spec.horizontal_sampling as usize,
            c.spec.vertical_sampling as usize,
            c.width,
            c.height,
        ] {
            word(out, v)?;
        }
        for v in &c.samples {
            out.write_all(&v.to_le_bytes())?;
        }
    }
    Ok(())
}
