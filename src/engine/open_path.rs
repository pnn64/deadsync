//! Cross-platform helper for revealing a file or directory in the OS file
//! explorer. Fire-and-forget: errors are logged at `warn` level and never
//! surfaced to the caller, since this is a UI-convenience action.
//!
//! Behavior summary:
//! * Windows: spawns `explorer.exe`. For files, uses `/select,<file>` so the
//!   parent directory opens with the file highlighted.
//! * macOS: spawns `open`. For files, uses `-R` to reveal the file in Finder.
//! * Linux / FreeBSD: spawns `xdg-open` against the directory itself (or the
//!   parent directory if `path` points to a file). `xdg-open` has no
//!   reveal-and-select mode.
//!
//! The function always returns `Ok(())` once a spawn has been attempted. The
//! `Result` is only useful for tests that want to assert what was spawned.

use std::path::{Path, PathBuf};

/// Reveals `path` in the OS file explorer. If `path` is a file, the parent
/// directory is opened (and on Windows/macOS the file is highlighted).
///
/// Returns `Ok(())` even if the spawn fails — the failure is logged. The
/// caller should treat this as advisory.
pub fn reveal(path: &Path) -> std::io::Result<()> {
    let target = resolve_target(path);
    spawn_reveal(&target, path)
}

/// Returns the directory that should be opened for `path`. If `path` is a
/// directory (or doesn't exist), returns `path`. If `path` is a file, returns
/// the parent directory.
fn resolve_target(path: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }
    if path.is_file() && let Some(parent) = path.parent() {
        return parent.to_path_buf();
    }
    path.to_path_buf()
}

#[cfg(target_os = "windows")]
fn spawn_reveal(target: &Path, original: &Path) -> std::io::Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("explorer.exe");
    if original.is_file() {
        // /select, foo.txt -> opens the parent dir with foo.txt highlighted.
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(original.as_os_str());
        cmd.arg(arg);
    } else {
        cmd.arg(target.as_os_str());
    }
    spawn_cmd(cmd)
}

#[cfg(target_os = "macos")]
fn spawn_reveal(target: &Path, original: &Path) -> std::io::Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("open");
    if original.is_file() {
        cmd.arg("-R").arg(original.as_os_str());
    } else {
        cmd.arg(target.as_os_str());
    }
    spawn_cmd(cmd)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn spawn_reveal(target: &Path, _original: &Path) -> std::io::Result<()> {
    use std::process::Command;

    let mut cmd = Command::new("xdg-open");
    cmd.arg(target.as_os_str());
    spawn_cmd(cmd)
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd"
)))]
fn spawn_reveal(_target: &Path, _original: &Path) -> std::io::Result<()> {
    log::warn!("open_path::reveal is not implemented for this platform");
    Ok(())
}

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "freebsd"
))]
fn spawn_cmd(mut cmd: std::process::Command) -> std::io::Result<()> {
    match cmd.spawn() {
        Ok(_child) => Ok(()),
        Err(e) => {
            log::warn!("failed to spawn file-explorer command: {e}");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_target_returns_directory_unchanged() {
        let dir = std::env::temp_dir();
        assert_eq!(resolve_target(&dir), dir);
    }

    #[test]
    fn resolve_target_returns_parent_for_existing_file() {
        let tmp = std::env::temp_dir();
        let file = tmp.join(format!(
            "deadsync-open-path-test-{}.tmp",
            std::process::id()
        ));
        fs::write(&file, b"x").expect("write tmp file");
        let parent = resolve_target(&file);
        assert_eq!(parent, tmp);
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn resolve_target_returns_path_for_nonexistent() {
        let tmp = std::env::temp_dir().join("deadsync-this-does-not-exist-xyz");
        assert_eq!(resolve_target(&tmp), tmp);
    }
}
