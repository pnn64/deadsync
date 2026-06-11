//! Tiny argv parser for the in-app updater driver.
//!
//! The recognised set is:
//!
//! * `--cleanup-old <staging-dir>`— runs the post-swap cleanup pass
//!   (delete `*.old` files under exe_dir, remove the staging dir),
//!   then continues into normal startup.  PR-11 describes the
//!   Windows side; the Unix side is a no-op cleanup but the flag is
//!   accepted so the same parent process can launch either platform.
//!
//! * `--restart` — informational marker logged at startup; the menu
//!   uses it to display "Updated to vX.Y.Z" once.
//!
//! * `--no-update-check` — skips the startup network check.
//!
//! * `--apply-update <archive> --apply-sha256 <hex>
//!   [--apply-parent-pid <pid>]` - helper mode used on Unix self-updates.
//!
//! Unknown flags are passed through unchanged; we don't want to
//! conflict with any future windowing-system or test runner argv.
//!
//! All parsing is pure and table-tested.

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyRequest {
    pub archive_path: PathBuf,
    pub sha256_hex: String,
    pub parent_pid: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpdaterCli {
    /// `Some(staging_dir)` if `--cleanup-old <path>` was passed.
    pub cleanup_old: Option<PathBuf>,
    /// `true` if `--restart` was passed (we just self-updated).
    pub restart: bool,
    /// `true` if `--no-update-check` was passed (skip startup check).
    pub no_update_check: bool,
    /// Helper-mode request. Normal startup must not continue when
    /// this is present.
    pub apply_update: Option<ApplyRequest>,
    /// Argv with our recognised flags removed; preserved for any
    /// downstream consumer (currently none, but keeps us future-proof).
    pub remaining: Vec<String>,
}

impl UpdaterCli {
    /// Parse the supplied argv, ignoring `argv[0]`.  Unknown flags
    /// pass through into [`Self::remaining`] unchanged.
    pub fn parse<I, S>(argv: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut iter = argv.into_iter().map(Into::into).peekable();
        // Skip program name if present.
        let _ = iter.next();
        let mut out = UpdaterCli::default();
        let mut apply_archive: Option<PathBuf> = None;
        let mut apply_sha256: Option<String> = None;
        let mut apply_parent_pid: Option<u32> = None;
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--cleanup-old" => {
                    if let Some(path) = iter.next() {
                        out.cleanup_old = Some(PathBuf::from(path));
                    }
                }
                a if a.starts_with("--cleanup-old=") => {
                    let value = &a["--cleanup-old=".len()..];
                    if !value.is_empty() {
                        out.cleanup_old = Some(PathBuf::from(value));
                    }
                }
                "--restart" => out.restart = true,
                "--no-update-check" => out.no_update_check = true,
                "--apply-update" => {
                    if let Some(path) = iter.next() {
                        apply_archive = Some(PathBuf::from(path));
                    }
                }
                a if a.starts_with("--apply-update=") => {
                    let value = &a["--apply-update=".len()..];
                    if !value.is_empty() {
                        apply_archive = Some(PathBuf::from(value));
                    }
                }
                "--apply-sha256" => {
                    if let Some(hex) = iter.next() {
                        apply_sha256 = Some(hex);
                    }
                }
                a if a.starts_with("--apply-sha256=") => {
                    let value = &a["--apply-sha256=".len()..];
                    if !value.is_empty() {
                        apply_sha256 = Some(value.to_string());
                    }
                }
                "--apply-parent-pid" => {
                    if let Some(pid) = iter.next().and_then(|s| s.parse::<u32>().ok()) {
                        apply_parent_pid = Some(pid);
                    }
                }
                a if a.starts_with("--apply-parent-pid=") => {
                    let value = &a["--apply-parent-pid=".len()..];
                    if let Ok(pid) = value.parse::<u32>() {
                        apply_parent_pid = Some(pid);
                    }
                }
                _ => out.remaining.push(arg),
            }
        }
        if let (Some(archive_path), Some(sha256_hex)) = (apply_archive, apply_sha256) {
            out.apply_update = Some(ApplyRequest {
                archive_path,
                sha256_hex,
                parent_pid: apply_parent_pid,
            });
        }
        out
    }

    /// Convenience: parse `std::env::args()`.  Lifted out so tests
    /// can hit the table-driven [`parse`](Self::parse) without
    /// touching process state.
    pub fn from_env() -> Self {
        Self::parse(std::env::args())
    }
}

/// Runs the post-update cleanup pass.
///
/// Errors are intentionally swallowed (the caller is post-startup
/// best-effort), but the (removed_count, staging_removed) tuple is
/// returned for diagnostics + tests.
pub fn run_cleanup(exe_dir: &std::path::Path, staging_dir: &std::path::Path) -> (usize, bool) {
    // The `staging_dir` argument is retained for back-compat with old
    // relaunch command lines but is no longer consulted: the journal
    // file at the install root is now the source of truth for both
    // the staging dir path and the per-op backup names.
    let _ = staging_dir;
    let report = crate::apply_journal::recover(exe_dir);
    let staging_removed = report.staging_removed;
    let removed_count = report.backups_removed + report.backups_restored + report.installed_removed;
    (removed_count, staging_removed)
}

/// Runs the helper mode launched by [`spawn_apply_helper`].  The
/// helper waits for the GUI parent to exit before mutating the install
/// tree, so tar extraction and relaunch do not run inside the live
/// graphics/audio process.
pub fn run_apply_helper(request: ApplyRequest) -> i32 {
    let fallback_exe = std::env::current_exe().ok();
    if !wait_for_parent_exit(request.parent_pid) {
        log::error!(
            "Updater helper timed out waiting for parent pid {:?}; refusing to apply",
            request.parent_pid
        );
        return 4;
    }
    let Some(sha256) = super::download::parse_hex32(&request.sha256_hex) else {
        log::error!(
            "Updater helper received invalid sha256 '{}'",
            request.sha256_hex
        );
        return 2;
    };
    match apply_archive_and_relaunch(&request.archive_path, &sha256) {
        Ok(ApplyOutcome::Relaunched) => 0,
        Ok(ApplyOutcome::AppliedNoRelaunch { detail }) => {
            log::error!("Updater helper applied update but could not relaunch: {detail}");
            3
        }
        Err(err) => {
            log::error!("Updater helper failed to apply update: {err}");
            if let Some(exe) = fallback_exe
                && let Err(spawn_err) =
                    relaunch_with_args(&exe, &[std::ffi::OsString::from("--no-update-check")])
            {
                log::error!("Updater helper could not relaunch old binary: {spawn_err}");
            }
            1
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
pub fn spawn_apply_helper(
    archive_path: &std::path::Path,
    sha256: &[u8; 32],
) -> Result<(), super::UpdaterError> {
    let exe = std::env::current_exe()
        .map_err(|e| super::UpdaterError::Io(format!("current_exe: {e}")))?;
    relaunch_with_args(
        &exe,
        &[
            std::ffi::OsString::from("--apply-update"),
            archive_path.as_os_str().to_owned(),
            std::ffi::OsString::from("--apply-sha256"),
            std::ffi::OsString::from(super::download::sha256_hex(sha256)),
            std::ffi::OsString::from("--apply-parent-pid"),
            std::ffi::OsString::from(std::process::id().to_string()),
        ],
    )
}

#[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
pub fn spawn_apply_helper(
    _archive_path: &std::path::Path,
    _sha256: &[u8; 32],
) -> Result<(), super::UpdaterError> {
    Err(super::UpdaterError::Io(
        "updater helper mode is only used on Unix apply targets".to_string(),
    ))
}

fn wait_for_parent_exit(parent_pid: Option<u32>) -> bool {
    let Some(pid) = parent_pid else {
        return true;
    };
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
    {
        use std::time::{Duration, Instant};
        let deadline = Instant::now() + Duration::from_secs(30);
        while process_exists(pid) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "macos")))]
    let _ = pid;
    true
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
fn process_exists(pid: u32) -> bool {
    let pid = pid as libc::pid_t;
    // SAFETY: kill(pid, 0) does not send a signal; it only asks the
    // kernel whether the process exists and is permission-checkable.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Outcome of [`apply_archive_and_relaunch`].
///
/// Distinguishes a true apply failure (install tree untouched or
/// rolled back) from the case where the apply succeeded but spawning
/// the new binary failed.  The caller treats these very differently:
/// an apply failure surfaces a generic Error phase the user can
/// dismiss, while an apply-ok / relaunch-fail leaves the install tree
/// already on the new version and asks the user to restart manually.
#[derive(Debug)]
pub enum ApplyOutcome {
    /// Apply succeeded and the new process was spawned.  Caller
    /// should `process::exit(0)` to release any binary locks.
    Relaunched,
    /// Apply succeeded but spawning the new exe failed.  The install
    /// tree is on the new version; the journal is `Applied` and will
    /// be cleaned up by the next launch.  Detail is the underlying
    /// spawn error for logs / overlay tooltips.
    AppliedNoRelaunch { detail: String },
}

/// Lower-level apply: caller has already chosen the archive (e.g. via
/// an updater action phase) and is responsible
/// for any phase bookkeeping.  Re-hashes the staged file and verifies
/// against `expected_sha256` before extraction so that any
/// modification, corruption, or mid-flight tampering between download
/// and apply surfaces as [`super::UpdaterError::ChecksumMismatch`]
/// rather than installing a different archive than the one the user
/// approved.  Then performs the platform-specific extract + swap and
/// spawns the new process with the appropriate cleanup arguments.
///
/// Returns:
/// - `Ok(ApplyOutcome::Relaunched)` on full success — caller should
///   `process::exit(0)`.
/// - `Ok(ApplyOutcome::AppliedNoRelaunch { detail })` when extraction
///   committed but the spawn failed.  Caller should publish a
///   restart-required phase; it must NOT roll back, since the install
///   tree is on the new version.
/// - `Err(_)` when apply itself failed (rolled back or partially
///   rolled back, with the journal preserved for next-launch
///   recovery).
#[allow(clippy::result_large_err)]
pub fn apply_archive_and_relaunch(
    archive_path: &std::path::Path,
    expected_sha256: &[u8; 32],
) -> Result<ApplyOutcome, super::UpdaterError> {
    // Capture the install-tree path of the currently-running binary
    // BEFORE any apply renames touch the filesystem.  On Linux (and
    // historically macOS) `std::env::current_exe()` resolves through
    // `/proc/self/exe`, which after apply points at the renamed-out
    // backup `<exe>.deadsync-bak-<token>`, not the new binary at the
    // original path.  Spawning that backup would run the *old* binary
    // against the new install tree, and its recovery pass would then
    // delete the very file it's executing.
    let original_exe = std::env::current_exe()
        .map_err(|e| super::UpdaterError::Io(format!("current_exe: {e}")))?;
    let exe_dir = original_exe
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| super::UpdaterError::Io("exe has no parent dir".to_string()))?;
    let relaunch_target = match original_exe.file_name() {
        Some(name) => exe_dir.join(name),
        None => original_exe.clone(),
    };
    reverify_archive(archive_path, expected_sha256)?;
    apply_for_host(archive_path, &exe_dir)?;
    // Apply already committed: the on-disk install is the new
    // version.  A relaunch failure here is NOT an apply failure --
    // surfacing it as Err would mislead the caller into rolling
    // back something that has no rollback hook left.
    match relaunch_self(&relaunch_target) {
        Ok(()) => Ok(ApplyOutcome::Relaunched),
        Err(e) => Ok(ApplyOutcome::AppliedNoRelaunch {
            detail: format!("{e}"),
        }),
    }
}

#[allow(clippy::result_large_err)]
fn reverify_archive(
    archive_path: &std::path::Path,
    expected_sha256: &[u8; 32],
) -> Result<(), super::UpdaterError> {
    let actual = super::download::sha256_of_file(archive_path)?;
    if !super::download::verify_sha256(&actual, expected_sha256) {
        // Drop the staged file so the next download cycle starts clean.
        let _ = std::fs::remove_file(archive_path);
        return Err(super::UpdaterError::ChecksumMismatch {
            expected: super::download::sha256_hex(expected_sha256),
            actual: super::download::sha256_hex(&actual),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn apply_for_host(
    archive_path: &std::path::Path,
    exe_dir: &std::path::Path,
) -> Result<(), super::UpdaterError> {
    let _ = super::apply_windows::apply_zip(archive_path, exe_dir)?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
fn apply_for_host(
    archive_path: &std::path::Path,
    exe_dir: &std::path::Path,
) -> Result<(), super::UpdaterError> {
    let _ = super::apply_unix::apply_tar_gz(archive_path, exe_dir)?;
    Ok(())
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
fn apply_for_host(
    _archive_path: &std::path::Path,
    _exe_dir: &std::path::Path,
) -> Result<(), super::UpdaterError> {
    Err(super::UpdaterError::Io(
        "in-app update apply is not supported on this platform".to_string(),
    ))
}

#[cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]
fn relaunch_self(exe: &std::path::Path) -> Result<(), super::UpdaterError> {
    // No `--cleanup-old <path>` is needed anymore: the new process
    // discovers the apply journal at its install root and runs
    // recovery unconditionally on startup.
    relaunch_with_args(exe, &[std::ffi::OsString::from("--restart")])
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
fn relaunch_self(_exe: &std::path::Path) -> Result<(), super::UpdaterError> {
    Ok(())
}

#[cfg(windows)]
fn relaunch_with_args(
    exe: &std::path::Path,
    args: &[std::ffi::OsString],
) -> Result<(), super::UpdaterError> {
    use std::process::Command;
    Command::new(exe)
        .args(args)
        .spawn()
        .map_err(|e| super::UpdaterError::Io(format!("spawn '{}': {e}", exe.display())))?;
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
fn relaunch_with_args(
    exe: &std::path::Path,
    args: &[std::ffi::OsString],
) -> Result<(), super::UpdaterError> {
    use std::ffi::{CString, OsStr, OsString};
    use std::os::unix::ffi::OsStrExt;

    fn cstring(os: &OsStr, label: &str) -> Result<CString, super::UpdaterError> {
        CString::new(os.as_bytes())
            .map_err(|_| super::UpdaterError::Io(format!("{label} contains an interior NUL byte")))
    }

    let exe_c = cstring(exe.as_os_str(), "executable path")?;
    let mut argv_c = Vec::with_capacity(args.len() + 1);
    argv_c.push(exe_c.clone());
    for arg in args {
        argv_c.push(cstring(arg.as_os_str(), "process argument")?);
    }
    let mut argv: Vec<*mut libc::c_char> = argv_c
        .iter()
        .map(|s| s.as_ptr() as *mut libc::c_char)
        .collect();
    argv.push(std::ptr::null_mut());

    let mut env_c = Vec::new();
    for (key, value) in std::env::vars_os() {
        let mut pair = OsString::from(key);
        pair.push("=");
        pair.push(value);
        env_c.push(cstring(pair.as_os_str(), "environment variable")?);
    }
    let mut envp: Vec<*mut libc::c_char> = env_c
        .iter()
        .map(|s| s.as_ptr() as *mut libc::c_char)
        .collect();
    envp.push(std::ptr::null_mut());

    let mut pid: libc::pid_t = 0;
    // SAFETY: exe_c, argv, and envp point to NUL-terminated storage
    // owned by this stack frame and remain alive for the duration of
    // posix_spawn. Null file-actions and attrs request default spawn
    // behavior. posix_spawn copies what it needs before returning.
    let rc = unsafe {
        libc::posix_spawn(
            &mut pid,
            exe_c.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            argv.as_mut_ptr(),
            envp.as_mut_ptr(),
        )
    };
    if rc != 0 {
        return Err(super::UpdaterError::Io(format!(
            "posix_spawn '{}': {}",
            exe.display(),
            std::io::Error::from_raw_os_error(rc)
        )));
    }
    Ok(())
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
)))]
fn relaunch_with_args(
    _exe: &std::path::Path,
    _args: &[std::ffi::OsString],
) -> Result<(), super::UpdaterError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_no_args_returns_default() {
        let cli = UpdaterCli::parse::<Vec<&str>, _>(vec!["deadsync"]);
        assert!(cli.cleanup_old.is_none());
        assert!(!cli.restart);
        assert!(!cli.no_update_check);
        assert!(cli.apply_update.is_none());
        assert!(cli.remaining.is_empty());
    }

    #[test]
    fn parse_no_update_check_sets_flag() {
        let cli = UpdaterCli::parse(vec!["deadsync", "--no-update-check"]);
        assert!(cli.no_update_check);
        assert!(!cli.restart);
        assert!(cli.cleanup_old.is_none());
    }

    #[test]
    fn parse_restart_sets_flag() {
        let cli = UpdaterCli::parse(vec!["deadsync", "--restart"]);
        assert!(cli.restart);
        assert!(!cli.no_update_check);
    }

    #[test]
    fn parse_cleanup_old_takes_path_argument() {
        let cli = UpdaterCli::parse(vec!["deadsync", "--cleanup-old", "C:\\stage"]);
        assert_eq!(
            cli.cleanup_old.as_deref(),
            Some(std::path::Path::new("C:\\stage"))
        );
    }

    #[test]
    fn parse_cleanup_old_supports_equals_form() {
        let cli = UpdaterCli::parse(vec!["deadsync", "--cleanup-old=/tmp/stage"]);
        assert_eq!(
            cli.cleanup_old.as_deref(),
            Some(std::path::Path::new("/tmp/stage"))
        );
    }

    #[test]
    fn parse_cleanup_old_without_value_is_ignored() {
        let cli = UpdaterCli::parse(vec!["deadsync", "--cleanup-old"]);
        assert!(cli.cleanup_old.is_none());
    }

    #[test]
    fn parse_passes_unknown_flags_through() {
        let cli = UpdaterCli::parse(vec!["deadsync", "--unknown", "value", "--restart"]);
        assert!(cli.restart);
        assert_eq!(cli.remaining, vec!["--unknown", "value"]);
    }

    #[test]
    fn parse_combines_all_flags() {
        let cli = UpdaterCli::parse(vec![
            "deadsync",
            "--no-update-check",
            "--restart",
            "--cleanup-old",
            "/x/y",
        ]);
        assert!(cli.no_update_check);
        assert!(cli.restart);
        assert_eq!(
            cli.cleanup_old.as_deref(),
            Some(std::path::Path::new("/x/y"))
        );
        assert!(cli.remaining.is_empty());
    }

    #[test]
    fn parse_apply_update_takes_archive_sha_and_parent() {
        let cli = UpdaterCli::parse(vec![
            "deadsync",
            "--apply-update",
            "/tmp/deadsync.tar.gz",
            "--apply-sha256",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "--apply-parent-pid",
            "1234",
        ]);
        let req = cli.apply_update.expect("apply request");
        assert_eq!(
            req.archive_path.as_path(),
            std::path::Path::new("/tmp/deadsync.tar.gz")
        );
        assert_eq!(
            req.sha256_hex,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        assert_eq!(req.parent_pid, Some(1234));
    }

    #[test]
    fn parse_apply_update_supports_equals_form() {
        let cli = UpdaterCli::parse(vec![
            "deadsync",
            "--apply-update=/tmp/deadsync.tar.gz",
            "--apply-sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "--apply-parent-pid=1234",
        ]);
        assert!(cli.apply_update.is_some());
        assert_eq!(cli.apply_update.unwrap().parent_pid, Some(1234));
    }

    #[test]
    fn run_cleanup_handles_missing_staging_silently() {
        let stem = format!(
            "deadsync-cli-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(&stem);
        std::fs::create_dir_all(&dir).unwrap();
        let staging = dir.join("does-not-exist");
        let (_n, staging_gone) = run_cleanup(&dir, &staging);
        assert!(!staging_gone);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reverify_archive_accepts_matching_digest() {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-reverify-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("archive.bin");
        let payload = b"deadsync-test-payload";
        std::fs::write(&path, payload).unwrap();
        let expected = super::super::download::sha256_of(payload);

        assert!(reverify_archive(&path, &expected).is_ok());
        // Archive must survive a successful re-verify so apply can read it.
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reverify_archive_rejects_modified_file_and_removes_it() {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-reverify-bad-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("archive.bin");
        std::fs::write(&path, b"original-bytes").unwrap();
        let expected = super::super::download::sha256_of(b"original-bytes");
        // Simulate corruption / tampering between download and apply.
        std::fs::write(&path, b"tampered-bytes").unwrap();

        let err = reverify_archive(&path, &expected).expect_err("must reject mismatch");
        assert!(matches!(
            err,
            super::super::UpdaterError::ChecksumMismatch { .. }
        ));
        // Mismatch must drop the staged archive so the next download starts clean.
        assert!(!path.exists(), "tampered archive should be removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reverify_archive_surfaces_io_error_for_missing_file() {
        let path = std::env::temp_dir().join(format!(
            "deadsync-reverify-missing-{}.bin",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let err = reverify_archive(&path, &[0u8; 32]).expect_err("must fail for missing file");
        assert!(matches!(err, super::super::UpdaterError::Io(_)));
    }
}
