//! Fallible package-path startup. No borrowed process strings escape this module.
//! Windows buffer contracts:
//! https://learn.microsoft.com/en-us/windows/win32/api/processenv/nf-processenv-getenvironmentvariablew
//! https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-getmodulefilenamew
use crate::{
    Error,
    memory::{Buffer, Memory},
};
use std::ffi::CStr;

fn append(out: &mut Buffer<u8>, bytes: &[u8]) -> Result<(), Error> {
    out.try_reserve(bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn expand(source: Option<&[u8]>, default: &[u8], memory: Memory) -> Result<Buffer<u8>, Error> {
    let mut out = Buffer::new_in(memory);
    let Some(mut source) = source else {
        append(&mut out, default)?;
        return Ok(out);
    };
    // Lua 5.1 first replaces each non-overlapping ';;' with ';\x01;', then
    // replaces every \x01 with the default. Keep literal sentinels equivalent.
    while let Some((&byte, rest)) = source.split_first() {
        if source.starts_with(b";;") {
            append(&mut out, b";")?;
            append(&mut out, default)?;
            append(&mut out, b";")?;
            source = &source[2..];
        } else {
            append(&mut out, if byte == 1 { default } else { &source[..1] })?;
            source = rest;
        }
    }
    Ok(out)
}

pub(super) fn initial(
    variable: &CStr,
    default: &[u8],
    memory: Memory,
) -> Result<Buffer<u8>, Error> {
    let source = environment(variable, memory.clone())?;
    let path = expand(source.as_deref(), default, memory.clone())?;
    #[cfg(windows)]
    {
        let parent = windows::executable_parent(memory.clone())?;
        let mut out = Buffer::new_in(memory);
        for &byte in path.iter() {
            append(
                &mut out,
                if byte == b'!' {
                    &parent
                } else {
                    std::slice::from_ref(&byte)
                },
            )?;
        }
        Ok(out)
    }
    #[cfg(not(windows))]
    {
        Ok(path)
    }
}

#[cfg(unix)]
fn environment(variable: &CStr, memory: Memory) -> Result<Option<Buffer<u8>>, Error> {
    unsafe extern "C" {
        fn getenv(name: *const std::ffi::c_char) -> *const std::ffi::c_char;
    }
    // SAFETY: variable is terminated. As with other libc environment readers,
    // hosts must not mutate the process environment concurrently. Rust marks
    // set_var/remove_var unsafe for this reason. Copy the result before return.
    let pointer = unsafe { getenv(variable.as_ptr()) };
    if pointer.is_null() {
        return Ok(None);
    }
    // SAFETY: getenv returned a live, terminated process-owned string.
    let source = unsafe { CStr::from_ptr(pointer) }.to_bytes();
    let mut out = Buffer::new_in(memory);
    for chunk in source.utf8_chunks() {
        append(&mut out, chunk.valid().as_bytes())?;
        if !chunk.invalid().is_empty() {
            append(&mut out, "\u{fffd}".as_bytes())?;
        }
    }
    Ok(Some(out))
}

#[cfg(windows)]
use windows::environment;

#[cfg(not(any(unix, windows)))]
fn environment(variable: &CStr, memory: Memory) -> Result<Option<Buffer<u8>>, Error> {
    // These targets retain the std environment bridge. Allocation-failure
    // guarantees are validated on the documented Windows/Linux profiles.
    let Some(source) = std::env::var_os(variable.to_str().expect("ASCII environment name")) else {
        return Ok(None);
    };
    let mut out = Buffer::new_in(memory);
    append(&mut out, source.to_string_lossy().as_bytes())?;
    Ok(Some(out))
}

#[cfg(windows)]
mod windows {
    use super::*;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetEnvironmentVariableW(name: *const u16, buffer: *mut u16, size: u32) -> u32;
        fn GetModuleFileNameW(module: *mut std::ffi::c_void, buffer: *mut u16, size: u32) -> u32;
        fn GetLastError() -> u32;
        fn SetLastError(error: u32);
    }
    fn utf8(source: &[u16], memory: Memory) -> Result<Buffer<u8>, Error> {
        let mut out = Buffer::new_in(memory);
        for value in char::decode_utf16(source.iter().copied()) {
            let mut bytes = [0; 4];
            append(
                &mut out,
                value
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
                    .encode_utf8(&mut bytes)
                    .as_bytes(),
            )?;
        }
        Ok(out)
    }
    pub(super) fn environment(
        variable: &CStr,
        memory: Memory,
    ) -> Result<Option<Buffer<u8>>, Error> {
        let mut name = [0u16; 32];
        assert!(variable.to_bytes().len() < name.len());
        for (out, byte) in name.iter_mut().zip(variable.to_bytes()) {
            *out = u16::from(*byte);
        }
        let mut wide = Buffer::new_in(memory.clone());
        wide.try_reserve_exact(256)?;
        wide.resize(256, 0);
        loop {
            let size = u32::try_from(wide.len()).map_err(|_| Error::memory())?;
            // SAFETY: terminated ASCII name, initialized writable buffer, and
            // matching element count. Reset last-error to distinguish empty
            // values from absent ones. A concurrent resize is retried below.
            let (length, error) = unsafe {
                SetLastError(0);
                let length = GetEnvironmentVariableW(name.as_ptr(), wide.as_mut_ptr(), size);
                (length as usize, GetLastError())
            };
            if length == 0 {
                return match error {
                    0 => Ok(Some(Buffer::new_in(memory))),
                    203 => Ok(None), // ERROR_ENVVAR_NOT_FOUND
                    _ => Err(Error::runtime_static("cannot read Lua package environment")),
                };
            }
            if length < wide.len() {
                return utf8(&wide[..length], memory).map(Some);
            }
            // GetEnvironmentVariableW reports required capacity including NUL.
            wide.try_reserve_exact(length - wide.len())?;
            wide.resize(length, 0);
        }
    }
    pub(super) fn executable_parent(memory: Memory) -> Result<Buffer<u8>, Error> {
        let mut wide = Buffer::new_in(memory.clone());
        wide.try_reserve_exact(256)?;
        wide.resize(256, 0);
        loop {
            let size = u32::try_from(wide.len()).map_err(|_| Error::memory())?;
            // SAFETY: NULL selects this executable; wide owns size initialized
            // UTF-16 slots. A size return denotes truncation, so retry it.
            let length =
                unsafe { GetModuleFileNameW(std::ptr::null_mut(), wide.as_mut_ptr(), size) }
                    as usize;
            if length == 0 {
                return Err(Error::runtime_static("cannot read executable path"));
            }
            if length < wide.len() {
                let mut path = utf8(&wide[..length], memory)?;
                let text = std::str::from_utf8(&path).expect("UTF-16 replacement produces UTF-8");
                let parent = std::path::Path::new(text)
                    .parent()
                    .ok_or_else(|| Error::runtime_static("executable has no parent directory"))?;
                let length = parent.as_os_str().as_encoded_bytes().len();
                path.truncate(length);
                return Ok(path);
            }
            let length = wide.len().checked_mul(2).ok_or_else(Error::memory)?;
            wide.try_reserve_exact(length - wide.len())?;
            wide.resize(length, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_insertion_preserves_lua51_sentinel_and_non_overlapping_pairs() {
        for (source, expected) in [
            (None, b"DEFAULT".as_slice()),
            (Some(b"".as_slice()), b""),
            (Some(b";;"), b";DEFAULT;"),
            (Some(b";;;;"), b";DEFAULT;;DEFAULT;"),
            (Some(b"a;;;b\x01c"), b"a;DEFAULT;;bDEFAULTc"),
        ] {
            assert_eq!(
                &*expand(source, b"DEFAULT", Memory::default()).unwrap(),
                expected
            );
        }
    }
}
