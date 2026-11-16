//! File-name storage is fallible before calling the operating system.
use crate::Error;

#[cfg(unix)]
mod platform {
    use super::*;
    use crate::memory::fallible::{SliceExt, VecExt};
    use std::ffi::c_char;
    unsafe extern "C" {
        #[link_name = "remove"]
        fn remove_path(path: *const c_char) -> i32;
        #[link_name = "rename"]
        fn rename_path(from: *const c_char, to: *const c_char) -> i32;
    }
    fn terminated(path: &[u8]) -> Result<Vec<u8>, Error> {
        let mut path = path.try_to_vec()?;
        path.try_push(0)?;
        Ok(path)
    }
    fn status(result: i32) -> Result<(), i32> {
        if result == 0 {
            Ok(())
        } else {
            Err(super::super::super::io::errno(
                std::io::Error::last_os_error(),
            ))
        }
    }
    pub(in crate::vm::os) fn remove(path: &[u8]) -> Result<Result<(), i32>, Error> {
        let path = terminated(path)?;
        // SAFETY: owned NUL-terminated byte path, live for this synchronous
        // call. POSIX remove handles both files and empty directories.
        Ok(status(unsafe { remove_path(path.as_ptr().cast()) }))
    }
    pub(in crate::vm::os) fn rename(from: &[u8], to: &[u8]) -> Result<Result<(), i32>, Error> {
        let from = terminated(from)?;
        let to = terminated(to)?;
        // SAFETY: both terminated paths live through the atomic native rename.
        Ok(status(unsafe {
            rename_path(from.as_ptr().cast(), to.as_ptr().cast())
        }))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::vm::io;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeleteFileW(path: *const u16) -> i32;
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    fn status(result: i32) -> Result<(), i32> {
        if result != 0 {
            Ok(())
        } else {
            Err(io::errno(std::io::Error::last_os_error()))
        }
    }
    pub(in crate::vm::os) fn remove(path: &[u8]) -> Result<Result<(), i32>, Error> {
        if path.is_empty() {
            // Preserve the native empty-name error rather than normalizing it
            // into an absolute path or treating it as the current directory.
            return Ok(status(unsafe { DeleteFileW([0u16].as_ptr()) }));
        }
        let path = match io::input::windows_path(path)? {
            Ok(path) => path,
            Err(code) => return Ok(Err(code)),
        };
        // SAFETY: the shared path converter returns terminated UTF-16 storage.
        // DeleteFileW retains the Windows reference's rejection of directories.
        Ok(status(unsafe { DeleteFileW(path.as_ptr()) }))
    }
    pub(in crate::vm::os) fn rename(from: &[u8], to: &[u8]) -> Result<Result<(), i32>, Error> {
        // Preserve the existing CRT rename's path/error behavior, including
        // empty and overlong names. It does not add a verbatim-path prefix.
        let from = io::input::windows_wide(from)?;
        let to = io::input::windows_wide(to)?;
        // SAFETY: both buffers remain live. COPY_ALLOWED matches the existing
        // CRT behavior; omitting REPLACE_EXISTING preserves the atomic refusal
        // to overwrite an existing destination, without a racy existence check.
        Ok(status(unsafe {
            MoveFileExW(from.as_ptr(), to.as_ptr(), 2)
        }))
    }
}

#[cfg(any(unix, windows))]
pub(super) use platform::{remove, rename};

#[cfg(not(any(unix, windows)))]
pub(super) fn remove(path: &[u8]) -> Result<Result<(), i32>, Error> {
    Ok(std::fs::remove_file(super::super::io::path(path)).map_err(super::super::io::errno))
}
#[cfg(not(any(unix, windows)))]
pub(super) fn rename(from: &[u8], to: &[u8]) -> Result<Result<(), i32>, Error> {
    Ok(
        std::fs::rename(super::super::io::path(from), super::super::io::path(to))
            .map_err(super::super::io::errno),
    )
}
