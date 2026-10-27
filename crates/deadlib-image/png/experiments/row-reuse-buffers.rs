//! Untimed buffer-capacity probe; these fields are not peak-memory measurements.
use deadlib_image_png::{Samples, decode, zlib};
use std::{io::Read, path::PathBuf};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let corpus = PathBuf::from(args.next().expect("corpus directory"));
    let output = PathBuf::from(args.next().expect("sample output directory"));
    assert!(args.next().is_none());
    std::fs::create_dir(&output).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(corpus)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    println!(
        "case\twidth\theight\tcolor\tdepth\tinterlaced\tfiltered_bytes\tinflate_capacity_bytes\tsample_bytes\tsample_capacity_bytes"
    );
    for path in files {
        let input = std::fs::read(&path).unwrap();
        let png = decode(&input).unwrap();
        assert!(png.animation.is_none());
        let mut idat = Vec::new();
        let mut offset = 8;
        while offset < input.len() {
            let size = u32::from_be_bytes(input[offset..offset + 4].try_into().unwrap()) as usize;
            if &input[offset + 4..offset + 8] == b"IDAT" {
                idat.extend_from_slice(&input[offset + 8..offset + 8 + size]);
            }
            offset += size + 12;
        }
        let mut reference = Vec::new();
        flate2::read::ZlibDecoder::new(idat.as_slice())
            .read_to_end(&mut reference)
            .unwrap();
        let raw = zlib::decompress(&idat, reference.len()).unwrap();
        assert_eq!(raw, reference);
        let image = png.image;
        let name = path.file_stem().unwrap().to_str().unwrap();
        let (samples, capacity) = match image.samples {
            Samples::U8(bytes) => {
                let capacity = bytes.capacity();
                (bytes, capacity)
            }
            Samples::U16(values) => (
                values.iter().flat_map(|v| v.to_be_bytes()).collect(),
                values.capacity() * 2,
            ),
        };
        std::fs::write(output.join(format!("{name}.samples")), &samples).unwrap();
        println!(
            "{name}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            image.width,
            image.height,
            image.color_type as u8,
            image.bit_depth,
            u8::from(png.interlaced),
            raw.len(),
            raw.capacity(),
            samples.len(),
            capacity
        );
    }
}
