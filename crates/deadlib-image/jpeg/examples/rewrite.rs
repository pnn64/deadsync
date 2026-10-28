//! Coefficient-preserving native JPEG rewrite, used by the external oracle gate.
use deadlib_image_jpeg::{CodingOptions, Limits, decode_coefficients, encode_coefficients};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: rewrite input.jpg output.jpg progressive[0/1] optimize[0/1] restart".into(),
        );
    }
    let input = std::fs::read(&args[0])?;
    let image = decode_coefficients(&input, Limits::default())?;
    let options = CodingOptions {
        progressive: args[2] == "1",
        optimize_huffman: args[3] == "1",
        restart_interval: args[4].parse()?,
        ..CodingOptions::default()
    };
    let bytes = encode_coefficients(&image, options)?;
    std::fs::write(&args[1], bytes)?;
    Ok(())
}
