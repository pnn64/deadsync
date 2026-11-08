//! Native sampling layouts requiring separate component scans.
use deadlib_image_jpeg::{
    CodingOptions, ColorSpace, EncodeInput, EncodeOptions, SampleData, ScanSpec, Subsampling,
    encode_coefficients, quantize,
};
mod support;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: scan_fixtures output-directory")?,
    );
    std::fs::create_dir_all(&root)?;
    for precision in [8, 12] {
        for (space, label, ids) in [
            (ColorSpace::Rgb, "rgb", &b"RGB"[..]),
            (ColorSpace::YCbCr, "ycbcr", &[1, 2, 3][..]),
            (ColorSpace::Cmyk, "cmyk", &b"CMYK"[..]),
            (ColorSpace::Ycck, "ycck", &[1, 2, 3, 4][..]),
        ] {
            let mask = (1usize << precision) - 1;
            let pixels: Vec<u16> = (0..19 * 17 * ids.len())
                .map(|i| ((i * 91 + i * i * 11) & mask) as u16)
                .collect();
            let input = EncodeInput {
                width: 19,
                height: 17,
                precision,
                color_space: space,
                samples: SampleData::U16(&pixels),
            };
            let script: Vec<_> = ids
                .iter()
                .rev()
                .map(|&id| ScanSpec::new(&[id], 0, 63, 0, 0).unwrap())
                .collect();
            for (sampling, factors) in [
                ("max", [(4, 4); 4]),
                ("fractional", [(4, 3), (3, 4), (2, 2), (1, 1)]),
                ("410", [(4, 2), (1, 1), (1, 1), (4, 2)]),
            ] {
                let options = EncodeOptions {
                    precision,
                    quality: 90,
                    rgb_to_ycbcr: false,
                    subsampling: Subsampling::Custom(factors),
                    coding: CodingOptions {
                        scan_script: &script,
                        restart_interval: 3,
                        ..CodingOptions::default()
                    },
                    ..EncodeOptions::default()
                };
                let image = quantize(&input, options)?;
                let name = format!("p{precision}-{label}-{sampling}");
                support::coefficients(&image, &root.join(format!("{name}-expected.coef")))?;
                std::fs::write(
                    root.join(format!("{name}.jpg")),
                    encode_coefficients(&image, options.coding)?,
                )?;
            }
        }
    }
    Ok(())
}
