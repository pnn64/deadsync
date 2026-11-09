//! Fallible copies of process environment values; no Lua callbacks occur here.
use crate::{Error, memory::fallible::VecExt};

#[cfg(windows)]
pub(super) fn value(name: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let terminated = super::super::io::input::windows_wide(name)?;
    let Some(wide) = wide_value(&terminated)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    for character in char::decode_utf16(wide) {
        let mut buffer = [0; 4];
        bytes.try_extend_from_slice(
            character
                .unwrap_or(char::REPLACEMENT_CHARACTER)
                .encode_utf8(&mut buffer)
                .as_bytes(),
        )?;
    }
    Ok(Some(bytes))
}

#[cfg(unix)]
pub(super) fn value(name: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    use crate::memory::fallible::SliceExt;
    use std::ffi::{CStr, c_char};
    unsafe extern "C" {
        fn getenv(name: *const c_char) -> *const c_char;
    }
    let mut terminated = name.try_to_vec()?;
    terminated.try_push(0)?;
    // SAFETY: name is terminated and the result is copied before returning.
    // As with the engine's package environment, the host must not mutate POSIX
    // environ concurrently with this call. Rust's environment mutators are unsafe.
    let pointer = unsafe { getenv(terminated.as_ptr().cast()) };
    if pointer.is_null() {
        return Ok(None);
    }
    // SAFETY: getenv returned a live process-owned, NUL-terminated value. Unix
    // os.getenv preserves arbitrary bytes rather than decoding them as UTF-8.
    unsafe { CStr::from_ptr(pointer) }
        .to_bytes()
        .try_to_vec()
        .map(Some)
}

// Preserve native UTF-16 when the caller passes the value back to Windows.
// `name` includes its terminator; returned values exclude it.
#[cfg(windows)]
pub(in crate::vm) fn wide_value(name: &[u16]) -> Result<Option<Vec<u16>>, Error> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetEnvironmentVariableW(name: *const u16, output: *mut u16, size: u32) -> u32;
        fn GetLastError() -> u32;
        fn SetLastError(code: u32);
    }
    assert_eq!(name.last(), Some(&0));
    let mut wide = Vec::new();
    wide.try_resize(256, 0)?;
    loop {
        let size = u32::try_from(wide.len()).map_err(|_| Error::memory())?;
        // SAFETY: the name is terminated; the output owns size initialized
        // units. Reset last-error to distinguish an empty value from absence.
        let (length, error) = unsafe {
            SetLastError(0);
            let length = GetEnvironmentVariableW(name.as_ptr(), wide.as_mut_ptr(), size);
            (length as usize, GetLastError())
        };
        if length == 0 {
            return match error {
                0 => Ok(Some(Vec::new())),
                8 | 14 => Err(Error::memory()),
                _ => Ok(None),
            };
        }
        if length >= wide.len() {
            // The required count includes NUL. A concurrent resize is retried.
            wide.try_resize(length, 0)?;
            continue;
        }
        wide.truncate(length);
        return Ok(Some(wide));
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn value(name: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    use crate::memory::fallible::SliceExt;
    // The documented allocation-failure profiles are Windows and Linux.
    std::env::var_os(String::from_utf8_lossy(name).as_ref())
        .map(|value| value.to_string_lossy().as_bytes().try_to_vec())
        .transpose()
}
