//! Raw sample encoder and native lossless transcoder for independent C gates.
use deadlib_image_jpeg::{
    CmykPolarity, ColorSpace, EncodeInput, Limits, LosslessOptions, LosslessScan, SampleData,
    decode_lossless, encode_lossless_samples,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rewrite = args.first().is_some_and(|s| s == "rewrite");
    let raw = args.first().is_some_and(|s| s == "raw");
    if !(rewrite || raw) {
        return Err("raw/rewrite command required".into());
    }
    let base = if rewrite { 3 } else { 7 };
    let predictor = args[base].parse::<u8>()?;
    let point = args[base + 1].parse::<u8>()?;
    let restart = args[base + 2].parse::<u16>()?;
    let layout = args[base + 3].parse::<u8>()?;
    let optimize = args[base + 4].parse::<u8>()? != 0;
    let input = std::fs::read(&args[1])?;
    let mut options = LosslessOptions {
        predictor,
        point_transform: point,
        restart_rows: restart,
        separate_components: layout == 1,
        optimize_huffman: optimize,
        ..LosslessOptions::default()
    };
    if args.get(base + 5).is_some_and(|a| a == "1") {
        options.entropy_coding = deadlib_image_jpeg::EntropyCoding::Arithmetic;
    }
    if let Some(lower) = args.get(base + 6) {
        options.arithmetic_conditioning.dc_lower = lower.parse()?;
    }
    if let Some(upper) = args.get(base + 7) {
        options.arithmetic_conditioning.dc_upper = upper.parse()?;
    }
    let script_for = |ids: &[u8], p: u8| -> Result<Vec<LosslessScan>, Box<dyn std::error::Error>> {
        let order: Vec<_> = if layout == 3 {
            (0..ids.len()).rev().collect()
        } else {
            (0..ids.len()).collect()
        };
        order
            .into_iter()
            .map(|i| {
                Ok(LosslessScan::new(
                    &[ids[i]],
                    (predictor - 1 + i as u8) % 7 + 1,
                    (point + i as u8) % p,
                    if layout == 3 && i % 2 == 0 {
                        0
                    } else {
                        restart
                    },
                )?)
            })
            .collect()
    };
    let encoded = if rewrite {
        let image = decode_lossless(&input, Limits::default())?;
        let script = script_for(
            &image
                .frame
                .components
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            image.frame.precision,
        )?;
        if layout >= 2 {
            options.scan_script = &script;
        }
        image.encode(options)?
    } else {
        let precision = args[5].parse::<u8>()?;
        let space = match args[6].as_str() {
            "gray" => ColorSpace::Grayscale,
            "rgb" => ColorSpace::Rgb,
            "ycbcr" => ColorSpace::YCbCr,
            "cmyk" | "cmykinv" => ColorSpace::Cmyk,
            "ycck" => ColorSpace::Ycck,
            _ => return Err("unknown color".into()),
        };
        if args[6] == "cmykinv" {
            options.cmyk_polarity = CmykPolarity::AdobeInverted;
        }
        let count = match space {
            ColorSpace::Grayscale => 1,
            ColorSpace::Rgb | ColorSpace::YCbCr => 3,
            _ => 4,
        };
        let ids = match space {
            ColorSpace::Rgb => *b"RGB\0",
            ColorSpace::Cmyk => *b"CMYK",
            _ => [1, 2, 3, 4],
        };
        let script = script_for(&ids[..count], precision)?;
        if layout >= 2 {
            options.scan_script = &script;
        }
        if !input.len().is_multiple_of(2) {
            return Err("raw LE u16 input length".into());
        }
        let values: Vec<_> = input
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&b| u16::from_le_bytes(b))
            .collect();
        encode_lossless_samples(
            &EncodeInput {
                width: args[3].parse()?,
                height: args[4].parse()?,
                precision,
                color_space: space,
                samples: SampleData::U16(&values),
            },
            options,
        )?
    };
    std::fs::write(&args[2], encoded)?;
    Ok(())
}
