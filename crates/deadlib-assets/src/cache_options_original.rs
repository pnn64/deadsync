//! Frozen cache naming from 294b5139a0b28c46973abcbd1a617431ea4c1fac.
use super::*;
pub(super) fn banner_cache_opthash(opts: BannerCacheOptions) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write_u8(1);
    hasher.write_u8(u8::from(opts.enabled));
    hasher.finish()
}
pub fn dynamic_image_cache_path_for(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &Path,
) -> Option<(PathBuf, String)> {
    let canonical = path.canonicalize().ok()?;
    let path_hash = banner_path_hash(&canonical.to_string_lossy());
    let path_hex = format!("{path_hash:016x}");
    let opt_hash = banner_cache_opthash(opts);
    let mut filename = *b"0000000000000000-0000000000000000.rgba";
    filename[..16].copy_from_slice(path_hex.as_bytes());
    let hex = b"0123456789abcdef";
    for (i, byte) in opt_hash.to_be_bytes().into_iter().enumerate() {
        filename[17 + i * 2] = hex[usize::from(byte >> 4)];
        filename[18 + i * 2] = hex[usize::from(byte & 15)];
    }
    // Reserve both separators, the two-character shard, and the fixed filename.
    let mut cache_path = PathBuf::with_capacity(cache_dir.as_os_str().len() + filename.len() + 4);
    cache_path.push(cache_dir);
    cache_path.push(&path_hex[..2]);
    cache_path.push(std::str::from_utf8(&filename).expect("cache filename is ASCII hex"));
    Some((cache_path, path_hex))
}