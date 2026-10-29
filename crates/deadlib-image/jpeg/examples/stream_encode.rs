//! Compare memory and file-stream output, retaining pre-entropy observations.
use deadlib_image_jpeg::*;
use std::path::{Path, PathBuf};
#[path = "support/hierarchy_dump.rs"]
mod dump;
mod support;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn append(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    name.into()
}
fn kind(bytes: &[u8]) -> Result<Option<u8>> {
    let mut hierarchy = false;
    for event in MarkerReader::new(bytes)? {
        match event? {
            Event::Segment { marker: 0xde, .. } => hierarchy = true,
            Event::Segment { marker, .. } if Process::try_from(marker).is_ok() => {
                return Ok(Some(if hierarchy {
                    2
                } else {
                    u8::from(Process::try_from(marker)?.is_lossless())
                }));
            }
            Event::End { .. } => return Ok(None),
            _ => {}
        }
    }
    Err("missing stream terminator".into())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 8 {
        return Err("usage: stream_encode PREFIX BUFFER ARITHMETIC[-1/0/1] PROGRESSIVE[-1/0/1] FIXED RESTART DNL INPUT...".into());
    }
    let prefix = Path::new(&args[0]);
    let buffer: usize = args[1].parse()?;
    let arithmetic: i8 = args[2].parse()?;
    let progressive: i8 = args[3].parse()?;
    let fixed = args[4] == "1";
    let restart: u16 = args[5].parse()?;
    let dnl = args[6] == "1";
    let limits = Limits {
        max_scans: 8192,
        ..Limits::default()
    };
    let height_declaration = if dnl {
        HeightDeclaration::Dnl { initial_height: 0 }
    } else {
        HeightDeclaration::FrameHeader
    };
    let entropy = |p: Process| {
        if if arithmetic < 0 {
            p.is_arithmetic()
        } else {
            arithmetic != 0
        } {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        }
    };
    let coding = |p: Process| CodingOptions {
        entropy_coding: entropy(p),
        progressive: if progressive < 0 {
            p.is_progressive()
        } else {
            progressive != 0
        },
        successive_approximation: 3,
        optimize_huffman: !fixed,
        restart_interval: restart,
        height_declaration,
        ..CodingOptions::default()
    };
    let lossless = |p: Process| LosslessOptions {
        entropy_coding: entropy(p),
        predictor: 7,
        restart_rows: restart,
        optimize_huffman: !fixed,
        height_declaration,
        limits,
        ..LosslessOptions::default()
    };
    let mut tables = Tables::new();
    let mut total = 0;
    for (index, input) in args[7..].iter().enumerate() {
        let bytes = std::fs::read(input)?;
        let name = append(prefix, &format!(".{index}"));
        let mut output = StreamEncoder::with_buffer_size(
            std::fs::File::create(append(&name, ".stream.jpg"))?,
            buffer,
        );
        let (memory, count) = match kind(&bytes)? {
            None => {
                let packet = tables.load(&bytes, limits)?;
                let memory = tables.encode_with_metadata(&packet.metadata, limits, 1 << 24)?;
                let count = output.encode_tables(&tables, &packet.metadata, limits, 1 << 24)?;
                (memory, count)
            }
            Some(0) => {
                let image = tables.decode_coefficients(&bytes, limits)?;
                support::coefficients(&image, &append(&name, ".expected.coef"))?;
                let options = coding(image.frame.process);
                let memory = encode_coefficients(&image, options)?;
                let count =
                    output.encode_coefficients(&image, options, MetadataOptions::default())?;
                (memory, count)
            }
            Some(1) => {
                let image = tables.decode_lossless(&bytes, limits)?;
                dump::sample_dump(
                    &append(&name, ".expected.samples"),
                    &image.frame,
                    &image.components,
                )?;
                let options = lossless(image.frame.process);
                let memory = image.encode(options)?;
                let count = output.encode_lossless(&image, options, MetadataOptions::default())?;
                (memory, count)
            }
            _ => {
                let image = tables.decode_hierarchical(&bytes, limits)?;
                let mut options = Vec::new();
                for (i, frame) in image.frames.iter().enumerate() {
                    match &frame.data {
                        HierarchicalData::Dct(_) => {
                            dump::frame_dump(&append(&name, &format!(".frame-{i}.coef")), frame)?;
                            options
                                .push(HierarchicalFrameOptions::Dct(coding(frame.frame.process)));
                        }
                        HierarchicalData::Lossless { .. } => {
                            dump::frame_dump(
                                &append(&name, &format!(".frame-{i}.samples")),
                                frame,
                            )?;
                            options.push(HierarchicalFrameOptions::Lossless(lossless(
                                frame.frame.process,
                            )));
                        }
                    }
                }
                let controls = HierarchicalOptions {
                    frame_options: &options,
                    limits,
                    omit_dhp_height: dnl,
                    ..HierarchicalOptions::default()
                };
                let memory = image.encode(controls)?;
                let count =
                    output.encode_hierarchical(&image, controls, MetadataOptions::default())?;
                (memory, count)
            }
        };
        assert_eq!(count, memory.len());
        assert!(output.buffer_capacity() <= buffer);
        drop(output.into_inner());
        let stream = std::fs::read(append(&name, ".stream.jpg"))?;
        assert_eq!(stream, memory);
        std::fs::write(append(&name, ".memory.jpg"), &memory)?;
        total += count;
    }
    println!("{} streams {total} encoded bytes", args.len() - 7);
    Ok(())
}
