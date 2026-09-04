use crate::media::{is_mac_resource_fork, is_song_art_image, resolve_song_asset_path_like_itg};
use crate::tags::latest_simfile_tag_values;
use image::image_dimensions;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ResolvedSongArtwork {
    pub banner_path: Option<PathBuf>,
    pub background_path: Option<PathBuf>,
    pub cdtitle_path: Option<PathBuf>,
}

#[derive(Default)]
struct ArtworkCandidates {
    banner: Option<PathBuf>,
    background: Option<PathBuf>,
    cdtitle: Option<PathBuf>,
    jacket: Option<PathBuf>,
    cdimage: Option<PathBuf>,
    disc: Option<PathBuf>,
}

#[must_use]
pub fn resolve_song_artwork_like_itg(
    song_dir: &Path,
    simfile_data: &[u8],
    banner_tag: &str,
    background_tag: &str,
    cdtitle_tag: &str,
    jacket_tag: &str,
) -> ResolvedSongArtwork {
    let banner = resolve_song_asset_path_like_itg(song_dir, banner_tag);
    let background = resolve_song_asset_path_like_itg(song_dir, background_tag);
    let cdtitle = resolve_song_asset_path_like_itg(song_dir, cdtitle_tag);
    let jacket = resolve_song_asset_path_like_itg(song_dir, jacket_tag);

    if banner.is_some() && background.is_some() && cdtitle.is_some() {
        return ResolvedSongArtwork {
            banner_path: banner,
            background_path: background,
            cdtitle_path: cdtitle,
        };
    }

    let [cdimage_tag, discimage_tag] = latest_simfile_tag_values(
        simfile_data,
        [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()],
    );
    let mut candidates = ArtworkCandidates {
        banner,
        background,
        cdtitle,
        jacket,
        cdimage: resolve_song_asset_path_like_itg(song_dir, &cdimage_tag),
        disc: resolve_song_asset_path_like_itg(song_dir, &discimage_tag),
    };
    let images = list_song_art_images(song_dir);
    fill_song_art_hints(&images, &mut candidates);

    for image in &images {
        if candidates.banner.is_some()
            && candidates.background.is_some()
            && candidates.cdtitle.is_some()
        {
            break;
        }
        if song_art_is_classified(image, &candidates) {
            continue;
        }

        let Ok((width, height)) = image_dimensions(image) else {
            continue;
        };
        if candidates.background.is_none() && width >= 320 && height >= 240 {
            candidates.background = Some(image.clone());
            continue;
        }
        if candidates.banner.is_none()
            && (100..=320).contains(&width)
            && (50..=240).contains(&height)
        {
            candidates.banner = Some(image.clone());
            continue;
        }
        if candidates.banner.is_none()
            && width > 200
            && height > 0
            && width as f32 / height as f32 > 2.0
        {
            candidates.banner = Some(image.clone());
            continue;
        }
        if candidates.cdtitle.is_none() && width <= 100 && height <= 48 {
            candidates.cdtitle = Some(image.clone());
            continue;
        }
        if candidates.jacket.is_none() && width == height {
            candidates.jacket = Some(image.clone());
            continue;
        }
        if candidates.disc.is_none()
            && width > height
            && candidates.banner.is_some()
            && !song_art_matches(image, &candidates.banner)
        {
            candidates.disc = Some(image.clone());
            continue;
        }
        if candidates.cdimage.is_none() && width == height {
            candidates.cdimage = Some(image.clone());
        }
    }

    ResolvedSongArtwork {
        banner_path: candidates.banner,
        background_path: candidates.background,
        cdtitle_path: candidates.cdtitle,
    }
}

fn list_song_art_images(song_dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(song_dir) else {
        return Vec::new();
    };
    let mut paths = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !is_mac_resource_fork(path) && path.is_file() && is_song_art_image(path))
        .collect::<Vec<_>>();
    sort_song_art_paths(&mut paths);
    paths
}

#[derive(Clone, Copy)]
struct ArtworkSortKey {
    start: usize,
    end: usize,
    path_index: usize,
}

fn sort_song_art_paths(paths: &mut [PathBuf]) {
    if paths.len() < 2 {
        return;
    }
    let folded_capacity = paths
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| name.as_encoded_bytes().len())
        .sum();
    let mut folded = Vec::with_capacity(folded_capacity);
    let mut keys = Vec::with_capacity(paths.len());
    for (path_index, path) in paths.iter().enumerate() {
        let start = folded.len();
        if let Some(name) = path.file_name() {
            folded.extend(
                name.to_string_lossy()
                    .bytes()
                    .map(|byte| byte.to_ascii_lowercase()),
            );
        }
        keys.push(ArtworkSortKey {
            start,
            end: folded.len(),
            path_index,
        });
    }
    keys.sort_by(|left, right| folded[left.start..left.end].cmp(&folded[right.start..right.end]));

    for target_index in 0..keys.len() {
        let original_index = keys[target_index].path_index;
        keys[original_index].start = target_index;
    }
    for index in 0..keys.len() {
        while keys[index].start != index {
            let target_index = keys[index].start;
            paths.swap(index, target_index);
            keys.swap(index, target_index);
        }
    }
}

fn fill_song_art_hints(images: &[PathBuf], candidates: &mut ArtworkCandidates) {
    for image in images {
        let Some(stem) = image.file_stem() else {
            continue;
        };
        let stem = stem.to_string_lossy();
        if candidates.banner.is_none()
            && song_art_stem_matches_text(&stem, &[], &["banner"], &[" bn"])
        {
            candidates.banner = Some(image.clone());
        }
        if candidates.background.is_none()
            && song_art_stem_matches_text(&stem, &[], &["background"], &["bg"])
        {
            candidates.background = Some(image.clone());
        }
        if candidates.jacket.is_none()
            && song_art_stem_matches_text(&stem, &["jk_"], &["jacket", "albumart"], &[])
        {
            candidates.jacket = Some(image.clone());
        }
        if candidates.cdimage.is_none() && song_art_stem_matches_text(&stem, &[], &[], &["-cd"]) {
            candidates.cdimage = Some(image.clone());
        }
        if candidates.disc.is_none()
            && song_art_stem_matches_text(&stem, &[], &[], &[" disc", " title"])
        {
            candidates.disc = Some(image.clone());
        }
        if candidates.cdtitle.is_none() && song_art_stem_matches_text(&stem, &[], &["cdtitle"], &[])
        {
            candidates.cdtitle = Some(image.clone());
        }
        if candidates.banner.is_some()
            && candidates.background.is_some()
            && candidates.cdtitle.is_some()
            && candidates.jacket.is_some()
            && candidates.cdimage.is_some()
            && candidates.disc.is_some()
        {
            break;
        }
    }
}

fn song_art_matches(candidate: &Path, selected: &Option<PathBuf>) -> bool {
    selected
        .as_ref()
        .is_some_and(|path| song_art_paths_match(path, candidate))
}

#[inline]
fn ascii_lowercase_starts_with(value: &str, expected: &str) -> bool {
    value
        .as_bytes()
        .get(..expected.len())
        .is_some_and(|prefix| {
            prefix
                .iter()
                .map(u8::to_ascii_lowercase)
                .eq(expected.bytes())
        })
}

#[inline]
fn ascii_lowercase_ends_with(value: &str, expected: &str) -> bool {
    value
        .as_bytes()
        .get(value.len().saturating_sub(expected.len())..)
        .filter(|suffix| suffix.len() == expected.len())
        .is_some_and(|suffix| {
            suffix
                .iter()
                .map(u8::to_ascii_lowercase)
                .eq(expected.bytes())
        })
}

#[inline]
fn ascii_lowercase_contains(value: &str, expected: &str) -> bool {
    expected.is_empty()
        || value.as_bytes().windows(expected.len()).any(|window| {
            window
                .iter()
                .map(u8::to_ascii_lowercase)
                .eq(expected.bytes())
        })
}

fn song_art_stem_matches_text(
    stem: &str,
    starts_with: &[&str],
    contains: &[&str],
    ends_with: &[&str],
) -> bool {
    starts_with
        .iter()
        .any(|needle| ascii_lowercase_starts_with(stem, needle))
        || ends_with
            .iter()
            .any(|needle| ascii_lowercase_ends_with(stem, needle))
        || contains
            .iter()
            .any(|needle| ascii_lowercase_contains(stem, needle))
}

fn song_art_paths_match(left: &Path, right: &Path) -> bool {
    let left = left.to_string_lossy();
    let right = right.to_string_lossy();
    left.bytes()
        .map(|byte| {
            if byte == b'\\' {
                b'/'
            } else {
                byte.to_ascii_lowercase()
            }
        })
        .eq(right.bytes().map(|byte| {
            if byte == b'\\' {
                b'/'
            } else {
                byte.to_ascii_lowercase()
            }
        }))
}

fn song_art_is_classified(image: &Path, candidates: &ArtworkCandidates) -> bool {
    song_art_matches(image, &candidates.banner)
        || song_art_matches(image, &candidates.background)
        || song_art_matches(image, &candidates.cdtitle)
        || song_art_matches(image, &candidates.jacket)
        || song_art_matches(image, &candidates.cdimage)
        || song_art_matches(image, &candidates.disc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn artwork_hint_matching_folds_ascii() {
        assert!(song_art_stem_matches_text("BANNER", &[], &["banner"], &[]));
        assert!(song_art_stem_matches_text("jk_Song", &["jk_"], &[], &[]));
        assert!(song_art_stem_matches_text("Song-CD", &[], &[], &["-cd"]));
        let banner = Path::new("Visuals/BANNER.PNG");
        assert!(song_art_paths_match(
            banner,
            Path::new("visuals\\banner.png")
        ));
        assert!(!song_art_paths_match(
            banner,
            Path::new("Visuals/background.png")
        ));
    }

    #[test]
    fn required_tagged_artwork_ignores_optional_simfile_art() {
        let root = test_dir("required-art-fast-path");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let banner = song_dir.join("banner.png");
        let background = song_dir.join("background.png");
        let cdtitle = song_dir.join("cdtitle.png");
        fs::write(&banner, b"banner").unwrap();
        fs::write(&background, b"background").unwrap();
        fs::write(&cdtitle, b"cdtitle").unwrap();

        let artwork = resolve_song_artwork_like_itg(
            &song_dir,
            b"#CDIMAGE:missing.png;#DISCIMAGE:also-missing.png;",
            "banner.png",
            "background.png",
            "cdtitle.png",
            "",
        );

        assert_eq!(artwork.banner_path, Some(banner));
        assert_eq!(artwork.background_path, Some(background));
        assert_eq!(artwork.cdtitle_path, Some(cdtitle));
    }

    #[test]
    fn does_not_use_tagged_cdtitle_as_background() {
        let root = test_dir("tagged-cdtitle-not-background");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let banner_path = song_dir.join("godspeed.png");
        let cdtitle_path = song_dir.join("cdtitle.png");
        image::RgbImage::new(1024, 400).save(&banner_path).unwrap();
        image::RgbaImage::new(512, 512).save(&cdtitle_path).unwrap();

        let artwork = resolve_song_artwork_like_itg(
            &song_dir,
            b"#CDIMAGE:;#DISCIMAGE:;",
            "godspeed.png",
            "",
            "cdtitle.png",
            "",
        );

        assert_eq!(artwork.banner_path, Some(banner_path));
        assert_eq!(artwork.background_path, None);
        assert_eq!(artwork.cdtitle_path, Some(cdtitle_path));
    }

    #[test]
    fn skips_cdtitle_hint_before_dimension_fallback() {
        let root = test_dir("cdtitle-hint-not-background");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        let cdtitle_path = song_dir.join("cdtitle.png");
        image::RgbaImage::new(512, 512).save(&cdtitle_path).unwrap();

        let artwork = resolve_song_artwork_like_itg(&song_dir, b"", "", "", "", "");

        assert_eq!(artwork.banner_path, None);
        assert_eq!(artwork.background_path, None);
        assert_eq!(artwork.cdtitle_path, Some(cdtitle_path));
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-artwork-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
