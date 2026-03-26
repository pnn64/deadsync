use crate::core::video;
use image::RgbaImage;
use log::{debug, warn};
use std::{
    collections::HashSet,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use twox_hash::XxHash64;

use super::textures::open_image_fallback_quiet;
static BANNER_CACHE_TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug)]
pub(crate) struct BannerCacheOptions {
    pub(crate) enabled: bool,
}

fn banner_cache_opthash(opts: BannerCacheOptions) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write_u8(1);
    hasher.write_u8(u8::from(opts.enabled));
    hasher.finish()
}

const BANNER_CACHE_MAGIC: [u8; 8] = *b"DSBNR02\0";
const BANNER_CACHE_HEADER_SIZE: usize = 16;

pub(crate) fn dynamic_image_cache_path_for(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> Option<(PathBuf, String)> {
    let canonical = path.canonicalize().ok()?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical.to_string_lossy().replace('\\', "/").as_bytes());
    let path_hash = hasher.finish();
    let path_hex = format!("{path_hash:016x}");
    let opt_hash = banner_cache_opthash(opts);
    let shard2 = &path_hex[..2];
    let stem = format!("{path_hex}-{opt_hash:016x}");
    let dir = Path::new(cache_dir).join(shard2);
    Some((dir.join(format!("{stem}.rgba")), path_hex))
}

fn source_newer_than_cache(src: &Path, cache: &Path) -> bool {
    let src_m = fs::metadata(src).ok().and_then(|m| m.modified().ok());
    let cache_m = fs::metadata(cache).ok().and_then(|m| m.modified().ok());
    match (src_m, cache_m) {
        (Some(src_m), Some(cache_m)) => src_m > cache_m,
        (Some(_), None) => true,
        _ => false,
    }
}

fn ensure_cache_parent(cache_path: &Path) -> bool {
    if let Some(parent) = cache_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(
            "Failed to create banner cache directory '{}': {e}",
            parent.display()
        );
        return false;
    }
    true
}

fn load_raw_cached_banner_image(cache_path: &Path) -> Option<RgbaImage> {
    let mut bytes = fs::read(cache_path).ok()?;
    if bytes.len() < BANNER_CACHE_HEADER_SIZE || bytes[..8] != BANNER_CACHE_MAGIC {
        return None;
    }
    let width = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let height = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let payload_len = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    if bytes.len() != BANNER_CACHE_HEADER_SIZE.saturating_add(payload_len) {
        return None;
    }
    let payload = bytes.split_off(BANNER_CACHE_HEADER_SIZE);
    RgbaImage::from_raw(width, height, payload)
}

pub(crate) fn save_raw_cached_banner_image(cache_path: &Path, rgba: &RgbaImage) -> bool {
    if !ensure_cache_parent(cache_path) {
        return false;
    }
    let raw = rgba.as_raw();
    let mut out = Vec::<u8>::with_capacity(BANNER_CACHE_HEADER_SIZE.saturating_add(raw.len()));
    out.extend_from_slice(&BANNER_CACHE_MAGIC);
    out.extend_from_slice(&rgba.width().to_le_bytes());
    out.extend_from_slice(&rgba.height().to_le_bytes());
    out.extend_from_slice(raw);

    let parent = cache_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = cache_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("banner.rgba");
    let tmp_seq = BANNER_CACHE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{tmp_seq}", std::process::id()));

    if let Err(e) = fs::write(&tmp_path, out) {
        warn!(
            "Failed to save raw banner cache '{}': {e}",
            cache_path.to_string_lossy()
        );
        let _ = fs::remove_file(&tmp_path);
        return false;
    }

    if fs::rename(&tmp_path, cache_path).is_err() {
        let _ = fs::remove_file(cache_path);
        if let Err(e) = fs::rename(&tmp_path, cache_path) {
            warn!(
                "Failed to finalize raw banner cache '{}': {e}",
                cache_path.to_string_lossy()
            );
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
    }

    true
}

fn load_cached_banner_image(cache_path: &Path, source_path: &Path) -> Option<RgbaImage> {
    if cache_path.is_file() && !source_newer_than_cache(source_path, cache_path) {
        if let Some(rgba) = load_raw_cached_banner_image(cache_path) {
            return Some(rgba);
        }
        let _ = fs::remove_file(cache_path);
        debug!(
            "Invalid raw banner cache '{}'; rebuilding.",
            cache_path.to_string_lossy()
        );
    }
    None
}

fn prune_stale_banner_cache_variants(cache_path: &Path, path_hex: &str) {
    let Some(parent) = cache_path.parent() else {
        return;
    };
    let Some(current_name) = cache_path.file_name().and_then(|n| n.to_str()) else {
        return;
    };

    let prefix = format!("{path_hex}-");
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == current_name || !name.starts_with(&prefix) {
            continue;
        }
        if !(name.ends_with(".rgba") || name.ends_with(".png")) {
            continue;
        }
        if let Err(e) = fs::remove_file(&path) {
            warn!(
                "Failed to remove stale banner cache variant '{}': {e}",
                path.display()
            );
        }
    }
}

pub(crate) fn save_cached_banner_image(cache_path: &Path, path_hex: &str, rgba: &RgbaImage) {
    if !save_raw_cached_banner_image(cache_path, rgba) {
        return;
    }
    prune_stale_banner_cache_variants(cache_path, path_hex);
}

pub(crate) fn load_or_build_cached_dynamic_image(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> image::ImageResult<RgbaImage> {
    let Some((cache_path, path_hex)) = dynamic_image_cache_path_for(path, opts, cache_dir) else {
        return build_cached_banner_rgba(path, opts);
    };

    if let Some(rgba) = load_cached_banner_image(&cache_path, path) {
        return Ok(rgba);
    }

    let rgba = build_cached_banner_rgba(path, opts)?;
    save_cached_banner_image(&cache_path, &path_hex, &rgba);
    Ok(rgba)
}

#[inline(always)]
pub(crate) fn is_cacheable_dynamic_image_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "tga"
            | "tif"
            | "tiff"
            | "mp4"
            | "avi"
            | "m4v"
            | "mov"
            | "webm"
            | "mkv"
            | "mpg"
            | "mpeg"
    )
}

#[inline(always)]
pub(crate) fn is_dynamic_video_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp4" | "avi" | "m4v" | "mov" | "webm" | "mkv" | "mpg" | "mpeg"
    )
}

pub(crate) fn ensure_cached_dynamic_image_on_disk(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> image::ImageResult<bool> {
    let Some((cache_path, path_hex)) = dynamic_image_cache_path_for(path, opts, cache_dir) else {
        return Ok(false);
    };
    if load_cached_banner_image(&cache_path, path).is_some() {
        return Ok(false);
    }
    let rgba = build_cached_banner_rgba(path, opts)?;
    save_cached_banner_image(&cache_path, &path_hex, &rgba);
    Ok(true)
}

fn build_cached_banner_rgba(
    path: &Path,
    _opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    if is_dynamic_video_path(path) {
        return video::load_poster(path)
            .map_err(|e| image::ImageError::IoError(std::io::Error::other(e)));
    }
    Ok(open_image_fallback_quiet(path)?.to_rgba8())
}

pub(crate) fn collect_stale_dynamic_keys<'a>(
    current: impl Iterator<Item = &'a String>,
    desired: &HashSet<String>,
) -> Vec<String> {
    current
        .filter(|key| !desired.contains(*key))
        .cloned()
        .collect()
}

pub(crate) fn dedupe_dynamic_keys(keys: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::with_capacity(keys.len());
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}
