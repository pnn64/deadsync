use crate::media::{
    is_mac_resource_fork, is_song_art_image, resolve_song_asset_path_like_itg, song_art_file_key,
    song_art_file_stem,
};
use crate::tags::latest_simfile_tag_value;
use image::image_dimensions;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct ResolvedSongArtwork {
    pub banner_path: Option<PathBuf>,
    pub background_path: Option<PathBuf>,
    pub cdtitle_path: Option<PathBuf>,
}

pub fn resolve_song_artwork_like_itg(
    song_dir: &Path,
    simfile_data: &[u8],
    banner_tag: &str,
    background_tag: &str,
    cdtitle_tag: &str,
    jacket_tag: &str,
) -> ResolvedSongArtwork {
    let mut banner = resolve_song_asset_path_like_itg(song_dir, banner_tag);
    let mut background = resolve_song_asset_path_like_itg(song_dir, background_tag);
    let mut cdtitle = resolve_song_asset_path_like_itg(song_dir, cdtitle_tag);
    let mut jacket = resolve_song_asset_path_like_itg(song_dir, jacket_tag);
    let mut cdimage = resolve_song_asset_path_like_itg(
        song_dir,
        &latest_simfile_tag_value(simfile_data, b"#CDIMAGE:"),
    );
    let mut disc = resolve_song_asset_path_like_itg(
        song_dir,
        &latest_simfile_tag_value(simfile_data, b"#DISCIMAGE:"),
    );

    if banner.is_some() && background.is_some() && cdtitle.is_some() {
        return ResolvedSongArtwork {
            banner_path: banner,
            background_path: background,
            cdtitle_path: cdtitle,
        };
    }

    let images = list_song_art_images(song_dir);
    if banner.is_none() {
        banner = find_song_art_hint(&images, &[], &["banner"], &[" bn"]);
    }
    if background.is_none() {
        background = find_song_art_hint(&images, &[], &["background"], &["bg"]);
    }
    if jacket.is_none() {
        jacket = find_song_art_hint(&images, &["jk_"], &["jacket", "albumart"], &[]);
    }
    if cdimage.is_none() {
        cdimage = find_song_art_hint(&images, &[], &[], &["-cd"]);
    }
    if disc.is_none() {
        disc = find_song_art_hint(&images, &[], &[], &[" disc", " title"]);
    }
    if cdtitle.is_none() {
        cdtitle = find_song_art_hint(&images, &[], &["cdtitle"], &[]);
    }

    for image in &images {
        if banner.is_some() && background.is_some() && cdtitle.is_some() {
            break;
        }
        if song_art_is_classified(
            image,
            &banner,
            &background,
            &cdtitle,
            &jacket,
            &cdimage,
            &disc,
        ) {
            continue;
        }

        let Ok((width, height)) = image_dimensions(image) else {
            continue;
        };
        if background.is_none() && width >= 320 && height >= 240 {
            background = Some(image.clone());
            continue;
        }
        if banner.is_none() && (100..=320).contains(&width) && (50..=240).contains(&height) {
            banner = Some(image.clone());
            continue;
        }
        if banner.is_none() && width > 200 && height > 0 && width as f32 / height as f32 > 2.0 {
            banner = Some(image.clone());
            continue;
        }
        if cdtitle.is_none() && width <= 100 && height <= 48 {
            cdtitle = Some(image.clone());
            continue;
        }
        if jacket.is_none() && width == height {
            jacket = Some(image.clone());
            continue;
        }
        if disc.is_none() && width > height && banner.is_some() && !song_art_matches(image, &banner)
        {
            disc = Some(image.clone());
            continue;
        }
        if cdimage.is_none() && width == height {
            cdimage = Some(image.clone());
        }
    }

    ResolvedSongArtwork {
        banner_path: banner,
        background_path: background,
        cdtitle_path: cdtitle,
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
    paths.sort_by_cached_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    paths
}

fn find_song_art_hint(
    images: &[PathBuf],
    starts_with: &[&str],
    contains: &[&str],
    ends_with: &[&str],
) -> Option<PathBuf> {
    for image in images {
        let Some(stem) = song_art_file_stem(image) else {
            continue;
        };
        if starts_with.iter().any(|needle| stem.starts_with(needle)) {
            return Some(image.clone());
        }
        if ends_with.iter().any(|needle| stem.ends_with(needle)) {
            return Some(image.clone());
        }
        if contains.iter().any(|needle| stem.contains(needle)) {
            return Some(image.clone());
        }
    }
    None
}

fn song_art_matches(candidate: &Path, selected: &Option<PathBuf>) -> bool {
    selected
        .as_ref()
        .is_some_and(|path| song_art_file_key(path) == song_art_file_key(candidate))
}

fn song_art_is_classified(
    image: &Path,
    banner: &Option<PathBuf>,
    background: &Option<PathBuf>,
    cdtitle: &Option<PathBuf>,
    jacket: &Option<PathBuf>,
    cdimage: &Option<PathBuf>,
    disc: &Option<PathBuf>,
) -> bool {
    song_art_matches(image, banner)
        || song_art_matches(image, background)
        || song_art_matches(image, cdtitle)
        || song_art_matches(image, jacket)
        || song_art_matches(image, cdimage)
        || song_art_matches(image, disc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

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
