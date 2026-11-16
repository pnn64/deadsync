//! System-shell execution. Lua receives the platform's raw 5.1 status code.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;
#[cfg(windows)]
pub(super) use windows::execute;

#[cfg(unix)]
#[allow(unsafe_code)]
pub(super) fn execute(command: Option<&[u8]>) -> Result<Result<i32, i32>, crate::Error> {
    use crate::memory::fallible::{SliceExt, VecExt};
    use std::{ffi::c_char, ptr};
    unsafe extern "C" {
        #[link_name = "system"]
        fn system_command(command: *const c_char) -> i32;
    }
    let command = command
        .map(|bytes| {
            let mut terminated = bytes.try_to_vec()?;
            terminated.try_push(0)?;
            Ok::<_, crate::Error>(terminated)
        })
        .transpose()?;
    // SAFETY: a null command queries availability; otherwise storage is NUL
    // terminated and lives through the synchronous call. Native system owns
    // signal masking/disposition, spawning and reaping, returning a wait status.
    // As with getenv/spawn, the host must not concurrently mutate POSIX environ.
    let status =
        unsafe { system_command(command.as_ref().map_or(ptr::null(), |v| v.as_ptr().cast())) };
    Ok(if status == -1 {
        Err(super::io::errno(std::io::Error::last_os_error()))
    } else {
        Ok(status)
    })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn execute(_: Option<&[u8]>) -> Result<Result<i32, i32>, crate::Error> {
    Ok(Err(38))
}
