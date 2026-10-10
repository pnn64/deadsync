// Frozen implementation from main 59ea18661e1ee0a054405f77faf724c601d79b67.
use super::*;

pub(super) fn list_texture_pages(font_dir: &Path, prefix: &str) -> std::io::Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for entry in fs::read_dir(font_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !has_png_suffix(name) {
            continue;
        }
        if !name.starts_with(prefix) {
            continue;
        }
        if name.contains("-stroke") {
            continue;
        }
        v.push(path);
    }
    v.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    Ok(v)
}
