// Frozen from 0f2bf56e6 (0.5.1226); function bodies unchanged.
use super::*;

pub(super) fn discover_graphic_textures_in_roots(
    folder: &str,
    roots: impl IntoIterator<Item = PathBuf>,
    love_first: bool,
    require_multiframe_hint: bool,
) -> Vec<DiscoveredTexture> {
    let mut discovered = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if require_multiframe_hint && !texture_filename_has_multiframe_hint(file_name) {
                continue;
            }
            if !require_multiframe_hint && !is_png_file(file_name) {
                continue;
            }
            let key = format!("{folder}/{file_name}");
            if !seen_keys.insert(key.to_ascii_lowercase()) {
                continue;
            }
            let label = strip_sprite_hints(file_name);
            if label.eq_ignore_ascii_case(NONE_TEXTURE_CHOICE_KEY) {
                continue;
            }
            discovered.push(DiscoveredTexture {
                key,
                label,
                source_path: absolute_or_self(&path),
            });
        }
    }
    sort_discovered_textures(&mut discovered, love_first);
    discovered
}

pub(super) fn noteskin_png_texture_entries(
    roots: &[PathBuf],
    canonical_key: impl Fn(&Path) -> String,
) -> Vec<(String, PathBuf)> {
    let mut list = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in roots {
        let mut dirs = vec![root.clone()];
        while let Some(dir) = dirs.pop() {
            // Pack assets and installer staging stay on disk. Player Options
            // and gameplay load the selected native components on demand.
            if dir.join("pack.json").is_file()
                || dir
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                {
                    continue;
                }
                let key = canonical_key(&path);
                if key.starts_with("noteskins/") && seen_keys.insert(key.clone()) {
                    list.push((key, path));
                }
            }
        }
    }
    list
}
