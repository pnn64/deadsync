// Frozen cache filename construction from f2dd892e7.
use super::*;
pub(super) fn cache_path_before(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &Path,
) -> Option<(PathBuf, String)> {
    let canonical = path.canonicalize().ok()?;
    let path_hash = banner_path_hash(&canonical.to_string_lossy());
    let path_hex = format!("{path_hash:016x}");
    let opt_hash = banner_cache_opthash(opts);
    let shard2 = &path_hex[..2];
    let sep = std::path::MAIN_SEPARATOR;
    Some((
        cache_dir.join(format!("{shard2}{sep}{path_hex}-{opt_hash:016x}.rgba")),
        path_hex,
    ))
}
