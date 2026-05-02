//! Unix-side "apply" half of the in-app updater.
//!
//! On Linux and FreeBSD the running executable file CAN be renamed
//! and replaced atomically (the kernel keeps the on-disk inode alive
//! until the running process exits), so technically we don't need a
//! backup-then-install dance.  We do it anyway, mirroring the Windows
//! flow, because the journal-driven design relies on having a backup
//! to roll back to if the apply crashes mid-way:
//!
//! 1. Extract the downloaded `tar.gz` into a sibling staging dir
//!    named after the per-apply token in the journal.
//! 2. Plan one [`apply_journal::Op`] per staged regular file and
//!    write the journal as
//!    [`apply_journal::JournalState::Applying`].
//! 3. For each op, rename the live target aside (to its
//!    `.deadsync-bak-<token>` sidecar), then rename the staged file
//!    over the target.  On any error, walk executed ops in reverse
//!    and restore the pre-apply state.
//! 4. Rewrite the journal as
//!    [`apply_journal::JournalState::Applied`] and re-exec the new
//!    binary with `--restart`.  Recovery cleanup happens on the next
//!    startup via [`apply_journal::recover`].
//!
//! `extract_archive` is portable (so the Windows test box can exercise
//! it), but [`apply_tar_gz`] itself is gated to Linux/FreeBSD because
//! it touches a real install root.

use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use tar::Archive;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use super::apply_journal::{self, Journal, JournalState, Op};
use super::UpdaterError;

/// Result of a successful apply.  Returned for diagnostics + tests.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub staging_dir: PathBuf,
    pub installed_file_count: usize,
}

/* ---------- archive extraction (cross-platform; tested on Windows) ---------- */

/// Detects a single shared top-level directory across every entry in
/// the archive.  Returns `Some(prefix)` only when **every** non-empty
/// entry path begins with the same first component, otherwise `None`.
pub fn detect_common_prefix<I, S>(entries: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut prefix: Option<String> = None;
    let mut saw_any = false;
    for raw in entries {
        let raw = raw.as_ref();
        let trimmed = raw.trim_end_matches('/');
        if trimmed.is_empty() {
            continue;
        }
        saw_any = true;
        let first = trimmed.split(['/', '\\']).next().unwrap_or("");
        if first.is_empty() {
            return None;
        }
        match &prefix {
            None => prefix = Some(first.to_string()),
            Some(existing) if existing == first => {}
            Some(_) => return None,
        }
    }
    if saw_any { prefix } else { None }
}

/// Strips the optional shared top-level prefix and validates the
/// remainder: rejects absolute paths and any component that isn't a
/// plain `Normal` segment.  Returns the cleaned relative path.
pub fn sanitize_entry(name: &str, prefix: Option<&str>) -> Option<PathBuf> {
    let trimmed = name.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let stripped = match prefix {
        Some(p) => {
            let with_slash = format!("{p}/");
            let with_back = format!("{p}\\");
            trimmed
                .strip_prefix(&with_slash)
                .or_else(|| trimmed.strip_prefix(&with_back))
                .or_else(|| if trimmed == p { Some("") } else { None })?
        }
        None => trimmed,
    };
    if stripped.is_empty() {
        return None;
    }
    let path = PathBuf::from(stripped.replace('\\', "/"));
    for comp in path.components() {
        match comp {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(path)
}

/// Extracts a gzipped tarball into `dest`, stripping a single shared
/// top-level directory if every entry shares one.  Returns the count
/// of regular files written.  Symlinks and "special" entries are
/// skipped (logged via the returned error only when nothing else was
/// extracted, otherwise silently — the release tarballs don't ship
/// symlinks).
///
/// Two passes: the first reads the entry list to compute the common
/// prefix, the second writes files.  We intentionally re-decode the
/// gzip stream the second time because `tar::Archive` is single-pass.
pub fn extract_tar_gz(zip_bytes: &[u8], dest: &Path) -> Result<usize, UpdaterError> {
    fs::create_dir_all(dest).map_err(|e| super::io_err_at("create_dir_all", dest, e))?;
    let prefix = {
        let dec = GzDecoder::new(zip_bytes);
        let mut archive = Archive::new(dec);
        let mut names = Vec::new();
        for entry in archive.entries().map_err(|e| super::io_err_op("read tar entries", e))? {
            let entry = entry.map_err(|e| super::io_err_op("read tar entry", e))?;
            let path = entry.path().map_err(|e| super::io_err_op("decode tar entry path", e))?;
            if let Some(s) = path.to_str() {
                names.push(s.to_string());
            }
        }
        detect_common_prefix(names)
    };
    let dec = GzDecoder::new(zip_bytes);
    let mut archive = Archive::new(dec);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);
    let mut written = 0usize;
    for entry in archive.entries().map_err(|e| super::io_err_op("read tar entries", e))? {
        let mut entry = entry.map_err(|e| super::io_err_op("read tar entry", e))?;
        let raw_name = entry
            .path()
            .map_err(|e| super::io_err_op("decode tar entry path", e))?
            .to_string_lossy()
            .to_string();
        let entry_type = entry.header().entry_type();
        let Some(rel) = sanitize_entry(&raw_name, prefix.as_deref()) else {
            if entry_type.is_dir() {
                continue;
            }
            return Err(UpdaterError::Io(format!(
                "rejected unsafe archive entry '{raw_name}'"
            )));
        };
        let out_path = dest.join(&rel);
        if entry_type.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|e| super::io_err_at("create_dir_all", &out_path, e))?;
            continue;
        }
        if !entry_type.is_file() {
            // Fail closed on symlinks, hardlinks, devices, FIFOs,
            // etc.  Release tarballs only contain regular files and
            // directories; encountering anything else means either
            // a packaging mistake or a hand-crafted archive trying
            // to slip a runtime-resolved indirection into the
            // install tree.  Surfacing this as an error is safer
            // than silently producing a partial install that a user
            // might run.
            return Err(UpdaterError::Io(format!(
                "rejected non-regular tar entry '{raw_name}' (type {:?})",
                entry_type,
            )));
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| super::io_err_at("create_dir_all", parent, e))?;
        }
        let mut out = File::create(&out_path)
            .map_err(|e| super::io_err_at("create", &out_path, e))?;
        io::copy(&mut entry, &mut out)
            .map_err(|e| super::io_err_at("write", &out_path, e))?;
        // Fsync the staged file's contents before it can be
        // rename()'d into place. The parent-dir fsync only makes the
        // directory entry durable; without this, a power loss between
        // rename and the next boot can resurrect a directory entry
        // that points at zero/partial bytes.
        out.sync_all()
            .map_err(|e| super::io_err_at("sync_all", &out_path, e))?;
        // Preserve the executable bit on Unix; on Windows this is a
        // no-op, but the call still typechecks.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mode) = entry.header().mode() {
                let _ = fs::set_permissions(
                    &out_path,
                    std::fs::Permissions::from_mode(mode),
                );
            }
        }
        written += 1;
    }
    Ok(written)
}

/* ---------- planning + journal-driven apply (Unix-only) ---------- */

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn plan_ops(
    journal: &Journal,
    staging_dir: &Path,
    target_dir: &Path,
) -> Result<Vec<Op>, UpdaterError> {
    let files = collect_files(staging_dir)?;
    let mut ops = Vec::with_capacity(files.len());
    for staged in files {
        let rel = staged.strip_prefix(staging_dir).map_err(|_| {
            UpdaterError::Io(format!(
                "staged path '{}' escapes staging dir '{}'",
                staged.display(),
                staging_dir.display(),
            ))
        })?;
        let target = target_dir.join(rel);
        let target_existed = match fs::symlink_metadata(&target) {
            Ok(m) => {
                if !m.is_file() {
                    // Refuse to clobber a directory or symlink with a
                    // regular file. The execute phase's `rename` would
                    // otherwise move the entire subtree aside as the
                    // "backup", and the `Applied` cleanup would then
                    // fail trying to `remove_file` it, wedging the
                    // journal forever.
                    return Err(UpdaterError::Io(format!(
                        "type mismatch for '{}': existing entry is not a regular file",
                        target.display(),
                    )));
                }
                true
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(super::io_err_at("metadata", &target, e)),
        };
        if !target_existed && apply_journal::is_portability_marker(rel) {
            // Don't introduce a portable.txt/ini marker the user didn't
            // already have — it would silently flip a non-portable
            // install into portable mode on the next launch and hide
            // their existing AppData/XDG config.
            continue;
        }
        let backup = journal.backup_path_for(&target);
        ops.push(Op {
            staged,
            target,
            backup,
            target_existed,
        });
    }
    apply_journal::check_no_case_collisions(&ops)?;
    Ok(ops)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn execute_with_rollback(journal: &Journal) -> Result<(), apply_journal::ExecuteFailure> {
    let mut executed: Vec<&Op> = Vec::with_capacity(journal.ops.len());
    for op in &journal.ops {
        if let Some(parent) = op.target.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                let rollback_clean = rollback(&executed);
                return Err(apply_journal::ExecuteFailure {
                    cause: super::io_err_at("create_dir_all", parent, e),
                    rollback_clean,
                });
            }
        }
        if op.target_existed {
            if op.backup.exists() {
                let rollback_clean = rollback(&executed);
                return Err(apply_journal::ExecuteFailure {
                    cause: UpdaterError::Io(format!(
                        "backup path '{}' already exists; refusing to overwrite",
                        op.backup.display(),
                    )),
                    rollback_clean,
                });
            }
            if let Err(e) = fs::rename(&op.target, &op.backup) {
                let rollback_clean = rollback(&executed);
                return Err(apply_journal::ExecuteFailure {
                    cause: UpdaterError::Io(format!(
                        "failed to rename '{}' -> '{}': {e}",
                        op.target.display(),
                        op.backup.display(),
                    )),
                    rollback_clean,
                });
            }
        }
        if let Err(e) = fs::rename(&op.staged, &op.target) {
            // Roll back this op's own backup before recursing.
            if op.target_existed {
                let _ = fs::rename(&op.backup, &op.target);
            }
            let rollback_clean = rollback(&executed);
            return Err(apply_journal::ExecuteFailure {
                cause: UpdaterError::Io(format!(
                    "failed to install '{}' -> '{}': {e}",
                    op.staged.display(),
                    op.target.display(),
                )),
                rollback_clean,
            });
        }
        executed.push(op);
    }
    // POSIX: each rename above mutated a directory entry; fsync each
    // unique parent so the install survives a power loss. Best-effort —
    // a failure here doesn't roll back the (already-fsynced) file
    // contents and would only mean the directory entry isn't durable.
    let mut synced: Vec<&Path> = Vec::new();
    for op in &journal.ops {
        if let Some(parent) = op.target.parent() {
            if !synced.iter().any(|p| *p == parent) {
                let _ = super::sync_dir(parent);
                synced.push(parent);
            }
        }
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn rollback(executed: &[&Op]) -> bool {
    let mut clean = true;
    let mut touched_parents: Vec<&Path> = Vec::new();
    for op in executed.iter().rev() {
        match fs::rename(&op.target, &op.staged) {
            Ok(()) => {
                if let Some(p) = op.target.parent() {
                    if !touched_parents.iter().any(|x| *x == p) {
                        touched_parents.push(p);
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "updater rollback: failed to restore '{}' -> '{}': {e}",
                    op.target.display(),
                    op.staged.display(),
                );
                clean = false;
            }
        }
        if op.target_existed {
            match fs::rename(&op.backup, &op.target) {
                Ok(()) => {
                    if let Some(p) = op.target.parent() {
                        if !touched_parents.iter().any(|x| *x == p) {
                            touched_parents.push(p);
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "updater rollback: failed to restore backup '{}' -> '{}': {e}",
                        op.backup.display(),
                        op.target.display(),
                    );
                    clean = false;
                }
            }
        }
    }
    // POSIX: make the restored directory entries durable so a power
    // loss between rollback and the next boot can't lose them.
    for p in &touched_parents {
        let _ = super::sync_dir(p);
    }
    clean
}

/* ---------- top-level orchestration ---------- */

/// Drives the full apply sequence under the durable journal: probe
/// writability → extract tarball into the journal's staging dir →
/// write the journal as `Applying` → execute per-op renames with
/// rollback on error → rewrite as `Applied`.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub fn apply_tar_gz(archive_path: &Path, exe_dir: &Path) -> Result<ApplyOutcome, UpdaterError> {
    if !is_dir_writable(exe_dir) {
        return Err(UpdaterError::Io(format!(
            "install directory is not writable: {}",
            exe_dir.display(),
        )));
    }
    let mut journal = Journal::new(exe_dir);
    if journal.staging_dir.exists() {
        let _ = fs::remove_dir_all(&journal.staging_dir);
    }
    let staging_guard =
        apply_journal::StagingGuard::new(journal.staging_dir.clone());
    let bytes = fs::read(archive_path)
        .map_err(|e| super::io_err_at("read", archive_path, e))?;
    let installed_file_count = extract_tar_gz(&bytes, &journal.staging_dir)?;
    journal.ops = plan_ops(&journal, &journal.staging_dir, exe_dir)?;
    journal.write_atomic(exe_dir)?;
    staging_guard.disarm();
    if let Err(failure) = execute_with_rollback(&journal) {
        if failure.rollback_clean {
            let _ = fs::remove_file(apply_journal::journal_path(exe_dir));
            let _ = fs::remove_dir_all(&journal.staging_dir);
        } else {
            log::warn!(
                "updater apply failed AND rollback was incomplete; \
                 leaving journal at '{}' for next-launch recovery: {}",
                apply_journal::journal_path(exe_dir).display(),
                failure.cause,
            );
        }
        return Err(failure.cause);
    }
    journal.state = JournalState::Applied;
    journal.write_atomic(exe_dir)?;
    Ok(ApplyOutcome {
        staging_dir: journal.staging_dir,
        installed_file_count,
    })
}

/// Probe writability the same way `apply_windows` does so the two
/// platforms refuse self-update with consistent UX.
pub fn is_dir_writable(dir: &Path) -> bool {
    use std::io::Write;
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(format!(
        ".deadsync-writable-probe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    match File::create(&probe) {
        Ok(mut f) => {
            let _ = f.write_all(b"ok");
            drop(f);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/* ---------- helpers ---------- */

#[cfg(any(target_os = "linux", target_os = "freebsd", test))]
fn collect_files(root: &Path) -> Result<Vec<PathBuf>, UpdaterError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| super::io_err_at("read_dir", &dir, e))? {
            let entry = entry.map_err(|e| super::io_err_at("read_dir entry", &dir, e))?;
            let path = entry.path();
            let ft = entry
                .file_type()
                .map_err(|e| super::io_err_at("file_type", &path, e))?;
            if ft.is_dir() {
                stack.push(path);
            } else if ft.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::{Builder, Header};

    fn tempdir(stem: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "deadsync-apply-unix-{stem}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn build_tar_gz(entries: &[(&str, &[u8])], top: Option<&str>) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let enc = GzEncoder::new(&mut buf, Compression::fast());
            let mut tar = Builder::new(enc);
            for (name, body) in entries {
                let full = match top {
                    Some(p) => format!("{p}/{name}"),
                    None => (*name).to_string(),
                };
                let mut header = Header::new_gnu();
                header.set_path(&full).unwrap();
                header.set_size(body.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar.append(&header, *body).unwrap();
            }
            tar.finish().unwrap();
        }
        buf
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn plan_ops_rejects_case_colliding_paths() {
        let staging = tempdir("plan-collide-staging");
        let target = tempdir("plan-collide-target");
        fs::write(staging.join("foo.dll"), b"A").unwrap();
        fs::write(staging.join("FOO.dll"), b"B").unwrap();

        let journal = apply_journal::Journal::new(&target);
        let err = plan_ops(&journal, &staging, &target).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.to_lowercase().contains("collision"),
            "expected collision error, got: {msg}"
        );

        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn plan_ops_skips_portability_marker_when_target_missing() {
        let staging = tempdir("plan-portable-skip-staging");
        let target = tempdir("plan-portable-skip-target");
        fs::write(staging.join("a.txt"), b"A").unwrap();
        fs::write(staging.join("portable.txt"), b"").unwrap();

        let journal = apply_journal::Journal::new(&target);
        let ops = plan_ops(&journal, &staging, &target).unwrap();
        assert_eq!(ops.len(), 1, "portable.txt should be skipped");
        assert_eq!(ops[0].target, target.join("a.txt"));

        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn plan_ops_replaces_portability_marker_when_target_exists() {
        let staging = tempdir("plan-portable-keep-staging");
        let target = tempdir("plan-portable-keep-target");
        fs::write(staging.join("portable.txt"), b"").unwrap();
        fs::write(target.join("portable.txt"), b"").unwrap();

        let journal = apply_journal::Journal::new(&target);
        let ops = plan_ops(&journal, &staging, &target).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].target, target.join("portable.txt"));
        assert!(ops[0].target_existed);

        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn apply_tar_gz_cleans_staging_when_extract_fails() {
        let exe_dir = tempdir("apply-tgz-extract-fail");
        let archive = exe_dir.join("garbage.tar.gz");
        fs::write(&archive, b"this is not a gzip stream").unwrap();

        let _ = apply_tar_gz(&archive, &exe_dir).unwrap_err();

        let mut leaked_staging = false;
        for entry in fs::read_dir(&exe_dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(apply_journal::STAGING_PREFIX) {
                leaked_staging = true;
            }
        }
        assert!(
            !leaked_staging,
            "staging dir leaked under {}",
            exe_dir.display()
        );
        assert!(!apply_journal::journal_path(&exe_dir).exists());

        let _ = fs::remove_dir_all(&exe_dir);
    }

    #[test]
    fn detect_common_prefix_returns_shared_dir() {
        assert_eq!(
            detect_common_prefix(["pkg/a", "pkg/b/c", "pkg/"]).as_deref(),
            Some("pkg")
        );
    }

    #[test]
    fn detect_common_prefix_returns_none_when_mixed() {
        assert_eq!(detect_common_prefix(["pkg/a", "other/b"]), None);
    }

    #[test]
    fn detect_common_prefix_returns_none_for_empty_input() {
        assert_eq!(detect_common_prefix(Vec::<&str>::new()), None);
    }

    #[test]
    fn sanitize_entry_strips_known_prefix() {
        let p = sanitize_entry("deadsync/assets/x.bin", Some("deadsync")).unwrap();
        assert_eq!(p, PathBuf::from("assets/x.bin"));
    }

    #[test]
    fn sanitize_entry_rejects_parent_dir() {
        assert!(sanitize_entry("deadsync/../etc/passwd", Some("deadsync")).is_none());
        assert!(sanitize_entry("../escape", None).is_none());
    }

    #[test]
    fn sanitize_entry_rejects_absolute() {
        assert!(sanitize_entry("/abs/path", None).is_none());
    }

    #[test]
    fn extract_tar_gz_strips_single_top_level_dir() {
        let bytes = build_tar_gz(
            &[("deadsync", b"NEWBIN"), ("assets/x.bin", b"ASSET")],
            Some("deadsync"),
        );
        let dest = tempdir("strip-prefix");
        let n = extract_tar_gz(&bytes, &dest).unwrap();
        assert_eq!(n, 2);
        assert_eq!(fs::read(dest.join("deadsync")).unwrap(), b"NEWBIN");
        assert_eq!(fs::read(dest.join("assets/x.bin")).unwrap(), b"ASSET");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn extract_tar_gz_keeps_layout_when_no_common_prefix() {
        let bytes = build_tar_gz(&[("a.txt", b"A"), ("dir/b.txt", b"B")], None);
        let dest = tempdir("no-prefix");
        let n = extract_tar_gz(&bytes, &dest).unwrap();
        assert_eq!(n, 2);
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"A");
        assert_eq!(fs::read(dest.join("dir/b.txt")).unwrap(), b"B");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn extract_tar_gz_rejects_traversal_entry() {
        // The tar crate refuses both to write and to read entries
        // containing "..".  We rely on that as defense-in-depth on
        // top of `sanitize_entry`.  This test asserts that an entry
        // *we* might produce by accident (a relative path with
        // ".." after stripping the prefix) is rejected by
        // sanitize_entry alone, which is the only path our
        // extractor uses.
        assert!(sanitize_entry("deadsync/../escape.txt", Some("deadsync")).is_none());
        assert!(sanitize_entry("../escape.txt", None).is_none());
    }

    #[test]
    fn extract_tar_gz_rejects_symlink_entry() {
        use tar::EntryType;
        let mut buf = Vec::new();
        {
            let enc = GzEncoder::new(&mut buf, Compression::fast());
            let mut tar = Builder::new(enc);
            // One legitimate file so the archive isn't trivially empty,
            // followed by a symlink that an attacker might use to redirect
            // a later write outside the staging dir.
            let mut hdr = Header::new_gnu();
            hdr.set_path("deadsync/keep.txt").unwrap();
            hdr.set_size(2);
            hdr.set_mode(0o644);
            hdr.set_cksum();
            tar.append(&hdr, &b"ok"[..]).unwrap();

            let mut link = Header::new_gnu();
            link.set_path("deadsync/evil").unwrap();
            link.set_size(0);
            link.set_mode(0o777);
            link.set_entry_type(EntryType::Symlink);
            link.set_link_name("/etc/passwd").unwrap();
            link.set_cksum();
            tar.append(&link, std::io::empty()).unwrap();

            tar.finish().unwrap();
        }
        let dest = tempdir("symlink-reject");
        let err = extract_tar_gz(&buf, &dest)
            .expect_err("symlink entry must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("rejected non-regular tar entry"),
            "unexpected error: {msg}",
        );
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn is_dir_writable_returns_true_for_temp_dir() {
        let dir = tempdir("writable");
        assert!(is_dir_writable(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_dir_writable_returns_false_for_missing_dir() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-no-such-dir-{}", std::process::id()));
        assert!(!is_dir_writable(&dir));
    }

    // The apply_tar_gz / plan_ops / execute_with_rollback entry
    // points are gated to Linux/FreeBSD because they assume a real
    // install root.  We still cover the cross-platform helpers below.

    #[test]
    fn collect_files_walks_recursively() {
        let root = tempdir("collect");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("top.txt"), b"t").unwrap();
        fs::write(root.join("a/inner.txt"), b"i").unwrap();
        fs::write(root.join("a/b/leaf.txt"), b"l").unwrap();
        let files = collect_files(&root).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert!(names.contains(&"top.txt".to_string()));
        assert!(names.contains(&"a/inner.txt".to_string()));
        assert!(names.contains(&"a/b/leaf.txt".to_string()));
        assert_eq!(names.len(), 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn rollback_reports_dirty_when_restore_rename_fails() {
        // When rollback can't restore a target, the function must
        // signal `clean = false` so the caller preserves the journal
        // for the next launch's `recover` to retry.  We simulate the
        // failure by pointing the op's `staged` path under a
        // non-existent parent directory: the rollback's
        // `rename(target -> staged)` then fails with ENOENT.
        let exe_dir = tempdir("rb-dirty");
        let journal = Journal::new(&exe_dir);
        let target = exe_dir.join("a.txt");
        fs::write(&target, b"NEW").unwrap();
        let staged = exe_dir.join("ghost").join("a.txt");
        let backup = journal.backup_path_for(&target);
        let ops = vec![Op {
            staged,
            target: target.clone(),
            backup,
            target_existed: false,
        }];
        let refs: Vec<&Op> = ops.iter().collect();
        assert!(!rollback(&refs));
        let _ = fs::remove_dir_all(&exe_dir);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn plan_ops_rejects_directory_at_target() {
        // If the install tree currently has a directory where the
        // new release wants to place a regular file, refuse the
        // apply at planning time. Otherwise execute_with_rollback's
        // `rename` would move the entire subtree to the backup path
        // and `Applied` cleanup would fail trying to `remove_file` a
        // directory, wedging the journal forever.
        let target_dir = tempdir("m25-dir-at-target");
        let staging_dir = tempdir("m25-staging");
        // Stage a regular file at "noteskins/foo.png".
        fs::create_dir_all(staging_dir.join("noteskins")).unwrap();
        fs::write(staging_dir.join("noteskins/foo.png"), b"new").unwrap();
        // Pre-create a directory at the same target path.
        fs::create_dir_all(target_dir.join("noteskins/foo.png")).unwrap();
        let journal = Journal::new(&target_dir);
        let err = plan_ops(&journal, &staging_dir, &target_dir).unwrap_err();
        match err {
            UpdaterError::Io(msg) => assert!(
                msg.contains("type mismatch"),
                "unexpected error: {msg}",
            ),
            other => panic!("expected Io, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&target_dir);
        let _ = fs::remove_dir_all(&staging_dir);
    }
}
