// Frozen from 3a64350287c8fda7f3d0bfa697cfa39c40c4af2c for differential tests and paired benchmarks.
use super::*;

pub(super) fn resolve_asset_path(paths: &AssetPaths, path: &str) -> PathBuf {
    let original = PathBuf::from(path);
    if original.is_absolute() {
        return original;
    }
    for root in &paths.search_roots {
        let candidate = root.join(path);
        if candidate.exists() {
            return candidate;
        }
    }
    original
}
