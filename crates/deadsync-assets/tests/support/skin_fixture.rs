use std::{fs, path::Path};

/// Keep the authored parser inputs; generate blank assets with their recorded
/// dimensions. These tests inspect actor geometry and commands, not artwork.
pub fn install(name: &str, destination: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/noteskin-regressions")
        .join(name);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(source.join("fixture.json")).unwrap()).unwrap();
    fs::create_dir_all(destination).unwrap();
    for file in manifest["sources"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        let target = destination.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(path), target).unwrap();
    }
    for image in manifest["images"].as_array().unwrap() {
        let target = destination.join(image["path"].as_str().unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(
            image["width"].as_u64().unwrap() as u32,
            image["height"].as_u64().unwrap() as u32,
            image::Rgba([255, 255, 255, 255]),
        )
        .save(target)
        .unwrap();
    }
    for audio in manifest["audio"].as_array().unwrap() {
        let target = destination.join(audio.as_str().unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, b"asset discovery only; audio is not decoded").unwrap();
    }
}
