pub(super) fn write_cache_file(path: &Path, mut payload: CacheFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cache path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    while bytes.len() as u64 > MAX_CACHE_BYTES && !payload.plots.is_empty() {
        payload.plots.pop();
        log::warn!(
            "Null-or-die cached visuals exceeded {} MiB; dropping the oldest plot.",
            MAX_CACHE_BYTES / (1024 * 1024)
        );
        bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    }
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(format!(
            "serialized cache exceeds the {} MiB limit",
            MAX_CACHE_BYTES / (1024 * 1024)
        ));
    }
    let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".null-or-die-sync-{}-{id}.tmp", std::process::id()));
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    if fs::rename(&temp, path).is_err() {
        let _ = fs::remove_file(path);
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
    }
    Ok(())
}
