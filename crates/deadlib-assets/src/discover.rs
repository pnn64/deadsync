use crate::{strip_sprite_hints, texture_filename_has_multiframe_hint};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub const NONE_TEXTURE_CHOICE_KEY: &str = "None";

#[derive(Clone, Debug)]
pub struct DiscoveredTexture {
    pub key: String,
    pub label: String,
    pub source_path: String,
}

pub trait TextureChoiceLike {
    fn key(&self) -> &str;
}

fn absolute_or_self(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn is_png_file(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

pub fn discover_graphic_textures_in_roots(
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
                source_path: absolute_or_self(&path).to_string_lossy().replace('\\', "/"),
            });
        }
    }
    discovered.sort_by(|a, b| {
        let a_love = love_first && a.label.eq_ignore_ascii_case("Love");
        let b_love = love_first && b.label.eq_ignore_ascii_case("Love");
        match (a_love, b_love) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .label
                .to_ascii_lowercase()
                .cmp(&b.label.to_ascii_lowercase()),
        }
    });
    discovered
}

pub fn canonical_texture_key_with_asset_roots(
    path: &Path,
    asset_roots: impl IntoIterator<Item = PathBuf>,
) -> String {
    for root in asset_roots {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.to_string_lossy().replace('\\', "/");
        }
    }
    let rel = path.strip_prefix(Path::new("assets")).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

pub fn noteskin_png_texture_entries(
    roots: &[PathBuf],
    folder: &str,
    canonical_key: impl Fn(&Path) -> String,
) -> Vec<(String, String)> {
    let mut list = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in roots {
        let base = root.parent().expect("noteskin root has parent");
        let mut dirs = vec![base.join(folder)];
        while let Some(dir) = dirs.pop() {
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
                    let file_path = path.to_string_lossy().replace('\\', "/");
                    list.push((key, file_path));
                }
            }
        }
    }
    list
}

pub fn resolve_texture_choice_key<'a, T: TextureChoiceLike>(
    requested: Option<&str>,
    choices: &'a [T],
) -> Option<&'a str> {
    resolve_texture_choice_entry(requested, choices).map(TextureChoiceLike::key)
}

pub fn resolve_texture_choice_entry<'a, T: TextureChoiceLike>(
    requested: Option<&str>,
    choices: &'a [T],
) -> Option<&'a T> {
    // When the caller explicitly opts out of a texture (e.g. user selected "None"),
    // honor that and render nothing. Only fall back to the first available choice
    // when a texture was requested but could not be located in the discovered set
    // (e.g. the user-customized file was removed).
    let key = requested?;
    choices
        .iter()
        .find(|choice| choice.key().eq_ignore_ascii_case(key))
        .or_else(|| {
            choices
                .iter()
                .find(|choice| !choice.key().eq_ignore_ascii_case(NONE_TEXTURE_CHOICE_KEY))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Choice(&'static str);

    impl TextureChoiceLike for Choice {
        fn key(&self) -> &str {
            self.0
        }
    }

    #[test]
    fn resolves_requested_texture_choice_case_insensitively() {
        let choices = [Choice("Love"), Choice("Metal")];

        assert_eq!(
            resolve_texture_choice_entry(Some("metal"), &choices),
            Some(&choices[1])
        );
    }

    #[test]
    fn falls_back_to_first_non_none_texture_choice() {
        let choices = [Choice(NONE_TEXTURE_CHOICE_KEY), Choice("Love")];

        assert_eq!(
            resolve_texture_choice_key(Some("missing"), &choices),
            Some("Love")
        );
    }

    #[test]
    fn explicit_none_request_keeps_none_choice() {
        let choices = [Choice(NONE_TEXTURE_CHOICE_KEY), Choice("Love")];

        assert_eq!(
            resolve_texture_choice_key(Some(NONE_TEXTURE_CHOICE_KEY), &choices),
            Some(NONE_TEXTURE_CHOICE_KEY)
        );
    }

    #[test]
    fn missing_request_resolves_to_no_choice() {
        let choices = [Choice("Love")];

        assert_eq!(resolve_texture_choice_key(None, &choices), None);
    }
}
