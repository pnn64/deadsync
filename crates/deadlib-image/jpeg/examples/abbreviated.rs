//! Stateful table-stream/image-stream oracle adapter; no production codec deps.
use deadlib_image_jpeg::{
    AbbreviatedEncoder, CodingOptions, EntropyCoding, Event, FrameHeader, HierarchicalData,
    HierarchicalFrameOptions, HierarchicalOptions, Limits, LosslessOptions, MarkerReader,
    MetadataOptions, Process, Tables,
};
use std::path::Path;
#[path = "support/hierarchy_dump.rs"]
mod dump;
mod support;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn kind(input: &[u8]) -> Result<Option<u8>> {
    let mut hierarchy = false;
    for event in MarkerReader::new(input)? {
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
fn append(prefix: &Path, suffix: &str) -> std::path::PathBuf {
    let mut path = prefix.as_os_str().to_os_string();
    path.push(suffix);
    path.into()
}
fn decode(prefix: &Path, inputs: &[String]) -> Result<()> {
    let mut cache = Tables::new();
    let (mut images, mut packets) = (0, 0);
    for path in inputs {
        let bytes = std::fs::read(path)?;
        let mut position = 0;
        while position < bytes.len() {
            let input = &bytes[position..];
            let Some(family) = kind(input)? else {
                let info = cache.load(input, Limits::default())?;
                position += info.consumed;
                packets += 1;
                continue;
            };
            let destination = append(prefix, &format!(".image-{images}"));
            match family {
                0 => {
                    let image = cache.decode_coefficients(input, Limits::default())?;
                    support::coefficients(&image, &append(&destination, ".coef"))?;
                    dump::sample_dump(
                        &append(&destination, ".target.samples"),
                        &image.frame,
                        &image.reconstruct()?.components,
                    )?;
                    position += image.consumed;
                }
                1 => {
                    let image = cache.decode_lossless(input, Limits::default())?;
                    dump::sample_dump(
                        &append(&destination, ".samples"),
                        &image.frame,
                        &image.components,
                    )?;
                    position += image.consumed;
                }
                _ => {
                    let image = cache.decode_hierarchical(input, Limits::default())?;
                    for (i, frame) in image.frames.iter().enumerate() {
                        dump::frame_dump(
                            &append(
                                &destination,
                                &format!(
                                    ".frame-{i}.{}",
                                    if matches!(frame.data, HierarchicalData::Dct(_)) {
                                        "coef"
                                    } else {
                                        "samples"
                                    }
                                ),
                            ),
                            frame,
                        )?;
                    }
                    let frame = FrameHeader {
                        process: Process::Lossless,
                        precision: image.header.precision,
                        width: image.header.width,
                        height: image.header.height,
                        components: image.header.components.clone(),
                    };
                    dump::sample_dump(
                        &append(&destination, ".samples"),
                        &frame,
                        &image.reconstruct_reference()?,
                    )?;
                    position += image.consumed;
                }
            }
            images += 1;
        }
    }
    println!("tables {packets} images {images}");
    Ok(())
}
fn encode(args: &[String]) -> Result<()> {
    if args.len() < 5 {
        return Err("encode prefix fixed restart repeat input.jpg... required".into());
    }
    let prefix = Path::new(&args[0]);
    let optimize_huffman = args[1] != "1";
    let restart = args[2].parse()?;
    let repeat: usize = args[3].parse()?;
    let mut encoder = AbbreviatedEncoder::new();
    let mut counter = 0;
    for source in &args[4..] {
        let bytes = std::fs::read(source)?;
        for _ in 0..repeat {
            let dct = |process: Process| CodingOptions {
                entropy_coding: if process.is_arithmetic() {
                    EntropyCoding::Arithmetic
                } else {
                    EntropyCoding::Huffman
                },
                progressive: process.is_progressive(),
                successive_approximation: 3,
                optimize_huffman,
                restart_interval: restart,
                ..CodingOptions::default()
            };
            let lossless = |process: Process| LosslessOptions {
                entropy_coding: if process.is_arithmetic() {
                    EntropyCoding::Arithmetic
                } else {
                    EntropyCoding::Huffman
                },
                optimize_huffman,
                restart_rows: restart,
                ..LosslessOptions::default()
            };
            let destination = append(prefix, &format!(".image-{counter}"));
            let result = match kind(&bytes)?.ok_or("image source required")? {
                0 => {
                    let image = deadlib_image_jpeg::decode_coefficients(&bytes, Limits::default())?;
                    support::coefficients(&image, &append(&destination, ".expected.coef"))?;
                    encoder.encode_coefficients(
                        &image,
                        dct(image.frame.process),
                        MetadataOptions::default(),
                    )?
                }
                1 => {
                    let image = deadlib_image_jpeg::decode_lossless(&bytes, Limits::default())?;
                    dump::sample_dump(
                        &append(&destination, ".expected.samples"),
                        &image.frame,
                        &image.components,
                    )?;
                    encoder.encode_lossless(
                        &image,
                        lossless(image.frame.process),
                        MetadataOptions::default(),
                    )?
                }
                _ => {
                    let image = deadlib_image_jpeg::decode_hierarchical(&bytes, Limits::default())?;
                    let mut options = Vec::new();
                    for (i, frame) in image.frames.iter().enumerate() {
                        dump::frame_dump(
                            &append(
                                &destination,
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
                        options.push(if frame.frame.process.is_lossless() {
                            HierarchicalFrameOptions::Lossless(lossless(frame.frame.process))
                        } else {
                            HierarchicalFrameOptions::Dct(dct(frame.frame.process))
                        });
                    }
                    let frame = FrameHeader {
                        process: Process::Lossless,
                        precision: image.header.precision,
                        width: image.header.width,
                        height: image.header.height,
                        components: image.header.components.clone(),
                    };
                    dump::sample_dump(
                        &append(&destination, ".expected.samples"),
                        &frame,
                        &image.reconstruct_reference()?,
                    )?;
                    encoder.encode_hierarchical(
                        &image,
                        HierarchicalOptions {
                            frame_options: &options,
                            ..HierarchicalOptions::default()
                        },
                        MetadataOptions::default(),
                    )?
                }
            };
            std::fs::write(append(&destination, ".tables.jpg"), result.tables)?;
            std::fs::write(append(&destination, ".jpg"), result.image)?;
            counter += 1;
        }
    }
    println!("images {counter}");
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("decode") if args.len() >= 3 => {
            let mut inputs = Vec::new();
            for path in &args[2..] {
                if let Some(list) = path.strip_prefix('@') {
                    inputs.extend(
                        std::fs::read_to_string(list)?
                            .lines()
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                    );
                } else {
                    inputs.push(path.clone());
                }
            }
            decode(Path::new(&args[1]), &inputs)
        }
        Some("encode") => encode(&args[1..]),
        _ => Err("decode prefix stream... | encode prefix fixed restart repeat input...".into()),
    }
}
