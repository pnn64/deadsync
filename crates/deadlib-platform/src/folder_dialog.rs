//! Native selection of one directory. Application workflows stay with the caller.

use std::{fmt, path::PathBuf};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod unix;
#[cfg(windows)]
mod windows;

/// Failure to open a folder dialog or retrieve its selected filesystem path.
#[derive(Debug)]
pub struct DialogError(String);

impl DialogError {
    fn new(message: impl fmt::Display) -> Self {
        Self(message.to_string())
    }
}

impl fmt::Display for DialogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DialogError {}

/// Blocks until the user selects a directory or cancels (`Ok(None)`).
///
/// Call from a worker while the application's event loop continues running.
/// On macOS the native panel is dispatched to that main event loop. On Windows
/// the calling thread must permit STA COM initialization (a fresh worker does).
/// Linux and FreeBSD use the desktop portal, falling back to Zenity on failure.
/// Errors are distinct from cancellation; a canceled portal never opens Zenity.
pub fn pick_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    if title.contains('\0') {
        return Err(DialogError::new(
            "Folder dialog title contains a NUL character",
        ));
    }
    #[cfg(windows)]
    return windows::pick_folder(title);
    #[cfg(target_os = "macos")]
    return macos::pick_folder(title);
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    return unix::pick_folder(title);
    #[cfg(not(any(
        windows,
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd"
    )))]
    Err(DialogError::new(
        "Folder dialogs are unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn nul_in_title_is_rejected_before_opening_a_dialog() {
        assert!(super::pick_folder("Choose\0folder").is_err());
    }
}
