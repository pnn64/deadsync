// Frozen from 91e9d8021 (0.5.1227); function bodies unchanged.
use super::*;

fn source_newer_than_cache(src: &Path, cache: &Path) -> bool {
    let src_m = fs::metadata(src).ok().and_then(|m| m.modified().ok());
    let cache_m = fs::metadata(cache).ok().and_then(|m| m.modified().ok());
    match (src_m, cache_m) {
        (Some(src_m), Some(cache_m)) => src_m > cache_m,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(super) fn load_raw_cached_banner_image(cache_path: &Path) -> Option<RgbaImage> {
    let mut file = fs::File::open(cache_path).ok()?;
    let file_len = usize::try_from(file.metadata().ok()?.len()).ok()?;
    let mut header = [0_u8; BANNER_CACHE_HEADER_SIZE];
    file.read_exact(&mut header).ok()?;
    if header[..8] != BANNER_CACHE_MAGIC {
        return None;
    }

    let width = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    let height = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
    let payload_len = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    if file_len != BANNER_CACHE_HEADER_SIZE.checked_add(payload_len)? {
        return None;
    }

    let mut payload = vec![0_u8; payload_len];
    file.read_exact(&mut payload).ok()?;
    RgbaImage::from_raw(width, height, payload)
}

pub(super) fn load_cached_banner_image(cache_path: &Path, source_path: &Path) -> Option<RgbaImage> {
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

pub(super) fn prune_stale_banner_cache_variants(cache_path: &Path, path_hex: &str) {
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

fn save_cached_banner_image(cache_path: &Path, path_hex: &str, rgba: &RgbaImage) {
    if !save_raw_cached_banner_image(cache_path, rgba) {
        return;
    }
    prune_stale_banner_cache_variants(cache_path, path_hex);
}

pub(super) fn ensure_cached_dynamic_image_at(
    path: &Path,
    opts: BannerCacheOptions,
    cache_path: &Path,
    path_hex: &str,
) -> image::ImageResult<bool> {
    if load_cached_banner_image(cache_path, path).is_some() {
        return Ok(false);
    }
    let rgba = build_cached_banner_rgba(path, opts)?;
    save_cached_banner_image(cache_path, path_hex, &rgba);
    Ok(true)
}
