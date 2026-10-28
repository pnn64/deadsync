//! Author hierarchical fixtures and pre-entropy expectations without a decoder.
use deadlib_image_jpeg::{
    CodingOptions, CoefficientComponent, ComponentPlane, ComponentSpec, EntropyCoding, Expansion,
    FrameHeader, HierarchicalData, HierarchicalFrame, HierarchicalFrameOptions, HierarchicalImage,
    HierarchicalOptions, HierarchyHeader, Limits, LosslessCoding, LosslessOptions, Process,
};
use std::{io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn word(out: &mut impl Write, n: usize) -> std::io::Result<()> {
    out.write_all(&(n as u32).to_le_bytes())
}
fn sample_dump(path: &Path, frame: &FrameHeader, planes: &[ComponentPlane]) -> Result<()> {
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
fn frame_dump(path: &Path, frame: &HierarchicalFrame) -> Result<()> {
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
fn initial(frame: FrameHeader, data: HierarchicalData, expand: bool) -> HierarchicalFrame {
    HierarchicalFrame {
        frame,
        data,
        expansion: Expansion {
            horizontal: expand,
            vertical: expand,
        },
        offset: 0,
        first_scan: 0,
        scans: 1,
    }
}
fn save(root: &Path, name: &str, image: &HierarchicalImage<'_>, bytes: &[u8]) -> Result<()> {
    std::fs::write(root.join(format!("{name}.jpg")), bytes)?;
    for (i, frame) in image.frames.iter().enumerate() {
        let suffix = if frame.frame.process.is_lossless() {
            "samples"
        } else {
            "coef"
        };
        frame_dump(&root.join(format!("{name}.frame-{i}.{suffix}")), frame)?;
    }
    Ok(())
}
fn expanded(c: &ComponentPlane, x: usize, y: usize) -> u16 {
    let (sx, sy) = (x / 2, y / 2);
    let get = |xx: usize, yy: usize| {
        u32::from(c.samples[yy.min(c.height - 1) * c.width + xx.min(c.width - 1)])
    };
    let horizontal = |yy| {
        if x.is_multiple_of(2) {
            get(sx, yy)
        } else {
            (get(sx, yy) + get(sx + 1, yy)) / 2
        }
    };
    if y.is_multiple_of(2) {
        horizontal(sy) as u16
    } else {
        ((horizontal(sy) + horizontal(sy + 1)) / 2) as u16
    }
}
fn lossless(root: &Path, p: u8, subsampled: bool, point: u8) -> Result<()> {
    let count = if subsampled { 3 } else { 1 };
    let maximum = (1u32 << p) - 1;
    let mut specs = vec![];
    for i in 0..count {
        specs.push(ComponentSpec {
            id: (i + 1) as u8,
            horizontal_sampling: if subsampled && i == 0 { 2 } else { 1 },
            vertical_sampling: if subsampled && i == 0 { 2 } else { 1 },
            quantization_table: 0,
        });
    }
    let make_header = |process, w, h| FrameHeader {
        process,
        precision: p,
        width: w,
        height: h,
        components: specs.clone(),
    };
    let (max_h, max_v) = if subsampled { (2, 2) } else { (1, 1) };
    let mut base = vec![];
    let mut residual = vec![];
    let mut expected = vec![];
    for (i, &spec) in specs.iter().enumerate() {
        let width = (9 * spec.horizontal_sampling as usize).div_ceil(max_h);
        let height = (12 * spec.vertical_sampling as usize).div_ceil(max_v);
        let samples = (0..width * height)
            .map(|j| ((j * 7319 + j * j * 117 + i * 911) as u32 & maximum) as u16)
            .collect();
        let plane = ComponentPlane {
            spec,
            width,
            height,
            samples,
        };
        let width = (17 * spec.horizontal_sampling as usize).div_ceil(max_h);
        let height = (24 * spec.vertical_sampling as usize).div_ceil(max_v);
        let mut differences = vec![];
        let mut targets = vec![];
        for y in 0..height {
            for x in 0..width {
                let reference = expanded(&plane, x, y);
                let target = ((x * 433 + y * 197 + x * y * 71 + i * 1291) as u32 & maximum) as u16;
                let delta = target.wrapping_sub(reference);
                differences.push(delta);
                let transformed =
                    ((i32::from(delta as i16) / (1i32 << point)) * (1i32 << point)) as u16;
                targets.push(reference.wrapping_add(transformed));
            }
        }
        base.push(plane);
        residual.push(ComponentPlane {
            spec,
            width,
            height,
            samples: differences,
        });
        expected.push(ComponentPlane {
            spec,
            width,
            height,
            samples: targets,
        });
    }
    let base_header = make_header(Process::Lossless, 9, 12);
    let final_header = make_header(Process::DifferentialLossless, 17, 24);
    let base_coding = vec![
        LosslessCoding {
            predictor: 1,
            point_transform: 0
        };
        count
    ];
    let diff_coding = vec![
        LosslessCoding {
            predictor: 0,
            point_transform: point
        };
        count
    ];
    let frames = vec![
        initial(
            base_header,
            HierarchicalData::Lossless {
                components: base,
                coding: base_coding,
            },
            false,
        ),
        initial(
            final_header.clone(),
            HierarchicalData::Lossless {
                components: residual,
                coding: diff_coding,
            },
            true,
        ),
    ];
    let mut image = HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: p,
            width: 17,
            height: 24,
            components: specs,
        },
        frames,
        vec![],
        Limits::default(),
    )?;
    let controls = [
        HierarchicalFrameOptions::Lossless(LosslessOptions {
            entropy_coding: EntropyCoding::Arithmetic,
            predictor: 4,
            ..LosslessOptions::default()
        }),
        HierarchicalFrameOptions::Lossless(LosslessOptions {
            entropy_coding: EntropyCoding::Arithmetic,
            point_transform: point,
            ..LosslessOptions::default()
        }),
    ];
    let encoded = image.encode(HierarchicalOptions {
        frame_options: &controls,
        ..HierarchicalOptions::default()
    })?;
    // Expected entropy-domain residuals are computed from the original integers
    // and the explicit signed Pt equation, never by decoding encoded bytes.
    if let HierarchicalData::Lossless { components, .. } = &mut image.frames[1].data {
        for c in components {
            for v in &mut c.samples {
                *v = ((i32::from(*v as i16) / (1i32 << point)) * (1i32 << point)) as u16;
            }
        }
    }
    let name = format!(
        "native-lossless-p{p}-{}-t{point}",
        if subsampled { "420" } else { "gray" }
    );
    save(root, &name, &image, &encoded)?;
    sample_dump(
        &root.join(format!("{name}.expected.samples")),
        &final_header,
        &expected,
    )?;
    Ok(())
}
fn dct(root: &Path, p: u8) -> Result<()> {
    let spec = ComponentSpec {
        id: 1,
        horizontal_sampling: 1,
        vertical_sampling: 1,
        quantization_table: 0,
    };
    let mut frames = vec![];
    for differential in [false, true] {
        let frame = FrameHeader {
            process: if differential {
                Process::DifferentialSequential
            } else {
                Process::ExtendedSequential
            },
            precision: p,
            width: 24,
            height: 16,
            components: vec![spec],
        };
        let dc = if p == 8 { 1023 } else { 16383 };
        let ac = if p == 8 {
            if differential { 2047 } else { 1023 }
        } else if differential {
            32767
        } else {
            16383
        };
        let mut blocks = vec![[0; 64]; 6];
        for (i, block) in blocks.iter_mut().enumerate() {
            let sign = if i.is_multiple_of(2) { 1 } else { -1 };
            block[0] = dc * sign;
            block[1] = ac * sign;
            block[8] = 3 * sign;
            block[26] = ac * sign;
            block[63] = -7 * sign;
        }
        let component = CoefficientComponent {
            spec,
            width: 24,
            height: 16,
            blocks_wide: 3,
            blocks_high: 2,
            quantization: [if differential { 2 } else { 1 }; 64],
            successive_approximation: [0; 64],
            blocks,
        };
        frames.push(initial(
            frame,
            HierarchicalData::Dct(vec![component]),
            false,
        ));
    }
    let image = HierarchicalImage::from_frames(
        HierarchyHeader {
            precision: p,
            width: 24,
            height: 16,
            components: vec![spec],
        },
        frames,
        vec![],
        Limits::default(),
    )?;
    let controls = [HierarchicalFrameOptions::Dct(CodingOptions {
        entropy_coding: EntropyCoding::Arithmetic,
        ..CodingOptions::default()
    }); 2];
    let encoded = image.encode(HierarchicalOptions {
        frame_options: &controls,
        ..HierarchicalOptions::default()
    })?;
    save(
        root,
        &format!("native-dct-p{p}-magnitude-boundaries"),
        &image,
        &encoded,
    )
}
fn main() -> Result<()> {
    let root = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    std::fs::create_dir_all(&root)?;
    for p in 2..=16 {
        for subsampled in [false, true] {
            for point in [0, 1] {
                lossless(&root, p, subsampled, point)?;
            }
        }
    }
    for p in [8, 12] {
        dct(&root, p)?;
    }
    println!("Authored 62 native hierarchy cases with pre-entropy frame expectations.");
    Ok(())
}
