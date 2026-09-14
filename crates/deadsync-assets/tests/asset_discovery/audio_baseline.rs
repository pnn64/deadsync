// Frozen from 0f2bf56e6 (0.5.1226); function bodies unchanged.
use super::*;

#[inline(always)]
fn is_eligible_ogg(path: &Path) -> bool {
    path.is_file() && is_ogg(path) && !is_skipped_stem(path)
}

pub(super) fn list_ogg_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_eligible_ogg(path))
        .collect();
    out.sort();
    Ok(out)
}
