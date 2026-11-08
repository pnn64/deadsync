// Frozen from main 1139884c for differential tests and paired benchmarks.
use super::*;

fn dynamic_image_cache_path_for_original(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &Path,
) -> Option<(PathBuf, String)> {
    let canonical = path.canonicalize().ok()?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical.to_string_lossy().replace('\\', "/").as_bytes());
    let path_hash = hasher.finish();
    let path_hex = format!("{path_hash:016x}");
    let opt_hash = banner_cache_opthash(opts);
    let shard2 = &path_hex[..2];
    let stem = format!("{path_hex}-{opt_hash:016x}");
    let dir = cache_dir.join(shard2);
    Some((dir.join(format!("{stem}.rgba")), path_hex))
}

fn load_raw_cached_banner_image_original(cache_path: &Path) -> Option<RgbaImage> {
    let mut file = fs::File::open(cache_path).ok()?;
    let file_len = usize::try_from(file.metadata().ok()?.len()).ok()?;
    let (width, height, payload_len) = read_raw_banner_header(&mut file, file_len)?;
    let mut payload = vec![0_u8; payload_len];
    file.read_exact(&mut payload).ok()?;
    RgbaImage::from_raw(width, height, payload)
}

fn validate_raw_cached_banner_original(cache_path: &Path) -> Option<()> {
    let mut file = fs::File::open(cache_path).ok()?;
    let file_len = usize::try_from(file.metadata().ok()?.len()).ok()?;
    validate_raw_banner(&mut file, file_len)
}

fn with_cached_banner_original<T>(
    cache_path: &Path,
    source_path: &Path,
    read: impl FnOnce(&Path) -> Option<T>,
) -> Option<T> {
    let metadata = fs::metadata(cache_path).ok()?;
    if metadata.is_file() && !source_newer_than_cache(source_path, &metadata) {
        if let Some(value) = read(cache_path) {
            return Some(value);
        }
        let _ = fs::remove_file(cache_path);
        debug!(
            "Invalid raw banner cache '{}'; rebuilding.",
            cache_path.to_string_lossy()
        );
    }
    None
}
