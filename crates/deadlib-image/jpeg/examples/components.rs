//! Test-only native many-component author and pre-entropy expectations.
use deadlib_image_jpeg::*;
use std::path::{Path, PathBuf};
#[path = "support/hierarchy_dump.rs"]
mod dump;
#[path = "../tests/support/components.rs"]
#[allow(dead_code)]
mod fixtures;
mod support;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn file(prefix: &Path, tail: &str) -> PathBuf {
    let mut p = prefix.as_os_str().to_os_string();
    p.push(tail);
    p.into()
}
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() != 13 {
        return Err("prefix count precision layout quantizers arithmetic fixed restart script dnl mode abbreviated progressive required".into());
    }
    let prefix = Path::new(&a[0]);
    let count: usize = a[1].parse()?;
    let p = a[2].parse()?;
    let layout = a[3].parse()?;
    let q = a[4].parse()?;
    let mut image = fixtures::model(count, p, layout, q);
    let mut script = vec![];
    if a[8] != "0" {
        let mut start = 0;
        while start < count {
            let mut end = start + 1;
            if a[8] == "2" && end < count {
                let blocks: usize = image.frame.components[start..=end]
                    .iter()
                    .map(|c| c.horizontal_sampling as usize * c.vertical_sampling as usize)
                    .sum();
                if blocks <= 10 {
                    end += 1;
                }
            }
            let ids: Vec<_> = image.frame.components[start..end]
                .iter()
                .map(|c| c.id)
                .collect();
            script.push(ScanSpec::new(&ids, 0, 63, 0, 0)?);
            start = end;
        }
        script.reverse();
    }
    let coding = CodingOptions {
        entropy_coding: if a[5] == "1" {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        },
        optimize_huffman: a[6] != "1",
        restart_interval: a[7].parse()?,
        scan_script: &script,
        progressive: a[12] == "1",
        successive_approximation: 3,
        height_declaration: if a[9] == "1" {
            HeightDeclaration::Dnl { initial_height: 0 }
        } else {
            HeightDeclaration::FrameHeader
        },
        ..CodingOptions::default()
    };
    let limits = Limits {
        max_scans: 2048,
        ..Limits::default()
    };
    let mut sender = AbbreviatedEncoder::new();
    let abbreviated = a[11] == "1";
    let (full, split) = if a[10] == "0" {
        support::coefficients(&image, &file(prefix, ".expected.frame-0.coef"))?;
        (
            encode_coefficients(&image, coding)?,
            if abbreviated {
                Some(sender.encode_coefficients(&image, coding, MetadataOptions::default())?)
            } else {
                None
            },
        )
    } else {
        let header = HierarchyHeader {
            precision: p,
            width: image.frame.width,
            height: image.frame.height,
            components: image.frame.components.clone(),
        };
        let (hierarchy, options) = if a[10] == "1" {
            let mut residual = image.components.clone();
            for c in &mut residual {
                for b in &mut c.blocks {
                    for v in b {
                        *v = -*v / 2;
                    }
                }
            }
            let mut frame = image.frame.clone();
            frame.process = Process::DifferentialSequential;
            let frames = vec![
                HierarchicalFrame {
                    frame: image.frame.clone(),
                    data: HierarchicalData::Dct(std::mem::take(&mut image.components)),
                    expansion: Expansion::default(),
                    offset: 0,
                    first_scan: 0,
                    scans: 1,
                },
                HierarchicalFrame {
                    frame,
                    data: HierarchicalData::Dct(residual),
                    expansion: Expansion::default(),
                    offset: 0,
                    first_scan: 0,
                    scans: 1,
                },
            ];
            (
                HierarchicalImage::from_frames(header, frames, vec![], limits)?,
                vec![HierarchicalFrameOptions::Dct(coding); 2],
            )
        } else {
            let planes: Vec<_> = image
                .components
                .iter()
                .enumerate()
                .map(|(i, c)| ComponentPlane {
                    spec: c.spec,
                    width: c.width,
                    height: c.height,
                    samples: (0..c.width * c.height)
                        .map(|j| ((i * 911 + j * 193) & ((1usize << p) - 1)) as u16)
                        .collect(),
                })
                .collect();
            let lossless = LosslessOptions {
                entropy_coding: coding.entropy_coding,
                restart_rows: coding.restart_interval,
                height_declaration: coding.height_declaration,
                ..LosslessOptions::default()
            };
            let mut levels = [
                PyramidLevel::dct(Expansion::default(), [90, 50, 10, 1][q], coding),
                PyramidLevel::dct(
                    Expansion {
                        horizontal: true,
                        vertical: true,
                    },
                    85,
                    coding,
                ),
                PyramidLevel::lossless(Expansion::default(), lossless),
            ];
            for level in &mut levels[..2] {
                level.baseline_quantization = q != 3;
            }
            let pyramid = build_hierarchical_planes(
                header,
                &planes,
                vec![],
                PyramidOptions {
                    limits,
                    ..PyramidOptions::new(&levels)
                },
            )?;
            let frame = FrameHeader {
                process: Process::Lossless,
                precision: p,
                width: pyramid.header.width,
                height: pyramid.header.height,
                components: pyramid.header.components.clone(),
            };
            dump::sample_dump(&file(prefix, ".source.samples"), &frame, &planes)?;
            (
                pyramid,
                vec![
                    HierarchicalFrameOptions::Dct(coding),
                    HierarchicalFrameOptions::Dct(coding),
                    HierarchicalFrameOptions::Lossless(lossless),
                ],
            )
        };
        for (i, frame) in hierarchy.frames.iter().enumerate() {
            dump::frame_dump(
                &file(
                    prefix,
                    &format!(
                        ".expected.frame-{i}.{}",
                        if frame.frame.process.is_lossless() {
                            "samples"
                        } else {
                            "coef"
                        }
                    ),
                ),
                frame,
            )?;
        }
        let frame = FrameHeader {
            process: Process::Lossless,
            precision: p,
            width: hierarchy.header.width,
            height: hierarchy.header.height,
            components: hierarchy.header.components.clone(),
        };
        dump::sample_dump(
            &file(prefix, ".expected.samples"),
            &frame,
            &hierarchy.reconstruct_reference()?,
        )?;
        let options = HierarchicalOptions {
            limits,
            frame_options: &options,
            ..HierarchicalOptions::default()
        };
        (
            hierarchy.encode(options)?,
            if abbreviated {
                Some(sender.encode_hierarchical(&hierarchy, options, MetadataOptions::default())?)
            } else {
                None
            },
        )
    };
    if let Some(pair) = split {
        std::fs::write(file(prefix, ".full.jpg"), full)?;
        std::fs::write(file(prefix, ".tables.jpg"), pair.tables)?;
        std::fs::write(file(prefix, ".jpg"), pair.image)?;
    } else {
        std::fs::write(file(prefix, ".jpg"), full)?;
    }
    Ok(())
}
