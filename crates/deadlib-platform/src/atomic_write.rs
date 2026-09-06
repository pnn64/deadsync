//! Atomic file replacement via a synced temp sibling and rename.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Temp sibling for `path`: same directory, `.tmp` appended to the file name.
#[must_use]
pub fn tmp_sibling_path(path: &Path) -> PathBuf {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

/// Write `contents` to `path` and sync it to disk, removing the file on error.
///
/// Syncing before renaming over a destination surfaces write errors (e.g.
/// ENOSPC) while the destination file is still intact.
pub fn write_synced(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let result = File::create(path).and_then(|mut file| {
        file.write_all(contents)?;
        file.sync_all()
    });
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

/// Best-effort sync of `path`'s parent directory so a rename into it is
/// durable.
#[cfg(unix)]
pub fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

/// Directories cannot be opened for syncing on non-unix platforms.
#[cfg(not(unix))]
pub fn sync_parent_dir(_path: &Path) {}

/// Replace `path` with `contents` atomically via a synced temp sibling and
/// rename. On any error the existing file is left untouched and the temp file
/// is removed.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let tmp = tmp_sibling_path(path);
    write_synced(&tmp, contents)?;
    fs::rename(&tmp, path).map_err(|error| {
        let _ = fs::remove_file(&tmp);
        error
    })?;
    sync_parent_dir(path);
    Ok(())
}
