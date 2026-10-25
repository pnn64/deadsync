//! System-shell execution. Lua receives the platform's raw 5.1 status code.
#[cfg(not(windows))]
use std::process::Command;

#[cfg(windows)]
fn shell_path(path: &std::path::Path) -> Result<std::path::PathBuf, i32> {
    if path.as_os_str().is_empty() {
        return Err(22);
    }
    // A non-PATH spawn resolves a bare filename in the current directory.
    let path = if path.components().count() == 1 {
        std::path::Path::new(".").join(path)
    } else {
        path.to_path_buf()
    };
    if path.extension().is_some() {
        path.metadata().map_err(super::io::errno)?;
        return Ok(path);
    }
    for extension in ["com", "exe", "bat", "cmd"] {
        let candidate = path.with_extension(extension);
        if candidate.try_exists().map_err(super::io::errno)? {
            return Ok(candidate);
        }
    }
    Err(2)
}

#[cfg(windows)]
pub(super) fn execute(command: Option<&[u8]>) -> Result<i32, i32> {
    let comspec = std::env::var_os("COMSPEC");
    if comspec.as_ref().is_some_and(|path| path.is_empty()) {
        // The pinned CRT invokes its fatal invalid-parameter handler for an
        // explicitly empty COMSPEC, including shell-availability queries.
        std::process::abort();
    }
    let Some(command) = command else {
        return Ok(i32::from(
            comspec.is_some_and(|path| std::path::Path::new(&path).exists()),
        ));
    };
    if let Some(path) = comspec {
        match shell_path(std::path::Path::new(&path))
            .and_then(|executable| super::platform::spawn(&executable, &path, command))
        {
            Ok(status) => return Ok(status),
            Err(2 | 13) => {}
            Err(code) => return Err(code),
        }
    }
    // Match the CRT's explicit current-directory/PATH fallback. Letting Rust
    // search also includes OS system directories that the CRT does not search.
    let name = std::ffi::OsStr::new("cmd.exe");
    match shell_path(std::path::Path::new(name))
        .and_then(|path| super::platform::spawn(&path, name, command))
    {
        Ok(status) => return Ok(status),
        Err(2) => {}
        Err(code) => return Err(code),
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return Err(2);
    };
    for directory in std::env::split_paths(&paths) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        let path = directory.join("cmd.exe");
        match shell_path(&path).and_then(|path| super::platform::spawn(&path, name, command)) {
            Ok(status) => return Ok(status),
            Err(2) => {}
            Err(code) => return Err(code),
        }
    }
    Err(2)
}

#[cfg(not(windows))]
pub(super) fn execute(command: Option<&[u8]>) -> Result<i32, i32> {
    use std::os::unix::{ffi::OsStrExt, process::ExitStatusExt};
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(std::ffi::OsStr::from_bytes(command.unwrap_or(b"exit 0")))
        .status();
    if command.is_none() {
        return Ok(i32::from(status.is_ok_and(|status| status.success())));
    }
    status
        .map(ExitStatusExt::into_raw)
        .map_err(super::io::errno)
}
