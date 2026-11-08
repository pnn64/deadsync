//! Native source-polarity/CMYK-to-YCCK encoder corpus.
use deadlib_image_jpeg::{
    CmykPolarity, CodingOptions, ColorSpace, EncodeInput, EncodeOptions, SampleData, Subsampling,
    encode_coefficients, quantize,
};
use std::io::Write;
mod support;
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
        "case\tprecision\tquality\tpolarity\tycck\tprogressive\tsampling\tsource\tfactors"
    )?;
    for precision in [8, 12] {
        let maximum = (1u32 << precision) - 1;
        let inks: Vec<_> = (0..25)
            .flat_map(|y| {
                (0..31).flat_map(move |x| {
                    (0..4).map(move |c| {
                        (((x * 193 + y * 317 + c * 911 + (x * y * 53) % 997) as u32 * maximum
                            / 4095)
                            & maximum) as u16
                    })
                })
            })
            .collect();
        let inverse: Vec<_> = inks.iter().map(|&v| maximum as u16 - v).collect();
        let source_name = format!("source-{precision}.raw");
        let mut raw = std::fs::File::create(root.join(&source_name))?;
        for &v in &inverse {
            raw.write_all(&v.to_le_bytes())?;
        }
        for quality in [1, 50, 90, 100] {
            for progressive in [false, true] {
                for ycck in [false, true] {
                    for (sampling, subsampling) in [
                        ("444", Subsampling::S444),
                        ("422", Subsampling::S422),
                        ("420", Subsampling::S420),
                    ] {
                        for (label, polarity, samples) in [
                            ("ink", CmykPolarity::Conventional, &inks),
                            ("inverse", CmykPolarity::AdobeInverted, &inverse),
                        ] {
                            let options = EncodeOptions {
                                quality,
                                precision,
                                subsampling,
                                cmyk_polarity: polarity,
                                cmyk_to_ycck: ycck,
                                coding: CodingOptions {
                                    progressive,
                                    restart_interval: 3,
                                    ..CodingOptions::default()
                                },
                                ..EncodeOptions::default()
                            };
                            let input = EncodeInput {
                                width: 31,
                                height: 25,
                                precision,
                                color_space: ColorSpace::Cmyk,
                                samples: SampleData::U16(samples),
                            };
                            let coefficients = quantize(&input, options)?;
                            let name = format!(
                                "p{precision}-q{quality}-{label}-y{}-p{}-{sampling}",
                                u8::from(ycck),
                                u8::from(progressive)
                            );
                            let factors = coefficients
                                .components
                                .iter()
                                .map(|c| {
                                    format!(
                                        "{},{}",
                                        c.spec.horizontal_sampling, c.spec.vertical_sampling
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join(",");
                            support::coefficients(
                                &coefficients,
                                &root.join(format!("{name}-expected.coef")),
                            )?;
                            std::fs::write(
                                root.join(format!("{name}.jpg")),
                                encode_coefficients(&coefficients, options.coding)?,
                            )?;
                            writeln!(
                                manifest,
                                "{name}\t{precision}\t{quality}\t{label}\t{}\t{}\t{sampling}\t{source_name}\t{factors}",
                                u8::from(ycck),
                                u8::from(progressive)
                            )?;
                        }
                    }
                }
            }
        }
    }
    println!("Generated 192 CMYK source/transform cases.");
    Ok(())
}
