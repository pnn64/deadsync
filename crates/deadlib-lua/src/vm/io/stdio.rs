//! Unbuffered process handles. Lua owns its fallible buffering; accessing a
//! standard stream must not initialize Rust's infallible global BufReader or
//! LineWriter. These handles are borrowed and are never closed here.
use std::io;

// The glibc profiles choose a FILE buffer from st_blksize, capped at BUFSIZ.
// Read that metadata directly without owning/duplicating the borrowed stdout fd.
#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    any(target_arch = "x86", target_arch = "x86_64")
))]
pub(super) fn output_buffer_size() -> usize {
    use std::ffi::c_void;
    unsafe extern "C" {
        fn fstat64(fd: i32, output: *mut c_void) -> i32;
    }
    // Public Linux/glibc stat64 ABI, checked by the retained x64/i686 C probe:
    // x64: 144 bytes, st_blksize at 56 (8 bytes); i686: 96, at 52 (4 bytes).
    #[repr(C, align(8))]
    struct Stat([u8; 144]);
    let mut stat = Stat([0; 144]);
    // SAFETY: storage is aligned and at least as large as either supported ABI.
    // Descriptor 1 is only queried; fstat64 reports an error if it is closed.
    if unsafe { fstat64(1, stat.0.as_mut_ptr().cast()) } != 0 {
        return 8192;
    }
    let offset = if size_of::<usize>() == 8 { 56 } else { 52 };
    let mut bytes = [0; size_of::<isize>()];
    bytes.copy_from_slice(&stat.0[offset..offset + size_of::<isize>()]);
    let size = isize::from_ne_bytes(bytes);
    if size > 0 {
        (size as usize).min(8192)
    } else {
        8192
    }
}

#[cfg(any(windows, test))]
mod codec;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::{is_terminal, read as read_raw, write as write_raw};

#[cfg(unix)]
mod unix {
    use super::*;
    use std::ffi::{c_int, c_void};
    unsafe extern "C" {
        #[link_name = "read"]
        fn read_fd(fd: c_int, output: *mut c_void, length: usize) -> isize;
        #[link_name = "write"]
        fn write_fd(fd: c_int, input: *const c_void, length: usize) -> isize;
        fn isatty(fd: c_int) -> c_int;
    }
    pub(in crate::vm::io) fn is_terminal(kind: usize) -> bool {
        // SAFETY: the known standard descriptor is only queried, never owned.
        unsafe { isatty(kind as c_int) == 1 }
    }
    pub(in crate::vm::io) fn read(bytes: &mut [u8]) -> io::Result<usize> {
        // SAFETY: descriptor 0 is borrowed. The exclusive, initialized output
        // buffer lives for the synchronous read and its length fits ssize_t.
        let count = unsafe {
            read_fd(
                0,
                bytes.as_mut_ptr().cast(),
                bytes.len().min(isize::MAX as usize),
            )
        };
        result(count)
    }
    pub(in crate::vm::io) fn write(kind: usize, bytes: &[u8]) -> io::Result<usize> {
        // SAFETY: kind is stdout/stderr; bytes stays live for the call. The
        // borrowed descriptor is not closed or retained beyond the operation.
        let count = unsafe {
            write_fd(
                kind as c_int,
                bytes.as_ptr().cast(),
                bytes.len().min(isize::MAX as usize),
            )
        };
        result(count)
    }
    fn result(count: isize) -> io::Result<usize> {
        if count < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(count as usize)
        }
    }
}
#[cfg(unix)]
pub(super) use unix::{is_terminal, read as read_raw, write as write_raw};

#[cfg(not(any(unix, windows)))]
pub(super) fn is_terminal(_: usize) -> bool {
    false
}
#[cfg(not(any(unix, windows)))]
fn read_raw(bytes: &mut [u8]) -> io::Result<usize> {
    std::io::Read::read(&mut std::io::stdin(), bytes)
}
#[cfg(not(any(unix, windows)))]
fn write_raw(kind: usize, bytes: &[u8]) -> io::Result<usize> {
    use std::io::Write;
    if kind == 1 {
        std::io::stdout().write(bytes)
    } else {
        std::io::stderr().write(bytes)
    }
}

fn invalid_handle(error: &io::Error) -> bool {
    error.raw_os_error() == Some(if cfg!(windows) { 6 } else { 9 })
}
pub(super) fn read(bytes: &mut [u8]) -> io::Result<usize> {
    match read_raw(bytes) {
        Err(error) if invalid_handle(&error) => Ok(0),
        result => result,
    }
}
fn write(kind: usize, bytes: &[u8]) -> io::Result<usize> {
    // Retain std's detached-process behavior: absent input is EOF and absent
    // stdout/stderr accepts bytes. Other OS failures remain observable.
    match write_raw(kind, bytes) {
        Err(error) if invalid_handle(&error) => Ok(bytes.len()),
        result => result,
    }
}
pub(super) fn write_all(kind: usize, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        match write(kind, bytes) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
