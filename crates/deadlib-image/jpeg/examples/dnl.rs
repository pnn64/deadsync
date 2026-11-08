//! DNL writer/decoder gate with expectations captured before entropy encoding.
use deadlib_image_jpeg::{
    CodingOptions, EntropyCoding, FrameHeader, HeightDeclaration, HierarchicalFrameOptions,
    HierarchicalOptions, Limits, LosslessOptions, Process, decode_coefficients,
    decode_hierarchical, decode_lossless, encode_coefficients,
};
use std::path::Path;
#[path = "support/hierarchy_dump.rs"]
mod dump;
mod support;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 7 {
        return Err("dct|lossless|hierarchy input.jpg output.jpg initial-height|none restart omit-DHP fixed required".into());
    }
    let bytes = std::fs::read(&args[1])?;
    let destination = Path::new(&args[2]);
    let initial = if args[3] == "none" || args[3] == "same" {
        0
    } else {
        args[3].parse()?
    };
    let declaration = |height| {
        if args[3] == "none" {
            HeightDeclaration::FrameHeader
        } else {
            HeightDeclaration::Dnl {
                initial_height: if args[3] == "same" { height } else { initial },
            }
        }
    };
    let restart = args[4].parse()?;
    let optimize_huffman = args[6] != "1";
    let dct = |process: Process, height| CodingOptions {
        height_declaration: declaration(height),
        restart_interval: restart,
        optimize_huffman,
        entropy_coding: if process.is_arithmetic() {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        },
        progressive: process.is_progressive(),
        successive_approximation: 3,
        ..CodingOptions::default()
    };
    let lossless = |process: Process, height| LosslessOptions {
        height_declaration: declaration(height),
        restart_rows: restart,
        optimize_huffman,
        entropy_coding: if process.is_arithmetic() {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        },
        ..LosslessOptions::default()
    };
    let result = match args[0].as_str() {
        "dct" => {
            let model = decode_coefficients(&bytes, Limits::default())?;
            support::coefficients(&model, &destination.with_extension("frame-0.coef"))?;
            dump::sample_dump(
                &destination.with_extension("expected.samples"),
                &model.frame,
                &model.reconstruct()?.components,
            )?;
            encode_coefficients(&model, dct(model.frame.process, model.frame.height))?
        }
        "lossless" => {
            let model = decode_lossless(&bytes, Limits::default())?;
            dump::sample_dump(
                &destination.with_extension("frame-0.samples"),
                &model.frame,
                &model.components,
            )?;
            dump::sample_dump(
                &destination.with_extension("expected.samples"),
                &model.frame,
                &model.components,
            )?;
            model.encode(lossless(model.frame.process, model.frame.height))?
        }
        "hierarchy" => {
            let model = decode_hierarchical(&bytes, Limits::default())?;
            let mut controls = Vec::new();
            for (i, frame) in model.frames.iter().enumerate() {
                dump::frame_dump(
                    &destination.with_extension(format!(
                        "frame-{i}.{}",
                        if frame.frame.process.is_lossless() {
                            "samples"
                        } else {
                            "coef"
                        }
                    )),
                    frame,
                )?;
                controls.push(if frame.frame.process.is_lossless() {
                    HierarchicalFrameOptions::Lossless(lossless(
                        frame.frame.process,
                        frame.frame.height,
                    ))
                } else {
                    HierarchicalFrameOptions::Dct(dct(frame.frame.process, frame.frame.height))
                });
            }
            let frame = FrameHeader {
                precision: model.header.precision,
                width: model.header.width,
                height: model.header.height,
                components: model.header.components.clone(),
                process: Process::Lossless,
            };
            dump::sample_dump(
                &destination.with_extension("expected.samples"),
                &frame,
                &model.reconstruct_reference()?,
            )?;
            model.encode(HierarchicalOptions {
                frame_options: &controls,
                omit_dhp_height: args[5] == "1",
                ..HierarchicalOptions::default()
            })?
        }
        _ => return Err("unknown process family".into()),
    };
    std::fs::write(destination, result)?;
    Ok(())
}
