//! Source-pyramid validation: pre-entropy frames and matching reference planes.
use deadlib_image_jpeg::{
    CmykPolarity, CodingOptions, ColorSpace, EncodeInput, EntropyCoding, Expansion,
    HierarchicalData, HierarchicalImage, Limits, LosslessOptions, PyramidLevel, PyramidOptions,
    PyramidRasterOptions, SampleData, ScanSpec, Subsampling, build_hierarchical_samples,
    encode_hierarchical_samples,
};
use std::{io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[path = "support/hierarchy_dump.rs"]
mod dump;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 16 {
        return Err("input.raw output.jpg P W H color mode depth quality sampling restart basePt midPt partial fixed correction required".into());
    }
    let precision: u8 = args[2].parse()?;
    let width = args[3].parse()?;
    let height = args[4].parse()?;
    let color_space = match args[5].as_str() {
        "gray" => ColorSpace::Grayscale,
        "rgb" => ColorSpace::Rgb,
        "ycbcr" => ColorSpace::YCbCr,
        "cmyk" => ColorSpace::Cmyk,
        "ycck" => ColorSpace::Ycck,
        _ => return Err("unknown color space".into()),
    };
    let lossless = args[6].starts_with("lp-");
    let entropy_coding = if args[6].ends_with("arith") {
        EntropyCoding::Arithmetic
    } else {
        EntropyCoding::Huffman
    };
    let depth: usize = args[7].parse()?;
    let quality = args[8].parse()?;
    let subsampling = match args[9].as_str() {
        "444" => Subsampling::S444,
        "420" => Subsampling::S420,
        "422" => Subsampling::S422,
        _ => return Err("unknown sampling".into()),
    };
    let restart = args[10].parse()?;
    let base_point = args[11].parse()?;
    let mid_point = args[12].parse()?;
    let partial = args[13] == "1";
    let optimize_huffman = args[14] != "1";
    let correction = args[15] == "1";
    if depth == 0 {
        return Err("empty hierarchy".into());
    }
    let bytes = std::fs::read(&args[0])?;
    if bytes.len() % 2 != 0 {
        return Err("odd source length".into());
    }
    let values: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let input = EncodeInput {
        width,
        height,
        precision,
        color_space,
        samples: SampleData::U16(&values),
    };
    let raster = PyramidRasterOptions {
        subsampling,
        cmyk_polarity: CmykPolarity::AdobeInverted,
        ..PyramidRasterOptions::default()
    };
    let ids: &[u8] = match color_space {
        ColorSpace::Grayscale => &[1],
        ColorSpace::Rgb => b"RGB",
        ColorSpace::Cmyk => b"CMYK",
        ColorSpace::YCbCr => &[1, 2, 3],
        ColorSpace::Ycck => &[1, 2, 3, 4],
        ColorSpace::Unknown => return Err("unknown color space".into()),
    };
    let script = [ScanSpec::new(ids, 0, 0, 0, 3)?];
    let mut levels = Vec::new();
    for i in 0..depth {
        let expansion = Expansion {
            horizontal: i != 0,
            vertical: i != 0,
        };
        let level = if lossless {
            PyramidLevel::lossless(
                expansion,
                LosslessOptions {
                    entropy_coding,
                    optimize_huffman,
                    restart_rows: restart,
                    predictor: ((usize::from(precision) + i) % 7 + 1) as u8,
                    point_transform: if i == 0 {
                        base_point
                    } else if i + 1 == depth {
                        0
                    } else {
                        mid_point
                    },
                    ..LosslessOptions::default()
                },
            )
        } else {
            PyramidLevel::dct(
                expansion,
                quality,
                CodingOptions {
                    entropy_coding,
                    optimize_huffman,
                    restart_interval: restart,
                    progressive: args[6].starts_with("prog-") || (partial && i == 0),
                    successive_approximation: 3,
                    scan_script: if partial && i == 0 { &script } else { &[] },
                    ..CodingOptions::default()
                },
            )
        };
        levels.push(level);
    }
    if correction && !lossless {
        levels.push(PyramidLevel::lossless(
            Expansion::default(),
            LosslessOptions {
                entropy_coding,
                optimize_huffman,
                restart_rows: restart,
                ..LosslessOptions::default()
            },
        ));
    }
    let options = PyramidOptions::new(&levels);
    let model = build_hierarchical_samples(&input, raster, options)?;
    let path = Path::new(&args[1]);
    for (i, frame) in model.frames.iter().enumerate() {
        let suffix = if matches!(frame.data, HierarchicalData::Lossless { .. }) {
            "samples"
        } else {
            "coef"
        };
        dump::frame_dump(&path.with_extension(format!("frame-{i}.{suffix}")), frame)?;
        // Validation-only prefix models expose the exact reference used to
        // compute the next residual. Production encoding performs no cloning.
        let mut header = model.header.clone();
        header.width = frame.frame.width;
        header.height = frame.frame.height;
        let prefix = HierarchicalImage::from_frames(
            header,
            model.frames[..=i].to_vec(),
            vec![],
            Limits::default(),
        )?;
        dump::sample_dump(
            &path.with_extension(format!("reference-{i}.samples")),
            &frame.frame,
            &prefix.reconstruct_reference()?,
        )?;
    }
    let encoded = encode_hierarchical_samples(&input, raster, options)?;
    std::fs::File::create(path)?.write_all(&encoded)?;
    Ok(())
}
