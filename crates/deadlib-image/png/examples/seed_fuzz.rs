//! Extract independent PNGSuite zlib streams and PNG files for fuzz campaigns.
use std::{fs, path::Path};
fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let png = root.join("fuzz/corpus/decode");
    let zlib = root.join("fuzz/corpus/zlib");
    fs::create_dir_all(&png)?;
    fs::create_dir_all(&zlib)?;
    for entry in fs::read_dir(root.join("tests/fixtures/pngsuite"))? {
        let path = entry?.path();
        if path.extension().is_none_or(|v| v != "png") {
            continue;
        }
        let name = path.file_name().unwrap();
        let bytes = fs::read(&path)?;
        fs::write(png.join(name), &bytes)?;
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
        fs::write(zlib.join(name), compressed)?;
    }
    fs::copy(
        root.join("tests/fixtures/apng/basic_f20.png"),
        png.join("basic_f20.png"),
    )?;
    Ok(())
}
