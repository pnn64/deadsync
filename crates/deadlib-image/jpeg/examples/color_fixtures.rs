//! Additional legal four-component sampling layouts for color differential tests.
use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, SampleData, Subsampling,
    encode_coefficients, quantize,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    std::fs::create_dir_all(&root)?;
    for precision in [8, 12] {
        for progressive in [false, true] {
            let maximum = (1u32 << precision) - 1;
            let samples: Vec<_> = (0..25)
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
            for (name, color_space, factors) in [
                (
                    "ycck-420k",
                    ColorSpace::Ycck,
                    [(2, 2), (1, 1), (1, 1), (2, 2)],
                ),
                (
                    "ycck-422k",
                    ColorSpace::Ycck,
                    [(2, 1), (1, 1), (1, 1), (2, 1)],
                ),
                (
                    "cmyk-k420",
                    ColorSpace::Cmyk,
                    [(1, 1), (1, 1), (1, 1), (2, 2)],
                ),
                (
                    "cmyk-c422",
                    ColorSpace::Cmyk,
                    [(2, 1), (1, 1), (1, 1), (1, 1)],
                ),
            ] {
                let input = EncodeInput {
                    width: 31,
                    height: 25,
                    precision,
                    color_space,
                    samples: SampleData::U16(&samples),
                };
                let coding = CodingOptions {
                    progressive,
                    restart_interval: 3,
                    ..CodingOptions::default()
                };
                let mut image = quantize(
                    &input,
                    EncodeOptions {
                        quality: 90,
                        precision,
                        subsampling: Subsampling::Custom(factors),
                        coding,
                        ..EncodeOptions::default()
                    },
                )?;
                let label = format!("p{precision}-{name}-p{}", u8::from(progressive));
                std::fs::write(
                    root.join(format!("{label}.jpg")),
                    encode_coefficients(&image, coding)?,
                )?;
                if color_space == ColorSpace::Cmyk {
                    image.metadata.retain(|s| s.marker != 0xee);
                    std::fs::write(
                        root.join(format!("{label}-untagged.jpg")),
                        encode_coefficients(&image, coding)?,
                    )?;
                }
            }
        }
    }
    println!("Generated 24 four-component color fixtures.");
    Ok(())
}
