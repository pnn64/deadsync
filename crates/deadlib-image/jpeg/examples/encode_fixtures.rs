//! Deterministic native pixel encoder corpus for the independent C gate.
use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, SampleData, Subsampling,
    encode_coefficients, quantize,
};
use std::io::Write;
mod support;

fn source(precision: u8, channels: usize) -> Vec<u16> {
    let maximum = (1u32 << precision) - 1;
    (0..25)
        .flat_map(|y| {
            (0..31).flat_map(move |x| {
                (0..channels).map(move |c| {
                    (((x * 193 + y * 317 + c * 911 + (x * y * 53) % 997) as u32 * maximum / 4095)
                        & maximum) as u16
                })
            })
        })
        .collect()
}

fn save_source(
    root: &std::path::Path,
    precision: u8,
    channels: usize,
    samples: &[u16],
) -> std::io::Result<String> {
    let name = format!("source-p{precision}-c{channels}.pnm");
    if channels == 4 {
        return Ok("-".into());
    }
    let mut file = std::fs::File::create(root.join(&name))?;
    write!(
        file,
        "P{}\n31 25\n{}\n",
        if channels == 1 { 5 } else { 6 },
        (1u32 << precision) - 1
    )?;
    for &v in samples {
        if precision == 8 {
            file.write_all(&[v as u8])?;
        } else {
            file.write_all(&v.to_be_bytes())?;
        }
    }
    Ok(name)
}

#[allow(clippy::too_many_arguments)]
fn case(
    root: &std::path::Path,
    manifest: &mut std::fs::File,
    precision: u8,
    quality: u8,
    color: &str,
    sampling_name: &str,
    subsampling: Subsampling,
    progressive: bool,
    baseline: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (space, channels) = match color {
        "gray" => (ColorSpace::Grayscale, 1),
        "cmyk" => (ColorSpace::Cmyk, 4),
        "ycck" => (ColorSpace::Ycck, 4),
        _ => (ColorSpace::Rgb, 3),
    };
    let samples = source(precision, channels);
    let source_name = save_source(root, precision, channels, &samples)?;
    let options = EncodeOptions {
        quality,
        precision,
        subsampling,
        rgb_to_ycbcr: color != "rgb",
        baseline_quantization: baseline,
        coding: CodingOptions {
            progressive,
            restart_interval: if progressive { 7 } else { 3 },
            ..CodingOptions::default()
        },
        ..EncodeOptions::default()
    };
    let input = EncodeInput {
        width: 31,
        height: 25,
        precision,
        color_space: space,
        samples: SampleData::U16(&samples),
    };
    let coefficients = quantize(&input, options)?;
    let name = format!(
        "p{precision}-q{quality}-{color}-{sampling_name}-p{}-b{}",
        u8::from(progressive),
        u8::from(baseline)
    );
    support::coefficients(&coefficients, &root.join(format!("{name}-expected.coef")))?;
    let jpeg = encode_coefficients(&coefficients, options.coding)?;
    std::fs::write(root.join(format!("{name}.jpg")), jpeg)?;
    writeln!(
        manifest,
        "{name}\t{precision}\t{quality}\t{color}\t{sampling_name}\t{}\t{}\t{source_name}",
        u8::from(progressive),
        u8::from(baseline)
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    std::fs::create_dir_all(&root)?;
    let mut manifest = std::fs::File::create(root.join("manifest.tsv"))?;
    writeln!(
        manifest,
        "case\tprecision\tquality\tcolor\tsampling\tprogressive\tbaseline\tsource"
    )?;
    for precision in [8, 12] {
        for quality in [1, 50, 90, 100] {
            for progressive in [false, true] {
                for color in ["gray", "ycbcr", "rgb", "cmyk", "ycck"] {
                    let samplings = if color == "ycbcr" {
                        vec![
                            ("444", Subsampling::S444),
                            ("422", Subsampling::S422),
                            ("420", Subsampling::S420),
                            ("411", Subsampling::S411),
                            ("440", Subsampling::S440),
                            ("410", Subsampling::S410),
                        ]
                    } else {
                        vec![("444", Subsampling::S444)]
                    };
                    for (name, sampling) in samplings {
                        case(
                            &root,
                            &mut manifest,
                            precision,
                            quality,
                            color,
                            name,
                            sampling,
                            progressive,
                            true,
                        )?;
                    }
                }
            }
        }
    }
    for precision in [8, 12] {
        for progressive in [false, true] {
            case(
                &root,
                &mut manifest,
                precision,
                90,
                "ycbcr",
                "fractional-h",
                Subsampling::Custom([(3, 1), (2, 1), (1, 1), (1, 1)]),
                progressive,
                true,
            )?;
            case(
                &root,
                &mut manifest,
                precision,
                90,
                "ycbcr",
                "fractional-v",
                Subsampling::Custom([(1, 3), (1, 2), (1, 1), (1, 1)]),
                progressive,
                true,
            )?;
            case(
                &root,
                &mut manifest,
                precision,
                1,
                "gray",
                "444",
                Subsampling::S444,
                progressive,
                false,
            )?;
        }
    }
    println!("Generated 172 native pixel encoder cases.");
    Ok(())
}
