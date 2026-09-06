//! Atomic file replacement via a synced temp sibling and rename.

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// One exclusively created temporary sibling, removed on drop unless committed.
///
/// The open handle is kept throughout writing, so concurrent writers never
/// truncate or remove each other's temporary files.
pub struct AtomicFile {
    target: PathBuf,
    tmp_path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl AtomicFile {
    /// Create a private temporary file alongside `target`. Existing Unix
    /// permission bits are copied before any contents are written.
    pub fn new(target: &Path) -> std::io::Result<Self> {
        #[cfg(unix)]
        let permissions = match fs::metadata(target) {
            Ok(metadata) => Some(metadata.permissions()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };

        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        for _ in 0..128 {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let mut tmp_path = target.as_os_str().to_owned();
            tmp_path.push(format!(".{}.{}.tmp", std::process::id(), id));
            let tmp_path = PathBuf::from(tmp_path);
            let file = match options.open(&tmp_path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let pending = Self {
                target: target.to_path_buf(),
                tmp_path,
                file: Some(file),
                committed: false,
            };
            #[cfg(unix)]
            if let Some(permissions) = permissions {
                pending
                    .file
                    .as_ref()
                    .expect("temporary file is open")
                    .set_permissions(permissions)?;
            }
            return Ok(pending);
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create an unused temporary sibling",
        ))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.tmp_path
    }

    /// Write and sync the temporary file before committing it. A write or sync
    /// error leaves the destination intact; dropping `self` removes the temp.
    pub fn write_synced(&mut self, contents: &[u8]) -> std::io::Result<()> {
        let file = self.file.as_mut().expect("temporary file is open");
        file.write_all(contents)?;
        file.sync_all()
    }

    /// Replace the destination after a successful `write_synced`, then
    /// best-effort sync its parent directory.
    pub fn commit(mut self) -> std::io::Result<()> {
        drop(self.file.take());
        fs::rename(&self.tmp_path, &self.target)?;
        self.committed = true;
        sync_parent_dir(&self.target);
        Ok(())
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        drop(self.file.take());
        if !self.committed {
            let _ = fs::remove_file(&self.tmp_path);
        }
    }
}

/// Best-effort sync of `path`'s parent directory so a rename into it is
/// durable.
#[cfg(unix)]
pub fn sync_parent_dir(path: &Path) {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
}

/// Directories cannot be opened for syncing on non-unix platforms.
#[cfg(not(unix))]
pub fn sync_parent_dir(_path: &Path) {}

/// Replace `path` with `contents` via a unique, synced temporary sibling and
/// rename. Unix permission bits are preserved; new Unix files are private.
/// On error the destination is left untouched and only this write's temp file
/// is removed. Concurrent saves publish complete files, with the last rename
/// winning.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let mut pending = AtomicFile::new(path)?;
    pending.write_synced(contents)?;
    pending.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Barrier, atomic::AtomicUsize};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("deadsync-atomic-write-{}-{id}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            // These tests create only files and empty directories directly here.
            if let Ok(entries) = fs::read_dir(&self.0) {
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        let _ = fs::remove_dir(entry.path());
                    } else {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
            let _ = fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn creates_and_replaces_without_touching_legacy_temp() {
        let dir = TestDir::new();
        let path = dir.0.join("config.ini");
        let legacy_temp = dir.0.join("config.ini.tmp");
        fs::write(&legacy_temp, b"another writer's data").unwrap();

        write_atomic(&path, b"first save").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first save");
        write_atomic(&path, b"next").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"next");
        assert_eq!(fs::read(&legacy_temp).unwrap(), b"another writer's data");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 2);
    }

    #[test]
    fn overlapping_saves_own_separate_temporary_files() {
        let dir = TestDir::new();
        let path = dir.0.join("config.ini");
        let mut first = AtomicFile::new(&path).unwrap();
        let mut second = AtomicFile::new(&path).unwrap();
        assert_ne!(first.path(), second.path());
        assert_eq!(first.path().parent(), path.parent());
        assert_eq!(second.path().parent(), path.parent());
        first.write_synced(b"first save").unwrap();
        second.write_synced(b"second save").unwrap();

        let abandoned = AtomicFile::new(&path).unwrap();
        let abandoned_path = abandoned.path().to_path_buf();
        drop(abandoned);
        assert!(!abandoned_path.exists());
        assert!(first.path().exists());
        assert!(second.path().exists());

        first.commit().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first save");
        assert_eq!(fs::read(second.path()).unwrap(), b"second save");
        second.commit().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second save");
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn concurrent_saves_publish_only_complete_contents() {
        let dir = TestDir::new();
        let path = dir.0.join("config.ini");
        fs::write(&path, b"original").unwrap();
        let payloads = [vec![b'a'; 1024 * 1024], vec![b'b'; 512 * 1024]];
        let start = Barrier::new(3);
        let done = AtomicUsize::new(0);
        let write_errors = AtomicUsize::new(0);
        let mut incomplete_reads = 0;

        std::thread::scope(|scope| {
            for payload in &payloads {
                scope.spawn(|| {
                    start.wait();
                    for _ in 0..16 {
                        if write_atomic(&path, payload).is_err() {
                            write_errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    done.fetch_add(1, Ordering::Release);
                });
            }
            start.wait();
            while done.load(Ordering::Acquire) != 2 {
                match fs::read(&path) {
                    Ok(bytes)
                        if bytes == b"original" || payloads.iter().any(|data| *data == bytes) => {}
                    _ => incomplete_reads += 1,
                }
                std::thread::yield_now();
            }
        });

        assert_eq!(write_errors.load(Ordering::Relaxed), 0);
        assert_eq!(incomplete_reads, 0);
        let contents = fs::read(&path).unwrap();
        assert!(payloads.contains(&contents));
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
    }

    #[test]
    fn abandoning_partial_write_preserves_destination_and_removes_temp() {
        let dir = TestDir::new();
        let path = dir.0.join("config.ini");
        fs::write(&path, b"original").unwrap();
        let mut pending = AtomicFile::new(&path).unwrap();
        let tmp_path = pending.path().to_path_buf();
        pending
            .file
            .as_mut()
            .unwrap()
            .write_all(b"partial")
            .unwrap();
        drop(pending);

        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(!tmp_path.exists());
    }

    #[test]
    fn failed_write_preserves_destination_and_removes_temp() {
        let dir = TestDir::new();
        let path = dir.0.join("config.ini");
        fs::write(&path, b"original").unwrap();
        let mut pending = AtomicFile::new(&path).unwrap();
        let tmp_path = pending.path().to_path_buf();
        // Inject a write failure using a read-only handle, without changing the
        // owned temporary path or the destination.
        pending.file = Some(File::open(&tmp_path).unwrap());
        assert!(pending.write_synced(b"replacement").is_err());
        drop(pending);

        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(!tmp_path.exists());
    }

    #[test]
    fn failed_rename_preserves_destination_and_removes_temp() {
        let dir = TestDir::new();
        let path = dir.0.join("config.ini");
        fs::create_dir(&path).unwrap();
        let mut pending = AtomicFile::new(&path).unwrap();
        let tmp_path = pending.path().to_path_buf();
        pending.write_synced(b"replacement").unwrap();
        assert!(pending.commit().is_err());

        assert!(path.is_dir());
        assert!(!tmp_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn preserves_unix_permissions_before_writing_and_after_commit() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let path = dir.0.join("credentials.ini");
        fs::write(&path, b"old credentials").unwrap();
        for mode in [0o600, 0o640, 0o644] {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
            let mut pending = AtomicFile::new(&path).unwrap();
            assert_eq!(
                fs::metadata(pending.path()).unwrap().permissions().mode() & 0o777,
                mode
            );
            pending.write_synced(b"new credentials").unwrap();
            pending.commit().unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                mode
            );
            assert_eq!(fs::read(&path).unwrap(), b"new credentials");
        }
    }

    #[cfg(unix)]
    #[test]
    fn new_unix_files_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TestDir::new();
        let path = dir.0.join("credentials.ini");
        let mut pending = AtomicFile::new(&path).unwrap();
        assert_eq!(
            fs::metadata(pending.path()).unwrap().permissions().mode() & 0o077,
            0
        );
        pending.write_synced(b"credentials").unwrap();
        pending.commit().unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
    }
}
