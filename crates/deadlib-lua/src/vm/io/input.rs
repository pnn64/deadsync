//! Fallible path storage and owned OS handles for Lua files.
use crate::{
    Error,
    memory::fallible::{self, VecExt},
};
use std::{
    fs::File,
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary_name() -> Result<String, Error> {
    let nonce = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    fallible::format(format_args!(
        "deadsync-lua-{}-{nonce}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

#[cfg(target_os = "linux")]
pub(super) fn open(path: &[u8], mode: u8, update: bool) -> Result<Result<File, i32>, Error> {
    use std::{
        ffi::{c_char, c_int},
        os::fd::FromRawFd,
    };
    unsafe extern "C" {
        #[link_name = "open64"]
        fn open_file(path: *const c_char, flags: c_int, ...) -> c_int;
    }
    let mut terminated = Vec::new();
    terminated.try_extend_from_slice(path)?;
    terminated.try_push(0)?;
    // SAFETY: terminated owns a NUL-terminated byte path for this call. Linux
    // open64 supports large files on i686. The caller validated r/w/a; the
    // creation mode matches OpenOptions' 0666, subject to the process umask.
    let access = if update { 2 } else { i32::from(mode != b'r') };
    let flags = access
        | 0x80000
        | match mode {
            b'w' => 0x40 | 0x200, // O_CREAT | O_TRUNC
            b'a' => 0x40 | 0x400, // O_CREAT | O_APPEND
            _ => 0,
        };
    let descriptor = unsafe { open_file(terminated.as_ptr().cast(), flags, 0o666u32) };
    if descriptor < 0 {
        return Ok(Err(super::errno(std::io::Error::last_os_error())));
    }
    // SAFETY: open64 returned a fresh owned descriptor. File becomes its only
    // owner, closes it on Drop, and requires no allocation to take ownership.
    Ok(Ok(unsafe { File::from_raw_fd(descriptor) }))
}

#[cfg(target_os = "linux")]
pub(super) fn temporary() -> Result<Result<File, i32>, Error> {
    use std::{
        ffi::{CStr, c_char, c_int},
        os::fd::FromRawFd,
    };
    unsafe extern "C" {
        fn getenv(name: *const c_char) -> *const c_char;
        #[link_name = "open64"]
        fn open_file(path: *const c_char, flags: c_int, ...) -> c_int;
        fn unlink(path: *const c_char) -> c_int;
    }
    let mut path = Vec::new();
    // SAFETY: the literal is terminated. As for the module search environment,
    // the host must not mutate POSIX environ concurrently. Copy before any call
    // back into the host; an absent TMPDIR selects the same /tmp default as std.
    let directory = unsafe { getenv(c"TMPDIR".as_ptr()) };
    let directory = if directory.is_null() {
        b"/tmp".as_slice()
    } else {
        unsafe { CStr::from_ptr(directory) }.to_bytes()
    };
    path.try_extend_from_slice(directory)?;
    if !path.is_empty() && !path.ends_with(b"/") {
        path.try_push(b'/')?;
    }
    let prefix = path.len();
    for _ in 0..64 {
        path.truncate(prefix);
        path.try_extend_from_slice(temporary_name()?.as_bytes())?;
        path.try_push(0)?;
        // SAFETY: path owns a terminated byte path. O_EXCL prevents replacement
        // or following a pre-existing link, O_CLOEXEC prevents inheritance.
        let descriptor =
            unsafe { open_file(path.as_ptr().cast(), 2 | 0x40 | 0x80 | 0x80000, 0o666u32) };
        if descriptor < 0 {
            let code = super::errno(std::io::Error::last_os_error());
            if code == 17 {
                continue;
            }
            return Ok(Err(code));
        }
        // SAFETY: this fresh descriptor has one owner. No Rust allocations are
        // needed after opening; unlink leaves its lifetime entirely with File.
        let file = unsafe { File::from_raw_fd(descriptor) };
        if unsafe { unlink(path.as_ptr().cast()) } != 0 {
            return Ok(Err(super::errno(std::io::Error::last_os_error())));
        }
        return Ok(Ok(file));
    }
    Ok(Err(17))
}

#[cfg(windows)]
pub(in crate::vm) use windows::{absolute as windows_absolute, temporary_directory};
#[cfg(windows)]
pub(super) use windows::{open, temporary};
#[cfg(windows)]
pub(in crate::vm) use windows::{path as windows_path, wide as windows_wide};

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{ffi::c_void, os::windows::io::FromRawHandle, ptr};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetTempPathW(size: u32, output: *mut u16) -> u32;
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const std::ffi::c_char) -> *const c_void;
        fn GetFullPathNameW(
            name: *const u16,
            size: u32,
            output: *mut u16,
            part: *mut *mut u16,
        ) -> u32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            security: *const c_void,
            creation: u32,
            flags: u32,
            template: *mut c_void,
        ) -> *mut c_void;
    }

    fn last_error() -> i32 {
        super::super::errno(std::io::Error::last_os_error())
    }

    pub(in crate::vm) fn temporary_directory() -> Result<Result<Vec<u16>, i32>, Error> {
        type GetTempPath = unsafe extern "system" fn(u32, *mut u16) -> u32;
        let module_name = b"kernel32.dll\0".map(u16::from);
        // SAFETY: both names are terminated literals. The borrowed kernel32
        // module stays loaded for the process lifetime. Match std's preference
        // for GetTempPath2W, retaining GetTempPathW on older Windows versions.
        let get_path: GetTempPath = unsafe {
            let module = GetModuleHandleW(module_name.as_ptr());
            let address = if module.is_null() {
                ptr::null()
            } else {
                GetProcAddress(module, c"GetTempPath2W".as_ptr())
            };
            if address.is_null() {
                GetTempPathW
            } else {
                std::mem::transmute::<*const c_void, GetTempPath>(address)
            }
        };
        let mut path = Vec::new();
        path.try_resize(261, 0)?;
        loop {
            let capacity = u32::try_from(path.len()).map_err(|_| Error::memory())?;
            // SAFETY: path is initialized for capacity units. A required size
            // larger than the supplied buffer is reserved fallibly and retried.
            let used = unsafe { get_path(capacity, path.as_mut_ptr()) } as usize;
            if used == 0 {
                return Ok(Err(last_error()));
            }
            if used >= path.len() {
                path.try_resize(used.checked_add(1).ok_or_else(Error::memory)?, 0)?;
                continue;
            }
            path.truncate(used);
            return Ok(Ok(path));
        }
    }

    pub(in crate::vm::io) fn temporary() -> Result<Result<File, i32>, Error> {
        for _ in 0..64 {
            let mut path = match temporary_directory()? {
                Ok(path) => path,
                Err(code) => return Ok(Err(code)),
            };
            path.try_extend(temporary_name()?.encode_utf16())?;
            path.try_push(0)?;
            let name = match absolute(path)? {
                Ok(name) => name,
                Err(code) => return Ok(Err(code)),
            };
            // SAFETY: name is terminated and live. CREATE_NEW avoids replacing
            // existing files; DELETE_ON_CLOSE owns cleanup even if later Lua
            // userdata allocation fails. These flags match the old OpenOptions.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    0xc0000000,
                    7,
                    ptr::null(),
                    1,
                    0x04000000 | 0x100,
                    ptr::null_mut(),
                )
            };
            if handle != (-1isize) as *mut c_void {
                // SAFETY: a fresh owned handle, with File as its sole owner.
                return Ok(Ok(unsafe { File::from_raw_handle(handle) }));
            }
            let code = last_error();
            if code != 17 {
                return Ok(Err(code));
            }
        }
        Ok(Err(17))
    }

    // Match the existing Rust Path conversion of Lua bytes, including replacement
    // of invalid UTF-8, without allocating a temporary lossy String.
    pub(in crate::vm) fn wide(path: &[u8]) -> Result<Vec<u16>, Error> {
        let mut result = Vec::new();
        for chunk in path.utf8_chunks() {
            for unit in chunk.valid().encode_utf16() {
                result.try_push(unit)?;
            }
            if !chunk.invalid().is_empty() {
                result.try_push(0xfffd)?;
            }
        }
        result.try_push(0)?;
        Ok(result)
    }

    pub(in crate::vm) fn absolute(mut path: Vec<u16>) -> Result<Result<Vec<u16>, i32>, Error> {
        const SLASH: u16 = b'\\' as u16;
        const FORWARD: u16 = b'/' as u16;
        const COLON: u16 = b':' as u16;
        const VERBATIM: &[u16] = &[92, 92, 63, 92];
        const NT: &[u16] = &[92, 63, 63, 92];
        if path.starts_with(VERBATIM) || path.starts_with(NT) {
            return Ok(Ok(path));
        }
        // Preserve std's short absolute/UNC path behavior; relative and long
        // paths are normalized once and then passed to CreateFileW verbatim.
        if path.len() < 248 {
            match path.as_slice() {
                [drive, COLON, 0] | [drive, COLON, SLASH | FORWARD, ..]
                    if !matches!(*drive, SLASH | FORWARD) =>
                {
                    return Ok(Ok(path));
                }
                [SLASH | FORWARD, SLASH | FORWARD, ..] => return Ok(Ok(path)),
                _ => {}
            }
        }
        let mut output = Vec::new();
        output.try_resize(256, 0)?;
        loop {
            let capacity = u32::try_from(output.len()).map_err(|_| Error::memory())?;
            // SAFETY: input is terminated and remains live; output is initialized
            // for capacity UTF-16 units. A larger required count is retried.
            let used = unsafe {
                GetFullPathNameW(
                    path.as_ptr(),
                    capacity,
                    output.as_mut_ptr(),
                    ptr::null_mut(),
                )
            } as usize;
            if used == 0 {
                return Ok(Err(last_error()));
            }
            if used >= output.len() {
                output.try_resize(used.checked_add(1).ok_or_else(Error::memory)?, 0)?;
                continue;
            }
            output.truncate(used);
            break;
        }
        let (prefix, skip): (&[u16], usize) = match output.as_slice() {
            [_, COLON, SLASH, ..] => (VERBATIM, 0),
            [92, 92, 46, 92, ..] => (VERBATIM, 4),
            [92, 92, 63, 92, ..] | [92, 63, 63, 92, ..] => (&[], 0),
            [92, 92, ..] => (&[92, 92, 63, 92, 85, 78, 67, 92], 2),
            _ => (&[], 0),
        };
        path.clear();
        path.try_extend_from_slice(prefix)?;
        path.try_extend_from_slice(&output[skip..])?;
        path.try_push(0)?;
        Ok(Ok(path))
    }

    pub(in crate::vm) fn path(bytes: &[u8]) -> Result<Result<Vec<u16>, i32>, Error> {
        absolute(wide(bytes)?)
    }

    pub(in crate::vm::io) fn open(
        path: &[u8],
        mode: u8,
        update: bool,
    ) -> Result<Result<File, i32>, Error> {
        if path.is_empty() {
            return Ok(Err(22));
        }
        let path = match self::path(path)? {
            Ok(path) => path,
            Err(code) => return Ok(Err(code)),
        };
        let access = match (mode, update) {
            (b'r', false) => 0x80000000, // GENERIC_READ
            (b'w', false) => 0x40000000, // GENERIC_WRITE
            (b'a', false) => 0x00120114, // FILE_GENERIC_WRITE without FILE_WRITE_DATA
            (b'a', true) => 0x80120114,
            _ => 0xc0000000,
        };
        let creation = match mode {
            b'w' => 2,
            b'a' => 4,
            _ => 3,
        };
        // SAFETY: path is terminated and live. Access/disposition match the
        // previous OpenOptions modes; append excludes positional writes. No
        // inherited handle, security descriptor or template is supplied.
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                3,
                ptr::null(),
                creation,
                0,
                ptr::null_mut(),
            )
        };
        if handle == (-1isize) as *mut c_void {
            return Ok(Err(last_error()));
        }
        // SAFETY: the successful call returned a fresh owned handle. File is
        // its sole owner and closes it on Drop, including rejected parser setup.
        Ok(Ok(unsafe { File::from_raw_handle(handle) }))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub(super) fn open(path: &[u8], mode: u8, update: bool) -> Result<Result<File, i32>, Error> {
    Ok(std::fs::OpenOptions::new()
        .read(mode == b'r' || update)
        .write(mode != b'r' || update)
        .create(mode != b'r')
        .truncate(mode == b'w')
        .append(mode == b'a')
        .open(super::path(path))
        .map_err(super::errno))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub(super) fn temporary() -> Result<Result<File, i32>, Error> {
    for _ in 0..64 {
        let path = std::env::temp_dir().join(temporary_name()?);
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                if let Err(error) = std::fs::remove_file(path) {
                    return Ok(Err(super::errno(error)));
                }
                return Ok(Ok(file));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Ok(Err(super::errno(error))),
        }
    }
    Ok(Err(17))
}
