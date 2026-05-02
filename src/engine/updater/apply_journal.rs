//! Durable journal for the in-app updater's transactional apply.
//!
//! The apply phase mutates the install tree in-place: for every file in
//! the freshly-extracted staging directory we move the existing target
//! aside (a "backup") and then move the staged file into the target's
//! place.  Any failure mid-way must leave the install bit-identical to
//! its pre-apply state, even across process crashes and power loss.
//!
//! That recovery story requires a persistent record of the planned
//! mutations.  This module owns:
//!
//! * The on-disk schema for that record (`Journal`, `Op`).
//! * Atomic writes (write-temp → fsync → rename) so a partially-written
//!   journal can never be observed.
//! * A [`recover`] driver invoked at startup that finishes any
//!   pending apply by either rolling forward (state == `Applied`,
//!   delete backups + staging) or rolling back (state == `Applying`,
//!   restore originals).
//! * Strict path validation so a corrupted journal can never be turned
//!   into an arbitrary-file-deletion primitive — every backup and
//!   staging path is required to live under the install root and to
//!   match the per-apply suffix/name pattern.
//!
//! Both `apply_windows` and `apply_unix` build a [`Journal`], call
//! [`Journal::write_atomic`] in the `Applying` state, perform the
//! per-op renames, then re-write the journal in the `Applied` state.
//! A successful relaunch's startup pass calls [`recover`] which
//! removes the backups and staging dir.  A crashed apply's startup
//! pass calls the same [`recover`] which restores any backups and
//! deletes any partially-installed staged files.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use rand::RngExt;
use serde::{Deserialize, Serialize};

use super::UpdaterError;

/// Schema version for [`Journal`].  Bumped whenever the on-disk format
/// changes in a way the recovery driver must distinguish.  Older
/// binaries that encounter a newer version leave the file in place
/// rather than acting on data they don't understand.
pub const JOURNAL_VERSION: u32 = 1;

/// Filename of the journal, written at the root of the install
/// directory (the parent of the executable).  Hidden by leading dot
/// to keep portable installs visually clean on macOS/Linux file
/// browsers; on Windows it shows like any other file but the name
/// itself signals "owned by deadsync".
pub const JOURNAL_FILENAME: &str = ".deadsync-update-journal.json";

/// Prefix of the staging directory; followed by `-<token>`.  Lives as
/// a sibling of the executable so `rename(2)` calls during the swap
/// stay on the same filesystem volume (a hard requirement for atomic
/// renames on every platform we support).
pub const STAGING_PREFIX: &str = ".deadsync-update-staging-";

/// Suffix appended to displaced live files; followed by the per-apply
/// random token so user content that happens to share an extension
/// (e.g. `songs/foo.sm.old`) cannot collide with updater-owned
/// backups.  The full backup name is `<original>.deadsync-bak-<token>`.
pub const BACKUP_INFIX: &str = ".deadsync-bak-";

/// Hex length of the randomness embedded in staging dir names and
/// backup suffixes.  16 bytes (128 bits) is overkill for accidental
/// collisions but trivially cheap and removes any debate around
/// adversarial collisions if the token were ever made observable.
pub const TOKEN_HEX_LEN: usize = 32;

/// Lifecycle of a journal file.
///
/// * `Applying` — written before any filesystem mutation.  If a crash
///   leaves the journal in this state, recovery rolls **back** any
///   per-op work that did happen (restore backups, delete partially
///   installed staged files), returning the install to its pre-apply
///   state.
/// * `Applied` — written after every per-op rename has succeeded.
///   Recovery rolls **forward**: removes the backup files and the
///   now-empty staging directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JournalState {
    Applying,
    Applied,
}

/// A single planned (target, backup, staged) move triple.
///
/// All paths are absolute and validated by [`Journal::validate`] to
/// live inside the install root, so a corrupted journal can never
/// instruct recovery to touch a path outside the install tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    /// Where the new file currently lives (under the staging dir).
    pub staged: PathBuf,
    /// The final install location to overwrite.
    pub target: PathBuf,
    /// `<target>.deadsync-bak-<token>` — only meaningful when
    /// `target_existed` is true.
    pub backup: PathBuf,
    /// Was there a file at `target` when planning ran?  When false,
    /// recovery from `Applying` removes any newly-installed file at
    /// `target`; it does not try to restore a backup that was never
    /// created.
    pub target_existed: bool,
}

/// Persistent record of one apply attempt.  Lives on disk at
/// `{install_root}/.deadsync-update-journal.json` for the duration of
/// the apply window and is removed by [`recover`] once the apply
/// settles (forward or back).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    pub state: JournalState,
    /// 32-hex-character per-apply random token.  Embedded in
    /// `staging_dir`'s basename and in every `Op::backup` suffix; used
    /// during validation to reject tampered or reused entries.
    pub token: String,
    /// Absolute path to the staging dir.  Must be a direct child of
    /// the install root and named `STAGING_PREFIX + token`.
    pub staging_dir: PathBuf,
    pub ops: Vec<Op>,
}

impl Journal {
    /// Generates a fresh, validated journal with a freshly-rolled
    /// token.  The returned journal is in the `Applying` state and
    /// has no ops yet — callers add ops via [`Self::push_op`] before
    /// the first [`Self::write_atomic`] call.
    pub fn new(exe_dir: &Path) -> Self {
        let token = generate_token();
        let staging_dir = exe_dir.join(format!("{STAGING_PREFIX}{token}"));
        Self {
            version: JOURNAL_VERSION,
            state: JournalState::Applying,
            token,
            staging_dir,
            ops: Vec::new(),
        }
    }

    /// Builds the backup path for `target` using this journal's token,
    /// matching what [`Self::validate`] requires.
    pub fn backup_path_for(&self, target: &Path) -> PathBuf {
        let mut s = target.as_os_str().to_owned();
        s.push(format!("{BACKUP_INFIX}{}", self.token));
        PathBuf::from(s)
    }

    /// Atomically writes the journal next to the executable.  Uses a
    /// `<filename>.tmp` sidecar plus `sync_all` + `rename` so a crash
    /// can never leave a partially-written JSON file that recovery
    /// would misinterpret.
    pub fn write_atomic(&self, exe_dir: &Path) -> Result<(), UpdaterError> {
        let path = journal_path(exe_dir);
        let tmp = with_tmp_suffix(&path);
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| super::io_err_op("serialize journal", e))?;
        {
            let mut f = File::create(&tmp).map_err(|e| super::io_err_at("create", &tmp, e))?;
            f.write_all(&bytes).map_err(|e| super::io_err_at("write", &tmp, e))?;
            // Best-effort durability: not every filesystem honours this
            // (tmpfs, some networked mounts), but on real disks it
            // ensures the journal hits stable storage before the
            // rename commits it.
            let _ = f.sync_all();
        }
        fs::rename(&tmp, &path).map_err(|e| {
            // On rename failure, leave the staging file in place but
            // try to clean it up so a partial write doesn't accumulate.
            let _ = fs::remove_file(&tmp);
            UpdaterError::Io(format!(
                "rename '{}' -> '{}': {e}",
                tmp.display(),
                path.display(),
            ))
        })?;
        // POSIX: the rename above only mutates `exe_dir`'s directory
        // entries; without an fsync of the directory itself the entry
        // can be lost on power loss even though the file bytes are
        // durable. Windows commits this as part of the rename.
        let _ = super::sync_dir(exe_dir);
        Ok(())
    }

    /// Reads and parses the journal at `exe_dir`.  Returns `Ok(None)`
    /// when no journal file exists; `Err` only when the file exists
    /// but cannot be read or parsed (which leaves it in place for
    /// inspection).
    pub fn load(exe_dir: &Path) -> Result<Option<Self>, UpdaterError> {
        let path = journal_path(exe_dir);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(super::io_err_at("read", &path, e)),
        };
        let parsed: Self = serde_json::from_slice(&bytes)
            .map_err(|e| UpdaterError::Io(format!("parse journal '{}': {e}", path.display())))?;
        Ok(Some(parsed))
    }

    /// Validates that every path in the journal lives under
    /// `exe_dir`, and that staging/backup names match the expected
    /// per-token pattern.  Required before recovery touches the
    /// filesystem so a corrupted or hostile journal cannot delete
    /// arbitrary files.
    pub fn validate(&self, exe_dir: &Path) -> Result<(), UpdaterError> {
        if self.version != JOURNAL_VERSION {
            return Err(io_err_msg(format!(
                "journal version {} not supported (expected {JOURNAL_VERSION})",
                self.version,
            )));
        }
        if !is_token_valid(&self.token) {
            return Err(io_err_msg(format!(
                "journal token '{}' is not {TOKEN_HEX_LEN} lowercase hex chars",
                self.token,
            )));
        }
        let expected_staging = exe_dir.join(format!("{STAGING_PREFIX}{}", self.token));
        if self.staging_dir != expected_staging {
            return Err(io_err_msg(format!(
                "journal staging_dir '{}' does not match expected '{}'",
                self.staging_dir.display(),
                expected_staging.display(),
            )));
        }
        if !is_simple_relative(&self.staging_dir, exe_dir) {
            return Err(io_err_msg(format!(
                "journal staging_dir '{}' escapes install root '{}'",
                self.staging_dir.display(),
                exe_dir.display(),
            )));
        }
        let backup_suffix = format!("{BACKUP_INFIX}{}", self.token);
        for op in &self.ops {
            if !is_simple_relative(&op.target, exe_dir) {
                return Err(io_err_msg(format!(
                    "journal op target '{}' escapes install root",
                    op.target.display(),
                )));
            }
            if !is_simple_relative(&op.staged, &self.staging_dir) {
                return Err(io_err_msg(format!(
                    "journal op staged '{}' escapes staging dir",
                    op.staged.display(),
                )));
            }
            let expected_backup = {
                let mut s = op.target.as_os_str().to_owned();
                s.push(&backup_suffix);
                PathBuf::from(s)
            };
            if op.backup != expected_backup {
                return Err(io_err_msg(format!(
                    "journal op backup '{}' does not match expected '{}'",
                    op.backup.display(),
                    expected_backup.display(),
                )));
            }
        }
        Ok(())
    }
}

/// Path of the journal file relative to the install root.
pub fn journal_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join(JOURNAL_FILENAME)
}

/// Returns true when `rel` is a top-level portability marker that
/// `crate::config::dirs` reads to switch between portable and
/// system-config modes (`portable.txt` / `portable.ini`).  The
/// updater uses this to leave the user's existing portability state
/// alone: if they didn't already have a marker, we don't create one
/// just because the release archive ships an empty placeholder; if
/// they did, we overwrite with the (also empty) replacement.
pub fn is_portability_marker(rel: &Path) -> bool {
    let mut comps = rel.components();
    let Some(first) = comps.next() else {
        return false;
    };
    if comps.next().is_some() {
        return false;
    }
    matches!(first.as_os_str().to_str(), Some("portable.txt") | Some("portable.ini"))
}

/// Rejects op lists where two paths fold to the same name on a
/// case-insensitive filesystem (NTFS, default APFS, FAT, casefolded
/// ext4).  A staging tree containing both `foo.dll` and `FOO.dll`
/// would otherwise install one and silently shadow the other on
/// Windows.  Also catches the (vanishingly unlikely) case where a
/// target path collides with another op's backup path.
pub fn check_no_case_collisions(ops: &[Op]) -> Result<(), UpdaterError> {
    use std::collections::HashMap;
    let mut seen: HashMap<String, PathBuf> = HashMap::with_capacity(ops.len() * 2);
    for op in ops {
        let key = op.target.to_string_lossy().to_lowercase();
        if let Some(prev) = seen.insert(key, op.target.clone()) {
            return Err(io_err_msg(format!(
                "case-insensitive path collision in release archive: '{}' and '{}' resolve to the same target on this filesystem",
                prev.display(),
                op.target.display(),
            )));
        }
        if op.target_existed {
            let key = op.backup.to_string_lossy().to_lowercase();
            if let Some(prev) = seen.insert(key, op.backup.clone()) {
                return Err(io_err_msg(format!(
                    "case-insensitive path collision between '{}' and backup path '{}'",
                    prev.display(),
                    op.backup.display(),
                )));
            }
        }
    }
    Ok(())
}

/// Removes its `path` on drop unless [`Self::disarm`] was called.
/// Wraps the pre-journal extract+plan window so a partial staging
/// tree never leaks when extraction or planning fails: once the
/// journal is durable on disk, recovery owns staging cleanup and the
/// guard can be disarmed.
pub struct StagingGuard {
    path: PathBuf,
    armed: bool,
}

impl StagingGuard {
    pub fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    pub fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn with_tmp_suffix(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Generates a fresh `TOKEN_HEX_LEN`-character lowercase-hex token
/// from the OS RNG.  Each apply gets its own token so backup names
/// from different attempts (or from a future attempt that runs while
/// a previous attempt's backups still linger) cannot collide.
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_HEX_LEN / 2];
    rand::rng().fill(&mut bytes);
    let mut s = String::with_capacity(TOKEN_HEX_LEN);
    for b in &bytes {
        use std::fmt::Write;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}

fn is_token_valid(token: &str) -> bool {
    token.len() == TOKEN_HEX_LEN
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `path` must be `root` joined with one or more plain (non-`..`,
/// non-absolute, non-prefix) path components.  Used to confine all
/// recovery operations to the install root.
fn is_simple_relative(path: &Path, root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    let mut saw_any = false;
    for comp in rel.components() {
        match comp {
            Component::Normal(_) => saw_any = true,
            Component::CurDir => {}
            _ => return false,
        }
    }
    saw_any
}

fn io_err_msg(msg: String) -> UpdaterError {
    UpdaterError::Io(msg)
}

/// Outcome of a [`recover`] call, returned for diagnostics + tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecoveryReport {
    /// Number of backups removed during a forward-cleanup of an
    /// `Applied` journal.
    pub backups_removed: usize,
    /// Number of originals restored from backup during a rollback of
    /// an `Applying` journal.
    pub backups_restored: usize,
    /// Number of partially-installed (no-backup-created) staged files
    /// removed from the target tree during rollback.
    pub installed_removed: usize,
    /// `true` when the staging dir was deleted (or was already gone).
    pub staging_removed: bool,
    /// `true` when the journal file itself was deleted as the final
    /// step.  Always paired with one of the other counters being
    /// non-zero unless the journal was empty.
    pub journal_removed: bool,
}

/// Outcome of a failed `execute_with_rollback` call.  `cause` is the
/// forward-pass error that triggered the abort; `rollback_clean` is
/// `true` when every restore rename succeeded and `false` when at
/// least one was blocked (AV, lock, permission denied, transient
/// filesystem error).
///
/// The caller uses `rollback_clean` to decide whether to remove the
/// journal: a clean rollback means the install is bit-identical to
/// pre-apply and the journal can be deleted; a dirty rollback means
/// the install is mixed and the journal must be preserved so the
/// next launch's [`recover`] can retry the restore.
#[derive(Debug)]
pub struct ExecuteFailure {
    pub cause: UpdaterError,
    pub rollback_clean: bool,
}

/// Inspects the journal at `exe_dir` and finishes whatever apply was
/// in flight.  Safe to call on every startup: returns a no-op report
/// when no journal is present.
///
/// * `JournalState::Applied` → forward cleanup: delete each backup
///   path and the staging dir, then delete the journal.
/// * `JournalState::Applying` → rollback: walk ops in reverse and,
///   for each op, restore the original from backup if one exists, or
///   remove a stray partially-installed file at `target` if the
///   original never existed.  Then delete the staging dir and the
///   journal.
///
/// All filesystem work is best-effort; errors are surfaced via the
/// returned report's counters and the journal is only removed when
/// every recoverable op completed successfully so a future startup can
/// retry on persistent errors (e.g. a locked file held by AV).  A
/// malformed or out-of-version journal is left in place with no
/// mutations.
///
/// The `Applying` branch is careful about the "crash mid-rename"
/// window: if both the backup and a partially-written new target
/// exist, the target is removed first so the subsequent
/// `rename(backup, target)` succeeds on Windows (where rename refuses
/// to replace an existing destination).  Without this, a crash in the
/// narrow window between `target -> backup` and `staged -> target`
/// completing would leave the install permanently mixed because the
/// rename would error and the journal would be dropped.
pub fn recover(exe_dir: &Path) -> RecoveryReport {
    let mut report = RecoveryReport::default();
    let journal = match Journal::load(exe_dir) {
        Ok(Some(j)) => j,
        Ok(None) => return report,
        Err(e) => {
            // Journal exists but cannot be parsed -- either a future
            // schema we don't understand, or on-disk corruption.  Leave
            // the file in place so a newer binary (or human triage) can
            // act on it, but make the situation visible: silently
            // ignoring it means a stuck install never produces any
            // signal in deadsync.log.
            log::warn!(
                "updater: journal at '{}' could not be loaded ({e}); leaving in place",
                journal_path(exe_dir).display(),
            );
            return report;
        }
    };
    if let Err(e) = journal.validate(exe_dir) {
        log::warn!(
            "updater: journal at '{}' failed validation ({e}); leaving in place",
            journal_path(exe_dir).display(),
        );
        return report;
    }
    // Tracks whether the journal's recipe is fully resolved.  If any
    // recoverable step fails (locked file, permission denied, partial
    // rename), the journal is left in place so the next startup can
    // retry instead of permanently losing the recovery instructions.
    let mut all_ops_succeeded = true;
    match journal.state {
        JournalState::Applied => {
            for op in &journal.ops {
                if !op.target_existed {
                    continue;
                }
                match fs::remove_file(&op.backup) {
                    Ok(()) => report.backups_removed += 1,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(_) => all_ops_succeeded = false,
                }
            }
        }
        JournalState::Applying => {
            for op in journal.ops.iter().rev() {
                let backup_exists = op.backup.try_exists().unwrap_or(false);
                let target_exists = op.target.try_exists().unwrap_or(false);
                if backup_exists {
                    if target_exists {
                        // Crash landed between `target -> backup` and
                        // `staged -> target` finishing, leaving a
                        // partial new target.  On Windows
                        // `fs::rename` refuses to replace an existing
                        // file, so drop the partial first.
                        if fs::remove_file(&op.target).is_err() {
                            all_ops_succeeded = false;
                            continue;
                        }
                    }
                    if fs::rename(&op.backup, &op.target).is_ok() {
                        report.backups_restored += 1;
                    } else {
                        all_ops_succeeded = false;
                    }
                } else if !op.target_existed && target_exists {
                    if fs::remove_file(&op.target).is_ok() {
                        report.installed_removed += 1;
                    } else {
                        all_ops_succeeded = false;
                    }
                }
                // Other shapes (no backup + target_existed=true, or
                // no backup + target gone) are noops: either the op
                // never started or a previous recovery pass already
                // restored it.
            }
        }
    }
    report.staging_removed = if journal.staging_dir.exists() {
        fs::remove_dir_all(&journal.staging_dir).is_ok()
    } else {
        true
    };
    // Staging cleanup failure is annoying but not corruption; only the
    // op-level state gates whether we drop the journal.
    report.journal_removed = if all_ops_succeeded {
        fs::remove_file(journal_path(exe_dir)).is_ok()
    } else {
        false
    };
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(stem: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "deadsync-journal-{stem}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_journal(exe_dir: &Path) -> Journal {
        let mut j = Journal::new(exe_dir);
        let target = exe_dir.join("subdir/file.bin");
        let backup = j.backup_path_for(&target);
        let staged = j.staging_dir.join("subdir/file.bin");
        j.ops.push(Op {
            staged,
            target,
            backup,
            target_existed: true,
        });
        j
    }

    #[test]
    fn token_is_correct_length_and_lowercase_hex() {
        let t = generate_token();
        assert_eq!(t.len(), TOKEN_HEX_LEN);
        assert!(is_token_valid(&t), "token '{t}' should be valid");
    }

    #[test]
    fn tokens_are_unique_across_calls() {
        // Not a strict guarantee, but a 128-bit collision in 1024
        // draws would be a serious bug.
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..1024 {
            assert!(seen.insert(generate_token()));
        }
    }

    fn op_at(target: PathBuf, backup: PathBuf, target_existed: bool) -> Op {
        Op {
            staged: PathBuf::from("/staging").join(target.file_name().unwrap()),
            target,
            backup,
            target_existed,
        }
    }

    #[test]
    fn check_no_case_collisions_accepts_distinct_paths() {
        let ops = vec![
            op_at(
                PathBuf::from("/install/foo.dll"),
                PathBuf::from("/install/foo.dll.bak"),
                true,
            ),
            op_at(
                PathBuf::from("/install/bar.dll"),
                PathBuf::from("/install/bar.dll.bak"),
                false,
            ),
        ];
        check_no_case_collisions(&ops).unwrap();
    }

    #[test]
    fn check_no_case_collisions_rejects_case_only_target_dupes() {
        let ops = vec![
            op_at(
                PathBuf::from("/install/foo.dll"),
                PathBuf::from("/install/foo.dll.bak"),
                false,
            ),
            op_at(
                PathBuf::from("/install/FOO.dll"),
                PathBuf::from("/install/FOO.dll.bak"),
                false,
            ),
        ];
        let err = check_no_case_collisions(&ops).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.to_lowercase().contains("collision"), "got: {msg}");
    }

    #[test]
    fn check_no_case_collisions_rejects_target_vs_backup_overlap() {
        let token_suffix = "deadsync-bak-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let target_a = PathBuf::from("/install/foo");
        let backup_a = PathBuf::from(format!("/install/foo.{token_suffix}"));
        // A second op whose target case-folds to the first op's backup.
        let target_b = PathBuf::from(format!("/install/FOO.{token_suffix}"));
        let backup_b = PathBuf::from(format!("/install/FOO.{token_suffix}.bak"));
        let ops = vec![
            op_at(target_a, backup_a, true),
            op_at(target_b, backup_b, false),
        ];
        check_no_case_collisions(&ops).unwrap_err();
    }

    #[test]
    fn staging_guard_armed_removes_dir_on_drop() {
        let dir = tempdir("guard-armed");
        assert!(dir.exists());
        {
            let _g = StagingGuard::new(dir.clone());
        }
        assert!(!dir.exists(), "armed guard should remove dir on drop");
    }

    #[test]
    fn staging_guard_disarmed_keeps_dir() {
        let dir = tempdir("guard-disarmed");
        {
            let g = StagingGuard::new(dir.clone());
            g.disarm();
        }
        assert!(dir.exists(), "disarmed guard must not remove dir");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn staging_guard_drop_tolerates_missing_dir() {
        let dir = tempdir("guard-missing");
        fs::remove_dir_all(&dir).unwrap();
        // Must not panic even though the path no longer exists.
        let _g = StagingGuard::new(dir);
    }

    #[test]
    fn write_then_load_roundtrips() {
        let dir = tempdir("roundtrip");
        let j = make_journal(&dir);
        j.write_atomic(&dir).unwrap();
        let loaded = Journal::load(&dir).unwrap().unwrap();
        assert_eq!(loaded, j);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_none_when_missing() {
        let dir = tempdir("missing");
        assert!(Journal::load(&dir).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_errors_on_garbage() {
        let dir = tempdir("garbage");
        fs::write(journal_path(&dir), b"not json").unwrap();
        assert!(Journal::load(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_wrong_version() {
        let dir = tempdir("badver");
        let mut j = make_journal(&dir);
        j.version = JOURNAL_VERSION + 1;
        assert!(j.validate(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_bad_token() {
        let dir = tempdir("badtok");
        let mut j = make_journal(&dir);
        j.token = "not-hex".to_string();
        assert!(j.validate(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_target_outside_root() {
        let dir = tempdir("escape-target");
        let mut j = make_journal(&dir);
        let evil = std::env::temp_dir().join("not-under-root.bin");
        j.ops[0].target = evil.clone();
        j.ops[0].backup = j.backup_path_for(&evil);
        assert!(j.validate(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_staging_outside_root() {
        let dir = tempdir("escape-staging");
        let mut j = make_journal(&dir);
        j.staging_dir = std::env::temp_dir().join("evil-staging");
        assert!(j.validate(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_rejects_mismatched_backup_suffix() {
        let dir = tempdir("badbak");
        let mut j = make_journal(&dir);
        j.ops[0].backup = j.ops[0].target.with_extension("bak");
        assert!(j.validate(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_no_journal_is_noop() {
        let dir = tempdir("recover-empty");
        let r = recover(&dir);
        assert_eq!(r, RecoveryReport::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_applied_deletes_backups_and_staging() {
        let dir = tempdir("recover-applied");
        let mut j = Journal::new(&dir);
        // Two ops, one with a pre-existing target backed up, one
        // freshly installed (no backup file).
        let t1 = dir.join("a.bin");
        let t2 = dir.join("b.bin");
        let b1 = j.backup_path_for(&t1);
        let b2 = j.backup_path_for(&t2);
        let s1 = j.staging_dir.join("a.bin");
        let s2 = j.staging_dir.join("b.bin");
        fs::write(&t1, b"NEW1").unwrap();
        fs::write(&t2, b"NEW2").unwrap();
        fs::write(&b1, b"OLD1").unwrap();
        // b2 does not exist (target_existed = false).
        fs::create_dir_all(&j.staging_dir).unwrap();
        j.ops.push(Op {
            staged: s1,
            target: t1.clone(),
            backup: b1.clone(),
            target_existed: true,
        });
        j.ops.push(Op {
            staged: s2,
            target: t2.clone(),
            backup: b2.clone(),
            target_existed: false,
        });
        j.state = JournalState::Applied;
        j.write_atomic(&dir).unwrap();

        let r = recover(&dir);
        assert_eq!(r.backups_removed, 1);
        assert!(r.staging_removed);
        assert!(r.journal_removed);
        // Targets remain (these are the new files).
        assert!(t1.exists() && t2.exists());
        // Backup file is gone.
        assert!(!b1.exists());
        // Staging dir + journal gone.
        assert!(!j.staging_dir.exists());
        assert!(!journal_path(&dir).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_applying_restores_backups_and_removes_partials() {
        let dir = tempdir("recover-applying");
        let mut j = Journal::new(&dir);
        // Op 1: target existed, was backed up, then overwritten by
        // new staged file. Recovery must restore the original.
        // Op 2: target did not exist, new staged file was installed.
        // Recovery must remove the new file.
        // Op 3: planned but never executed (no backup, no install).
        let t1 = dir.join("a.bin");
        let t2 = dir.join("b.bin");
        let t3 = dir.join("c.bin");
        let b1 = j.backup_path_for(&t1);
        let b2 = j.backup_path_for(&t2);
        let b3 = j.backup_path_for(&t3);
        fs::write(&t1, b"NEW1").unwrap();
        fs::write(&b1, b"OLD1").unwrap();
        fs::write(&t2, b"NEW2").unwrap();
        // No t3, no b3 — op 3 never started.
        fs::create_dir_all(&j.staging_dir).unwrap();
        j.ops.push(Op {
            staged: j.staging_dir.join("a.bin"),
            target: t1.clone(),
            backup: b1.clone(),
            target_existed: true,
        });
        j.ops.push(Op {
            staged: j.staging_dir.join("b.bin"),
            target: t2.clone(),
            backup: b2.clone(),
            target_existed: false,
        });
        j.ops.push(Op {
            staged: j.staging_dir.join("c.bin"),
            target: t3.clone(),
            backup: b3.clone(),
            target_existed: false,
        });
        j.state = JournalState::Applying;
        j.write_atomic(&dir).unwrap();

        let r = recover(&dir);
        assert_eq!(r.backups_restored, 1);
        assert_eq!(r.installed_removed, 1);
        assert!(r.staging_removed);
        assert!(r.journal_removed);
        // Original restored to t1.
        assert_eq!(fs::read(&t1).unwrap(), b"OLD1");
        // Partial install at t2 removed.
        assert!(!t2.exists());
        // t3 never existed and stays gone.
        assert!(!t3.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_does_not_touch_user_old_files() {
        // Regression test: a file ending in `.old` under the install
        // root must survive recovery completely untouched.
        let dir = tempdir("user-old-safe");
        fs::create_dir_all(dir.join("songs")).unwrap();
        let user_file = dir.join("songs/foo.old");
        fs::write(&user_file, b"USER CONTENT").unwrap();
        // Stand up an Applied journal that touches a different file.
        let mut j = Journal::new(&dir);
        let t = dir.join("a.bin");
        let b = j.backup_path_for(&t);
        fs::write(&t, b"NEW").unwrap();
        fs::write(&b, b"OLD").unwrap();
        fs::create_dir_all(&j.staging_dir).unwrap();
        j.ops.push(Op {
            staged: j.staging_dir.join("a.bin"),
            target: t,
            backup: b,
            target_existed: true,
        });
        j.state = JournalState::Applied;
        j.write_atomic(&dir).unwrap();

        recover(&dir);
        assert_eq!(fs::read(&user_file).unwrap(), b"USER CONTENT");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_leaves_invalid_journal_in_place() {
        let dir = tempdir("invalid");
        let mut j = make_journal(&dir);
        j.version = 999;
        j.write_atomic(&dir).unwrap();
        let r = recover(&dir);
        assert!(!r.journal_removed);
        assert!(journal_path(&dir).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_leaves_unknown_state_journal_in_place() {
        // A future binary may write a `state` value this binary does
        // not understand.  Make sure recover() neither parses through
        // it nor deletes the file -- the next run of a newer binary
        // (or human triage) needs it to stay put.
        let dir = tempdir("unknown-state");
        let path = journal_path(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &path,
            br#"{"version":1,"token":"00000000000000000000000000000000","state":"future_state","staging_dir":"/x","ops":[]}"#,
        )
        .unwrap();
        let r = recover(&dir);
        assert!(!r.journal_removed);
        assert_eq!(r.backups_removed, 0);
        assert_eq!(r.backups_restored, 0);
        assert!(path.exists(), "journal must be preserved for newer binary");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_replaces_existing_journal() {
        let dir = tempdir("replace");
        let j1 = make_journal(&dir);
        j1.write_atomic(&dir).unwrap();
        let mut j2 = make_journal(&dir);
        j2.state = JournalState::Applied;
        j2.write_atomic(&dir).unwrap();
        let loaded = Journal::load(&dir).unwrap().unwrap();
        assert_eq!(loaded.state, JournalState::Applied);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_simple_relative_accepts_nested() {
        let root = PathBuf::from("/install");
        assert!(is_simple_relative(
            &PathBuf::from("/install/a/b/c.bin"),
            &root
        ));
    }

    #[test]
    fn is_simple_relative_rejects_outside() {
        let root = PathBuf::from("/install");
        assert!(!is_simple_relative(&PathBuf::from("/etc/passwd"), &root));
    }

    #[test]
    fn is_simple_relative_rejects_root_itself() {
        let root = PathBuf::from("/install");
        assert!(!is_simple_relative(&PathBuf::from("/install"), &root));
    }

    #[test]
    fn recover_applying_overwrites_partial_target_with_backup() {
        // Simulates a crash in the narrow window between
        // `target -> backup` and `staged -> target` finishing: backup
        // exists AND a partially-written new target exists.  On
        // Windows the naive `rename(backup, target)` would fail
        // because rename refuses to replace an existing destination,
        // and the install would be left mixed with the journal
        // dropped.  The recovery code must remove the partial first.
        let dir = tempdir("recover-partial-target");
        let mut j = Journal::new(&dir);
        let t = dir.join("a.bin");
        let b = j.backup_path_for(&t);
        fs::write(&t, b"PARTIAL_NEW").unwrap();
        fs::write(&b, b"OLD").unwrap();
        fs::create_dir_all(&j.staging_dir).unwrap();
        j.ops.push(Op {
            staged: j.staging_dir.join("a.bin"),
            target: t.clone(),
            backup: b.clone(),
            target_existed: true,
        });
        j.state = JournalState::Applying;
        j.write_atomic(&dir).unwrap();

        let r = recover(&dir);
        assert_eq!(r.backups_restored, 1, "backup should restore over partial target");
        assert!(r.journal_removed, "journal should be removed after successful rollback");
        assert_eq!(fs::read(&t).unwrap(), b"OLD");
        assert!(!b.exists(), "backup file must be consumed by the rename");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_applying_preserves_journal_when_partial_install_cant_be_removed() {
        // If a rollback step fails (here: a partially-installed
        // target is actually a non-empty directory, so remove_file
        // errors), the journal must be preserved so a future startup
        // can retry.  Pre-fix this would silently drop the journal
        // and the user would be stuck with the partial install
        // forever.
        let dir = tempdir("recover-retry");
        let mut j = Journal::new(&dir);
        let t = dir.join("blocker");
        fs::create_dir(&t).unwrap();
        fs::write(t.join("inside"), b"x").unwrap();
        let b = j.backup_path_for(&t);
        fs::create_dir_all(&j.staging_dir).unwrap();
        j.ops.push(Op {
            staged: j.staging_dir.join("blocker"),
            target: t.clone(),
            backup: b,
            target_existed: false,
        });
        j.state = JournalState::Applying;
        j.write_atomic(&dir).unwrap();

        let r = recover(&dir);
        assert_eq!(r.installed_removed, 0);
        assert!(
            !r.journal_removed,
            "journal must persist when a rollback step fails"
        );
        assert!(journal_path(&dir).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recover_applied_skips_already_missing_backup_and_still_drops_journal() {
        // A previous (partial) recovery may have already removed some
        // backups; the second recovery pass must treat NotFound as
        // success and still complete cleanup.
        let dir = tempdir("recover-applied-missing");
        let mut j = Journal::new(&dir);
        let t = dir.join("a.bin");
        let b = j.backup_path_for(&t);
        fs::write(&t, b"NEW").unwrap();
        // Note: backup b deliberately not created.
        fs::create_dir_all(&j.staging_dir).unwrap();
        j.ops.push(Op {
            staged: j.staging_dir.join("a.bin"),
            target: t,
            backup: b,
            target_existed: true,
        });
        j.state = JournalState::Applied;
        j.write_atomic(&dir).unwrap();

        let r = recover(&dir);
        assert_eq!(r.backups_removed, 0, "no backup file existed to remove");
        assert!(r.journal_removed, "missing backup is treated as already cleaned");
        let _ = fs::remove_dir_all(&dir);
    }
}
