//! Console window management.
//!
//! The release binary is built with `windows_subsystem = "windows"`, so it has
//! no console of its own — double-clicking the executable (or launching it from
//! Steam/a frontend) shows the game with no stray terminal window, matching what
//! players expect from a shipped game.
//!
//! [`init`] reconciles that with the two cases where output *is* wanted:
//!   * Launched from an existing terminal (e.g. a developer running it from a
//!     shell): we reattach to the parent console so logs print there, without
//!     spawning a window.
//!   * The user explicitly opts in (`ShowConsole=1` in `deadsync.ini` or the
//!     `--console` flag): we allocate a fresh console window.
//!
//! On non-Windows platforms the process is always attached to its controlling
//! terminal, so this is a no-op.

/// Set up the console according to the user's preference.
///
/// `show` is the resolved `ShowConsole` preference (config value, possibly
/// overridden by the `--console` CLI flag). Call this once, as early as possible
/// during startup and before the logger is initialized, so the very first log
/// lines land in the right place.
pub fn init(show: bool) {
    #[cfg(windows)]
    imp::init(show);
    #[cfg(not(windows))]
    let _ = show;
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{
        GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK,
        FILE_TYPE_PIPE, GetFileType, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AllocConsole, AttachConsole, GetConsoleWindow, GetStdHandle,
        STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows::core::{PCWSTR, w};

    pub fn init(show: bool) {
        // A console-subsystem build (e.g. debug) already owns a console that the
        // OS wired up to our stdio handles — leave it alone.
        if have_console() {
            return;
        }

        // GUI-subsystem build. If we were launched from a terminal, reattach to
        // it so CLI usage still prints; otherwise only create a window when the
        // user asked for one.
        // SAFETY: `AttachConsole`/`AllocConsole` take no Rust-owned pointers and
        // we check their results; on success we rebind the process std handles to
        // the now-current console below.
        unsafe {
            let attached = AttachConsole(ATTACH_PARENT_PROCESS).is_ok();
            if attached || (show && AllocConsole().is_ok()) {
                bind_std_handles();
            }
        }
    }

    fn have_console() -> bool {
        // SAFETY: `GetConsoleWindow` takes no arguments and returns a borrowed
        // window handle (or null) that we only test for nullness.
        unsafe { !GetConsoleWindow().0.is_null() }
    }

    /// Point the process std handles at the freshly (re)attached console so
    /// Rust's `std::io::{stdout, stderr, stdin}` and the `log` backend reach it.
    /// Without this, a GUI-subsystem process has null std handles and all output
    /// is silently discarded even after attaching a console.
    ///
    /// SAFETY: callers must have an active console (just attached or allocated).
    unsafe fn bind_std_handles() {
        // stdout and stderr each get their own `CONOUT$` handle: registering one
        // shared handle in two slots means closing either (e.g. by FFI/C runtime
        // code) would dangle the other.
        bind_console_handle(STD_OUTPUT_HANDLE, w!("CONOUT$"));
        bind_console_handle(STD_ERROR_HANDLE, w!("CONOUT$"));
        bind_console_handle(STD_INPUT_HANDLE, w!("CONIN$"));
    }

    /// (Re)point a single std handle at the console device, opening a dedicated
    /// handle for it. Skips handles the parent shell already redirected to a
    /// file or pipe (e.g. `deadsync.exe > out.txt`), so attaching the console
    /// doesn't silently clobber the redirection.
    fn bind_console_handle(std_id: STD_HANDLE, device: PCWSTR) {
        if is_redirected(std_id) {
            return;
        }
        if let Some(handle) = open_console_handle(device) {
            // SAFETY: `handle` is a valid console handle from `CreateFileW`.
            unsafe {
                let _ = SetStdHandle(std_id, handle);
            }
        }
    }

    /// True if `std_id` already holds a valid file or pipe handle — i.e. the
    /// parent process redirected this stream into us and we must not overwrite
    /// it. A null/invalid handle (the normal GUI-subsystem case) is not a
    /// redirect, and a console handle is replaced with our own.
    fn is_redirected(std_id: STD_HANDLE) -> bool {
        // SAFETY: both calls take only the handle id / a borrowed handle and
        // return copies we inspect; nothing is retained.
        unsafe {
            let Ok(handle) = GetStdHandle(std_id) else {
                return false;
            };
            if handle.0.is_null() || handle == INVALID_HANDLE_VALUE {
                return false;
            }
            let kind = GetFileType(handle);
            kind == FILE_TYPE_DISK || kind == FILE_TYPE_PIPE
        }
    }

    fn open_console_handle(name: PCWSTR) -> Option<HANDLE> {
        // SAFETY: `name` is a static, NUL-terminated wide string (`CONOUT$` /
        // `CONIN$`); the remaining arguments are plain flag values and the call
        // borrows nothing beyond the string for its duration.
        unsafe {
            CreateFileW(
                name,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
            .ok()
        }
    }
}
