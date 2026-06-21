//! Auto-discovery of ITGmania local profiles.
//!
//! ITGmania stores per-machine save data in a platform-specific directory, with
//! local profiles under `<Save>/LocalProfiles/<id>/`. This module scans the
//! likely save roots and returns every directory that looks like an ITGmania
//! profile so the import UI can present a picker without asking the user to
//! browse the filesystem manually.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::itg;

/// One ITGmania local profile found on disk.
#[derive(Debug, Clone)]
pub struct ItgProfileCandidate {
    /// Absolute path to the `LocalProfiles/<id>/` directory.
    pub dir: PathBuf,
    /// `DisplayName` from `Editable.ini`, falling back to the folder name.
    pub display_name: String,
}

/// Scans the known ITGmania save locations and returns every local profile
/// found, sorted by display name. Returns an empty list when ITGmania isn't
/// installed or has no profiles.
pub fn detect_itg_local_profiles() -> Vec<ItgProfileCandidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in local_profiles_roots() {
        collect_from_root(&root, &mut out, &mut seen);
    }
    out.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    out
}

fn collect_from_root(root: &Path, out: &mut Vec<ItgProfileCandidate>, seen: &mut HashSet<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() || !itg::is_itg_profile_dir(&dir) {
            continue;
        }
        let key = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !seen.insert(key) {
            continue;
        }
        let display_name = itg::read_display_name(&dir).unwrap_or_else(|| {
            dir.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        out.push(ItgProfileCandidate { dir, display_name });
    }
}

/// Candidate `LocalProfiles` directories across platforms. Most won't exist on
/// any given machine; non-existent roots are silently skipped during scanning.
fn local_profiles_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    // Windows: %APPDATA%\ITGmania\Save\LocalProfiles
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(
            PathBuf::from(&appdata)
                .join("ITGmania")
                .join("Save")
                .join("LocalProfiles"),
        );
    }

    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        // Linux: ~/.itgmania/Save/LocalProfiles
        roots.push(home.join(".itgmania").join("Save").join("LocalProfiles"));
        // macOS: ~/Library/Application Support/ITGmania/Save/LocalProfiles
        roots.push(
            home.join("Library")
                .join("Application Support")
                .join("ITGmania")
                .join("Save")
                .join("LocalProfiles"),
        );
    }

    roots
}
