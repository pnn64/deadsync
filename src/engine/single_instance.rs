//! Cross-platform single-instance enforcement.
//!
//! Acquires a process-wide lock on startup; a second instance trying
//! to launch will see [`acquire`] return [`AcquireError::AlreadyRunning`]
//! and is expected to exit with a non-zero status.  The lock is held
//! by the returned [`InstanceGuard`]; dropping the guard (including
//! via process exit or crash) releases it.
//!
//! ### Implementation
//!
//! * **Windows**: a named mutex in the per-session `Local\` namespace.
//!   The kernel cleans up the handle automatically on process death,
//!   so no stale-lock recovery is needed.  The name is derived from
//!   the `cache_dir` path so two installs rooted in different data
//!   dirs (rare, but supported via `--portable`) can coexist.
//!
//! * **Linux / FreeBSD**: an exclusive [`flock(2)`] on a small file
//!   under the cache dir.  `flock` advisory locks are released by
//!   the kernel when the owning fd is closed (also on process death),
//!   so no PID file or stale-lock dance is required.
//!
//! ### Relaunch race
//!
//! After a self-update the old process spawns the new one and exits
//! milliseconds later; the new process may briefly observe the old
//! lock as held.  [`acquire_with_retry`] polls a short while before
//! giving up, which the [`crate::main`] startup uses when the
//! `--restart` flag is set.

use std::path::Path;
use std::time::{Duration, Instant};

/// RAII guard returned by [`acquire`].  Holds the OS handle/fd that
/// represents this process's claim to the singleton lock; dropping
/// it releases the lock.  `#[must_use]` so callers don't accidentally
/// discard the guard and immediately lose the lock.
#[must_use = "dropping the guard releases the single-instance lock"]
pub struct InstanceGuard {
    #[cfg(windows)]
    #[allow(dead_code)] // held only for its Drop impl
    handle: WindowsHandle,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[allow(dead_code)] // held only for its Drop impl
    fd: UnixFd,
    // On macOS / unsupported platforms, the guard is a no-op
    // placeholder so the calling code stays uniform.
    #[cfg(not(any(windows, target_os = "linux", target_os = "freebsd")))]
    _private: (),
}

/// Reason [`acquire`] failed.
#[derive(Debug)]
pub enum AcquireError {
    /// Another instance currently holds the lock.
    AlreadyRunning,
    /// The OS reported an error while trying to take the lock; the
    /// caller should treat this as a hard failure and decide whether
    /// to bail or proceed without the lock.
    Os(String),
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "another instance is already running"),
            Self::Os(s) => write!(f, "single-instance lock error: {s}"),
        }
    }
}

impl std::error::Error for AcquireError {}

/// Tries to acquire the singleton lock once.  See module docs for the
/// per-platform mechanism.
///
/// `cache_dir` is used to scope the lock to this install (different
/// portable installs do not contend with each other).
pub fn acquire(cache_dir: &Path) -> Result<InstanceGuard, AcquireError> {
    #[cfg(windows)]
    {
        windows_impl::acquire(cache_dir)
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        unix_impl::acquire(cache_dir)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "freebsd")))]
    {
        let _ = cache_dir;
        Ok(InstanceGuard { _private: () })
    }
}

/// Tries to acquire the lock, polling for up to `total_wait` if the
/// first attempt sees `AlreadyRunning`.  Used after `--restart` to
/// bridge the brief window where the old process is still exiting.
pub fn acquire_with_retry(
    cache_dir: &Path,
    total_wait: Duration,
) -> Result<InstanceGuard, AcquireError> {
    let deadline = Instant::now() + total_wait;
    loop {
        match acquire(cache_dir) {
            Ok(g) => return Ok(g),
            Err(AcquireError::AlreadyRunning) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Builds a stable, filesystem-friendly slug from `cache_dir`'s
/// canonical bytes.  Used to name the Windows mutex so installs in
/// distinct data dirs don't contend.  Pure-Rust 64-bit FNV-1a — fast,
/// deterministic, no extra deps.
fn install_slug(cache_dir: &Path) -> String {
    let bytes = cache_dir.as_os_str().to_string_lossy();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/* ---------- Windows ---------- */

#[cfg(windows)]
struct WindowsHandle(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for WindowsHandle {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateMutexW; release exactly once.
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::HSTRING;

    pub fn acquire(cache_dir: &Path) -> Result<InstanceGuard, AcquireError> {
        let name = format!("Local\\deadsync-singleton-{}", install_slug(cache_dir));
        let wide = HSTRING::from(name.as_str());
        // SAFETY: CreateMutexW is a thread-safe Win32 API; we
        // immediately check GetLastError before relying on the
        // handle's existence semantics.
        let handle = unsafe {
            CreateMutexW(None, false, &wide)
                .map_err(|e| AcquireError::Os(format!("CreateMutexW: {e}")))?
        };
        let last = unsafe { GetLastError() };
        if last == ERROR_ALREADY_EXISTS {
            // The mutex existed before this call: another instance
            // owns it.  Close our handle (Drop does it) and report.
            let _ = WindowsHandle(handle);
            return Err(AcquireError::AlreadyRunning);
        }
        Ok(InstanceGuard {
            handle: WindowsHandle(handle),
        })
    }
}

/* ---------- Unix (Linux + FreeBSD) ---------- */

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
struct UnixFd(libc::c_int);

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl Drop for UnixFd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            // SAFETY: fd is owned by this struct; close exactly once.
            unsafe {
                libc::close(self.0);
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod unix_impl {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    pub fn acquire(cache_dir: &Path) -> Result<InstanceGuard, AcquireError> {
        // Best-effort: ensure the cache dir exists so the open below
        // doesn't fail on a fresh install.
        let _ = std::fs::create_dir_all(cache_dir);
        let lock_path = cache_dir.join("deadsync.lock");
        let cpath = CString::new(lock_path.as_os_str().as_bytes())
            .map_err(|e| AcquireError::Os(format!("lock path contains NUL: {e}")))?;
        // SAFETY: open with valid C string + flags; on success we
        // wrap the returned fd so it gets closed via Drop.
        let fd = unsafe {
            libc::open(
                cpath.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            let err = std::io::Error::last_os_error();
            return Err(AcquireError::Os(format!(
                "open lock file '{}': {err}",
                lock_path.display(),
            )));
        }
        let owned = UnixFd(fd);
        // SAFETY: owned fd outlives this call.
        let rc = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let err = std::io::Error::last_os_error();
            return match err.raw_os_error() {
                Some(libc::EWOULDBLOCK) => Err(AcquireError::AlreadyRunning),
                _ => Err(AcquireError::Os(format!("flock: {err}"))),
            };
        }
        Ok(InstanceGuard { fd: owned })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(stem: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "deadsync-singleton-{stem}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_slug_is_stable_and_path_dependent() {
        let a = install_slug(Path::new("/foo/bar"));
        let b = install_slug(Path::new("/foo/bar"));
        let c = install_slug(Path::new("/foo/baz"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn first_acquire_succeeds_and_second_reports_already_running() {
        let dir = tempdir("contend");
        let guard = acquire(&dir).expect("first acquire");
        match acquire(&dir) {
            Err(AcquireError::AlreadyRunning) => {}
            Err(other) => panic!("expected AlreadyRunning, got {other:?}"),
            Ok(_) => panic!("expected AlreadyRunning, got Ok"),
        }
        drop(guard);
        // Once the first guard is released, a fresh acquire should
        // succeed again.
        let _ = acquire(&dir).expect("re-acquire after drop");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn distinct_cache_dirs_do_not_contend() {
        let a = tempdir("install-a");
        let b = tempdir("install-b");
        let _ga = acquire(&a).expect("acquire a");
        let _gb = acquire(&b).expect("acquire b");
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn acquire_with_retry_returns_immediately_when_uncontended() {
        let dir = tempdir("retry-fast");
        let start = Instant::now();
        let _g = acquire_with_retry(&dir, Duration::from_secs(5)).expect("acquire");
        assert!(start.elapsed() < Duration::from_millis(500));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
