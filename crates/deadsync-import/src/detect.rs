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
    /// ITGmania `Stats.xml` `Guid`, when present. Used to detect a profile that
    /// has already been imported. `None` when the profile has no `Stats.xml`.
    pub source_guid: Option<String>,
}

/// Scans the known ITGmania save locations and returns every local profile
/// found, sorted by display name. Returns an empty list when ITGmania isn't
/// installed or has no profiles.
pub fn detect_itg_local_profiles() -> Vec<ItgProfileCandidate> {
    collect_sorted(local_profiles_roots())
}

/// Resolves where ITGmania would load profiles from for an install at
/// `game_dir`, replicating ITGmania's own methodology (`ArchHooks`): if a
/// `Portable.ini` marker sits next to the executable the install is **portable**
/// and `Save/` lives under the game folder; otherwise ITGmania uses the
/// platform per-user data directory. Profiles are always under
/// `<save>/LocalProfiles/`. Returns the profiles found there, sorted by display
/// name.
pub fn detect_itg_profiles_from_game_dir(game_dir: &Path) -> Vec<ItgProfileCandidate> {
    collect_sorted(itg_local_profiles_roots_for_game_dir(game_dir))
}

/// `true` when `game_dir` is a portable ITGmania install — i.e. it contains a
/// `Portable.ini` marker (matched case-insensitively, like ITGmania's VFS).
pub fn is_portable_install(game_dir: &Path) -> bool {
    if game_dir.join("Portable.ini").is_file() {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(game_dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case("portable.ini"))
            && e.path().is_file()
    })
}

/// The `LocalProfiles` root(s) ITGmania would use for an install at `game_dir`:
/// the portable `<game_dir>/Save/LocalProfiles` when `Portable.ini` is present,
/// otherwise the platform per-user roots (the same ones auto-detection scans).
fn itg_local_profiles_roots_for_game_dir(game_dir: &Path) -> Vec<PathBuf> {
    if is_portable_install(game_dir) {
        vec![game_dir.join("Save").join("LocalProfiles")]
    } else {
        local_profiles_roots()
    }
}

/// Scans every root for profile directories, de-duplicates by canonical path and
/// returns them sorted by display name.
fn collect_sorted(roots: Vec<PathBuf>) -> Vec<ItgProfileCandidate> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
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
        let source_guid = itg::read_source_guid(&dir);
        out.push(ItgProfileCandidate {
            dir,
            display_name,
            source_guid,
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn unique_temp_dir(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("deadsync-detect-{tag}-{pid}-{n}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Creates a minimal ITGmania profile at `root/<id>/Editable.ini`.
    fn make_profile(root: &Path, id: &str, display_name: &str) {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).expect("create profile dir");
        std::fs::write(
            dir.join("Editable.ini"),
            format!("[Editable]\nDisplayName={display_name}\n"),
        )
        .expect("write Editable.ini");
    }

    #[test]
    fn detects_portable_install_profiles() {
        let game_dir = unique_temp_dir("portable");
        std::fs::write(game_dir.join("Portable.ini"), "").unwrap();
        let lp = game_dir.join("Save").join("LocalProfiles");
        make_profile(&lp, "00000000", "Zelda");
        make_profile(&lp, "00000001", "Alpha");

        assert!(is_portable_install(&game_dir));
        let found = detect_itg_profiles_from_game_dir(&game_dir);
        let names: Vec<&str> = found.iter().map(|c| c.display_name.as_str()).collect();
        // Sorted by display name, case-insensitive.
        assert_eq!(names, vec!["Alpha", "Zelda"]);

        std::fs::remove_dir_all(&game_dir).ok();
    }

    #[test]
    fn portable_ini_is_case_insensitive() {
        let game_dir = unique_temp_dir("portable-case");
        std::fs::write(game_dir.join("PORTABLE.INI"), "").unwrap();
        assert!(is_portable_install(&game_dir));
        std::fs::remove_dir_all(&game_dir).ok();
    }

    #[test]
    fn non_portable_resolves_to_per_user_roots() {
        // Without Portable.ini we must NOT read a local Save folder — ITGmania
        // wouldn't either; resolution falls back to the per-user roots.
        let game_dir = unique_temp_dir("non-portable");
        let lp = game_dir.join("Save").join("LocalProfiles");
        make_profile(&lp, "00000000", "ShouldNotBeFound");

        assert!(!is_portable_install(&game_dir));
        assert_eq!(
            itg_local_profiles_roots_for_game_dir(&game_dir),
            local_profiles_roots()
        );

        std::fs::remove_dir_all(&game_dir).ok();
    }
}
