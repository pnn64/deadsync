//! Fallible UTF-16 shell lookup, preserving the existing CRT-style search rules.
use crate::vm::{
    io,
    os::{environment, platform},
};
use crate::{Error, memory::fallible::VecExt};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileAttributesW(path: *const u16) -> u32;
}
fn exists(path: &[u16]) -> Result<bool, i32> {
    debug_assert_eq!(path.last(), Some(&0));
    // SAFETY: path is terminated and lives for this synchronous query. This
    // matches the CRT accessibility query without allocating a Rust PathBuf.
    if unsafe { GetFileAttributesW(path.as_ptr()) } != u32::MAX {
        return Ok(true);
    }
    match io::errno(std::io::Error::last_os_error()) {
        2 => Ok(false),
        code => Err(code),
    }
}
fn shell_path(name: &[u16]) -> Result<Result<Vec<u16>, i32>, Error> {
    if name.is_empty() {
        return Ok(Err(22));
    }
    let mut path = Vec::new();
    if !name.iter().any(|u| matches!(u, 47 | 92 | 58)) {
        path.try_extend_from_slice(&[46, 92])?; // Explicit current-directory lookup.
    }
    path.try_extend_from_slice(name)?;
    let filename = name
        .rsplit(|u| matches!(u, 47 | 92 | 58))
        .next()
        .unwrap_or_default();
    let has_extension = filename
        .iter()
        .rposition(|&u| u == 46)
        .is_some_and(|i| i != 0);
    let no_filename = filename.is_empty() || filename == [46] || filename == [46, 46];
    if has_extension || no_filename {
        path.try_push(0)?;
        return Ok(match exists(&path) {
            Ok(true) => Ok(path),
            Ok(false) => Err(2),
            Err(code) => Err(code),
        });
    }
    let prefix = path.len();
    for extension in [".com", ".exe", ".bat", ".cmd"] {
        path.truncate(prefix);
        path.try_extend(extension.encode_utf16())?;
        path.try_push(0)?;
        match exists(&path) {
            Ok(true) => return Ok(Ok(path)),
            Ok(false) => {}
            Err(code) => return Ok(Err(code)),
        }
    }
    Ok(Err(2))
}
fn attempt(path: &[u16], name: &[u16], command: &[u8]) -> Result<Result<i32, i32>, Error> {
    match shell_path(path)? {
        Ok(path) => platform::spawn(&path, name, command),
        Err(code) => Ok(Err(code)),
    }
}
pub(in crate::vm::os) fn execute(command: Option<&[u8]>) -> Result<Result<i32, i32>, Error> {
    let comspec = environment::wide_value(&b"COMSPEC\0".map(u16::from))?;
    if comspec.as_ref().is_some_and(|v| v.is_empty()) {
        // Retain the separately tested pinned-CRT invalid-parameter behavior.
        std::process::abort();
    }
    let Some(command) = command else {
        let Some(mut path) = comspec else {
            return Ok(Ok(0));
        };
        path.try_push(0)?;
        return Ok(Ok(i32::from(exists(&path).unwrap_or(false))));
    };
    if let Some(path) = comspec {
        match attempt(&path, &path, command)? {
            Ok(status) => return Ok(Ok(status)),
            Err(2 | 13) => {}
            Err(code) => return Ok(Err(code)),
        }
    }
    let name = b"cmd.exe".map(u16::from);
    match attempt(&name, &name, command)? {
        Ok(status) => return Ok(Ok(status)),
        Err(2) => {}
        Err(code) => return Ok(Err(code)),
    }
    let Some(paths) = environment::wide_value(&b"PATH\0".map(u16::from))? else {
        return Ok(Err(2));
    };
    let mut path = Vec::new();
    let (mut start, mut quoted) = (0, false);
    for end in 0..=paths.len() {
        if end < paths.len() && paths[end] == 34 {
            quoted = !quoted;
        }
        if end != paths.len() && (paths[end] != 59 || quoted) {
            continue;
        }
        path.clear();
        path.try_extend(paths[start..end].iter().copied().filter(|&u| u != 34))?;
        start = end + 1;
        if path.is_empty() {
            continue;
        }
        if !matches!(path.last(), Some(47 | 92 | 58)) {
            path.try_push(92)?;
        }
        path.try_extend_from_slice(&name)?;
        match attempt(&path, &name, command)? {
            Ok(status) => return Ok(Ok(status)),
            Err(2) => {}
            Err(code) => return Ok(Err(code)),
        }
    }
    Ok(Err(2))
}
