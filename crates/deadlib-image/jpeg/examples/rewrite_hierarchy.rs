//! Rewrite native hierarchy frame data with configurable entropy/scan coding.
use deadlib_image_jpeg::{
    CodingOptions, EntropyCoding, HierarchicalFrameOptions, HierarchicalOptions, Limits,
    LosslessOptions, decode_hierarchical,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = std::fs::read(args.next().ok_or("input hierarchy required")?)?;
    let destination = args.next().ok_or("output hierarchy required")?;
    let mode = args.next().unwrap_or_else(|| "preserve".into());
    if ![
        "preserve",
        "sequential-huffman",
        "progressive-huffman",
        "sequential-arithmetic",
        "progressive-arithmetic",
    ]
    .contains(&mode.as_str())
    {
        return Err("invalid output mode".into());
    }
    let optimize_huffman = args.next().is_none_or(|v| v != "fixed");
    let restart = args.next().map_or(Ok(0), |v| v.parse::<u16>())?;
    let initial = args.next().map_or(Ok(0), |v| v.parse::<u8>())?;
    let source = decode_hierarchical(&input, Limits::default())?;
    let controls: Vec<_> = source
        .frames
        .iter()
        .map(|f| {
            let entropy_coding = if mode == "preserve" {
                if f.frame.process.is_arithmetic() {
                    EntropyCoding::Arithmetic
                } else {
                    EntropyCoding::Huffman
                }
            } else if mode.ends_with("arithmetic") {
                EntropyCoding::Arithmetic
            } else {
                EntropyCoding::Huffman
            };
            if f.frame.process.is_lossless() {
                HierarchicalFrameOptions::Lossless(LosslessOptions {
                    entropy_coding,
                    optimize_huffman,
                    predictor: 4,
                    restart_rows: restart,
                    ..LosslessOptions::default()
                })
            } else {
                let progressive = if mode == "preserve" {
                    f.frame.process.is_progressive()
                } else {
                    mode.starts_with("progressive")
                };
                HierarchicalFrameOptions::Dct(CodingOptions {
                    entropy_coding,
                    progressive,
                    optimize_huffman,
                    restart_interval: restart,
                    successive_approximation: if progressive { initial } else { 0 },
                    ..CodingOptions::default()
                })
            }
        })
        .collect();
    let output = source.encode(HierarchicalOptions {
        frame_options: &controls,
        ..HierarchicalOptions::default()
    })?;
    std::fs::write(destination, &output)?;
    println!("{} frames, {} bytes", source.frames.len(), output.len());
    Ok(())
}
