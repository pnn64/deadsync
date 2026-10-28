//! Coefficient-preserving native JPEG rewrite, used by the external oracle gate.
use deadlib_image_jpeg::{
    ArithmeticConditioning, CodingOptions, EntropyCoding, Limits, decode_coefficients,
    encode_coefficients,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 5 && args.len() != 6 && args.len() != 9 {
        return Err(
            "usage: rewrite input.jpg output.jpg progressive[0/1] optimize[0/1] restart [arithmetic[0/1] [DC-L DC-U AC-Kx]]".into(),
        );
    }
    let input = std::fs::read(&args[0])?;
    let image = decode_coefficients(&input, Limits::default())?;
    let options = CodingOptions {
        progressive: args[2] == "1",
        optimize_huffman: args[3] == "1",
        restart_interval: args[4].parse()?,
        entropy_coding: if args.get(5).is_some_and(|s| s == "1") {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        },
        arithmetic_conditioning: if args.len() == 9 {
            ArithmeticConditioning {
                dc_lower: args[6].parse()?,
                dc_upper: args[7].parse()?,
                ac_split: args[8].parse()?,
            }
        } else {
            ArithmeticConditioning::default()
        },
        ..CodingOptions::default()
    };
    let bytes = encode_coefficients(&image, options)?;
    std::fs::write(&args[1], bytes)?;
    Ok(())
}
