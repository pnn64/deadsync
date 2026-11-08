// Frozen from 3a64350287c8fda7f3d0bfa697cfa39c40c4af2c for differential tests and paired benchmarks.
use super::*;

pub(super) fn source_hash(game: &str, data: &noteskin_itg::NoteskinData) -> Result<String, String> {
    let sources = labeled_source_paths(data, source_paths(data));
    let mut hasher = XxHash64::default();
    hasher.write_u32(noteskin_compiled::CACHE_SCHEMA_VERSION);
    hasher.write_u32(COMPILER_VERSION);
    hasher.write(game.as_bytes());
    hasher.write(data.name.as_bytes());
    for (label, path) in sources {
        hasher.write(label.as_bytes());
        let bytes = fs::read(&path)
            .map_err(|err| format!("failed to read '{}' for hashing: {err}", path.display()))?;
        hasher.write(&bytes);
    }
    Ok(format!("{:016x}", hasher.finish()))
}
