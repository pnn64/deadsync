// Frozen from 071bd24d7 (0.5.1160); imports/visibility only adapted.
use super::*;

pub(super) fn file_metadata_hash(path: &Path) -> Result<u64, std::io::Error> {
    let meta = fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    Ok(modified.wrapping_add(meta.len()))
}

pub(super) fn get_song_directory_hash(simfile_path: &Path) -> Result<u64, std::io::Error> {
    let parent = simfile_path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "simfile path has no parent directory",
        )
    })?;
    let dir = parent.canonicalize()?;
    let mut hash = path_hash(&dir);
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("._"))
        {
            continue;
        }
        hash = hash.wrapping_add(file_metadata_hash(&entry.path())?);
    }
    Ok(hash)
}
