// Frozen from 856b03b86 (0.5.1154); helper visibility adapted.
use super::*;

pub(super) fn portable_archive_parts(name: &str) -> Result<Vec<String>, StepManiaOnlineError> {
    if name.is_empty() || name.starts_with('/') || name.starts_with('\\') || name.contains('\0') {
        return Err(StepManiaOnlineError::Archive(format!(
            "entry '{name}' has an invalid path"
        )));
    }
    let mut parts = Vec::new();
    for part in name.split(['/', '\\']) {
        if part.is_empty() {
            continue;
        }
        let is_prefix = parts.is_empty();
        let drive_prefix = is_prefix
            && part.len() == 2
            && part.as_bytes()[0].is_ascii_alphabetic()
            && part.as_bytes()[1] == b':';
        let invalid_output_name = !is_prefix && part.chars().any(invalid_path_char);
        if part == "."
            || part == ".."
            || part.chars().any(char::is_control)
            || drive_prefix
            || invalid_output_name
        {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{name}' has an invalid path component"
            )));
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        return Err(StepManiaOnlineError::Archive(format!(
            "entry '{name}' has no path components"
        )));
    }
    Ok(parts)
}

pub(super) fn inspect_archive(
    archive_path: &Path,
    archive_bytes: u64,
) -> Result<ArchivePlan, StepManiaOnlineError> {
    let file =
        File::open(archive_path).map_err(|error| io_error("open the pack archive", error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
    if archive.is_empty() {
        return Err(StepManiaOnlineError::Archive(
            "archive is empty".to_string(),
        ));
    }
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(StepManiaOnlineError::Archive(format!(
            "archive exceeds {MAX_ARCHIVE_ENTRIES} entries"
        )));
    }
    let uncompressed_limit = archive_bytes
        .saturating_mul(12)
        .saturating_add(UNCOMPRESSED_HEADROOM_BYTES)
        .min(MAX_UNCOMPRESSED_BYTES);
    let mut prefix: Option<String> = None;
    let mut total_bytes = 0u64;
    let mut has_simfile = false;
    let mut output_files = HashSet::with_capacity(archive.len().min(4096));
    for idx in 0..archive.len() {
        let entry = archive
            .by_index(idx)
            .map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
        if entry.name().len() > MAX_ARCHIVE_PATH_BYTES {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry path exceeds {MAX_ARCHIVE_PATH_BYTES} bytes"
            )));
        }
        if entry.enclosed_name().is_none() {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' escapes the pack root",
                entry.name()
            )));
        }
        if entry.is_symlink() {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' is a symbolic link",
                entry.name()
            )));
        }
        let is_dir = entry.is_dir() || entry.name().ends_with('\\');
        if !safe_unix_entry_type(entry.unix_mode(), is_dir) {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' is not a regular file or directory",
                entry.name()
            )));
        }
        let parts = portable_archive_parts(entry.name())?;
        let entry_prefix = parts
            .first()
            .ok_or_else(|| StepManiaOnlineError::Archive("entry has no path".to_string()))?;
        match &prefix {
            Some(expected) if expected != entry_prefix => {
                return Err(StepManiaOnlineError::Archive(
                    "archive does not have one top-level pack directory".to_string(),
                ));
            }
            None => prefix = Some(entry_prefix.clone()),
            _ => {}
        }
        if is_dir {
            continue;
        }
        if parts.len() == 1 {
            return Err(StepManiaOnlineError::Archive(
                "archive contains a file outside its pack directory".to_string(),
            ));
        }
        total_bytes = total_bytes.checked_add(entry.size()).ok_or_else(|| {
            StepManiaOnlineError::Archive("uncompressed size overflow".to_string())
        })?;
        if total_bytes > uncompressed_limit {
            return Err(StepManiaOnlineError::Archive(format!(
                "uncompressed content exceeds {uncompressed_limit} bytes"
            )));
        }
        let output_key = parts[1..].join("/").to_lowercase();
        if !output_files.insert(output_key) {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' duplicates another output path",
                entry.name()
            )));
        }
        has_simfile |= is_simfile(parts.last().map(String::as_str).unwrap_or_default());
    }
    if !has_simfile {
        return Err(StepManiaOnlineError::Archive(
            "archive contains no .sm, .ssc, or .dwi simfiles".to_string(),
        ));
    }
    Ok(ArchivePlan {
        prefix: prefix.unwrap_or_default(),
    })
}

pub(super) fn extract_archive(
    archive_path: &Path,
    staging: &Path,
    archive_bytes: u64,
) -> Result<(), StepManiaOnlineError> {
    let plan = inspect_archive(archive_path, archive_bytes)?;
    fs::create_dir(staging).map_err(|error| io_error("create the staging directory", error))?;
    let file =
        File::open(archive_path).map_err(|error| io_error("open the pack archive", error))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|error| StepManiaOnlineError::Archive(error.to_string()))?;
        let parts = portable_archive_parts(entry.name())?;
        if parts.first().is_none_or(|prefix| prefix != &plan.prefix) {
            return Err(StepManiaOnlineError::Archive(
                "archive roots changed while extracting".to_string(),
            ));
        }
        let relative = &parts[1..];
        if relative.is_empty() {
            continue;
        }
        let output = relative
            .iter()
            .fold(staging.to_path_buf(), |path, part| path.join(part));
        let is_dir = entry.is_dir() || entry.name().ends_with('\\');
        if is_dir {
            fs::create_dir_all(&output)
                .map_err(|error| io_error("create an extracted directory", error))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create an extracted directory", error))?;
        }
        let mut output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|error| io_error("create an extracted file", error))?;
        let expected = entry.size();
        let mut limited = (&mut entry).take(expected.saturating_add(1));
        let copied = std::io::copy(&mut limited, &mut output_file)
            .map_err(|error| io_error("extract a pack file", error))?;
        if copied != expected {
            return Err(StepManiaOnlineError::Archive(format!(
                "entry '{}' produced {copied} bytes, expected {expected}",
                entry.name()
            )));
        }
    }
    Ok(())
}

pub(super) fn sanitized_pack_name(raw: &str, pack_id: u64) -> (String, bool) {
    let trimmed = raw.trim();
    let mut changed = trimmed != raw;
    let mut output = String::with_capacity(trimmed.len().min(DESTINATION_MAX_CHARS));
    for ch in trimmed.chars() {
        let replacement = invalid_path_char(ch) || matches!(ch, '/' | '\\');
        if replacement {
            changed = true;
            if !output.ends_with('_') {
                output.push('_');
            }
        } else if output.chars().count() < DESTINATION_MAX_CHARS {
            output.push(ch);
        } else {
            changed = true;
        }
    }
    let clean_len = output.trim_end_matches([' ', '.']).len();
    if clean_len != output.len() {
        output.truncate(clean_len);
        changed = true;
    }
    if output.is_empty() || matches!(output.as_str(), "." | "..") {
        output = "StepManiaOnline Pack".to_string();
        changed = true;
    }
    let stem = output.split('.').next().unwrap_or_default();
    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        output.insert(0, '_');
        changed = true;
    }
    if changed {
        output = with_pack_id(output.as_str(), pack_id);
    }
    (output, changed)
}

pub(super) fn with_pack_id(name: &str, pack_id: u64) -> String {
    let suffix = format!(" [SMO {pack_id}]");
    let keep = DESTINATION_MAX_CHARS.saturating_sub(suffix.chars().count());
    let mut base: String = name.chars().take(keep).collect();
    let clean_len = base.trim_end_matches([' ', '.']).len();
    base.truncate(clean_len);
    base.push_str(suffix.as_str());
    base
}

pub(super) fn update_install(
    runtime: &mut RuntimeState,
    pack_id: u64,
    update: impl FnOnce(&mut InstallSnapshot),
) {
    let mut snapshot = (*runtime.snapshot).clone();
    if let Some(install) = snapshot
        .installs
        .iter_mut()
        .find(|install| install.pack_id == pack_id)
    {
        update(install);
        runtime.snapshot = Arc::new(snapshot);
    }
}

pub(super) fn queue_install_snapshot(
    runtime: &mut RuntimeState,
    pack: &PackInfo,
) -> Result<(), String> {
    let mut snapshot = (*runtime.snapshot).clone();
    if let Some(install) = snapshot
        .installs
        .iter_mut()
        .find(|install| install.pack_id == pack.id)
    {
        match install.phase {
            InstallPhase::Queued | InstallPhase::Downloading | InstallPhase::Extracting => {
                return Err(format!("'{}' is already queued.", pack.name));
            }
            InstallPhase::Installed => {
                return Err(format!(
                    "'{}' was already installed this session.",
                    pack.name
                ));
            }
            InstallPhase::Error => {
                *install = queued_install(pack);
                runtime.snapshot = Arc::new(snapshot);
                return Ok(());
            }
        }
    }
    if snapshot.installs.len() == MAX_INSTALLS {
        let terminal = snapshot.installs.iter().position(|install| {
            matches!(install.phase, InstallPhase::Installed | InstallPhase::Error)
        });
        let Some(index) = terminal else {
            return Err("Too many pack installs are active.".to_string());
        };
        let evicted = snapshot.installs.remove(index);
        log::debug!(
            "Evicted terminal StepManiaOnline install history for pack {}.",
            evicted.pack_id
        );
    }
    snapshot.installs.push(queued_install(pack));
    runtime.snapshot = Arc::new(snapshot);
    Ok(())
}

// Original set_install_progress closure, with the mutex acquisition omitted so
// benchmarks and tests can use an isolated RuntimeState on both sides.
pub(super) fn update_install_progress(
    runtime: &mut RuntimeState,
    pack_id: u64,
    downloaded_bytes: u64,
    total_bytes: u64,
) {
    update_install(runtime, pack_id, |install| {
        install.phase = InstallPhase::Downloading;
        install.downloaded_bytes = downloaded_bytes;
        install.total_bytes = total_bytes;
        install.message = Some("Downloading pack archive...".to_string());
    });
}
