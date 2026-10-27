//! Extract independent PNGSuite zlib streams and PNG files for fuzz campaigns.
//! An optional first argument creates a fresh corpus root instead of merging seeds.
#[path = "../tests/support/huffman.rs"]
mod huffman;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus = if let Some(destination) = std::env::args_os().nth(1) {
        let path = PathBuf::from(destination);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(&path)?;
        path
    } else {
        root.join("fuzz/corpus")
    };
    let png = corpus.join("decode");
    let zlib = corpus.join("zlib");
    let png_stream = corpus.join("png_stream");
    let inflate_stream = corpus.join("inflate_stream");
    let encode_stream = corpus.join("encode_stream");
    let profiles = corpus.join("profiles");
    let deflate_stream = corpus.join("deflate_stream");
    let huffman_dir = corpus.join("huffman");
    fs::create_dir_all(&huffman_dir)?;
    fs::create_dir_all(&zlib)?;
    let cases = huffman::all_cases();
    let seeds = huffman::seed_indices(&cases);
    for &index in &seeds {
        let mut input = (index as u32).to_le_bytes().to_vec();
        input.extend_from_slice(&[0, 2, 3, 0, 0, 0, 0, 0]);
        fs::write(huffman_dir.join(format!("case-{index}")), input)?;
        fs::write(zlib.join(format!("huffman-{index}")), &cases[index].bytes)?;
    }
    println!("Huffman: {} fixtures, {} seeds", cases.len(), seeds.len());
    fs::create_dir_all(&deflate_stream)?;
    for size in [32767, 32768, 32769, 65535, 65536, 131072] {
        let mut random = 0x7139_4a55u32;
        for style in 0..4 {
            let raw: Vec<_> = (0..size)
                .map(|n| match style {
                    0 => 0,
                    1 => {
                        random ^= random << 13;
                        random ^= random >> 17;
                        random ^= random << 5;
                        random as u8
                    }
                    2 => {
                        if n / 32768 % 2 == 0 {
                            (n % 7) as u8
                        } else {
                            (n * 71 + n / 13) as u8
                        }
                    }
                    _ => ((n % 32768) * 71 + (n % 32768) / 31) as u8,
                })
                .collect();
            fs::write(deflate_stream.join(format!("block-{size}-{style}")), raw)?;
        }
    }
    fs::create_dir_all(&profiles)?;
    for format in 0..15 {
        for profile in 0..8 {
            for flags in [0, 1, 2, 3, 4, 5, 8, 9, 16, 20, 32, 64, 65, 128, 129] {
                fs::write(
                    profiles.join(format!("profile-{format}-{profile}-{flags}")),
                    [flags, profile, format, 9, 0, 0, 0, 0],
                )?;
            }
        }
    }
    fs::create_dir_all(&encode_stream)?;
    for format in 0..15 {
        for flags in [0, 1, 2, 3, 4, 6, 12, 14, 16, 32, 64, 127] {
            for filter in 0..6 {
                fs::write(
                    encode_stream.join(format!("format-{format}-{flags}-{filter}")),
                    [format, 17, 13, flags, filter, 0, 7, 127, 255, 1, 42, 128],
                )?;
            }
        }
    }
    fs::create_dir_all(&png)?;
    fs::create_dir_all(&zlib)?;
    fs::create_dir_all(&png_stream)?;
    fs::create_dir_all(&inflate_stream)?;
    for entry in fs::read_dir(root.join("tests/fixtures/pngsuite"))? {
        let path = entry?.path();
        if path.extension().is_none_or(|v| v != "png") {
            continue;
        }
        let name = path.file_name().unwrap();
        let bytes = fs::read(&path)?;
        fs::write(png.join(name), &bytes)?;
        fs::write(png_stream.join(name), &bytes)?;
        if deadlib_image_png::decode(&bytes).is_err() {
            continue;
        }
        let mut compressed = Vec::new();
        let mut pos = 8;
        while pos + 12 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            if &bytes[pos + 4..pos + 8] == b"IDAT" {
                compressed.extend_from_slice(&bytes[pos + 8..pos + 8 + len]);
            }
            pos += len + 12;
        }
        fs::write(zlib.join(name), &compressed)?;
        fs::write(inflate_stream.join(name), &compressed)?;
        let (mut raw, _) = deadlib_image_png::zlib::decompress_prefix(&compressed, 1 << 20)
            .map_err(std::io::Error::other)?;
        raw.truncate(131072);
        fs::write(deflate_stream.join(name), raw)?;
    }
    fs::copy(
        root.join("tests/fixtures/apng/basic_f20.png"),
        png.join("basic_f20.png"),
    )?;
    fs::copy(
        root.join("tests/fixtures/apng/basic_f20.png"),
        png_stream.join("basic_f20.png"),
    )?;
    Ok(())
}
